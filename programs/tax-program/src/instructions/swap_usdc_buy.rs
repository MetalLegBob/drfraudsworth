//! swap_usdc_buy: USDC -> CRIME/FRAUD with buy tax.
//!
//! Deducts buy tax from USDC input, sends all tax to the Rebalancer's USDC
//! accumulator via a single SPL Token transfer, then invokes AMM swap via CPI.
//!
//! Dramatically simpler than swap_sol_buy: no 3-way split, no native SOL
//! transfers, no staking/carnage/treasury distribution. 7 fewer accounts.
//!
//! Source: Phase 127 USDC Tax Instructions

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_signed},
};
use anchor_lang::AccountDeserialize;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{
    amm_program_id, crime_mint, epoch_program_id, fraud_mint, rebalancer_program_id,
    usdc_mint, MINIMUM_OUTPUT_FLOOR_BPS, SWAP_AUTHORITY_SEED, USDC_ACCUMULATOR_SEED,
};
use crate::errors::TaxError;
use crate::events::{PoolType, SwapDirection, TaxedSwap};
use crate::helpers::pool_reader::read_pool_reserves_for_usdc;
use crate::helpers::tax_math::{calculate_output_floor, calculate_tax};
use crate::state::EpochState;

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Execute a USDC -> CRIME/FRAUD swap with buy tax.
///
/// Flow:
/// 1. Read tax rate from EpochState (dynamic per epoch)
/// 2. Derive tax identity from on-chain mint (defense-in-depth)
/// 3. Calculate tax = amount_in * tax_bps / 10_000
/// 4. Calculate usdc_to_swap = amount_in - tax
/// 5. Enforce protocol minimum output floor (SEC-10)
/// 6. Transfer USDC tax to accumulator via SPL Token transfer (user signs)
/// 7. Snapshot token balance, execute AMM CPI, compute output via balance-diff
/// 8. Emit TaxedSwap event with accumulator_portion = tax_amount
///
/// # Arguments
/// * `amount_in` - Total USDC amount to spend (including tax, 6 decimals)
/// * `minimum_output` - Minimum CRIME/FRAUD tokens expected (slippage protection)
/// * `is_crime` - true = CRIME pool, false = FRAUD pool
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, SwapUsdcBuy<'info>>,
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
    // 1b. Derive tax-side identity from on-chain mints (defense-in-depth)
    //
    // Determine which side of the pool is USDC vs CRIME/FRAUD. The token mint
    // (non-USDC side) determines the tax schedule. The caller-supplied is_crime
    // flag is a witness cross-checked against the on-chain mint.
    // =========================================================================
    let usdc_key = usdc_mint();
    let usdc_is_mint_a = ctx.accounts.mint_a.key() == usdc_key;

    // The token mint (CRIME/FRAUD) is whichever side is NOT USDC
    let token_mint_key = if usdc_is_mint_a {
        ctx.accounts.mint_b.key()
    } else {
        ctx.accounts.mint_a.key()
    };

    let derived_is_crime = if token_mint_key == crime_mint() {
        true
    } else if token_mint_key == fraud_mint() {
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
    // 3. Calculate USDC to swap (after tax deduction)
    // =========================================================================
    let usdc_to_swap = amount_in
        .checked_sub(tax_amount)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // Validate we have something to swap
    require!(usdc_to_swap > 0, TaxError::InsufficientInput);

    // =========================================================================
    // 3b. Enforce protocol minimum output floor (SEC-10) AND bind pool's
    //     token mint to the passed mint account.
    //
    // For buy (USDC -> Token): reserve_in = usdc_reserve, reserve_out = token_reserve.
    // Uses usdc_to_swap (post-tax), not amount_in, because tax is deducted from
    // input before the swap.
    // =========================================================================
    let (usdc_reserve, token_reserve, pool_token_mint) =
        read_pool_reserves_for_usdc(&ctx.accounts.pool, &usdc_key)?;

    // Bind the pool's token-side mint to the passed mint account
    require_keys_eq!(
        pool_token_mint,
        token_mint_key,
        TaxError::PoolMintMismatch
    );

    let output_floor = calculate_output_floor(usdc_reserve, token_reserve, usdc_to_swap, MINIMUM_OUTPUT_FLOOR_BPS)
        .ok_or(error!(TaxError::TaxOverflow))?;
    require!(
        minimum_output >= output_floor,
        TaxError::MinimumOutputFloorViolation
    );

    // =========================================================================
    // 4. Transfer USDC tax to accumulator
    //
    // BEFORE the AMM CPI (tax deducted from input). Uses invoke() -- user's
    // signature propagates via CPI, no PDA signing needed.
    //
    // The USDC side's token program is SPL Token (classic), discriminator = 3.
    // =========================================================================
    let (usdc_token_program, usdc_user_ata) = if usdc_is_mint_a {
        (
            ctx.accounts.token_program_a.to_account_info(),
            ctx.accounts.user_token_a.to_account_info(),
        )
    } else {
        (
            ctx.accounts.token_program_b.to_account_info(),
            ctx.accounts.user_token_b.to_account_info(),
        )
    };

    if tax_amount > 0 {
        let transfer_tax_ix = Instruction {
            program_id: usdc_token_program.key(),
            accounts: vec![
                AccountMeta::new(usdc_user_ata.key(), false),
                AccountMeta::new(ctx.accounts.usdc_accumulator.key(), false),
                AccountMeta::new_readonly(ctx.accounts.user.key(), true),
            ],
            data: {
                let mut d = vec![3u8]; // SPL Token Transfer instruction discriminator
                d.extend_from_slice(&tax_amount.to_le_bytes());
                d
            },
        };
        invoke(
            &transfer_tax_ix,
            &[
                usdc_user_ata.clone(),
                ctx.accounts.usdc_accumulator.to_account_info(),
                ctx.accounts.user.to_account_info(),
                usdc_token_program.clone(),
            ],
        )?;
    }

    // =========================================================================
    // 5. Snapshot output token balance for balance-diff after CPI
    // =========================================================================
    let (token_account_before, is_token_b) = if usdc_is_mint_a {
        // USDC is mint_a, so token (CRIME/FRAUD) is mint_b side
        (ctx.accounts.user_token_b.amount, true)
    } else {
        // USDC is mint_b, so token (CRIME/FRAUD) is mint_a side
        (ctx.accounts.user_token_a.amount, false)
    };

    // =========================================================================
    // 6. Build and execute AMM CPI
    // =========================================================================

    // 6a. Build swap_authority PDA signer seeds
    let swap_authority_seeds: &[&[u8]] = &[
        SWAP_AUTHORITY_SEED,
        &[ctx.bumps.swap_authority],
    ];

    // 6b. Build account metas for AMM swap_sol_pool instruction
    //     Order matches AMM's SwapSolPool struct
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
    //     Precomputed: [0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a]
    //
    //     Direction is DYNAMIC based on USDC's canonical position:
    //     - If USDC is mint_a: AtoB (0) -- swapping USDC (A) for tokens (B)
    //     - If USDC is mint_b: BtoA (1) -- swapping USDC (B) for tokens (A)
    const AMM_SWAP_SOL_POOL_DISCRIMINATOR: [u8; 8] = [0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a];

    let direction: u8 = if usdc_is_mint_a { 0 } else { 1 };

    let mut ix_data = Vec::with_capacity(25);
    ix_data.extend_from_slice(&AMM_SWAP_SOL_POOL_DISCRIMINATOR);
    ix_data.extend_from_slice(&usdc_to_swap.to_le_bytes());
    ix_data.push(direction);
    ix_data.extend_from_slice(&minimum_output.to_le_bytes());

    // 6e. Build the instruction
    let ix = Instruction {
        program_id: ctx.accounts.amm_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    // 6f. Build account infos for CPI
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

    // 6g. Execute CPI with swap_authority PDA signature
    invoke_signed(
        &ix,
        &account_infos,
        &[swap_authority_seeds],
    )?;

    // =========================================================================
    // 7. Compute actual output via balance-diff
    //
    // After invoke_signed, the runtime's AccountInfo has been mutated by the
    // AMM CPI, but Anchor's InterfaceAccount wrapper still has stale values.
    // .reload() re-reads from the runtime AccountInfo.
    // =========================================================================
    let tokens_received = if is_token_b {
        ctx.accounts.user_token_b.reload()?;
        ctx.accounts.user_token_b.amount
            .checked_sub(token_account_before)
            .ok_or(error!(TaxError::TaxOverflow))?
    } else {
        ctx.accounts.user_token_a.reload()?;
        ctx.accounts.user_token_a.amount
            .checked_sub(token_account_before)
            .ok_or(error!(TaxError::TaxOverflow))?
    };

    // =========================================================================
    // 8. Emit TaxedSwap event
    // =========================================================================
    let clock = Clock::get()?;

    emit!(TaxedSwap {
        user: ctx.accounts.user.key(),
        pool_type: if derived_is_crime { PoolType::UsdcCrime } else { PoolType::UsdcFraud },
        direction: SwapDirection::Buy,
        input_amount: amount_in,
        output_amount: tokens_received,
        tax_amount,
        tax_rate_bps: tax_bps,
        staking_portion: 0,
        carnage_portion: 0,
        treasury_portion: 0,
        accumulator_portion: tax_amount,
        epoch: epoch_state.current_epoch,
        slot: clock.slot,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

/// Accounts for swap_usdc_buy instruction (USDC -> CRIME/FRAUD).
///
/// Buy tax is deducted from USDC INPUT before passing to AMM.
/// All tax routes to the Rebalancer's USDC accumulator (no 3-way split).
///
/// Direction is dynamic based on USDC's canonical position:
/// - AtoB if USDC is mint_a, BtoA if USDC is mint_b.
///
/// 6 fewer accounts than SwapSolBuy (no staking/carnage/treasury distribution).
#[derive(Accounts)]
pub struct SwapUsdcBuy<'info> {
    /// User initiating the swap - signs and pays USDC for tax
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

    /// Tax Program's swap_authority PDA - signs AMM CPI.
    /// NOT mut -- no lamports flow through it for USDC swaps.
    /// CHECK: PDA derived from seeds, used as signer for CPI
    #[account(
        seeds = [SWAP_AUTHORITY_SEED],
        bump,
    )]
    pub swap_authority: AccountInfo<'info>,

    // === Pool State (AMM) ===
    /// AMM pool state - mutable for reserve updates
    /// CHECK: Validated in AMM CPI
    #[account(mut)]
    pub pool: AccountInfo<'info>,

    // === Pool Vaults ===
    /// Pool vault for whichever mint sorts first canonically
    #[account(mut)]
    pub pool_vault_a: InterfaceAccount<'info, TokenAccount>,

    /// Pool vault for whichever mint sorts second canonically
    #[account(mut)]
    pub pool_vault_b: InterfaceAccount<'info, TokenAccount>,

    // === Mints ===
    /// Whichever mint sorts first canonically (could be USDC or CRIME/FRAUD)
    pub mint_a: InterfaceAccount<'info, Mint>,

    /// Whichever mint sorts second canonically
    pub mint_b: InterfaceAccount<'info, Mint>,

    // === User Token Accounts ===
    /// User's token account for mint_a
    #[account(
        mut,
        constraint = user_token_a.owner == user.key() @ TaxError::InvalidTokenOwner,
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    /// User's token account for mint_b
    #[account(
        mut,
        constraint = user_token_b.owner == user.key() @ TaxError::InvalidTokenOwner,
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    // === USDC Tax Destination ===
    /// Rebalancer's USDC accumulator PDA. Receives all USDC tax.
    /// Validated as token account implicitly by SPL Token transfer.
    ///
    /// CHECK: PDA derived from Rebalancer Program seeds
    #[account(
        mut,
        seeds = [USDC_ACCUMULATOR_SEED],
        bump,
        seeds::program = rebalancer_program_id(),
    )]
    pub usdc_accumulator: AccountInfo<'info>,

    // === Programs ===
    /// AMM Program for swap CPI
    /// CHECK: Address validated against known AMM program ID
    #[account(address = amm_program_id() @ TaxError::InvalidAmmProgram)]
    pub amm_program: AccountInfo<'info>,

    /// Token program for mint_a side (SPL Token for USDC, Token-2022 for CRIME/FRAUD)
    pub token_program_a: Interface<'info, TokenInterface>,

    /// Token program for mint_b side
    pub token_program_b: Interface<'info, TokenInterface>,
}
