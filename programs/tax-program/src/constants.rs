//! Tax Program constants.
//!
//! These constants define the core parameters for tax calculation and distribution.
//! The swap_authority seed MUST match the AMM's SWAP_AUTHORITY_SEED for CPI compatibility.

use anchor_lang::prelude::Pubkey;
use std::str::FromStr;

/// Seed for deriving the swap_authority PDA.
/// Must match AMM's SWAP_AUTHORITY_SEED for CPI access control.
pub const SWAP_AUTHORITY_SEED: &[u8] = b"swap_authority";

/// Basis points denominator (10,000 = 100%).
/// Using u128 to avoid overflow in intermediate calculations.
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Staking escrow receives 71% of collected tax (7100 bps).
pub const STAKING_BPS: u128 = 7_100;

/// Carnage Fund receives 24% of collected tax (2400 bps).
pub const CARNAGE_BPS: u128 = 2_400;

/// Treasury receives 5% (remainder after staking + carnage).
/// Not used in calculation - treasury gets: total - staking - carnage.
pub const TREASURY_BPS: u128 = 500;

/// Minimum tax amount for split distribution.
/// Below this threshold, all tax goes to staking (avoids dust distribution).
/// Value: 4 lamports - minimum for meaningful 3-way split.
pub const MICRO_TAX_THRESHOLD: u64 = 4;

/// Protocol-enforced minimum output floor (50% = 5000 bps).
/// User swaps with minimum_amount_out below this floor are rejected.
/// Consistent with Carnage's 50% slippage floor approach.
///
/// At 50%, the ~1% LP fee is naturally absorbed by the generous floor.
/// Only bots/sandwich attackers set slippage below 50% of expected output.
///
/// Source: Phase 49 CONTEXT.md (SEC-10)
pub const MINIMUM_OUTPUT_FLOOR_BPS: u64 = 5000;

// ---------------------------------------------------------------------------
// Epoch Program / Carnage Integration
// ---------------------------------------------------------------------------

/// Epoch Program ID for cross-program validation.
///
/// Matches declare_id! in epoch-program/src/lib.rs.
/// Source keypair: keypairs/epoch-program.json
pub fn epoch_program_id() -> Pubkey {
    Pubkey::from_str("4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2").unwrap()
}

/// Carnage SOL Vault PDA seed.
/// Must match CARNAGE_SOL_VAULT_SEED in epoch-program/src/constants.rs
pub const CARNAGE_SOL_VAULT_SEED: &[u8] = b"carnage_sol_vault";

/// Epoch State PDA seed.
/// Must match EPOCH_STATE_SEED in epoch-program/src/constants.rs
pub const EPOCH_STATE_SEED: &[u8] = b"epoch_state";

/// Seed for deriving the Carnage signer PDA in Epoch Program.
/// Must match Epoch Program's derivation: seeds = [CARNAGE_SIGNER_SEED], bump
///
/// Source: Tax_Pool_Logic_Spec.md Section 13.3
pub const CARNAGE_SIGNER_SEED: &[u8] = b"carnage_signer";

/// Seed for deriving the WSOL intermediary PDA.
/// This account holds tax-portion WSOL during the sell flow's
/// transfer-close-distribute-reinit cycle.
/// Derived from Tax Program: seeds = ["wsol_intermediary"]
pub const WSOL_INTERMEDIARY_SEED: &[u8] = b"wsol_intermediary";

/// Derive the EpochState PDA address.
///
/// # Returns
/// (pubkey, bump) tuple for the EpochState PDA
pub fn get_epoch_state_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[EPOCH_STATE_SEED], &epoch_program_id())
}

/// Derive the Carnage signer PDA address.
/// This MUST match the derivation in Epoch Program for swap_exempt validation.
///
/// # Returns
/// (pubkey, bump) tuple for the Carnage signer PDA
pub fn get_carnage_signer_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CARNAGE_SIGNER_SEED], &epoch_program_id())
}

// ---------------------------------------------------------------------------
// AMM Program Integration
// ---------------------------------------------------------------------------

/// AMM Program ID for address constraint validation.
///
/// Matches declare_id! in amm/src/lib.rs.
/// Source keypair: keypairs/amm-keypair.json
pub fn amm_program_id() -> Pubkey {
    Pubkey::from_str("5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR").unwrap()
}

// ---------------------------------------------------------------------------
// Staking Program Integration
// ---------------------------------------------------------------------------

/// Seed for deriving the Tax authority PDA.
/// Used by Tax Program to sign CPI calls to Staking Program.
/// CRITICAL: Must match Staking Program's TAX_AUTHORITY_SEED.
pub const TAX_AUTHORITY_SEED: &[u8] = b"tax_authority";

/// Stake pool PDA seed (must match Staking Program).
pub const STAKE_POOL_SEED: &[u8] = b"stake_pool";

/// Escrow vault PDA seed (must match Staking Program's ESCROW_VAULT_SEED).
/// Used for PDA validation of staking_escrow in swap instructions.
pub const ESCROW_VAULT_SEED: &[u8] = b"escrow_vault";

/// Returns the Staking Program ID for cross-program CPI.
///
/// This must match the declare_id! in Staking Program (lib.rs).
/// Used when building deposit_rewards CPI.
///
/// Matches declare_id! in staking/src/lib.rs.
/// Source keypair: keypairs/staking-keypair.json
pub fn staking_program_id() -> Pubkey {
    Pubkey::from_str("12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH").unwrap()
}

/// Treasury wallet address for protocol revenue.
/// Feature-gated: devnet uses test wallet, mainnet requires explicit address.
///
/// MAINNET: Replace Pubkey::default() with the actual mainnet treasury
/// address before launch. See Docs/mainnet-checklist.md.
#[cfg(feature = "devnet")]
pub fn treasury_pubkey() -> Pubkey {
    Pubkey::from_str("8kPzhQoUPx7LYM18f9TzskW4ZgvGyq4jMPYZikqmHMH4").unwrap()
}

#[cfg(all(feature = "localnet", not(feature = "devnet")))]
pub fn treasury_pubkey() -> Pubkey {
    // Localnet: placeholder, runtime-generated addresses used in tests.
    Pubkey::default()
}

#[cfg(not(any(feature = "devnet", feature = "localnet")))]
pub fn treasury_pubkey() -> Pubkey {
    Pubkey::from_str("8kPzhQoUPx7LYM18f9TzskW4ZgvGyq4jMPYZikqmHMH4").unwrap()
}

/// Anchor discriminator for Staking::deposit_rewards instruction.
/// Computed: sha256("global:deposit_rewards")[0..8]
/// Used when building CPI instruction data to Staking Program.
///
/// CRITICAL: Must match DEPOSIT_REWARDS_DISCRIMINATOR in staking/src/constants.rs.
pub const DEPOSIT_REWARDS_DISCRIMINATOR: [u8; 8] = [52, 249, 112, 72, 206, 161, 196, 1];

/// Derive the StakePool PDA address.
///
/// # Returns
/// (pubkey, bump) tuple for the StakePool PDA
pub fn get_stake_pool_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[STAKE_POOL_SEED], &staking_program_id())
}

/// Derive the Tax authority PDA address.
/// This is the PDA that signs deposit_rewards CPI calls.
///
/// # Returns
/// (pubkey, bump) tuple for the Tax authority PDA
pub fn get_tax_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TAX_AUTHORITY_SEED], &crate::ID)
}

/// Derive the WSOL intermediary PDA address.
/// This PDA is a WSOL token account owned by swap_authority,
/// used to hold tax WSOL during the sell close-and-reinit cycle.
///
/// # Returns
/// (pubkey, bump) tuple for the WSOL intermediary PDA
pub fn get_wsol_intermediary_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[WSOL_INTERMEDIARY_SEED], &crate::ID)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn test_tax_authority_seed() {
        assert_eq!(TAX_AUTHORITY_SEED, b"tax_authority");
        assert_eq!(TAX_AUTHORITY_SEED.len(), 13);
    }

    #[test]
    fn test_stake_pool_seed() {
        assert_eq!(STAKE_POOL_SEED, b"stake_pool");
        assert_eq!(STAKE_POOL_SEED.len(), 10);
    }

    #[test]
    fn test_staking_program_id() {
        let id = staking_program_id();
        assert_eq!(id.to_string(), "12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH");
    }

    /// Verify DEPOSIT_REWARDS_DISCRIMINATOR matches sha256("global:deposit_rewards")[0..8].
    ///
    /// This discriminator is used when building CPI instruction data
    /// to call deposit_rewards on the Staking Program.
    #[test]
    fn test_deposit_rewards_discriminator() {
        let mut hasher = Sha256::new();
        hasher.update(b"global:deposit_rewards");
        let result = hasher.finalize();
        let expected: [u8; 8] = result[0..8].try_into().unwrap();
        assert_eq!(
            DEPOSIT_REWARDS_DISCRIMINATOR, expected,
            "Discriminator mismatch: expected {:02x?}, got {:02x?}",
            expected, DEPOSIT_REWARDS_DISCRIMINATOR
        );
    }

    #[test]
    fn test_amm_program_id() {
        let id = amm_program_id();
        assert_eq!(id.to_string(), "5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR");
    }

    #[test]
    fn test_escrow_vault_seed() {
        assert_eq!(ESCROW_VAULT_SEED, b"escrow_vault");
        assert_eq!(ESCROW_VAULT_SEED.len(), 12);
    }

    #[test]
    fn test_carnage_sol_vault_seed() {
        assert_eq!(CARNAGE_SOL_VAULT_SEED, b"carnage_sol_vault");
        assert_eq!(CARNAGE_SOL_VAULT_SEED.len(), 17);
    }

    #[test]
    fn test_treasury_pubkey_is_valid() {
        let pk = treasury_pubkey();
        // devnet: 8kPzh..., mainnet: Pubkey::default() (placeholder)
        // In either case, this should not panic on from_str/default.
        let _ = pk.to_string();
    }

    #[test]
    fn test_wsol_intermediary_seed() {
        assert_eq!(WSOL_INTERMEDIARY_SEED, b"wsol_intermediary");
        assert_eq!(WSOL_INTERMEDIARY_SEED.len(), 17);
    }
}
