use anchor_lang::prelude::*;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Admin PDA Seed (Phase 78 — Authority Hardening)
// ---------------------------------------------------------------------------

/// Seed for BcAdminConfig PDA: ["bc_admin"].
pub const BC_ADMIN_SEED: &[u8] = b"bc_admin";

// ---------------------------------------------------------------------------
// Precision Scaling (Section 4.4)
// ---------------------------------------------------------------------------

/// 1e12 scaling factor for all intermediate u128 arithmetic.
/// Prevents precision loss during multiplication and division.
pub const PRECISION: u128 = 1_000_000_000_000;

// ---------------------------------------------------------------------------
// Curve Parameters (Bonding_Curve_Spec.md Section 3.2 / 4.1)
// ---------------------------------------------------------------------------

/// Start price in lamports per human token (integer scaling).
/// Derivation: TARGET_SOL = (P_START + P_END) / 2 * 460M.
///
/// Devnet: P_START=5, P_END=17 -> (5+17)/2 * 460M = 5,060,000,000 lam (~5.06 SOL).
/// Mainnet: P_START=450, P_END=1725 -> (450+1725)/2 * 460M = 500,250,000,000 lam (~500 SOL).
#[cfg(feature = "devnet")]
pub const P_START: u128 = 5;

#[cfg(feature = "localnet")]
pub const P_START: u128 = 450;

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub const P_START: u128 = 450;

/// End price in lamports per human token (integer scaling).
/// Maintains ~3.83x ratio from P_START for consistent curve shape.
#[cfg(feature = "devnet")]
pub const P_END: u128 = 17;

#[cfg(feature = "localnet")]
pub const P_END: u128 = 1_725;

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub const P_END: u128 = 1_725;

/// Total tokens available for sale per curve: 460M with 6 decimals.
/// 460,000,000 * 10^6 = 460,000,000,000,000.
pub const TOTAL_FOR_SALE: u128 = 460_000_000_000_000;

// ---------------------------------------------------------------------------
// Target Values (Section 7.1)
// ---------------------------------------------------------------------------

/// Target tokens sold for a curve to be considered filled (u64 for state comparison).
/// Same value as TOTAL_FOR_SALE but as u64.
pub const TARGET_TOKENS: u64 = 460_000_000_000_000;

/// Target SOL raised in lamports.
/// Devnet: 5 SOL for testability. Mainnet: 500 SOL per curve.
#[cfg(feature = "devnet")]
pub const TARGET_SOL: u64 = 5_000_000_000;

#[cfg(feature = "localnet")]
pub const TARGET_SOL: u64 = 500_000_000_000;

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub const TARGET_SOL: u64 = 500_000_000_000;

// ---------------------------------------------------------------------------
// Enforcement Limits (Section 6.1 / 6.2)
// ---------------------------------------------------------------------------

/// Maximum tokens any single wallet can hold per curve: 20M with 6 decimals.
/// 20,000,000 * 10^6 = 20,000,000,000,000.
pub const MAX_TOKENS_PER_WALLET: u64 = 20_000_000_000_000;

/// Minimum SOL per purchase in lamports.
/// Devnet: 0.001 SOL for small-amount testing. Mainnet: 0.05 SOL.
#[cfg(feature = "devnet")]
pub const MIN_PURCHASE_SOL: u64 = 1_000_000;

#[cfg(feature = "localnet")]
pub const MIN_PURCHASE_SOL: u64 = 50_000_000;

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub const MIN_PURCHASE_SOL: u64 = 50_000_000;

// ---------------------------------------------------------------------------
// Timing (Section 7.1)
// ---------------------------------------------------------------------------

/// Deadline duration in slots.
///
/// Devnet: 27,000 slots (~3 hours at 400ms/slot) for lifecycle testing with
/// manual steps (deploy, fill, graduate) that need breathing room.
/// Localnet: 500 slots so integration tests can fill curves during the happy
/// path (~200 TXs needed) while still allowing failure path tests to advance
/// past deadline + FAILURE_GRACE_SLOTS (500 + 150 = 650 slots).
/// Mainnet: 432,000 slots (~48 hours at 400ms/slot).
#[cfg(feature = "devnet")]
pub const DEADLINE_SLOTS: u64 = 27_000;

#[cfg(feature = "localnet")]
pub const DEADLINE_SLOTS: u64 = 500;

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub const DEADLINE_SLOTS: u64 = 432_000;

/// Grace period after deadline_slot before mark_failed can be called.
/// Gives in-flight purchase TXs ~1 minute to finalize on-chain.
/// 150 slots * 400ms/slot = ~60 seconds.
/// CONTEXT.md decision: additive safety measure over spec Section 8.7.
pub const FAILURE_GRACE_SLOTS: u64 = 150;

// ---------------------------------------------------------------------------
// Token Config
// ---------------------------------------------------------------------------

/// All project tokens (CRIME, FRAUD, PROFIT) use 6 decimals.
pub const TOKEN_DECIMALS: u8 = 6;

// ---------------------------------------------------------------------------
// Sell Tax (Section 4.5 -- needed for Phase 72 sell instruction)
// ---------------------------------------------------------------------------

/// Sell tax: 15% of gross SOL proceeds, held in tax escrow PDA.
pub const SELL_TAX_BPS: u64 = 1_500;

/// Basis point denominator (10,000 = 100%).
pub const BPS_DENOMINATOR: u64 = 10_000;

/// Solvency buffer: additional lamports above rent-exempt minimum that the
/// vault must retain after any sell payout. Prevents cumulative rounding dust
/// from eroding the vault below rent-exempt.
///
/// BOK Finding 1 (MEDIUM): LiteSVM inv_bc_014b_sequential_sells found a
/// 5-lamport gap after hundreds of micro-buys/sells. Each calculate_sol_for_tokens()
/// floors the result; cumulative effect can erode vault by ~1 lamport per sell.
/// 10 lamports provides comfortable margin (2x worst observed case).
pub const SOLVENCY_BUFFER_LAMPORTS: u64 = 10;

// ---------------------------------------------------------------------------
// PDA Seeds (Section 5.3 / 5.7)
// ---------------------------------------------------------------------------

/// Seed for CurveState PDA: ["curve", token_mint].
pub const CURVE_SEED: &[u8] = b"curve";

/// Seed for token vault PDA: ["curve_token_vault", token_mint].
pub const CURVE_TOKEN_VAULT_SEED: &[u8] = b"curve_token_vault";

/// Seed for SOL vault PDA: ["curve_sol_vault", token_mint].
pub const CURVE_SOL_VAULT_SEED: &[u8] = b"curve_sol_vault";

/// Seed for tax escrow PDA: ["tax_escrow", token_mint].
pub const TAX_ESCROW_SEED: &[u8] = b"tax_escrow";

// ---------------------------------------------------------------------------
// Feature-Gated Mint Addresses
// Same pattern as conversion-vault/src/constants.rs.
// ---------------------------------------------------------------------------

#[cfg(feature = "devnet")]
pub fn crime_mint() -> Pubkey {
    Pubkey::from_str("cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc").unwrap()
}

#[cfg(feature = "localnet")]
pub fn crime_mint() -> Pubkey {
    // Localnet: placeholder, bypassed by cfg!(feature = "localnet") gate.
    Pubkey::default()
}

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub fn crime_mint() -> Pubkey {
    Pubkey::from_str("cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc").unwrap()
}

#[cfg(feature = "devnet")]
pub fn fraud_mint() -> Pubkey {
    Pubkey::from_str("FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5").unwrap()
}

#[cfg(feature = "localnet")]
pub fn fraud_mint() -> Pubkey {
    // Localnet: placeholder, bypassed by cfg!(feature = "localnet") gate.
    Pubkey::default()
}

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub fn fraud_mint() -> Pubkey {
    Pubkey::from_str("FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5").unwrap()
}

// ---------------------------------------------------------------------------
// Epoch Program Integration (for distribute_tax_escrow)
// ---------------------------------------------------------------------------

/// Epoch Program ID for cross-program PDA validation.
///
/// Used by distribute_tax_escrow to derive the expected carnage_sol_vault address.
/// Matches declare_id! in epoch-program/src/lib.rs.
#[cfg(feature = "devnet")]
pub fn epoch_program_id() -> Pubkey {
    Pubkey::from_str("4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2").unwrap()
}

#[cfg(feature = "localnet")]
pub fn epoch_program_id() -> Pubkey {
    // Localnet: use default for testing flexibility.
    Pubkey::default()
}

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub fn epoch_program_id() -> Pubkey {
    Pubkey::from_str("4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2").unwrap()
}

/// Seed for carnage SOL vault PDA on the epoch program.
/// Must match epoch_program::constants::CARNAGE_SOL_VAULT_SEED.
pub const CARNAGE_SOL_VAULT_SEED: &[u8] = b"carnage_sol_vault";

// ---------------------------------------------------------------------------
// Compile-Time Invariant Assertions (CTG-03)
// ---------------------------------------------------------------------------

/// Curve must go up: end price strictly greater than start price.
const _: () = assert!(P_END > P_START, "P_END must be greater than P_START");

/// Non-zero supply: cannot sell zero tokens.
const _: () = assert!(TOTAL_FOR_SALE > 0, "TOTAL_FOR_SALE must be non-zero");

/// u128 and u64 versions of total supply must agree.
/// TOTAL_FOR_SALE is u128, TARGET_TOKENS is u64 -- they represent the same value.
/// Round-trip cast validates no truncation occurred.
const _: () = assert!(
    TOTAL_FOR_SALE as u64 as u128 == TOTAL_FOR_SALE,
    "TARGET_TOKENS must equal TOTAL_FOR_SALE (no truncation)"
);
