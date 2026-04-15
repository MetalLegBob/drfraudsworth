//! initialize_rebalancer: Create RebalancerConfig, holdings, accumulator, and bounty vault.
//!
//! One-time admin instruction that creates all PDAs needed by the Rebalancer program:
//! - RebalancerConfig singleton with default parameters
//! - 4 token holdings (CRIME, FRAUD via Token-2022; WSOL, USDC via SPL Token)
//! - USDC accumulator (Tax Program transfers USDC tax here)
//! - Bounty vault (native SOL PDA for crank incentives)
//!
//! All token accounts are owned by the RebalanceAuthority PDA (seeds=["rebalance"]).
//!
//! Source: Phase 128, Plan 01

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token::Token;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::constants::{
    jupiter_program_id, BOUNTY_LAMPORTS, BOUNTY_VAULT_SEED, DEFAULT_COST_CEILING_BPS,
    DEFAULT_MIN_DELTA, DEFAULT_TARGET_BPS, HOLDING_SEED, REBALANCER_CONFIG_SEED, REBALANCE_SEED,
    USDC_ACCUMULATOR_SEED,
};
use crate::events::ConfigUpdated;
use crate::state::RebalancerConfig;

/// Handler for initialize_rebalancer.
///
/// Sets config defaults and logs initialization event.
/// All account creation (init) is handled by Anchor constraints.
pub fn handler(ctx: Context<InitializeRebalancer>) -> Result<()> {
    // =========================================================================
    // 1. Populate RebalancerConfig with defaults
    // =========================================================================
    let config = &mut ctx.accounts.rebalancer_config;
    config.admin = ctx.accounts.admin.key();
    config.target_bps = DEFAULT_TARGET_BPS;
    config.min_delta = DEFAULT_MIN_DELTA;
    config.cost_ceiling_bps = DEFAULT_COST_CEILING_BPS;
    config.jupiter_program_id = jupiter_program_id();
    config.bounty_lamports = BOUNTY_LAMPORTS;
    config.rebalance_bounty_lamports = 0; // Dormant in v1.7
    config.initialized = true;
    config.bump = ctx.bumps.rebalancer_config;
    config.reserved = [0u8; 64];

    // =========================================================================
    // 2. Seed the bounty vault with initial SOL from admin
    //
    // The bounty vault is a bare PDA (no data). Admin transfers a small
    // amount so the first convert_usdc can pay a crank bounty before
    // distribute_converted_sol has run to refill it.
    // =========================================================================
    let rent = Rent::get()?;
    let rent_exempt_min = rent.minimum_balance(0);

    // Transfer enough for rent-exempt minimum + one bounty payout
    let initial_funding = rent_exempt_min
        .checked_add(BOUNTY_LAMPORTS)
        .ok_or(error!(crate::errors::RebalancerError::MathOverflow))?;

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.admin.to_account_info(),
                to: ctx.accounts.bounty_vault.to_account_info(),
            },
        ),
        initial_funding,
    )?;

    // =========================================================================
    // 3. Emit initialization event
    // =========================================================================
    let clock = Clock::get()?;

    emit!(ConfigUpdated {
        field: "initialized".to_string(),
        old_value: 0,
        new_value: 1,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Rebalancer initialized: admin={}, target_bps={}, min_delta={}, cost_ceiling_bps={}, bounty_vault_funded={}",
        config.admin,
        config.target_bps,
        config.min_delta,
        config.cost_ceiling_bps,
        initial_funding,
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeRebalancer<'info> {
    /// Admin who initializes the Rebalancer. Becomes config.admin.
    /// Pays for all account creation rent and initial bounty vault funding.
    #[account(mut)]
    pub admin: Signer<'info>,

    /// RebalancerConfig singleton PDA.
    #[account(
        init,
        payer = admin,
        space = 8 + RebalancerConfig::INIT_SPACE,
        seeds = [REBALANCER_CONFIG_SEED],
        bump,
    )]
    pub rebalancer_config: Account<'info, RebalancerConfig>,

    /// RebalanceAuthority PDA -- owns all token holdings and signs CPI calls.
    /// Not initialized with data (SystemAccount), just needs to exist for
    /// token account authority derivation.
    ///
    /// CHECK: PDA derived from known seeds; validated by seeds constraint.
    #[account(
        seeds = [REBALANCE_SEED],
        bump,
    )]
    pub rebalance_authority: SystemAccount<'info>,

    // === USDC Accumulator ===
    /// USDC accumulator token account. Tax Program transfers USDC tax here.
    /// Authority = rebalance_authority so Rebalancer can transfer out.
    /// Uses SPL Token (USDC is not Token-2022).
    #[account(
        init,
        payer = admin,
        token::mint = usdc_mint,
        token::authority = rebalance_authority,
        token::token_program = token_program,
        seeds = [USDC_ACCUMULATOR_SEED],
        bump,
    )]
    pub usdc_accumulator: Box<InterfaceAccount<'info, TokenAccount>>,

    // === Per-token Holdings ===
    /// CRIME holding -- Token-2022
    #[account(
        init,
        payer = admin,
        token::mint = crime_mint,
        token::authority = rebalance_authority,
        token::token_program = token_2022_program,
        seeds = [HOLDING_SEED, crime_mint.key().as_ref()],
        bump,
    )]
    pub holding_crime: Box<InterfaceAccount<'info, TokenAccount>>,

    /// FRAUD holding -- Token-2022
    #[account(
        init,
        payer = admin,
        token::mint = fraud_mint,
        token::authority = rebalance_authority,
        token::token_program = token_2022_program,
        seeds = [HOLDING_SEED, fraud_mint.key().as_ref()],
        bump,
    )]
    pub holding_fraud: Box<InterfaceAccount<'info, TokenAccount>>,

    /// WSOL holding -- SPL Token
    #[account(
        init,
        payer = admin,
        token::mint = wsol_mint,
        token::authority = rebalance_authority,
        token::token_program = token_program,
        seeds = [HOLDING_SEED, wsol_mint.key().as_ref()],
        bump,
    )]
    pub holding_wsol: Box<InterfaceAccount<'info, TokenAccount>>,

    /// USDC holding -- SPL Token (for rebalance flow, separate from accumulator)
    #[account(
        init,
        payer = admin,
        token::mint = usdc_mint,
        token::authority = rebalance_authority,
        token::token_program = token_program,
        seeds = [HOLDING_SEED, usdc_mint.key().as_ref()],
        bump,
    )]
    pub holding_usdc: Box<InterfaceAccount<'info, TokenAccount>>,

    // === Bounty Vault ===
    /// Native SOL PDA for crank bounty payouts.
    /// Created as a bare system account (no data).
    ///
    /// CHECK: PDA derived from known seeds; validated by seeds constraint.
    /// Lamports managed via system_program::transfer.
    #[account(
        mut,
        seeds = [BOUNTY_VAULT_SEED],
        bump,
    )]
    pub bounty_vault: SystemAccount<'info>,

    // === Mints (read-only) ===
    /// CRIME mint (Token-2022).
    /// Validated by token::mint constraint on holding_crime.
    pub crime_mint: Box<InterfaceAccount<'info, Mint>>,

    /// FRAUD mint (Token-2022).
    /// Validated by token::mint constraint on holding_fraud.
    pub fraud_mint: Box<InterfaceAccount<'info, Mint>>,

    /// WSOL (Native Mint) -- SPL Token.
    /// Validated: must be the native SOL mint.
    #[account(
        address = anchor_spl::token::spl_token::native_mint::id()
    )]
    pub wsol_mint: Box<InterfaceAccount<'info, Mint>>,

    /// USDC mint -- SPL Token.
    /// Validated: must match feature-gated USDC address.
    #[account(
        address = crate::constants::usdc_mint()
    )]
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    // === Programs ===
    /// SPL Token program (for WSOL + USDC accounts).
    pub token_program: Program<'info, Token>,

    /// Token-2022 program (for CRIME + FRAUD accounts).
    pub token_2022_program: Program<'info, Token2022>,

    /// System program for PDA creation and SOL transfers.
    pub system_program: Program<'info, System>,
}
