//! Cross-crate parity tests: SDK vault quotes vs on-chain conversion math.
//!
//! These tests prove that VaultAmm::quote() produces IDENTICAL output to the
//! on-chain Conversion Vault's compute_output_with_mints() function by importing
//! the actual on-chain function and comparing results directly.
//!
//! Verification approach:
//!   1. Import compute_output_with_mints from conversion_vault::instructions::convert
//!   2. Call it with the same (input_mint, output_mint, amount) as the SDK
//!   3. Call VaultAmm::quote() for the same inputs
//!   4. Assert: SDK output == on-chain output (EXACT match -- vault math is
//!      deterministic integer division/multiplication, no rounding variance)
//!
//! Why not full LiteSVM:
//!   Vault conversion deploys T22 mints with transfer hook extensions, whitelist
//!   PDAs, and VaultConfig state. The math is trivially deterministic (divide or
//!   multiply by 100), so cross-crate function import provides equivalent proof
//!   with dramatically simpler setup. The SOL pool tests (parity_sol_pool.rs)
//!   provide the stronger cross-crate proof for the more complex AMM math.
//!
//! Run: cargo test -p drfraudsworth-jupiter-adapter --test parity_vault

use drfraudsworth_jupiter_adapter::vault_amm::VaultAmm;
use drfraudsworth_jupiter_adapter::math::vault_math::compute_vault_output as sdk_vault_output;
use drfraudsworth_jupiter_adapter::accounts::addresses::{CRIME_MINT, FRAUD_MINT, PROFIT_MINT};

use jupiter_amm_interface::{Amm, QuoteParams, SwapMode};

// On-chain vault conversion function (imported from actual program crate)
use conversion_vault::instructions::convert::compute_output_with_mints as onchain_vault_output;

// =============================================================================
// Helper: call on-chain function with mainnet mint addresses
// =============================================================================

/// Call the on-chain compute_output_with_mints with our mainnet mint addresses.
/// Returns Ok(amount) or Err for invalid conversions.
fn onchain_convert(
    input_mint: &solana_sdk::pubkey::Pubkey,
    output_mint: &solana_sdk::pubkey::Pubkey,
    amount_in: u64,
) -> Result<u64, ()> {
    // The on-chain function uses anchor_lang::prelude::Pubkey which is the same
    // as solana_sdk::pubkey::Pubkey in the current version.
    // We pass our mainnet addresses as the known mints.
    let crime = anchor_lang::prelude::Pubkey::new_from_array(CRIME_MINT.to_bytes());
    let fraud = anchor_lang::prelude::Pubkey::new_from_array(FRAUD_MINT.to_bytes());
    let profit = anchor_lang::prelude::Pubkey::new_from_array(PROFIT_MINT.to_bytes());
    let input = anchor_lang::prelude::Pubkey::new_from_array(input_mint.to_bytes());
    let output = anchor_lang::prelude::Pubkey::new_from_array(output_mint.to_bytes());

    onchain_vault_output(&input, &output, amount_in, &crime, &fraud, &profit)
        .map_err(|_| ())
}

// =============================================================================
// PART 1: Function-level parity (SDK vault math == on-chain vault math)
//
// Direct comparison of compute_vault_output vs compute_output_with_mints
// for a comprehensive set of inputs.
// =============================================================================

#[test]
fn parity_vault_math_crime_to_profit() {
    let test_cases: Vec<u64> = vec![
        100,                        // minimum exact conversion (100/100 = 1)
        10_000,                     // standard (10000/100 = 100)
        1_000_000_000,              // 1B (1B/100 = 10M)
        100_000_000_000,            // 100B
        1_000_000_000_000_000_000,  // max realistic (1B tokens at 9 decimals)
        u64::MAX,                   // maximum possible
    ];

    for amount in test_cases {
        let sdk = sdk_vault_output(&CRIME_MINT, &PROFIT_MINT, amount);
        let onchain = onchain_convert(&CRIME_MINT, &PROFIT_MINT, amount);
        match (sdk, onchain) {
            (Some(s), Ok(o)) => assert_eq!(s, o, "CRIME->PROFIT mismatch for amount={}", amount),
            (None, Err(_)) => {} // Both correctly rejected
            (s, o) => panic!(
                "CRIME->PROFIT disagreement for amount={}: sdk={:?}, onchain={:?}",
                amount, s, o
            ),
        }
    }
}

#[test]
fn parity_vault_math_fraud_to_profit() {
    let test_cases: Vec<u64> = vec![
        100, 10_000, 1_000_000_000, u64::MAX,
    ];

    for amount in test_cases {
        let sdk = sdk_vault_output(&FRAUD_MINT, &PROFIT_MINT, amount);
        let onchain = onchain_convert(&FRAUD_MINT, &PROFIT_MINT, amount);
        match (sdk, onchain) {
            (Some(s), Ok(o)) => assert_eq!(s, o, "FRAUD->PROFIT mismatch for amount={}", amount),
            (None, Err(_)) => {} // Both correctly rejected
            (s, o) => panic!(
                "FRAUD->PROFIT disagreement for amount={}: sdk={:?}, onchain={:?}",
                amount, s, o
            ),
        }
    }
}

#[test]
fn parity_vault_math_profit_to_crime() {
    let test_cases: Vec<u64> = vec![
        1, 100, 10_000, 1_000_000_000,
        u64::MAX / 100,  // max safe (doesn't overflow when * 100)
    ];

    for amount in test_cases {
        let sdk = sdk_vault_output(&PROFIT_MINT, &CRIME_MINT, amount);
        let onchain = onchain_convert(&PROFIT_MINT, &CRIME_MINT, amount);
        match (sdk, onchain) {
            (Some(s), Ok(o)) => assert_eq!(s, o, "PROFIT->CRIME mismatch for amount={}", amount),
            (None, Err(_)) => {} // Both correctly rejected
            (s, o) => panic!(
                "PROFIT->CRIME disagreement for amount={}: sdk={:?}, onchain={:?}",
                amount, s, o
            ),
        }
    }
}

#[test]
fn parity_vault_math_profit_to_fraud() {
    let test_cases: Vec<u64> = vec![
        1, 50, 10_000_000, u64::MAX / 100,
    ];

    for amount in test_cases {
        let sdk = sdk_vault_output(&PROFIT_MINT, &FRAUD_MINT, amount);
        let onchain = onchain_convert(&PROFIT_MINT, &FRAUD_MINT, amount);
        match (sdk, onchain) {
            (Some(s), Ok(o)) => assert_eq!(s, o, "PROFIT->FRAUD mismatch for amount={}", amount),
            (None, Err(_)) => {} // Both correctly rejected
            (s, o) => panic!(
                "PROFIT->FRAUD disagreement for amount={}: sdk={:?}, onchain={:?}",
                amount, s, o
            ),
        }
    }
}

// =============================================================================
// PART 2: Pipeline parity (VaultAmm::quote() == on-chain execution)
//
// These tests use VaultAmm instances and verify quote output matches on-chain.
// =============================================================================

// --- CRIME -> PROFIT ---

#[test]
fn parity_crime_to_profit_exact() {
    let amm = VaultAmm::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let amount_in: u64 = 10_000; // 10000 / 100 = 100

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&CRIME_MINT, &PROFIT_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain, "CRIME->PROFIT exact");
    assert_eq!(sdk_quote.out_amount, 100);
    assert_eq!(sdk_quote.fee_amount, 0, "Vault must have zero fees");
    assert_eq!(sdk_quote.fee_pct, rust_decimal::Decimal::ZERO, "Vault fee_pct must be zero");
}

#[test]
fn parity_crime_to_profit_with_remainder() {
    let amm = VaultAmm::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let amount_in: u64 = 10_099; // 10099 / 100 = 100 (99 lost to integer division)

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&CRIME_MINT, &PROFIT_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 100, "Remainder truncated");
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_crime_to_profit_dust() {
    // 99 CRIME / 100 = 0 -> should error (OutputTooSmall)
    let amm = VaultAmm::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let amount_in: u64 = 99;

    let sdk_result = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactIn,
    });

    let onchain_result = onchain_convert(&CRIME_MINT, &PROFIT_MINT, amount_in);

    assert!(sdk_result.is_err(), "SDK should error on dust (99 CRIME)");
    assert!(onchain_result.is_err(), "On-chain should error on dust (99 CRIME)");
}

// --- FRAUD -> PROFIT ---

#[test]
fn parity_fraud_to_profit_exact() {
    let amm = VaultAmm::new_for_testing(FRAUD_MINT, PROFIT_MINT);
    let amount_in: u64 = 100_000_000_000; // 100B / 100 = 1B

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: FRAUD_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&FRAUD_MINT, &PROFIT_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 1_000_000_000);
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_fraud_to_profit_large() {
    let amm = VaultAmm::new_for_testing(FRAUD_MINT, PROFIT_MINT);
    let amount_in: u64 = 500_000_000_000_000; // 500T / 100 = 5T

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: FRAUD_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&FRAUD_MINT, &PROFIT_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 5_000_000_000_000);
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_fraud_to_profit_small() {
    let amm = VaultAmm::new_for_testing(FRAUD_MINT, PROFIT_MINT);
    let amount_in: u64 = 100; // 100 / 100 = 1 (minimum non-dust)

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: FRAUD_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&FRAUD_MINT, &PROFIT_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 1);
    assert_eq!(sdk_quote.fee_amount, 0);
}

// --- PROFIT -> CRIME ---

#[test]
fn parity_profit_to_crime_exact() {
    let amm = VaultAmm::new_for_testing(PROFIT_MINT, CRIME_MINT);
    let amount_in: u64 = 100; // 100 * 100 = 10000

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: PROFIT_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&PROFIT_MINT, &CRIME_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 10_000);
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_profit_to_crime_large() {
    let amm = VaultAmm::new_for_testing(PROFIT_MINT, CRIME_MINT);
    let amount_in: u64 = 1_000_000_000; // 1B * 100 = 100B

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: PROFIT_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&PROFIT_MINT, &CRIME_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 100_000_000_000);
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_profit_to_crime_overflow() {
    // u64::MAX / 50 > u64::MAX / 100, so *100 overflows
    let amm = VaultAmm::new_for_testing(PROFIT_MINT, CRIME_MINT);
    let amount_in: u64 = u64::MAX / 50;

    let sdk_result = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: PROFIT_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    });

    let onchain_result = onchain_convert(&PROFIT_MINT, &CRIME_MINT, amount_in);

    assert!(sdk_result.is_err(), "SDK should error on overflow");
    assert!(onchain_result.is_err(), "On-chain should error on overflow");
}

// --- PROFIT -> FRAUD ---

#[test]
fn parity_profit_to_fraud_exact() {
    let amm = VaultAmm::new_for_testing(PROFIT_MINT, FRAUD_MINT);
    let amount_in: u64 = 1; // 1 * 100 = 100

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: PROFIT_MINT,
        output_mint: FRAUD_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&PROFIT_MINT, &FRAUD_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 100);
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_profit_to_fraud_large() {
    let amm = VaultAmm::new_for_testing(PROFIT_MINT, FRAUD_MINT);
    let amount_in: u64 = 10_000_000; // 10M * 100 = 1B

    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: PROFIT_MINT,
        output_mint: FRAUD_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&PROFIT_MINT, &FRAUD_MINT, amount_in).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, 1_000_000_000);
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_profit_to_fraud_overflow_guard() {
    // u64::MAX PROFIT -> should overflow on *100
    let amm = VaultAmm::new_for_testing(PROFIT_MINT, FRAUD_MINT);
    let amount_in: u64 = u64::MAX;

    let sdk_result = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: PROFIT_MINT,
        output_mint: FRAUD_MINT,
        swap_mode: SwapMode::ExactIn,
    });

    let onchain_result = onchain_convert(&PROFIT_MINT, &FRAUD_MINT, amount_in);

    assert!(sdk_result.is_err(), "SDK should error on u64::MAX overflow");
    assert!(onchain_result.is_err(), "On-chain should error on u64::MAX overflow");
}

// =============================================================================
// PART 3: Edge cases and invariants
// =============================================================================

#[test]
fn parity_zero_amount_both_error() {
    let sdk = sdk_vault_output(&CRIME_MINT, &PROFIT_MINT, 0);
    let onchain = onchain_convert(&CRIME_MINT, &PROFIT_MINT, 0);

    assert!(sdk.is_none(), "SDK should reject zero amount");
    assert!(onchain.is_err(), "On-chain should reject zero amount");
}

#[test]
fn parity_max_safe_profit_to_crime() {
    // Maximum PROFIT that doesn't overflow when *100
    let max_safe = u64::MAX / 100;

    let amm = VaultAmm::new_for_testing(PROFIT_MINT, CRIME_MINT);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: max_safe,
        input_mint: PROFIT_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    let onchain = onchain_convert(&PROFIT_MINT, &CRIME_MINT, max_safe).unwrap();

    assert_eq!(sdk_quote.out_amount, onchain);
    assert_eq!(sdk_quote.out_amount, max_safe * 100);
    assert_eq!(sdk_quote.fee_amount, 0);
}

#[test]
fn parity_all_vault_fees_zero() {
    // Verify EVERY successful vault quote has fee_amount == 0 and fee_pct == 0
    let directions: Vec<(solana_sdk::pubkey::Pubkey, solana_sdk::pubkey::Pubkey, u64)> = vec![
        (CRIME_MINT, PROFIT_MINT, 10_000),
        (FRAUD_MINT, PROFIT_MINT, 10_000),
        (PROFIT_MINT, CRIME_MINT, 100),
        (PROFIT_MINT, FRAUD_MINT, 100),
    ];

    for (input, output, amount) in directions {
        let amm = VaultAmm::new_for_testing(input, output);
        let quote = amm.quote(&QuoteParams {
            amount,
            input_mint: input,
            output_mint: output,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();

        assert_eq!(quote.fee_amount, 0, "Fee must be 0 for {:?}->{:?}", input, output);
        assert_eq!(quote.fee_pct, rust_decimal::Decimal::ZERO, "Fee pct must be 0");
    }
}

#[test]
fn parity_exact_out_not_supported() {
    // Both SDK and on-chain reject ExactOut for vault
    let amm = VaultAmm::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let result = amm.quote(&QuoteParams {
        amount: 10_000,
        input_mint: CRIME_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactOut,
    });
    assert!(result.is_err(), "ExactOut must be rejected");
}
