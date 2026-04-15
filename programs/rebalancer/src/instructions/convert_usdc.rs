//! convert_usdc: Swap accumulated USDC to WSOL via Jupiter CPI (mainnet)
//! or skip CPI (devnet). Pays bounty to caller from bounty vault.
//!
//! Step 1 of the USDC conversion pipeline (REBAL-02):
//!   convert_usdc (USDC -> WSOL) -> distribute_converted_sol (WSOL -> SOL split)
//!
//! Permissionless: anyone can call with valid Jupiter route data.
//! On devnet, Jupiter CPI is feature-gated out entirely.
//!
//! Source: Phase 128, Plan 02

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::rent::Rent;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::sysvar::Sysvar;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::constants::{
    BOUNTY_VAULT_SEED, MIN_CONVERT_AMOUNT, REBALANCE_SEED, USDC_ACCUMULATOR_SEED,
    HOLDING_SEED,
};
use crate::errors::RebalancerError;
use crate::events::UsdcConverted;
use crate::state::RebalancerConfig;

// ---------------------------------------------------------------------------
// Mainnet-only imports
// ---------------------------------------------------------------------------
#[cfg(not(feature = "devnet"))]
use crate::constants::SHARED_ACCOUNTS_ROUTE_DISCRIMINATOR;

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Convert accumulated USDC to WSOL via Jupiter CPI.
///
/// # Flow
/// 1. Check minimum: skip if accumulator < 1 USDC (REBAL-03)
/// 2. Feature gate: devnet skips Jupiter CPI, mainnet executes it
/// 3. Pay bounty to caller from bounty vault (if funded)
/// 4. Emit UsdcConverted event
///
/// # Arguments
/// * `route_data` - Serialized Jupiter SharedAccountsRoute instruction data.
///   Built by the crank from Jupiter Quote + Swap APIs. Ignored on devnet.
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, ConvertUsdc<'info>>,
    route_data: Vec<u8>,
) -> Result<()> {
    // =========================================================================
    // 1. Check minimum accumulator balance (REBAL-03)
    // =========================================================================
    let usdc_amount = ctx.accounts.usdc_accumulator.amount;

    if usdc_amount < MIN_CONVERT_AMOUNT {
        msg!(
            "Below minimum convert amount: {} < {} (skipping)",
            usdc_amount,
            MIN_CONVERT_AMOUNT
        );
        return Ok(());
    }

    msg!("Converting {} USDC-lamports via Jupiter", usdc_amount);

    // =========================================================================
    // 2. Feature-gated Jupiter CPI
    // =========================================================================
    let rebalance_bump = ctx.bumps.rebalance_authority;
    let rebalance_seeds: &[&[u8]] = &[REBALANCE_SEED, &[rebalance_bump]];

    // Devnet: skip CPI, USDC stays in accumulator
    #[cfg(feature = "devnet")]
    let sol_received: u64 = {
        let _ = &route_data; // suppress unused warning
        msg!("Jupiter CPI skipped (devnet mode)");
        0
    };

    // Mainnet: execute Jupiter SharedAccountsRoute CPI
    #[cfg(not(feature = "devnet"))]
    let sol_received: u64 = {
        execute_jupiter_cpi(
            &ctx,
            rebalance_seeds,
            &route_data,
        )?
    };

    // =========================================================================
    // 3. Bounty payout (follows epoch_program trigger_epoch_transition pattern)
    // =========================================================================
    let bounty_lamports = ctx.accounts.rebalancer_config.bounty_lamports;
    let rent = Rent::get()?;
    let rent_exempt_min = rent.minimum_balance(0);
    let vault_balance = ctx.accounts.bounty_vault.lamports();
    let bounty_threshold = bounty_lamports
        .checked_add(rent_exempt_min)
        .ok_or(RebalancerError::MathOverflow)?;

    let bounty_paid = if vault_balance >= bounty_threshold {
        let vault_bump = ctx.bumps.bounty_vault;
        let vault_seeds: &[&[u8]] = &[BOUNTY_VAULT_SEED, &[vault_bump]];

        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.bounty_vault.to_account_info().key,
                ctx.accounts.caller.to_account_info().key,
                bounty_lamports,
            ),
            &[
                ctx.accounts.bounty_vault.to_account_info(),
                ctx.accounts.caller.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[vault_seeds],
        )?;

        msg!(
            "Bounty paid: {} lamports to {}",
            bounty_lamports,
            ctx.accounts.caller.key()
        );
        bounty_lamports
    } else {
        msg!(
            "Bounty vault insufficient: {} < {} (skipped)",
            vault_balance,
            bounty_threshold
        );
        0
    };

    // =========================================================================
    // 4. Emit event
    // =========================================================================
    // Cost BPS: measure how much was lost in conversion.
    // For devnet (sol_received=0), cost_bps is 0 (no real conversion).
    let cost_bps: u16 = if sol_received > 0 && usdc_amount > 0 {
        // Note: USDC is 6 decimals, WSOL is 9 decimals. These are different
        // denominations so a direct BPS comparison isn't meaningful without
        // a price oracle. We emit the raw amounts and let the crank/indexer
        // compute the actual cost using off-chain price data.
        // For the event, emit 0 as a placeholder — crank validates cost
        // off-chain before submitting the transaction.
        0
    } else {
        0
    };

    let clock = Clock::get()?;

    emit!(UsdcConverted {
        usdc_amount,
        sol_received,
        cost_bps,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "USDC conversion complete: usdc={}, sol_received={}, bounty_paid={}",
        usdc_amount,
        sol_received,
        bounty_paid
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Jupiter CPI (mainnet only)
// ---------------------------------------------------------------------------

/// Execute Jupiter SharedAccountsRoute CPI.
///
/// Account layout for SharedAccountsRoute (13 fixed + route accounts):
///   [0]  token_program (read-only, signer=false)
///   [1]  program_authority (read-only) — Jupiter internal PDA
///   [2]  user_transfer_authority (signer) — our rebalance_authority PDA
///   [3]  source_token_account (mut) — usdc_accumulator
///   [4]  program_source_token_account (mut) — Jupiter's source ATA
///   [5]  program_destination_token_account (mut) — Jupiter's dest ATA
///   [6]  destination_token_account (mut) — our holding_wsol
///   [7]  source_mint (read-only) — usdc_mint
///   [8]  destination_mint (read-only) — wsol_mint
///   [9]  platform_fee_account (read-only) — passed but unused
///   [10] token_2022_program (read-only, optional)
///   [11] event_authority (read-only)
///   [12] program (read-only) — Jupiter program itself
///   [13..] route-specific DEX accounts from remaining_accounts
#[cfg(not(feature = "devnet"))]
fn execute_jupiter_cpi<'info>(
    ctx: &Context<'_, '_, 'info, 'info, ConvertUsdc<'info>>,
    rebalance_seeds: &[&[u8]],
    route_data: &[u8],
) -> Result<u64> {
    use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};

    let remaining = ctx.remaining_accounts;
    // Minimum remaining_accounts: jupiter_program, program_authority,
    // program_source_token_account, program_destination_token_account,
    // platform_fee_account, token_2022_program (optional), event_authority,
    // plus at least one route account.
    require!(
        remaining.len() >= 6,
        RebalancerError::InsufficientRemainingAccounts
    );

    // Jupiter program is the FIRST remaining account
    let jupiter_program = &remaining[0];

    // Validate Jupiter program ID matches config
    require!(
        jupiter_program.key() == ctx.accounts.rebalancer_config.jupiter_program_id,
        RebalancerError::JupiterProgramMismatch
    );

    // Record pre-swap WSOL balance
    let wsol_before = {
        let holding_info = ctx.accounts.holding_wsol.to_account_info();
        let data = holding_info.try_borrow_data()?;
        // SPL Token account: amount is at offset 64, 8 bytes LE
        if data.len() >= 72 {
            u64::from_le_bytes(data[64..72].try_into().unwrap())
        } else {
            0
        }
    };

    // Build account metas for SharedAccountsRoute
    // remaining_accounts layout from crank:
    //   [0] jupiter_program
    //   [1] program_authority (Jupiter PDA)
    //   [2] program_source_token_account (Jupiter's USDC ATA)
    //   [3] program_destination_token_account (Jupiter's WSOL ATA)
    //   [4] platform_fee_account
    //   [5] token_2022_program (optional)
    //   [6] event_authority
    //   [7..] route-specific DEX accounts

    let mut account_metas = Vec::with_capacity(13 + remaining.len().saturating_sub(7));

    // [0] token_program
    account_metas.push(AccountMeta::new_readonly(
        ctx.accounts.token_program.key(),
        false,
    ));
    // [1] program_authority (Jupiter PDA) — from remaining[1]
    account_metas.push(AccountMeta::new_readonly(remaining[1].key(), false));
    // [2] user_transfer_authority (our rebalance_authority, signer)
    account_metas.push(AccountMeta::new_readonly(
        ctx.accounts.rebalance_authority.key(),
        true,
    ));
    // [3] source_token_account (usdc_accumulator)
    account_metas.push(AccountMeta::new(
        ctx.accounts.usdc_accumulator.key(),
        false,
    ));
    // [4] program_source_token_account (Jupiter's USDC ATA) — from remaining[2]
    account_metas.push(AccountMeta::new(remaining[2].key(), false));
    // [5] program_destination_token_account (Jupiter's WSOL ATA) — from remaining[3]
    account_metas.push(AccountMeta::new(remaining[3].key(), false));
    // [6] destination_token_account (our holding_wsol)
    account_metas.push(AccountMeta::new(
        ctx.accounts.holding_wsol.key(),
        false,
    ));
    // [7] source_mint (usdc_mint)
    account_metas.push(AccountMeta::new_readonly(
        ctx.accounts.usdc_mint.key(),
        false,
    ));
    // [8] destination_mint (wsol_mint)
    account_metas.push(AccountMeta::new_readonly(
        ctx.accounts.wsol_mint.key(),
        false,
    ));
    // [9] platform_fee_account — from remaining[4]
    account_metas.push(AccountMeta::new_readonly(remaining[4].key(), false));
    // [10] token_2022_program — from remaining[5]
    account_metas.push(AccountMeta::new_readonly(remaining[5].key(), false));
    // [11] event_authority — from remaining[6]
    account_metas.push(AccountMeta::new_readonly(remaining[6].key(), false));
    // [12] jupiter_program
    account_metas.push(AccountMeta::new_readonly(jupiter_program.key(), false));

    // [13..] route-specific DEX accounts from remaining[7..]
    for account in remaining.iter().skip(7) {
        if account.is_writable {
            account_metas.push(AccountMeta::new(account.key(), account.is_signer));
        } else {
            account_metas.push(AccountMeta::new_readonly(account.key(), account.is_signer));
        }
    }

    // Build instruction data: discriminator + route_data
    let mut ix_data = Vec::with_capacity(8 + route_data.len());
    ix_data.extend_from_slice(&SHARED_ACCOUNTS_ROUTE_DISCRIMINATOR);
    ix_data.extend_from_slice(route_data);

    let jupiter_ix = Instruction {
        program_id: jupiter_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    // Collect all AccountInfos for invoke_signed
    let mut account_infos = Vec::with_capacity(13 + remaining.len().saturating_sub(7));
    account_infos.push(ctx.accounts.token_program.to_account_info());
    account_infos.push(remaining[1].to_account_info()); // program_authority
    account_infos.push(ctx.accounts.rebalance_authority.to_account_info());
    account_infos.push(ctx.accounts.usdc_accumulator.to_account_info());
    account_infos.push(remaining[2].to_account_info()); // program_source
    account_infos.push(remaining[3].to_account_info()); // program_dest
    account_infos.push(ctx.accounts.holding_wsol.to_account_info());
    account_infos.push(ctx.accounts.usdc_mint.to_account_info());
    account_infos.push(ctx.accounts.wsol_mint.to_account_info());
    account_infos.push(remaining[4].to_account_info()); // platform_fee
    account_infos.push(remaining[5].to_account_info()); // token_2022
    account_infos.push(remaining[6].to_account_info()); // event_authority
    account_infos.push(jupiter_program.to_account_info());

    for account in remaining.iter().skip(7) {
        account_infos.push(account.to_account_info());
    }

    invoke_signed(&jupiter_ix, &account_infos, &[rebalance_seeds])?;

    // Reload WSOL balance after CPI
    // Must re-borrow data since CPI may have modified it
    let wsol_after = {
        let holding_info = ctx.accounts.holding_wsol.to_account_info();
        let data = holding_info.try_borrow_data()?;
        if data.len() >= 72 {
            u64::from_le_bytes(data[64..72].try_into().unwrap())
        } else {
            0
        }
    };

    let sol_received = wsol_after
        .checked_sub(wsol_before)
        .ok_or(RebalancerError::MathOverflow)?;

    msg!(
        "Jupiter CPI complete: wsol_before={}, wsol_after={}, received={}",
        wsol_before,
        wsol_after,
        sol_received
    );

    Ok(sol_received)
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct ConvertUsdc<'info> {
    /// Caller who triggers the conversion. Receives bounty on success.
    /// Permissionless — anyone can call with valid route data.
    #[account(mut)]
    pub caller: Signer<'info>,

    /// RebalancerConfig singleton — read-only for bounty_lamports, jupiter_program_id.
    #[account(
        seeds = [crate::constants::REBALANCER_CONFIG_SEED],
        bump = rebalancer_config.bump,
        constraint = rebalancer_config.initialized @ RebalancerError::AlreadyInitialized,
    )]
    pub rebalancer_config: Account<'info, RebalancerConfig>,

    /// RebalanceAuthority PDA — signs Jupiter CPI and owns token accounts.
    ///
    /// CHECK: PDA derived from known seeds; validated by seeds constraint.
    #[account(
        seeds = [REBALANCE_SEED],
        bump,
    )]
    pub rebalance_authority: SystemAccount<'info>,

    /// USDC accumulator — source of USDC for conversion.
    /// Authority = rebalance_authority (set during initialize_rebalancer).
    #[account(
        mut,
        seeds = [USDC_ACCUMULATOR_SEED],
        bump,
        token::authority = rebalance_authority,
    )]
    pub usdc_accumulator: Box<InterfaceAccount<'info, TokenAccount>>,

    /// WSOL holding — destination for converted WSOL.
    /// Authority = rebalance_authority.
    #[account(
        mut,
        seeds = [HOLDING_SEED, wsol_mint.key().as_ref()],
        bump,
        token::authority = rebalance_authority,
    )]
    pub holding_wsol: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Bounty vault — native SOL PDA that pays crank bounties.
    ///
    /// CHECK: PDA derived from known seeds; lamports managed via system_program transfer.
    #[account(
        mut,
        seeds = [BOUNTY_VAULT_SEED],
        bump,
    )]
    pub bounty_vault: SystemAccount<'info>,

    /// USDC mint — validated against feature-gated address.
    #[account(
        address = crate::constants::usdc_mint()
    )]
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    /// WSOL (Native Mint) — validated against spl_token native mint.
    #[account(
        address = anchor_spl::token::spl_token::native_mint::id()
    )]
    pub wsol_mint: Box<InterfaceAccount<'info, Mint>>,

    /// SPL Token program (for WSOL + USDC accounts).
    pub token_program: Program<'info, Token>,

    /// System program for bounty transfer.
    pub system_program: Program<'info, System>,
}
