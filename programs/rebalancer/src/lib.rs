//! Dr Fraudsworth Rebalancer Program
//!
//! Converts accumulated USDC tax to SOL via Jupiter, distributes SOL via
//! 71/24/5 split, and rebalances liquidity between SOL and USDC pool pairs.
//!
//! Source: Phase 128 (v1.7 USDC Pools & Rebalancer)

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

declare_id!("HSfSLtfXvXCeEEamnhPHo8kv8zYvydBCQzU2EazXqdZf");

#[program]
pub mod rebalancer {
    use super::*;

    /// Initialize the Rebalancer program.
    ///
    /// Creates RebalancerConfig PDA with defaults, 4 token holdings
    /// (CRIME/FRAUD via T22, WSOL/USDC via SPL Token), USDC accumulator,
    /// and bounty vault with initial SOL funding.
    ///
    /// Can only be called once (init constraint prevents re-initialization).
    pub fn initialize_rebalancer(ctx: Context<InitializeRebalancer>) -> Result<()> {
        instructions::initialize_rebalancer::handler(ctx)
    }

    /// Convert accumulated USDC to WSOL via Jupiter CPI.
    ///
    /// Permissionless: anyone can trigger with valid Jupiter route data.
    /// Pays bounty to caller from bounty vault on success.
    /// Skips when accumulator balance < 1 USDC (REBAL-03).
    /// On devnet, Jupiter CPI is feature-gated out.
    ///
    /// # Arguments
    /// * `route_data` - Serialized Jupiter SharedAccountsRoute instruction data
    pub fn convert_usdc<'info>(
        ctx: Context<'_, '_, 'info, 'info, ConvertUsdc<'info>>,
        route_data: Vec<u8>,
    ) -> Result<()> {
        instructions::convert_usdc::handler(ctx, route_data)
    }

    /// Distribute converted WSOL as native SOL via 71/24/5 split.
    ///
    /// Step 2 of the USDC conversion pipeline:
    /// Unwraps WSOL holding, skims bounty refill, splits remaining SOL
    /// to staking escrow (71%), carnage fund (24%), treasury (5%).
    /// Recreates WSOL holding at same PDA for next convert_usdc cycle.
    ///
    /// Permissionless: anyone can call after convert_usdc deposited WSOL.
    pub fn distribute_converted_sol(ctx: Context<DistributeConvertedSol>) -> Result<()> {
        instructions::distribute_converted_sol::handler(ctx)
    }

    /// Execute pool liquidity rebalancing between SOL and USDC pool pairs.
    ///
    /// Reads reserves from all 4 pools, calculates allocation delta, and
    /// CPIs into AMM withdraw_liquidity + add_liquidity to rebalance.
    /// Skips when delta < min_delta (REBAL-05).
    /// Permissionless: pays rebalance_bounty_lamports (0 in v1.7).
    ///
    /// # Arguments
    /// * `withdraw_bps` - BPS to withdraw from overweight pools (max 5000)
    /// * `sol_usd_price_x1000` - SOL price in milli-USD (e.g., 150_123 = $150.123)
    pub fn execute_rebalance<'info>(
        ctx: Context<'_, '_, 'info, 'info, ExecuteRebalance<'info>>,
        withdraw_bps: u16,
        sol_usd_price_x1000: u64,
    ) -> Result<()> {
        instructions::execute_rebalance::handler(ctx, withdraw_bps, sol_usd_price_x1000)
    }

    /// Update RebalancerConfig parameters.
    ///
    /// Admin-gated: only config.admin can call. Each parameter is optional --
    /// only provided fields are updated. Validates inputs before applying.
    ///
    /// # Arguments
    /// * `target_bps` - Target SOL allocation in BPS (0-10000)
    /// * `min_delta` - Minimum delta to trigger rebalance (0-10000)
    /// * `cost_ceiling_bps` - Maximum conversion cost (0-10000)
    /// * `jupiter_program_id` - Jupiter program ID (must not be default)
    /// * `bounty_lamports` - Crank bounty for convert_usdc
    /// * `rebalance_bounty_lamports` - Crank bounty for execute_rebalance
    pub fn update_config(
        ctx: Context<UpdateConfig>,
        target_bps: Option<u16>,
        min_delta: Option<u16>,
        cost_ceiling_bps: Option<u16>,
        jupiter_program_id: Option<Pubkey>,
        bounty_lamports: Option<u64>,
        rebalance_bounty_lamports: Option<u64>,
    ) -> Result<()> {
        instructions::update_config::handler(
            ctx,
            target_bps,
            min_delta,
            cost_ceiling_bps,
            jupiter_program_id,
            bounty_lamports,
            rebalance_bounty_lamports,
        )
    }
}
