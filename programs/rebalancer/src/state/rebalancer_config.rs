//! RebalancerConfig account state.
//!
//! Singleton PDA storing all Rebalancer program configuration.
//! Initialized once via initialize_rebalancer, updated via update_config.

use anchor_lang::prelude::*;

/// Global configuration for the Rebalancer program.
///
/// Seeds: [REBALANCER_CONFIG_SEED]
/// Authority: admin (Squads multisig for governance)
#[account]
#[repr(C)]
pub struct RebalancerConfig {
    /// Admin pubkey (Squads multisig). Only signer authorized for update_config.
    pub admin: Pubkey,

    /// Target allocation for SOL-denominated pools in basis points.
    /// 5000 = 50% SOL / 50% USDC (default). Adjustable for testing.
    pub target_bps: u16,

    /// Minimum allocation delta (in BPS) to trigger a rebalance.
    /// 300 = 3% (default). Prevents rebalancing on negligible drift.
    pub min_delta: u16,

    /// Maximum acceptable conversion cost in basis points.
    /// 50 = 0.5% (default). convert_usdc fails if Jupiter cost exceeds this.
    pub cost_ceiling_bps: u16,

    /// Jupiter Aggregator v6 program ID for USDC->SOL conversion CPI.
    pub jupiter_program_id: Pubkey,

    /// Bounty paid to crank caller for successful convert_usdc (lamports).
    /// 1_000_000 = 0.001 SOL (default).
    pub bounty_lamports: u64,

    /// Bounty paid to crank caller for successful execute_rebalance (lamports).
    /// 0 for v1.7 (dormant, wired for v1.8 when PM fees fund it).
    pub rebalance_bounty_lamports: u64,

    /// Whether config has been initialized.
    pub initialized: bool,

    /// PDA bump seed for the config account.
    pub bump: u8,

    /// Reserved bytes for future schema evolution without reallocation.
    pub reserved: [u8; 64],
}

impl RebalancerConfig {
    /// Space required for the account data (excluding 8-byte Anchor discriminator).
    /// Pubkey(32) + u16(2) + u16(2) + u16(2) + Pubkey(32) + u64(8) + u64(8) + bool(1) + u8(1) + [u8;64](64)
    pub const INIT_SPACE: usize = 32 + 2 + 2 + 2 + 32 + 8 + 8 + 1 + 1 + 64;
}
