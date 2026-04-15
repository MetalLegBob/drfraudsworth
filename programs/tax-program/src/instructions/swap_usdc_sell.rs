//! swap_usdc_sell: CRIME/FRAUD -> USDC with sell tax.
//!
//! Executes AMM CPI, computes tax on gross USDC output via balance-diff,
//! sends all tax to the Rebalancer's USDC accumulator via SPL Token transfer.
//!
//! Dramatically simpler than swap_sol_sell: no WSOL intermediary, no
//! close/reinit cycle, no native SOL transfers, no 3-way split.
//! The sell flow is: AMM CPI -> compute tax -> one SPL transfer -> emit event.
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

/// Execute a CRIME/FRAUD -> USDC swap with sell tax on output.
///
/// Flow:
/// 1. Read tax rate from EpochState (dynamic per epoch)
/// 2. Derive tax identity from on-chain mint (defense-in-depth)
/// 3. Compute gross output floor (accounts for tax so net >= minimum_output)
/// 4. Snapshot USDC balance, execute AMM CPI
/// 5. Compute gross output, calculate tax, verify slippage
/// 6. Transfer USDC tax to accumulator via SPL Token transfer (user signs)
/// 7. Emit TaxedSwap event with accumulator_portion = tax_amount
///
/// # Arguments
/// * `amount_in` - Token amount to sell (CRIME or FRAUD)
/// * `minimum_output` - Minimum USDC to receive AFTER tax (slippage protection, 6 decimals)
/// * `is_crime` - true = CRIME pool, false = FRAUD pool
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, SwapUsdcSell<'info>>,
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
    // Same logic as swap_usdc_buy: determine USDC position, derive which
    // token (CRIME/FRAUD) is being sold.
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

    // Get the appropriate tax rate (is_buy = false for sell direction).
    let tax_bps = epoch_state.get_tax_bps(derived_is_crime, false);

    // =========================================================================
    // 2. Enforce protocol minimum output floor (SEC-10) AND bind pool's
    //    token mint to the passed mint account.
    //
    // For sell (Token -> USDC): reserve_in = token_reserve, reserve_out = usdc_reserve.
    // The floor protects against zero-slippage sandwich attacks BEFORE CPI.
    // =========================================================================
    let (usdc_reserve, token_reserve, pool_token_mint) =
        read_pool_reserves_for_usdc(&ctx.accounts.pool, &usdc_key)?;

    // Bind the pool's token-side mint to the passed mint account
    require_keys_eq!(
        pool_token_mint,
        token_mint_key,
        TaxError::PoolMintMismatch
    );

    // Compute gross_floor: minimum AMM output such that after tax deduction,
    // user gets >= minimum_output.
    // Formula: gross_floor = ceil(minimum_output * 10000 / (10000 - tax_bps))
    let bps_denom: u64 = 10_000;
    let gross_floor = if minimum_output > 0 && (tax_bps as u64) < bps_denom {
        let numerator = (minimum_output as u128)
            .checked_mul(bps_denom as u128)
            .ok_or(error!(TaxError::TaxOverflow))?;
        let denominator = (bps_denom as u128)
            .checked_sub(tax_bps as u128)
            .ok_or(error!(TaxError::TaxOverflow))?;
        // Ceil division: (numerator + denominator - 1) / denominator
        let result = numerator
            .checked_add(denominator - 1)
            .ok_or(error!(TaxError::TaxOverflow))?
            / denominator;
        u64::try_from(result).map_err(|_| error!(TaxError::TaxOverflow))?
    } else {
        0
    };

    // Output floor check: the gross amount the AMM must produce must be above
    // the protocol's sandwich-protection floor.
    // For sell: reserve_in = token_reserve, reserve_out = usdc_reserve
    let output_floor = calculate_output_floor(token_reserve, usdc_reserve, amount_in, MINIMUM_OUTPUT_FLOOR_BPS)
        .ok_or(error!(TaxError::TaxOverflow))?;
    require!(
        gross_floor >= output_floor,
        TaxError::MinimumOutputFloorViolation
    );

    // =========================================================================
    // 3. Snapshot USDC balance before AMM CPI
    // =========================================================================
    let usdc_before = if usdc_is_mint_a {
        ctx.accounts.user_token_a.amount
    } else {
        ctx.accounts.user_token_b.amount
    };

    // =========================================================================
    // 4. Build and execute AMM CPI
    //
    // Direction is OPPOSITE of buy:
    // - If USDC is mint_a: BtoA (1) -- selling token (B) for USDC (A)
    // - If USDC is mint_b: AtoB (0) -- selling token (A) for USDC (B)
    // =========================================================================
    let swap_authority_seeds: &[&[u8]] = &[SWAP_AUTHORITY_SEED, &[ctx.bumps.swap_authority]];

    const AMM_SWAP_SOL_POOL_DISCRIMINATOR: [u8; 8] = [0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a];

    let direction: u8 = if usdc_is_mint_a { 1 } else { 0 };
    let amm_minimum: u64 = gross_floor;

    let mut ix_data = Vec::with_capacity(25);
    ix_data.extend_from_slice(&AMM_SWAP_SOL_POOL_DISCRIMINATOR);
    ix_data.extend_from_slice(&amount_in.to_le_bytes());
    ix_data.push(direction);
    ix_data.extend_from_slice(&amm_minimum.to_le_bytes());

    // Build account metas (same order as AMM's SwapSolPool struct)
    let mut account_metas = vec![
        AccountMeta::new_readonly(ctx.accounts.swap_authority.key(), true),
        AccountMeta::new(ctx.accounts.pool.key(), false),
        AccountMeta::new(ctx.accounts.pool_vault_a.key(), false),
        AccountMeta::new(ctx.accounts.pool_vault_b.key(), false),
        AccountMeta::new_readonly(ctx.accounts.mint_a.key(), false),
        AccountMeta::new_readonly(ctx.accounts.mint_b.key(), false),
        AccountMeta::new(ctx.accounts.user_token_a.key(), false),
        AccountMeta::new(ctx.accounts.user_token_b.key(), false),
        AccountMeta::new_readonly(ctx.accounts.user.key(), true),
        AccountMeta::new_readonly(ctx.accounts.token_program_a.key(), false),
        AccountMeta::new_readonly(ctx.accounts.token_program_b.key(), false),
    ];

    // Forward remaining_accounts for transfer hook
    for account in ctx.remaining_accounts.iter() {
        if account.is_writable {
            account_metas.push(AccountMeta::new(account.key(), account.is_signer));
        } else {
            account_metas.push(AccountMeta::new_readonly(account.key(), account.is_signer));
        }
    }

    let swap_ix = Instruction {
        program_id: ctx.accounts.amm_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

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
        ctx.accounts.amm_program.to_account_info(),
    ];

    for acc in ctx.remaining_accounts.iter() {
        account_infos.push(acc.clone());
    }

    invoke_signed(&swap_ix, &account_infos, &[swap_authority_seeds])?;

    // =========================================================================
    // 5. Compute gross output and tax
    // =========================================================================
    let gross_usdc_output = if usdc_is_mint_a {
        ctx.accounts.user_token_a.reload()?;
        ctx.accounts.user_token_a.amount
            .checked_sub(usdc_before)
            .ok_or(error!(TaxError::TaxOverflow))?
    } else {
        ctx.accounts.user_token_b.reload()?;
        ctx.accounts.user_token_b.amount
            .checked_sub(usdc_before)
            .ok_or(error!(TaxError::TaxOverflow))?
    };

    let tax_amount = calculate_tax(gross_usdc_output, tax_bps)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // Guard: reject sells where tax >= gross output
    require!(tax_amount < gross_usdc_output, TaxError::InsufficientOutput);

    let net_output = gross_usdc_output
        .checked_sub(tax_amount)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // Slippage check: net output after tax must meet user's minimum
    require!(net_output >= minimum_output, TaxError::SlippageExceeded);

    // =========================================================================
    // 6. Transfer USDC tax to accumulator
    //
    // AFTER computing tax on output. Uses invoke() -- user's signature
    // propagates via CPI, no PDA signing needed.
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
    // 7. Emit TaxedSwap event
    // =========================================================================
    let clock = Clock::get()?;
    emit!(TaxedSwap {
        user: ctx.accounts.user.key(),
        pool_type: if derived_is_crime { PoolType::UsdcCrime } else { PoolType::UsdcFraud },
        direction: SwapDirection::Sell,
        input_amount: amount_in,
        output_amount: net_output,
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

/// Accounts for swap_usdc_sell instruction (CRIME/FRAUD -> USDC).
///
/// Sell tax is deducted from USDC OUTPUT after AMM CPI.
/// All tax routes to the Rebalancer's USDC accumulator (no 3-way split).
///
/// Direction is dynamic based on USDC's canonical position:
/// - BtoA if USDC is mint_a (selling token B for USDC A)
/// - AtoB if USDC is mint_b (selling token A for USDC B)
///
/// No WSOL intermediary, no close/reinit cycle, no system_program needed.
#[derive(Accounts)]
pub struct SwapUsdcSell<'info> {
    /// User initiating the swap - signs SPL Token transfer of tax USDC
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
    /// NOT mut -- no lamports flow through it for USDC swaps
    /// (unlike SOL sell where swap_authority receives unwrapped WSOL).
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

    /// Token program for mint_a side
    pub token_program_a: Interface<'info, TokenInterface>,

    /// Token program for mint_b side
    pub token_program_b: Interface<'info, TokenInterface>,
}
