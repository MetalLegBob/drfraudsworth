use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{MAX_WITHDRAW_BPS, POOL_SEED, REBALANCER_PROGRAM_ID, REBALANCE_SEED};
use crate::errors::AmmError;
use crate::events::LiquidityWithdrawnEvent;
use crate::helpers::math::calculate_withdraw_amounts;
use crate::helpers::transfers::{transfer_spl, transfer_t22_checked};
use crate::state::pool::PoolState;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Returns true if the given key is the Token-2022 program.
fn is_t22(key: &Pubkey) -> bool {
    *key == anchor_spl::token_2022::ID
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Withdraw proportional liquidity from a pool, authorized by the Rebalancer program.
///
/// Follows strict CEI (Checks-Effects-Interactions) ordering:
/// 1. CHECKS: capture immutable state, set reentrancy guard, validate BPS
/// 2. EFFECTS: calculate withdrawal amounts, update reserves
/// 3. INTERACTIONS: execute token transfers (vault -> destination)
/// 4. POST-INTERACTION: clear reentrancy guard, emit event
///
/// Only callable via CPI from the Rebalancer program, which signs the
/// RebalanceAuthority PDA via invoke_signed. Direct calls will fail
/// because no external signer can produce the correct PDA signature.
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, WithdrawLiquidity<'info>>,
    withdraw_bps: u16,
) -> Result<()> {
    // =========================================================================
    // Save immutable values from pool BEFORE any mutable access.
    //
    // Anchor's Account type uses RefCell internally. Once we mutate any field
    // via `&mut ctx.accounts.pool`, we cannot read other fields through a
    // separate immutable borrow. Capturing these upfront avoids borrow conflicts.
    // (Pattern 5 from 126-RESEARCH.md, same as swap_sol_pool.rs lines 70-77)
    // =========================================================================
    let mint_a_key = ctx.accounts.pool.mint_a;
    let mint_b_key = ctx.accounts.pool.mint_b;
    let pool_bump = ctx.accounts.pool.bump;
    let reserve_a = ctx.accounts.pool.reserve_a;
    let reserve_b = ctx.accounts.pool.reserve_b;
    let token_program_a_key = ctx.accounts.pool.token_program_a;
    let token_program_b_key = ctx.accounts.pool.token_program_b;

    // =====================================================================
    // CHECKS
    // =====================================================================

    // 1. Set reentrancy guard (Anchor constraint already verified !pool.locked)
    ctx.accounts.pool.locked = true;

    // 2. Validate withdraw_bps bounds (defense-in-depth -- math function also checks)
    require!(withdraw_bps > 0, AmmError::ZeroWithdrawBps);
    require!(
        withdraw_bps <= MAX_WITHDRAW_BPS,
        AmmError::WithdrawExceedsMax
    );

    // =====================================================================
    // EFFECTS (calculate amounts and update reserves)
    // =====================================================================

    // 3. Calculate proportional withdrawal amounts from both sides
    let (amount_a, amount_b) =
        calculate_withdraw_amounts(reserve_a, reserve_b, withdraw_bps).ok_or(AmmError::Overflow)?;

    // 4. Guard: if BOTH amounts are zero, the withdrawal is a no-op (reserves too small for BPS)
    require!(
        amount_a > 0 || amount_b > 0,
        AmmError::ZeroWithdrawAmounts
    );

    // 5. Update reserves (checked_sub prevents underflow -- should never happen given BPS <= 50%)
    ctx.accounts.pool.reserve_a = reserve_a
        .checked_sub(amount_a)
        .ok_or(AmmError::Overflow)?;
    ctx.accounts.pool.reserve_b = reserve_b
        .checked_sub(amount_b)
        .ok_or(AmmError::Overflow)?;

    // =====================================================================
    // INTERACTIONS (token transfers: vault -> destination)
    // =====================================================================

    // REMAINING_ACCOUNTS CONTRACT (mirrors VH-I001 from swap_sol_pool.rs):
    // In MixedPool (the only type currently used), one side is T22 and one is SPL.
    // The T22 transfer consumes hook accounts from remaining_accounts; the SPL
    // transfer ignores them. The full remaining_accounts goes to the T22 side.
    // If a PureT22Pool is ever used, the caller (Rebalancer) must pre-partition
    // remaining_accounts as [side_a_hooks, side_b_hooks].

    // 6. Build PDA signer seeds for vault-to-destination transfers.
    //    Uses saved immutable values (mint keys, bump) from before mutations.
    let mint_a_bytes = mint_a_key.to_bytes();
    let mint_b_bytes = mint_b_key.to_bytes();
    let bump_bytes = [pool_bump];
    let pool_seeds: &[&[u8]] = &[POOL_SEED, &mint_a_bytes, &mint_b_bytes, &bump_bytes];
    let signer_seeds: &[&[&[u8]]] = &[pool_seeds];

    let pool_account_info = ctx.accounts.pool.to_account_info();

    // 7. Transfer side A (vault_a -> destination_a) if amount_a > 0
    if amount_a > 0 {
        if is_t22(&token_program_a_key) {
            transfer_t22_checked(
                &ctx.accounts.token_program_a.to_account_info(),
                &ctx.accounts.vault_a.to_account_info(),
                &ctx.accounts.mint_a.to_account_info(),
                &ctx.accounts.destination_a.to_account_info(),
                &pool_account_info,
                amount_a,
                ctx.accounts.mint_a.decimals,
                signer_seeds,
                ctx.remaining_accounts,
            )?;
        } else {
            transfer_spl(
                &ctx.accounts.token_program_a.to_account_info(),
                &ctx.accounts.vault_a.to_account_info(),
                &ctx.accounts.mint_a.to_account_info(),
                &ctx.accounts.destination_a.to_account_info(),
                &pool_account_info,
                amount_a,
                ctx.accounts.mint_a.decimals,
                signer_seeds,
            )?;
        }
    }

    // 8. Transfer side B (vault_b -> destination_b) if amount_b > 0
    if amount_b > 0 {
        if is_t22(&token_program_b_key) {
            transfer_t22_checked(
                &ctx.accounts.token_program_b.to_account_info(),
                &ctx.accounts.vault_b.to_account_info(),
                &ctx.accounts.mint_b.to_account_info(),
                &ctx.accounts.destination_b.to_account_info(),
                &pool_account_info,
                amount_b,
                ctx.accounts.mint_b.decimals,
                signer_seeds,
                ctx.remaining_accounts,
            )?;
        } else {
            transfer_spl(
                &ctx.accounts.token_program_b.to_account_info(),
                &ctx.accounts.vault_b.to_account_info(),
                &ctx.accounts.mint_b.to_account_info(),
                &ctx.accounts.destination_b.to_account_info(),
                &pool_account_info,
                amount_b,
                ctx.accounts.mint_b.decimals,
                signer_seeds,
            )?;
        }
    }

    // =====================================================================
    // POST-INTERACTION
    // =====================================================================

    // 9. Clear reentrancy guard
    ctx.accounts.pool.locked = false;

    // 10. Emit withdrawal event for monitoring/indexing
    let clock = Clock::get()?;
    emit!(LiquidityWithdrawnEvent {
        pool: ctx.accounts.pool.key(),
        withdraw_bps,
        amount_a,
        amount_b,
        reserve_a_after: ctx.accounts.pool.reserve_a,
        reserve_b_after: ctx.accounts.pool.reserve_b,
        timestamp: clock.unix_timestamp,
        slot: clock.slot,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

/// Accounts for the `withdraw_liquidity` instruction.
///
/// Withdraws proportional liquidity from a pool, transferring tokens from
/// pool vaults to caller-provided destination accounts. Only callable via
/// CPI from the Rebalancer program (RebalanceAuthority PDA gate).
///
/// The three-layer security model (same as swap_sol_pool):
/// - Layer 1: RebalanceAuthority PDA gates who can call (seeds::program = REBALANCER_PROGRAM_ID)
/// - Layer 2: Rebalancer code always provides its own PDA-derived holdings as destinations
/// - Layer 3: Holding accounts are owned by a Rebalancer PDA (only Rebalancer can move tokens out)
///
/// No ownership constraint on destination_a/destination_b -- the caller provides
/// them, and the security model handles trust via the layers above.
#[derive(Accounts)]
pub struct WithdrawLiquidity<'info> {
    /// RebalanceAuthority PDA: must be signed by Rebalancer program via invoke_signed.
    ///
    /// The Signer type validates this account actually signed the transaction.
    /// The seeds + seeds::program constraint validates the PDA is derived
    /// from REBALANCER_PROGRAM_ID with seeds ["rebalance"].
    ///
    /// This ensures only the Rebalancer program can invoke this instruction --
    /// direct calls without valid rebalance_authority will fail deserialization.
    #[account(
        seeds = [REBALANCE_SEED],
        bump,
        seeds::program = REBALANCER_PROGRAM_ID,
    )]
    pub rebalance_authority: Signer<'info>,

    /// Pool state PDA. Mutable for reserve updates and reentrancy guard.
    /// Seeds validate this is the correct pool for the given mint pair.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.mint_a.as_ref(), pool.mint_b.as_ref()],
        bump = pool.bump,
        constraint = pool.initialized @ AmmError::PoolNotInitialized,
        constraint = !pool.locked @ AmmError::PoolLocked,
    )]
    pub pool: Account<'info, PoolState>,

    /// Vault A: PDA-owned token account holding reserve A.
    /// Validated against pool state to prevent vault substitution attacks.
    #[account(
        mut,
        constraint = vault_a.key() == pool.vault_a @ AmmError::VaultMismatch,
    )]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault B: PDA-owned token account holding reserve B.
    #[account(
        mut,
        constraint = vault_b.key() == pool.vault_b @ AmmError::VaultMismatch,
    )]
    pub vault_b: InterfaceAccount<'info, TokenAccount>,

    /// Mint A: used for decimals in transfer_checked.
    #[account(constraint = mint_a.key() == pool.mint_a @ AmmError::InvalidMint)]
    pub mint_a: InterfaceAccount<'info, Mint>,

    /// Mint B: used for decimals in transfer_checked.
    #[account(constraint = mint_b.key() == pool.mint_b @ AmmError::InvalidMint)]
    pub mint_b: InterfaceAccount<'info, Mint>,

    /// Destination token account for token A (Rebalancer's holding account).
    /// No ownership constraint -- three-layer security model handles trust.
    #[account(mut)]
    pub destination_a: InterfaceAccount<'info, TokenAccount>,

    /// Destination token account for token B (Rebalancer's holding account).
    #[account(mut)]
    pub destination_b: InterfaceAccount<'info, TokenAccount>,

    /// Token program for mint A (SPL Token or Token-2022).
    /// Validated against pool state to prevent program substitution.
    #[account(constraint = token_program_a.key() == pool.token_program_a @ AmmError::InvalidTokenProgram)]
    pub token_program_a: Interface<'info, TokenInterface>,

    /// Token program for mint B (SPL Token or Token-2022).
    #[account(constraint = token_program_b.key() == pool.token_program_b @ AmmError::InvalidTokenProgram)]
    pub token_program_b: Interface<'info, TokenInterface>,
}
