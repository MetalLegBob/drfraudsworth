use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{POOL_SEED, REBALANCER_PROGRAM_ID, REBALANCE_SEED};
use crate::errors::AmmError;
use crate::events::LiquidityAddedEvent;
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

/// Inject liquidity into a pool, authorized by the Rebalancer program.
///
/// Follows strict CEI (Checks-Effects-Interactions) ordering:
/// 1. CHECKS: capture immutable state, set reentrancy guard, validate amounts
/// 2. EFFECTS: update reserves with checked arithmetic
/// 3. INTERACTIONS: execute token transfers (source -> vault)
/// 4. POST-INTERACTION: clear reentrancy guard, emit event
///
/// Only callable via CPI from the Rebalancer program, which signs the
/// RebalanceAuthority PDA via invoke_signed. Direct calls will fail
/// because no external signer can produce the correct PDA signature.
///
/// Accepts arbitrary token amounts -- no proportionality enforcement.
/// Pool price adjusts naturally; arbitrageurs correct minor imbalances.
/// One side can be zero (e.g., inject only token A); only both-zero is rejected.
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, AddLiquidity<'info>>,
    amount_a: u64,
    amount_b: u64,
) -> Result<()> {
    // =========================================================================
    // Save immutable values from pool BEFORE any mutable access.
    //
    // Anchor's Account type uses RefCell internally. Once we mutate any field
    // via `&mut ctx.accounts.pool`, we cannot read other fields through a
    // separate immutable borrow. Capturing these upfront avoids borrow conflicts.
    // (Pattern 5 from 126-RESEARCH.md, same as swap_sol_pool.rs lines 70-77)
    // =========================================================================
    let _mint_a_key = ctx.accounts.pool.mint_a;
    let _mint_b_key = ctx.accounts.pool.mint_b;
    let _pool_bump = ctx.accounts.pool.bump;
    let reserve_a = ctx.accounts.pool.reserve_a;
    let reserve_b = ctx.accounts.pool.reserve_b;
    let token_program_a_key = ctx.accounts.pool.token_program_a;
    let token_program_b_key = ctx.accounts.pool.token_program_b;

    // =====================================================================
    // CHECKS
    // =====================================================================

    // 1. Set reentrancy guard (Anchor constraint already verified !pool.locked)
    ctx.accounts.pool.locked = true;

    // 2. Validate that at least one side has a non-zero amount
    require!(
        amount_a > 0 || amount_b > 0,
        AmmError::ZeroInjectionAmounts
    );

    // =====================================================================
    // EFFECTS (update reserves with checked arithmetic)
    // =====================================================================

    // 3. Update reserves via checked_add (opposite of withdraw which uses checked_sub)
    ctx.accounts.pool.reserve_a = reserve_a
        .checked_add(amount_a)
        .ok_or(AmmError::Overflow)?;
    ctx.accounts.pool.reserve_b = reserve_b
        .checked_add(amount_b)
        .ok_or(AmmError::Overflow)?;

    // =====================================================================
    // INTERACTIONS (token transfers: source -> vault)
    // =====================================================================

    // REMAINING_ACCOUNTS CONTRACT (mirrors VH-I001 from swap_sol_pool.rs):
    // In MixedPool (the only type currently used), one side is T22 and one is SPL.
    // The T22 transfer consumes hook accounts from remaining_accounts; the SPL
    // transfer ignores them. The full remaining_accounts goes to the T22 side.
    // If a PureT22Pool is ever used, the caller (Rebalancer) must pre-partition
    // remaining_accounts as [side_a_hooks, side_b_hooks].

    // TRANSFER AUTHORITY: rebalance_authority signs source-to-vault transfers.
    // The Rebalancer's holding accounts use rebalance_authority PDA as their
    // token account authority. Since rebalance_authority is already a Signer
    // in the TX (Rebalancer signed via invoke_signed), we pass it as the
    // authority with empty signer_seeds `&[]` -- same pattern as user-signed
    // transfers in swap_sol_pool.rs lines 226-248.

    let rebalance_authority_info = ctx.accounts.rebalance_authority.to_account_info();

    // Note: We do NOT need pool PDA signer seeds here. Unlike withdraw_liquidity
    // where the pool PDA signs vault-to-destination transfers, add_liquidity has
    // the rebalance_authority sign source-to-vault transfers (already a Signer).
    // Pool vaults accept deposits from any source -- no PDA signature required
    // on the vault side (the vault is the recipient, not the sender).

    // 4. Transfer side A (source_a -> vault_a) if amount_a > 0
    if amount_a > 0 {
        if is_t22(&token_program_a_key) {
            transfer_t22_checked(
                &ctx.accounts.token_program_a.to_account_info(),
                &ctx.accounts.source_a.to_account_info(),
                &ctx.accounts.mint_a.to_account_info(),
                &ctx.accounts.vault_a.to_account_info(),
                &rebalance_authority_info,
                amount_a,
                ctx.accounts.mint_a.decimals,
                &[], // rebalance_authority already signed the outer TX
                ctx.remaining_accounts,
            )?;
        } else {
            transfer_spl(
                &ctx.accounts.token_program_a.to_account_info(),
                &ctx.accounts.source_a.to_account_info(),
                &ctx.accounts.mint_a.to_account_info(),
                &ctx.accounts.vault_a.to_account_info(),
                &rebalance_authority_info,
                amount_a,
                ctx.accounts.mint_a.decimals,
                &[], // rebalance_authority already signed the outer TX
            )?;
        }
    }

    // 5. Transfer side B (source_b -> vault_b) if amount_b > 0
    if amount_b > 0 {
        if is_t22(&token_program_b_key) {
            transfer_t22_checked(
                &ctx.accounts.token_program_b.to_account_info(),
                &ctx.accounts.source_b.to_account_info(),
                &ctx.accounts.mint_b.to_account_info(),
                &ctx.accounts.vault_b.to_account_info(),
                &rebalance_authority_info,
                amount_b,
                ctx.accounts.mint_b.decimals,
                &[], // rebalance_authority already signed the outer TX
                ctx.remaining_accounts,
            )?;
        } else {
            transfer_spl(
                &ctx.accounts.token_program_b.to_account_info(),
                &ctx.accounts.source_b.to_account_info(),
                &ctx.accounts.mint_b.to_account_info(),
                &ctx.accounts.vault_b.to_account_info(),
                &rebalance_authority_info,
                amount_b,
                ctx.accounts.mint_b.decimals,
                &[], // rebalance_authority already signed the outer TX
            )?;
        }
    }

    // =====================================================================
    // POST-INTERACTION
    // =====================================================================

    // 6. Clear reentrancy guard
    ctx.accounts.pool.locked = false;

    // 7. Emit injection event for monitoring/indexing
    let clock = Clock::get()?;
    emit!(LiquidityAddedEvent {
        pool: ctx.accounts.pool.key(),
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

/// Accounts for the `add_liquidity` instruction.
///
/// Injects liquidity into a pool, transferring tokens from caller-provided
/// source accounts to pool vaults. Only callable via CPI from the Rebalancer
/// program (RebalanceAuthority PDA gate).
///
/// The three-layer security model (same as withdraw_liquidity):
/// - Layer 1: RebalanceAuthority PDA gates who can call (seeds::program = REBALANCER_PROGRAM_ID)
/// - Layer 2: Rebalancer code always provides its own PDA-derived holdings as sources
/// - Layer 3: Holding accounts are owned by a Rebalancer PDA (only Rebalancer can move tokens out)
///
/// Transfer authority is the rebalance_authority Signer, NOT the pool PDA.
/// The Rebalancer's holding accounts use rebalance_authority PDA as their
/// token account authority, so the rebalance_authority signature (already
/// present from invoke_signed) authorizes the source-to-vault transfers.
#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    /// RebalanceAuthority PDA: must be signed by Rebalancer program via invoke_signed.
    ///
    /// The Signer type validates this account actually signed the transaction.
    /// The seeds + seeds::program constraint validates the PDA is derived
    /// from REBALANCER_PROGRAM_ID with seeds ["rebalance"].
    ///
    /// Also serves as transfer authority for source-to-vault transfers,
    /// since Rebalancer's holding accounts use this PDA as their authority.
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

    /// Source token account for token A (Rebalancer's holding account).
    /// No ownership constraint -- three-layer security model handles trust.
    #[account(mut)]
    pub source_a: InterfaceAccount<'info, TokenAccount>,

    /// Source token account for token B (Rebalancer's holding account).
    #[account(mut)]
    pub source_b: InterfaceAccount<'info, TokenAccount>,

    /// Token program for mint A (SPL Token or Token-2022).
    /// Validated against pool state to prevent program substitution.
    #[account(constraint = token_program_a.key() == pool.token_program_a @ AmmError::InvalidTokenProgram)]
    pub token_program_a: Interface<'info, TokenInterface>,

    /// Token program for mint B (SPL Token or Token-2022).
    #[account(constraint = token_program_b.key() == pool.token_program_b @ AmmError::InvalidTokenProgram)]
    pub token_program_b: Interface<'info, TokenInterface>,
}
