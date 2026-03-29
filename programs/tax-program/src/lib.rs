//! Dr Fraudsworth Tax Program
//!
//! Asymmetric taxation and atomic distribution for SOL pool swaps.
//! Routes swaps through the AMM with tax calculation and 3-way distribution:
//! - 71% to staking escrow
//! - 24% to carnage fund
//! - 5% to treasury (remainder)
//!
//! Source: Tax_Pool_Logic_Spec.md

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod helpers;
pub mod instructions;
pub mod state;

use instructions::*;

#[cfg(not(feature = "no-entrypoint"))]
use solana_security_txt::security_txt;

#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "Dr Fraudsworth's Finance Factory",
    project_url: "https://fraudsworth.fun",
    contacts: "email:drfraudsworth@gmail.com,twitter:@fraudsworth",
    policy: "https://fraudsworth.fun/docs/security/security-policy",
    preferred_languages: "en",
    auditors: "Internal audits: SOS, BOK, VulnHunter (v1.3)",
    expiry: "2027-03-20"
}

declare_id!("43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj");

#[program]
pub mod tax_program {
    use super::*;

    /// Execute a SOL -> CRIME/FRAUD swap with buy tax.
    ///
    /// Tax is deducted from SOL input before swap execution.
    /// Distribution: 71% staking, 24% carnage, 5% treasury.
    ///
    /// # Arguments
    /// * `amount_in` - Total SOL amount to spend (including tax)
    /// * `minimum_output` - Minimum tokens expected (slippage protection)
    /// * `is_crime` - true = CRIME pool, false = FRAUD pool
    pub fn swap_sol_buy<'info>(
        ctx: Context<'_, '_, 'info, 'info, SwapSolBuy<'info>>,
        amount_in: u64,
        minimum_output: u64,
        is_crime: bool,
    ) -> Result<()> {
        instructions::swap_sol_buy::handler(ctx, amount_in, minimum_output, is_crime)
    }

    /// Execute a CRIME/FRAUD -> SOL swap with sell tax.
    ///
    /// Tax is deducted from SOL output after swap execution.
    /// Distribution: 71% staking, 24% carnage, 5% treasury.
    ///
    /// # Arguments
    /// * `amount_in` - Token amount to sell
    /// * `minimum_output` - Minimum SOL to receive AFTER tax (slippage protection)
    /// * `is_crime` - true = CRIME pool, false = FRAUD pool
    pub fn swap_sol_sell<'info>(
        ctx: Context<'_, '_, 'info, 'info, SwapSolSell<'info>>,
        amount_in: u64,
        minimum_output: u64,
        is_crime: bool,
    ) -> Result<()> {
        instructions::swap_sol_sell::handler(ctx, amount_in, minimum_output, is_crime)
    }

    /// Initialize the WSOL intermediary account (one-time admin setup).
    /// Must be called before the first sell swap.
    /// Creates a WSOL token account at the intermediary PDA, owned by swap_authority.
    pub fn initialize_wsol_intermediary(
        ctx: Context<InitializeWsolIntermediary>,
    ) -> Result<()> {
        instructions::initialize_wsol_intermediary::handler(ctx)
    }

    /// Execute tax-exempt swap for Carnage Fund (bidirectional).
    ///
    /// Called by Epoch Program during Carnage rebalancing.
    /// No tax applied - only AMM LP fee (1%) applies.
    ///
    /// # Arguments
    /// * `amount_in` - Amount to swap (SOL for buy, token for sell)
    /// * `direction` - 0 = buy (SOL->Token), 1 = sell (Token->SOL)
    /// * `is_crime` - true = CRIME pool, false = FRAUD pool
    pub fn swap_exempt<'info>(
        ctx: Context<'_, '_, 'info, 'info, SwapExempt<'info>>,
        amount_in: u64,
        direction: u8,
        is_crime: bool,
    ) -> Result<()> {
        instructions::swap_exempt::handler(ctx, amount_in, direction, is_crime)
    }
}
