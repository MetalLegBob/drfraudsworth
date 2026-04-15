use anchor_lang::prelude::*;

/// Seed for the swap_authority PDA derived by Tax Program.
/// Both Tax Program and AMM must use identical seeds.
pub const SWAP_AUTHORITY_SEED: &[u8] = b"swap_authority";

/// Tax Program ID - the only program authorized to sign swap_authority.
/// This is hardcoded like SPL Token program IDs.
/// Production Tax Program ID (deployed in Phase 18-01).
pub const TAX_PROGRAM_ID: Pubkey = pubkey!("43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj");

/// LP fee for SOL pools (CRIME/SOL, FRAUD/SOL) in basis points.
/// 100 bps = 1.0% fee per swap.
/// Source: AMM_Implementation.md Section 6
pub const SOL_POOL_FEE_BPS: u16 = 100;

/// Maximum LP fee in basis points.
/// 500 bps = 5% -- a reasonable upper bound to prevent admin misconfiguration.
/// Source: Phase 37 audit finding -- no upper bound on lp_fee_bps.
pub const MAX_LP_FEE_BPS: u16 = 500;

/// Basis points denominator (10,000 = 100%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// PDA seed for the global AdminConfig account.
pub const ADMIN_SEED: &[u8] = b"admin";

/// PDA seed prefix for pool state accounts.
/// Full seeds: [POOL_SEED, mint_a.as_ref(), mint_b.as_ref()]
pub const POOL_SEED: &[u8] = b"pool";

/// PDA seed prefix for pool vault token accounts.
/// Full seeds: [VAULT_SEED, pool.as_ref(), VAULT_A_SEED or VAULT_B_SEED]
pub const VAULT_SEED: &[u8] = b"vault";

/// PDA seed suffix for vault A.
pub const VAULT_A_SEED: &[u8] = b"a";

/// PDA seed suffix for vault B.
pub const VAULT_B_SEED: &[u8] = b"b";

// ---------------------------------------------------------------------------
// Phase 126: Rebalancer integration constants
// ---------------------------------------------------------------------------

/// Seed for the rebalance_authority PDA derived by Rebalancer Program.
/// Both Rebalancer Program and AMM must use identical seeds.
pub const REBALANCE_SEED: &[u8] = b"rebalance";

/// Maximum withdrawal in basis points (5000 = 50%).
/// Prevents draining a pool in a single call. Rebalancer can call twice
/// for extreme rebalances. Matches worst-case spec scenario (50/50 -> 100/0 shift).
///
/// This constant is used in instruction-level validation (errors.rs / withdraw_liquidity.rs).
/// The pure math function calculate_withdraw_amounts in helpers/math.rs hardcodes
/// the same 5000 limit to stay dependency-free (no crate::constants import).
pub const MAX_WITHDRAW_BPS: u16 = 5000;

/// Rebalancer Program ID -- the only program authorized to sign rebalance_authority.
/// Generated from keypairs/rebalancer-program.json (Phase 126).
pub const REBALANCER_PROGRAM_ID: Pubkey = pubkey!("HSfSLtfXvXCeEEamnhPHo8kv8zYvydBCQzU2EazXqdZf");
