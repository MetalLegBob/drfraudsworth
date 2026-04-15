//! Rebalancer Program constants.
//!
//! Seeds, BPS values, cross-program IDs, and feature-gated addresses.
//! Cross-program seeds MUST match their counterparts in AMM and Tax Program.

use anchor_lang::prelude::Pubkey;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// PDA Seeds
// ---------------------------------------------------------------------------

/// Singleton config PDA seed.
pub const REBALANCER_CONFIG_SEED: &[u8] = b"rebalancer_config";

/// RebalanceAuthority PDA seed. Must match AMM's REBALANCE_SEED.
pub const REBALANCE_SEED: &[u8] = b"rebalance";

/// Per-token holding account PDA seed prefix.
/// Full seeds: [HOLDING_SEED, mint.as_ref()]
pub const HOLDING_SEED: &[u8] = b"holding";

/// Bounty vault PDA seed (native SOL).
pub const BOUNTY_VAULT_SEED: &[u8] = b"bounty_vault";

/// USDC accumulator PDA seed. Must match Tax Program's USDC_ACCUMULATOR_SEED.
pub const USDC_ACCUMULATOR_SEED: &[u8] = b"usdc_accumulator";

// ---------------------------------------------------------------------------
// Distribution BPS (71/24/5 split)
// ---------------------------------------------------------------------------

/// Staking escrow receives 71% of converted SOL (7100 bps).
pub const STAKING_BPS: u128 = 7_100;

/// Carnage Fund receives 24% of converted SOL (2400 bps).
pub const CARNAGE_BPS: u128 = 2_400;

/// Basis points denominator (10,000 = 100%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Below this threshold, all tax goes to staking (avoids dust distribution).
pub const MICRO_TAX_THRESHOLD: u64 = 4;

// ---------------------------------------------------------------------------
// Config Defaults
// ---------------------------------------------------------------------------

/// Default target allocation: 50/50 SOL/USDC (5000 bps = 50%).
pub const DEFAULT_TARGET_BPS: u16 = 5_000;

/// Default minimum delta to trigger rebalance: 3% (300 bps).
pub const DEFAULT_MIN_DELTA: u16 = 300;

/// Default maximum conversion cost: 0.5% (50 bps).
pub const DEFAULT_COST_CEILING_BPS: u16 = 50;

// ---------------------------------------------------------------------------
// Bounty
// ---------------------------------------------------------------------------

/// Bounty paid to crank caller for successful convert_usdc (0.001 SOL).
pub const BOUNTY_LAMPORTS: u64 = 1_000_000;

/// Amount skimmed from converted SOL to refill bounty vault (0.002 SOL = 2x bounty).
pub const BOUNTY_SKIM_LAMPORTS: u64 = 2_000_000;

/// Minimum USDC accumulator balance to trigger conversion (1 USDC at 6 decimals).
pub const MIN_CONVERT_AMOUNT: u64 = 1_000_000;

/// Maximum withdrawal BPS per pool (50%). Matches AMM's MAX_WITHDRAW_BPS.
/// Prevents draining a pool in a single call. Inline here to stay import-free.
pub const MAX_WITHDRAW_BPS: u16 = 5_000;

// ---------------------------------------------------------------------------
// CPI Discriminators (precomputed, verified in tests)
// ---------------------------------------------------------------------------

/// sha256("global:withdraw_liquidity")[0..8]
pub const WITHDRAW_LIQUIDITY_DISCRIMINATOR: [u8; 8] = [149, 158, 33, 185, 47, 243, 253, 31];

/// sha256("global:add_liquidity")[0..8]
pub const ADD_LIQUIDITY_DISCRIMINATOR: [u8; 8] = [181, 157, 89, 67, 143, 182, 52, 72];

/// sha256("global:shared_accounts_route")[0..8]
pub const SHARED_ACCOUNTS_ROUTE_DISCRIMINATOR: [u8; 8] = [193, 32, 155, 51, 65, 214, 156, 129];

// ---------------------------------------------------------------------------
// Cross-program IDs
// ---------------------------------------------------------------------------

/// AMM Program ID for CPI calls.
/// Matches declare_id! in amm/src/lib.rs.
pub fn amm_program_id() -> Pubkey {
    Pubkey::from_str("5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR").unwrap()
}

// ---------------------------------------------------------------------------
// Feature-gated addresses
// ---------------------------------------------------------------------------

/// Jupiter Aggregator v6 program ID (mainnet only -- no devnet deployment).
#[cfg(feature = "devnet")]
pub fn jupiter_program_id() -> Pubkey {
    // Jupiter doesn't exist on devnet. Use a placeholder so config stores
    // a non-default value. CPI is feature-gated to skip on devnet.
    Pubkey::from_str("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4").unwrap()
}

#[cfg(not(feature = "devnet"))]
pub fn jupiter_program_id() -> Pubkey {
    Pubkey::from_str("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4").unwrap()
}

/// USDC mint address for the active cluster.
#[cfg(feature = "devnet")]
pub fn usdc_mint() -> Pubkey {
    Pubkey::from_str("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU").unwrap()
}

#[cfg(not(feature = "devnet"))]
pub fn usdc_mint() -> Pubkey {
    Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap()
}

/// Treasury wallet address.
#[cfg(feature = "devnet")]
pub fn treasury_address() -> Pubkey {
    Pubkey::from_str("8kPzhQoUPx7LYM18f9TzskW4ZgvGyq4jMPYZikqmHMH4").unwrap()
}

#[cfg(not(feature = "devnet"))]
pub fn treasury_address() -> Pubkey {
    Pubkey::from_str("3ihhwLnEJ2duwPSLYxhLbFrdhhxXLcvcrV9rAHqMgzCv").unwrap()
}

/// Staking escrow vault PDA address.
/// Derived from Staking Program: seeds = ["escrow_vault"], program = staking_program_id.
#[cfg(feature = "devnet")]
pub fn staking_escrow_address() -> Pubkey {
    // Devnet staking escrow -- derived from devnet staking program.
    let staking_id = Pubkey::from_str("12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH").unwrap();
    let (pda, _) = Pubkey::find_program_address(&[b"escrow_vault"], &staking_id);
    pda
}

#[cfg(not(feature = "devnet"))]
pub fn staking_escrow_address() -> Pubkey {
    // Mainnet staking escrow -- same program ID, same derivation.
    let staking_id = Pubkey::from_str("12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH").unwrap();
    let (pda, _) = Pubkey::find_program_address(&[b"escrow_vault"], &staking_id);
    pda
}

/// Carnage SOL vault PDA address.
/// Derived from Epoch Program: seeds = ["carnage_sol_vault"], program = epoch_program_id.
#[cfg(feature = "devnet")]
pub fn carnage_sol_vault_address() -> Pubkey {
    let epoch_id = Pubkey::from_str("4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2").unwrap();
    let (pda, _) = Pubkey::find_program_address(&[b"carnage_sol_vault"], &epoch_id);
    pda
}

#[cfg(not(feature = "devnet"))]
pub fn carnage_sol_vault_address() -> Pubkey {
    let epoch_id = Pubkey::from_str("4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2").unwrap();
    let (pda, _) = Pubkey::find_program_address(&[b"carnage_sol_vault"], &epoch_id);
    pda
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebalance_seed_matches_amm() {
        // Must match AMM's REBALANCE_SEED = b"rebalance"
        assert_eq!(REBALANCE_SEED, b"rebalance");
    }

    #[test]
    fn test_usdc_accumulator_seed_matches_tax() {
        // Must match Tax Program's USDC_ACCUMULATOR_SEED
        assert_eq!(USDC_ACCUMULATOR_SEED, b"usdc_accumulator");
        assert_eq!(USDC_ACCUMULATOR_SEED.len(), 16);
    }

    #[test]
    fn test_distribution_bps_sum() {
        // 71% + 24% + 5% = 100%
        assert_eq!(STAKING_BPS + CARNAGE_BPS + 500, BPS_DENOMINATOR);
    }

    #[test]
    fn test_amm_program_id() {
        assert_eq!(
            amm_program_id().to_string(),
            "5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR"
        );
    }

    #[test]
    fn test_jupiter_program_id_non_default() {
        assert_ne!(jupiter_program_id(), Pubkey::default());
    }

    #[test]
    fn test_usdc_mint_non_default() {
        assert_ne!(usdc_mint(), Pubkey::default());
    }

    #[test]
    fn test_treasury_address_non_default() {
        assert_ne!(treasury_address(), Pubkey::default());
    }

    #[test]
    fn test_discriminators() {
        use sha2::{Digest, Sha256};

        let check = |name: &[u8], expected: [u8; 8]| {
            let mut h = Sha256::new();
            h.update(name);
            let result = h.finalize();
            let computed: [u8; 8] = result[0..8].try_into().unwrap();
            assert_eq!(
                computed, expected,
                "Discriminator mismatch for {:?}: expected {:?}, got {:?}",
                std::str::from_utf8(name).unwrap(),
                expected,
                computed
            );
        };

        check(b"global:withdraw_liquidity", WITHDRAW_LIQUIDITY_DISCRIMINATOR);
        check(b"global:add_liquidity", ADD_LIQUIDITY_DISCRIMINATOR);
        check(b"global:shared_accounts_route", SHARED_ACCOUNTS_ROUTE_DISCRIMINATOR);
    }
}
