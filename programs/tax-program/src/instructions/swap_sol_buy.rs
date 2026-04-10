//! swap_sol_buy: SOL -> CRIME/FRAUD with buy tax.
//!
//! Deducts buy tax from SOL input, distributes tax atomically (71% staking,
//! 24% carnage, 5% treasury), then invokes AMM swap_sol_pool via CPI.
//!
//! Source: Tax_Pool_Logic_Spec.md Section 10.2, 14.2

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
    system_instruction,
};
use anchor_lang::AccountDeserialize;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{
    amm_program_id, crime_mint, epoch_program_id, fraud_mint, staking_program_id,
    treasury_pubkey, CARNAGE_SOL_VAULT_SEED, DEPOSIT_REWARDS_DISCRIMINATOR,
    ESCROW_VAULT_SEED, MINIMUM_OUTPUT_FLOOR_BPS, STAKE_POOL_SEED, SWAP_AUTHORITY_SEED,
    TAX_AUTHORITY_SEED,
};
use crate::errors::TaxError;
use crate::events::{PoolType, SwapDirection, TaxedSwap};
use crate::helpers::pool_reader::read_pool_reserves_with_token_mint;
use crate::helpers::tax_math::{calculate_output_floor, calculate_tax, split_distribution};
use crate::state::EpochState;

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Execute a SOL -> CRIME/FRAUD swap with buy tax.
///
/// Flow (per Tax_Pool_Logic_Spec.md Section 14.2):
/// 1. Read tax rate from EpochState (dynamic per epoch)
/// 2. Calculate tax = amount_in * tax_bps / 10_000
/// 3. Calculate sol_to_swap = amount_in - tax
/// 4. Split distribution: 71% staking, 24% carnage, 5% treasury
/// 5. Execute native SOL transfers for tax distribution
/// 6. Build and execute AMM CPI with swap_authority PDA signing
/// 7. Emit TaxedSwap event
///
/// # Arguments
/// * `amount_in` - Total SOL amount user wants to spend (including tax)
/// * `minimum_output` - Minimum CRIME/FRAUD tokens expected (slippage protection)
/// * `is_crime` - true = CRIME pool, false = FRAUD pool
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, SwapSolBuy<'info>>,
    amount_in: u64,
    minimum_output: u64,
    is_crime: bool,
) -> Result<()> {
    // =========================================================================
    // 1. Read and validate EpochState
    // =========================================================================

    // Owner check: EpochState must be owned by Epoch Program.
    // CRITICAL: This prevents attackers from passing a fake EpochState with 0% tax.
    let epoch_program = epoch_program_id();
    require!(
        ctx.accounts.epoch_state.owner == &epoch_program,
        TaxError::InvalidEpochState
    );

    // Deserialize EpochState data.
    // try_deserialize validates the discriminator automatically (sha256("account:EpochState")[0..8]).
    let epoch_state = {
        let data = ctx.accounts.epoch_state.try_borrow_data()?;
        let mut data_slice: &[u8] = &data;
        EpochState::try_deserialize(&mut data_slice)
            .map_err(|_| error!(TaxError::InvalidEpochState))?
    };

    // Validate EpochState is initialized (defense-in-depth).
    require!(epoch_state.initialized, TaxError::InvalidEpochState);

    // =========================================================================
    // 1b. Bind tax-side identity to the validated mint_b account (Option B)
    //
    // The caller-supplied `is_crime` flag must NOT be the source of truth for
    // which tax schedule applies — it is treated as a witness and cross-checked
    // against the on-chain mint_b account. The mint_b account is itself
    // validated against pool.mint_b in step 3b below (Option C), so the chain
    // is: pool bytes -> mint_b account -> derived_is_crime -> tax schedule.
    //
    // See 122.1-CONTEXT.md §3.1.
    // =========================================================================
    let mint_b_key = ctx.accounts.mint_b.key();
    let derived_is_crime = if mint_b_key == crime_mint() {
        true
    } else if mint_b_key == fraud_mint() {
        false
    } else {
        return err!(TaxError::UnknownTaxedMint);
    };
    require!(
        is_crime == derived_is_crime,
        TaxError::TaxIdentityMismatch
    );

    // Get the appropriate tax rate (is_buy = true for buy direction).
    // Use derived_is_crime so the on-chain state is the only source of truth.
    let tax_bps = epoch_state.get_tax_bps(derived_is_crime, true);

    // =========================================================================
    // 2. Calculate tax amount
    // =========================================================================
    let tax_amount = calculate_tax(amount_in, tax_bps)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // =========================================================================
    // 3. Calculate SOL to swap (after tax deduction)
    // =========================================================================
    let sol_to_swap = amount_in
        .checked_sub(tax_amount)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // Validate we have something to swap
    require!(sol_to_swap > 0, TaxError::InsufficientInput);

    // =========================================================================
    // 3b. Enforce protocol minimum output floor (SEC-10) AND bind pool.mint_b
    //     to the passed mint_b account (Option C, see 122.1-CONTEXT.md §3.2).
    //
    // Read pool reserves from raw AccountInfo bytes (no AMM crate dependency).
    // For buy (AtoB): reserve_in = reserve_a (SOL), reserve_out = reserve_b (token).
    // Uses sol_to_swap (post-tax), not amount_in, because tax is deducted from
    // input before the swap. Using amount_in would compute a higher expected
    // output than achievable, making the floor too tight.
    //
    // The extended pool reader also returns the pool's stored token-side mint
    // (whichever of pool.mint_a / pool.mint_b is NOT NATIVE_MINT). We bind it
    // to the passed mint_b account so a caller cannot pair a CRIME pool with
    // the FRAUD mint account (or vice versa) to bypass the tax schedule
    // selected in step 1b.
    // =========================================================================
    let (sol_reserve, token_reserve, pool_token_mint) =
        read_pool_reserves_with_token_mint(&ctx.accounts.pool)?;
    require_keys_eq!(
        pool_token_mint,
        ctx.accounts.mint_b.key(),
        TaxError::PoolMintMismatch
    );
    let output_floor = calculate_output_floor(sol_reserve, token_reserve, sol_to_swap, MINIMUM_OUTPUT_FLOOR_BPS)
        .ok_or(error!(TaxError::TaxOverflow))?;
    require!(
        minimum_output >= output_floor,
        TaxError::MinimumOutputFloorViolation
    );

    // =========================================================================
    // 4. Split tax distribution: 71% staking, 24% carnage, 5% treasury
    // =========================================================================
    let (staking_portion, carnage_portion, treasury_portion) =
        split_distribution(tax_amount)
            .ok_or(error!(TaxError::TaxOverflow))?;

    // =========================================================================
    // 5. Execute native SOL transfers for tax distribution
    //    These are CPI calls to System Program
    // =========================================================================

    // 5a. Transfer staking portion (71%) and notify Staking Program
    if staking_portion > 0 {
        // Transfer SOL to staking escrow
        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.user.key,
                ctx.accounts.staking_escrow.key,
                staking_portion,
            ),
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.staking_escrow.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[], // User signs, no PDA signature needed
        )?;

        // CPI to Staking Program's deposit_rewards to update pending_rewards counter.
        // The SOL is already in escrow; this just updates the state.
        let tax_authority_seeds: &[&[u8]] =
            &[TAX_AUTHORITY_SEED, &[ctx.bumps.tax_authority]];

        // Build deposit_rewards instruction data: discriminator (8) + amount (8)
        let mut deposit_ix_data = Vec::with_capacity(16);
        deposit_ix_data.extend_from_slice(&DEPOSIT_REWARDS_DISCRIMINATOR);
        deposit_ix_data.extend_from_slice(&staking_portion.to_le_bytes());

        // Build account metas for deposit_rewards:
        // Order matches Staking's DepositRewards struct: tax_authority, stake_pool, escrow_vault
        let deposit_accounts = vec![
            AccountMeta::new_readonly(ctx.accounts.tax_authority.key(), true), // signer
            AccountMeta::new(ctx.accounts.stake_pool.key(), false),
            AccountMeta::new_readonly(ctx.accounts.staking_escrow.key(), false), // escrow_vault (balance reconciliation)
        ];

        let deposit_ix = Instruction {
            program_id: ctx.accounts.staking_program.key(),
            accounts: deposit_accounts,
            data: deposit_ix_data,
        };

        invoke_signed(
            &deposit_ix,
            &[
                ctx.accounts.tax_authority.to_account_info(),
                ctx.accounts.stake_pool.to_account_info(),
                ctx.accounts.staking_escrow.to_account_info(),
                ctx.accounts.staking_program.to_account_info(),
            ],
            &[tax_authority_seeds],
        )?;
    }

    // 5b. Transfer carnage portion (24%)
    if carnage_portion > 0 {
        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.user.key,
                ctx.accounts.carnage_vault.key,
                carnage_portion,
            ),
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.carnage_vault.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[], // User signs, no PDA signature needed
        )?;
    }

    // 5c. Transfer treasury portion (5% / remainder)
    if treasury_portion > 0 {
        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.user.key,
                ctx.accounts.treasury.key,
                treasury_portion,
            ),
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[], // User signs, no PDA signature needed
        )?;
    }

    // =========================================================================
    // 6. Build and execute AMM CPI
    // =========================================================================

    // 6a. Build swap_authority PDA signer seeds
    let swap_authority_seeds: &[&[u8]] = &[
        SWAP_AUTHORITY_SEED,
        &[ctx.bumps.swap_authority],
    ];

    // 6b. Build account metas for AMM swap_sol_pool instruction
    //     Order matches AMM's SwapSolPool struct (see swap_sol_pool.rs):
    //     swap_authority, pool, vault_a, vault_b, mint_a, mint_b,
    //     user_token_a, user_token_b, user, token_program_a, token_program_b
    let mut account_metas = vec![
        AccountMeta::new_readonly(ctx.accounts.swap_authority.key(), true), // signer
        AccountMeta::new(ctx.accounts.pool.key(), false),
        AccountMeta::new(ctx.accounts.pool_vault_a.key(), false),
        AccountMeta::new(ctx.accounts.pool_vault_b.key(), false),
        AccountMeta::new_readonly(ctx.accounts.mint_a.key(), false),
        AccountMeta::new_readonly(ctx.accounts.mint_b.key(), false),
        AccountMeta::new(ctx.accounts.user_token_a.key(), false),
        AccountMeta::new(ctx.accounts.user_token_b.key(), false),
        AccountMeta::new_readonly(ctx.accounts.user.key(), true), // user also signs
        AccountMeta::new_readonly(ctx.accounts.token_program_a.key(), false),
        AccountMeta::new_readonly(ctx.accounts.token_program_b.key(), false),
    ];

    // 6c. Forward remaining_accounts for transfer hook support
    //     The AMM passes these to Token-2022 transfer_checked calls
    for account in ctx.remaining_accounts.iter() {
        if account.is_writable {
            account_metas.push(AccountMeta::new(account.key(), account.is_signer));
        } else {
            account_metas.push(AccountMeta::new_readonly(account.key(), account.is_signer));
        }
    }

    // 6d. Build instruction data for AMM swap_sol_pool
    //     Format: discriminator (8 bytes) + amount_in (8) + direction (1) + minimum_out (8)
    //
    //     Anchor discriminator = first 8 bytes of sha256("global:swap_sol_pool")
    //     Precomputed: sha256("global:swap_sol_pool")[0..8] = [0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a]
    //     Direction: AtoB = 0 (SOL -> Token, which is what we want for buy)
    const AMM_SWAP_SOL_POOL_DISCRIMINATOR: [u8; 8] = [0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a];

    let mut ix_data = Vec::with_capacity(25);
    ix_data.extend_from_slice(&AMM_SWAP_SOL_POOL_DISCRIMINATOR);
    ix_data.extend_from_slice(&sol_to_swap.to_le_bytes());
    ix_data.push(0u8); // SwapDirection::AtoB = 0
    ix_data.extend_from_slice(&minimum_output.to_le_bytes());

    // 6e. Build the instruction
    let ix = Instruction {
        program_id: ctx.accounts.amm_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    // 6f. Build account infos for CPI (same order as account_metas, plus AMM program)
    let mut account_infos = vec![
        ctx.accounts.swap_authority.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.pool_vault_a.to_account_info(),
        ctx.accounts.pool_vault_b.to_account_info(),
        ctx.accounts.mint_a.to_account_info(),
        ctx.accounts.mint_b.to_account_info(),
        ctx.accounts.user_token_a.to_account_info(),
        ctx.accounts.user_token_b.to_account_info(),
        ctx.accounts.user.to_account_info(),
        ctx.accounts.token_program_a.to_account_info(),
        ctx.accounts.token_program_b.to_account_info(),
    ];

    // Forward remaining_accounts for transfer hook
    for account in ctx.remaining_accounts.iter() {
        account_infos.push(account.clone());
    }

    // Add AMM program account info (required for CPI)
    account_infos.push(ctx.accounts.amm_program.to_account_info());

    // =========================================================================
    // 6gb. Snapshot output token balance for event (FIX-06)
    //
    // After CPI, Anchor's cached struct has stale values. We snapshot before
    // and reload after to compute the actual tokens received via balance-diff.
    // This is the same pattern used in swap_sol_sell.rs (proven in Phase 48).
    // =========================================================================
    let token_b_before = ctx.accounts.user_token_b.amount;

    // 6g. Execute CPI with swap_authority PDA signature
    invoke_signed(
        &ix,
        &account_infos,
        &[swap_authority_seeds],
    )?;

    // =========================================================================
    // 6h. Compute actual output via balance-diff (FIX-06)
    //
    // After invoke_signed, the runtime's AccountInfo has been mutated by the
    // AMM CPI, but Anchor's InterfaceAccount wrapper still has stale values.
    // .reload() re-reads from the runtime AccountInfo.
    // =========================================================================
    ctx.accounts.user_token_b.reload()?;
    let tokens_received = ctx.accounts.user_token_b.amount
        .checked_sub(token_b_before)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // =========================================================================
    // 7. Emit TaxedSwap event
    // =========================================================================
    let clock = Clock::get()?;

    emit!(TaxedSwap {
        user: ctx.accounts.user.key(),
        pool_type: if derived_is_crime { PoolType::SolCrime } else { PoolType::SolFraud },
        direction: SwapDirection::Buy,
        input_amount: amount_in,
        output_amount: tokens_received,
        tax_amount,
        tax_rate_bps: tax_bps,
        staking_portion,
        carnage_portion,
        treasury_portion,
        epoch: epoch_state.current_epoch,
        slot: clock.slot,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

/// Accounts for swap_sol_buy instruction (SOL -> CRIME/FRAUD).
///
/// Buy tax is deducted from SOL INPUT before passing to AMM.
/// Direction is AtoB (Token A = WSOL in, Token B = CRIME/FRAUD out).
///
/// Source: Tax_Pool_Logic_Spec.md Section 10.2
#[derive(Accounts)]
pub struct SwapSolBuy<'info> {
    /// User initiating the swap - signs and pays SOL for tax
    #[account(mut)]
    pub user: Signer<'info>,

    /// EpochState account from Epoch Program.
    /// Provides current tax rates for the swap.
    ///
    /// CHECK: Validated manually in handler:
    /// - Owner check: must be Epoch Program (prevents fake 0% tax)
    /// - Deserialization validates discriminator
    /// - initialized flag checked
    pub epoch_state: AccountInfo<'info>,

    /// Tax Program's swap_authority PDA - signs AMM CPI
    /// CHECK: PDA derived from seeds, used as signer for CPI
    #[account(
        seeds = [SWAP_AUTHORITY_SEED],
        bump,
    )]
    pub swap_authority: AccountInfo<'info>,

    /// Tax Program's tax_authority PDA - signs Staking Program CPI
    /// CHECK: PDA derived from seeds, used as signer for deposit_rewards CPI
    #[account(
        seeds = [TAX_AUTHORITY_SEED],
        bump,
    )]
    pub tax_authority: AccountInfo<'info>,

    // === Pool State (AMM) ===
    /// AMM pool state - mutable for reserve updates
    /// CHECK: Validated in AMM CPI
    #[account(mut)]
    pub pool: AccountInfo<'info>,

    // === Pool Vaults ===
    /// Pool's WSOL vault (Token A)
    #[account(mut)]
    pub pool_vault_a: InterfaceAccount<'info, TokenAccount>,

    /// Pool's CRIME/FRAUD vault (Token B)
    #[account(mut)]
    pub pool_vault_b: InterfaceAccount<'info, TokenAccount>,

    // === Mints ===
    /// WSOL mint
    pub mint_a: InterfaceAccount<'info, Mint>,

    /// CRIME or FRAUD mint (Token-2022)
    pub mint_b: InterfaceAccount<'info, Mint>,

    // === User Token Accounts ===
    /// User's WSOL token account
    #[account(mut)]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    /// User's CRIME/FRAUD token account
    #[account(mut)]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    // === Staking Program Integration ===
    /// Staking Program's StakePool PDA - updated by deposit_rewards CPI
    /// CHECK: PDA validated by Staking Program via seeds constraint
    #[account(
        mut,
        seeds = [STAKE_POOL_SEED],
        bump,
        seeds::program = staking_program_id(),
    )]
    pub stake_pool: AccountInfo<'info>,

    // === Tax Distribution Targets ===
    /// Staking Program escrow - receives 71% of tax (native SOL)
    /// CHECK: PDA derived from Staking Program seeds
    #[account(
        mut,
        seeds = [ESCROW_VAULT_SEED],
        bump,
        seeds::program = staking_program_id(),
        constraint = true @ TaxError::InvalidStakingEscrow,
    )]
    pub staking_escrow: AccountInfo<'info>,

    /// Carnage Fund vault - receives 24% of tax (native SOL)
    /// CHECK: PDA derived from Epoch Program seeds
    #[account(
        mut,
        seeds = [CARNAGE_SOL_VAULT_SEED],
        bump,
        seeds::program = epoch_program_id(),
        constraint = true @ TaxError::InvalidCarnageVault,
    )]
    pub carnage_vault: AccountInfo<'info>,

    /// Protocol treasury - receives 5% of tax (native SOL)
    /// CHECK: Address validated against known treasury pubkey
    #[account(
        mut,
        address = treasury_pubkey() @ TaxError::InvalidTreasury,
    )]
    pub treasury: AccountInfo<'info>,

    // === Programs ===
    /// AMM Program for swap CPI
    /// CHECK: Address validated against known AMM program ID
    #[account(address = amm_program_id() @ TaxError::InvalidAmmProgram)]
    pub amm_program: AccountInfo<'info>,

    /// SPL Token program (for WSOL)
    pub token_program_a: Interface<'info, TokenInterface>,

    /// Token-2022 program (for CRIME/FRAUD)
    pub token_program_b: Interface<'info, TokenInterface>,

    /// System program (for native SOL transfers)
    pub system_program: Program<'info, System>,

    /// Staking Program for deposit_rewards CPI
    /// CHECK: Program ID validated in constants.rs staking_program_id()
    #[account(address = staking_program_id() @ TaxError::InvalidStakingProgram)]
    pub staking_program: AccountInfo<'info>,
}
