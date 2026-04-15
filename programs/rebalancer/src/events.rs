//! Rebalancer Program events.
//!
//! Emitted by instructions for off-chain indexing and monitoring.

use anchor_lang::prelude::*;

/// Emitted when USDC is converted to SOL via Jupiter CPI.
#[event]
pub struct UsdcConverted {
    /// USDC amount sent to Jupiter (6 decimals).
    pub usdc_amount: u64,
    /// SOL received from Jupiter (lamports).
    pub sol_received: u64,
    /// Conversion cost in basis points.
    pub cost_bps: u16,
    /// Unix timestamp of the conversion.
    pub timestamp: i64,
}

/// Emitted when converted SOL is distributed via 71/24/5 split.
#[event]
pub struct SolDistributed {
    /// Total SOL distributed (lamports).
    pub total_sol: u64,
    /// SOL sent to staking escrow (71%).
    pub staking: u64,
    /// SOL sent to carnage fund (24%).
    pub carnage: u64,
    /// SOL sent to treasury (5%).
    pub treasury: u64,
    /// SOL skimmed to refill bounty vault.
    pub bounty_skim: u64,
    /// Unix timestamp of the distribution.
    pub timestamp: i64,
}

/// Emitted when pool liquidity is rebalanced between SOL and USDC pairs.
#[event]
pub struct LiquidityRebalanced {
    /// SOL pool allocation delta in signed BPS before rebalance.
    pub sol_pool_delta_bps: i32,
    /// USDC pool allocation delta in signed BPS before rebalance.
    pub usdc_pool_delta_bps: i32,
    /// Pool pair liquidity was withdrawn from.
    pub withdrawn_from: String,
    /// Pool pair liquidity was injected into.
    pub injected_into: String,
    /// Unix timestamp of the rebalance.
    pub timestamp: i64,
}

/// Emitted when a config field is updated via update_config.
#[event]
pub struct ConfigUpdated {
    /// Name of the field that was updated.
    pub field: String,
    /// Previous value (cast to u64 for uniform encoding).
    pub old_value: u64,
    /// New value (cast to u64 for uniform encoding).
    pub new_value: u64,
    /// Unix timestamp of the update.
    pub timestamp: i64,
}
