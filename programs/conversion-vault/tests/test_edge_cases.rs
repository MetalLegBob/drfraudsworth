/// Edge case tests for Conversion Vault program.
///
/// Covers gaps from docs/edge-case-audit.md:
/// - VAULT-01 (MEDIUM): PROFIT->CRIME overflow with u64::MAX input
/// - VAULT-02 (MEDIUM): Unknown mint (not CRIME/FRAUD/PROFIT) produces InvalidMintPair

use anchor_lang::prelude::Pubkey;
use conversion_vault::instructions::compute_output_with_mints;
use conversion_vault::constants::CONVERSION_RATE;

/// Test mint addresses
fn test_crime() -> Pubkey {
    Pubkey::new_from_array([1u8; 32])
}
fn test_fraud() -> Pubkey {
    Pubkey::new_from_array([2u8; 32])
}
fn test_profit() -> Pubkey {
    Pubkey::new_from_array([3u8; 32])
}
fn unknown_mint() -> Pubkey {
    Pubkey::new_from_array([99u8; 32])
}

fn compute(input: &Pubkey, output: &Pubkey, amount: u64) -> anchor_lang::Result<u64> {
    compute_output_with_mints(input, output, amount, &test_crime(), &test_fraud(), &test_profit())
}

// ===========================================================================
// VAULT-01: PROFIT->CRIME overflow with u64::MAX input
//
// PROFIT->CRIME multiplies by CONVERSION_RATE (100).
// u64::MAX * 100 overflows u64 -> should return MathOverflow.
// ===========================================================================

#[test]
fn vault_01_profit_to_crime_overflow_u64_max() {
    let result = compute(&test_profit(), &test_crime(), u64::MAX);
    assert!(result.is_err(), "u64::MAX * 100 should overflow");

    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("6005"), // VaultError::MathOverflow
        "Expected MathOverflow (6005), got: {}",
        err_str
    );
}

#[test]
fn vault_01_profit_to_fraud_overflow_u64_max() {
    let result = compute(&test_profit(), &test_fraud(), u64::MAX);
    assert!(result.is_err(), "u64::MAX * 100 should overflow");

    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("6005"),
        "Expected MathOverflow (6005), got: {}",
        err_str
    );
}

#[test]
fn vault_01_profit_to_crime_max_safe_value() {
    // Maximum value that doesn't overflow: u64::MAX / 100
    let max_safe = u64::MAX / CONVERSION_RATE;
    let result = compute(&test_profit(), &test_crime(), max_safe);
    assert!(result.is_ok(), "u64::MAX/100 * 100 should not overflow");

    let output = result.unwrap();
    assert_eq!(output, max_safe * CONVERSION_RATE);
}

#[test]
fn vault_01_profit_to_crime_one_above_max_safe() {
    // One above max safe should overflow
    let max_safe = u64::MAX / CONVERSION_RATE;
    let result = compute(&test_profit(), &test_crime(), max_safe + 1);
    assert!(result.is_err(), "One above max safe should overflow");
}

// ===========================================================================
// VAULT-02: Unknown mint produces InvalidMintPair
//
// Only CRIME<->PROFIT and FRAUD<->PROFIT conversions are valid.
// Passing an unknown mint should fail with InvalidMintPair.
// ===========================================================================

#[test]
fn vault_02_unknown_to_profit_rejected() {
    let result = compute(&unknown_mint(), &test_profit(), 1_000_000);
    assert!(result.is_err(), "Unknown mint -> PROFIT should fail");

    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("6002"), // VaultError::InvalidMintPair
        "Expected InvalidMintPair (6002), got: {}",
        err_str
    );
}

#[test]
fn vault_02_profit_to_unknown_rejected() {
    let result = compute(&test_profit(), &unknown_mint(), 1_000_000);
    assert!(result.is_err(), "PROFIT -> unknown mint should fail");

    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("6002"),
        "Expected InvalidMintPair (6002), got: {}",
        err_str
    );
}

#[test]
fn vault_02_unknown_to_unknown_rejected() {
    let other_unknown = Pubkey::new_from_array([88u8; 32]);
    let result = compute(&unknown_mint(), &other_unknown, 1_000_000);
    assert!(result.is_err(), "Unknown -> unknown should fail");

    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("6002"),
        "Expected InvalidMintPair (6002), got: {}",
        err_str
    );
}

#[test]
fn vault_02_crime_to_fraud_still_rejected() {
    // This was tested in existing tests but verify it here too
    // as part of exhaustive invalid pair coverage
    let result = compute(&test_crime(), &test_fraud(), 1_000_000);
    assert!(result.is_err(), "CRIME -> FRAUD direct conversion should fail");
}

// ===========================================================================
// VAULT-03: Delta mode arithmetic edge cases
//
// Tests the checked_sub arithmetic that convert_v2's delta mode relies on.
// These are unit tests on the math — handler-level tests with real accounts
// are covered by devnet E2E validation (Step 7).
// ===========================================================================

#[test]
fn vault_03_delta_dust_below_conversion_threshold() {
    // User holds 500 CRIME, swap deposits 50 raw units (< 100 threshold)
    // Delta = 50, which is too small for CRIME->PROFIT (50/100 = 0)
    let pre_balance: u64 = 500_000_000; // 500 CRIME
    let current_balance: u64 = 500_000_050; // 500 CRIME + 50 raw dust
    let delta = current_balance.checked_sub(pre_balance).unwrap();
    assert_eq!(delta, 50);

    // This delta should fail OutputTooSmall
    let result = compute(&test_crime(), &test_profit(), delta);
    assert!(result.is_err(), "Delta of 50 raw should be too small for CRIME->PROFIT");
    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("6001"), // OutputTooSmall
        "Expected OutputTooSmall (6001), got: {}",
        err_str
    );
}

#[test]
fn vault_03_delta_exact_threshold() {
    // Delta of exactly 100 raw units — minimum viable conversion
    let pre_balance: u64 = 500_000_000;
    let current_balance: u64 = 500_000_100;
    let delta = current_balance.checked_sub(pre_balance).unwrap();
    assert_eq!(delta, 100);

    let output = compute(&test_crime(), &test_profit(), delta).unwrap();
    assert_eq!(output, 1, "100 raw CRIME -> 1 raw PROFIT");
}

#[test]
fn vault_03_delta_zero_deposit() {
    // pre_balance equals current_balance — no tokens deposited
    // checked_sub succeeds but delta = 0 → ZeroAmount
    let pre_balance: u64 = 500_000_000;
    let current_balance: u64 = 500_000_000;
    let delta = current_balance.checked_sub(pre_balance).unwrap();
    assert_eq!(delta, 0);

    let result = compute(&test_crime(), &test_profit(), delta);
    assert!(result.is_err(), "Zero delta should fail");
    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("6000"), // ZeroAmount
        "Expected ZeroAmount (6000), got: {}",
        err_str
    );
}

#[test]
fn vault_03_delta_underflow_stale_snapshot() {
    // pre_balance > current_balance (snapshot is stale — tokens were
    // transferred out between snapshot and execution)
    let pre_balance: u64 = 500_000_000;
    let current_balance: u64 = 400_000_000; // 100 CRIME transferred out
    let result = current_balance.checked_sub(pre_balance);
    assert!(result.is_none(), "Should underflow when pre_balance > current");
}

#[test]
fn vault_03_delta_large_holdings_small_deposit() {
    // User holds 1B CRIME (entire supply), swap deposits 100 CRIME
    // Delta should be exactly 100 CRIME regardless of holdings size
    let pre_balance: u64 = 1_000_000_000_000_000; // 1B CRIME (6 decimals)
    let deposit: u64 = 100_000_000; // 100 CRIME
    let current_balance = pre_balance + deposit;
    let delta = current_balance.checked_sub(pre_balance).unwrap();
    assert_eq!(delta, deposit);

    let output = compute(&test_crime(), &test_profit(), delta).unwrap();
    assert_eq!(output, 1_000_000, "100 CRIME -> 1 PROFIT");
}
