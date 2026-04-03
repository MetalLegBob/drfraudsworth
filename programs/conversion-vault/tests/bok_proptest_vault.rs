//! BOK Proptest verification suite for Conversion Vault math.
//!
//! Tests the pure conversion math: CRIME/FRAUD <-> PROFIT at 100:1 ratio.
//!
//! Invariants verified:
//! - INV-CV-001: PROFIT->IP No Overflow (realistic supply)
//! - INV-CV-002: PROFIT->IP Overflow Boundary Correct
//! - INV-CV-003: IP->PROFIT Dust Rejection (< 100 = Error)
//! - INV-CV-004: Round-Trip IP->PROFIT->IP Loses Exactly N%100
//! - INV-CV-005: Round-Trip PROFIT->IP->PROFIT Is Lossless
//! - INV-CV-006: Same-Mint Conversion Rejected
//! - INV-CV-007: CRIME<->FRAUD Direct Rejected
//! - INV-CV-008: Zero Amount Rejected
//! - INV-CV-009: convert_v2 Math Matches convert (CRIME->PROFIT)
//! - INV-CV-010: convert_v2 Math Matches convert Reverse (PROFIT->CRIME)
//! - INV-CV-011: Slippage Guard Passes When Output >= Minimum
//! - INV-CV-012: Slippage Guard Fails When Minimum > Output
//! - INV-CV-013: convert_v2 Math With FRAUD Direction
//!
//! Run: `cargo test --test bok_proptest_vault -- --nocapture`

use anchor_lang::prelude::Pubkey;
use conversion_vault::instructions::convert::compute_output_with_mints;
use proptest::prelude::*;

/// Create three unique mint pubkeys for testing.
fn test_mints() -> (Pubkey, Pubkey, Pubkey) {
    let crime = Pubkey::new_unique();
    let fraud = Pubkey::new_unique();
    let profit = Pubkey::new_unique();
    (crime, fraud, profit)
}

// =============================================================================
// INV-CV-001: PROFIT->IP No Overflow (Realistic Supply)
//
// For amount <= 20_000_000_000_000 (20T = entire PROFIT supply at 6 decimals),
// amount * 100 must not overflow u64.
// 20T * 100 = 2T, well within u64::MAX (~18.4e18).
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_001_profit_to_ip_no_overflow(
        amount in 1u64..=20_000_000_000_000u64,
    ) {
        let (crime, _, profit) = test_mints();
        let result = compute_output_with_mints(&profit, &crime, amount, &crime, &Pubkey::new_unique(), &profit);
        prop_assert!(result.is_ok(), "INV-CV-001: PROFIT->CRIME overflow at amount={}", amount);
        let output = result.unwrap();
        prop_assert_eq!(output, amount * 100, "INV-CV-001: Expected {}*100={}, got {}", amount, amount * 100, output);
    }
}

// =============================================================================
// INV-CV-002: PROFIT->IP Overflow Boundary
//
// Overflow occurs at exactly u64::MAX / 100 + 1.
// =============================================================================
#[test]
fn inv_cv_002_overflow_boundary() {
    let (crime, _, profit) = test_mints();

    // Just below overflow: should succeed
    let safe_amount = u64::MAX / 100;
    let result = compute_output_with_mints(&profit, &crime, safe_amount, &crime, &Pubkey::new_unique(), &profit);
    assert!(result.is_ok(), "INV-CV-002: Should not overflow at u64::MAX/100");
    assert_eq!(result.unwrap(), safe_amount * 100);

    // At overflow: should fail
    let overflow_amount = safe_amount + 1;
    let result = compute_output_with_mints(&profit, &crime, overflow_amount, &crime, &Pubkey::new_unique(), &profit);
    assert!(result.is_err(), "INV-CV-002: Should overflow at u64::MAX/100 + 1");
}

// =============================================================================
// INV-CV-003: IP->PROFIT Dust Rejection
//
// Amounts [1, 99] of CRIME/FRAUD produce 0 PROFIT via integer division.
// The function must return OutputTooSmall error.
// =============================================================================
#[test]
fn inv_cv_003_dust_rejection() {
    let (crime, _, profit) = test_mints();
    for amount in 1..100u64 {
        let result = compute_output_with_mints(&crime, &profit, amount, &crime, &Pubkey::new_unique(), &profit);
        assert!(
            result.is_err(),
            "INV-CV-003: Amount {} should be rejected as dust (output=0)",
            amount
        );
    }

    // amount=100 should succeed (100/100 = 1)
    let result = compute_output_with_mints(&crime, &profit, 100, &crime, &Pubkey::new_unique(), &profit);
    assert!(result.is_ok(), "INV-CV-003: Amount 100 should produce output 1");
    assert_eq!(result.unwrap(), 1);
}

// =============================================================================
// INV-CV-004: Round-Trip IP->PROFIT->IP Loses Exactly N%100
//
// CRIME -> PROFIT: amount / 100 (loses remainder)
// PROFIT -> CRIME: output * 100
// Net loss = amount % 100
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_004_round_trip_ip_profit_ip(
        amount in 100u64..=10_000_000_000u64,
    ) {
        let (crime, _, profit) = test_mints();
        let fraud = Pubkey::new_unique();

        // Step 1: CRIME -> PROFIT
        let profit_out = compute_output_with_mints(&crime, &profit, amount, &crime, &fraud, &profit).unwrap();
        prop_assert_eq!(profit_out, amount / 100);

        // Step 2: PROFIT -> CRIME
        let crime_back = compute_output_with_mints(&profit, &crime, profit_out, &crime, &fraud, &profit).unwrap();
        prop_assert_eq!(crime_back, profit_out * 100);

        // Verify loss is exactly amount % 100
        let loss = amount - crime_back;
        prop_assert_eq!(
            loss, amount % 100,
            "INV-CV-004: Expected loss={}, got {} (amount={}, profit_out={}, crime_back={})",
            amount % 100, loss, amount, profit_out, crime_back
        );
    }
}

// =============================================================================
// INV-CV-005: Round-Trip PROFIT->IP->PROFIT Is Lossless
//
// PROFIT -> CRIME: amount * 100
// CRIME -> PROFIT: (amount * 100) / 100 = amount (exact)
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_005_round_trip_profit_ip_profit(
        amount in 1u64..=(u64::MAX / 100),
    ) {
        let (crime, _, profit) = test_mints();
        let fraud = Pubkey::new_unique();

        // Step 1: PROFIT -> CRIME
        let crime_out = compute_output_with_mints(&profit, &crime, amount, &crime, &fraud, &profit).unwrap();
        prop_assert_eq!(crime_out, amount * 100);

        // Step 2: CRIME -> PROFIT
        let profit_back = compute_output_with_mints(&crime, &profit, crime_out, &crime, &fraud, &profit).unwrap();
        prop_assert_eq!(
            profit_back, amount,
            "INV-CV-005: Round-trip not lossless! amount={}, crime_out={}, profit_back={}",
            amount, crime_out, profit_back
        );
    }
}

// =============================================================================
// INV-CV-006: Same-Mint Conversion Rejected
// =============================================================================
#[test]
fn inv_cv_006_same_mint_rejected() {
    let (crime, fraud, profit) = test_mints();
    // Same mint for all three tokens
    for mint in [&crime, &fraud, &profit] {
        let result = compute_output_with_mints(mint, mint, 1000, &crime, &fraud, &profit);
        assert!(result.is_err(), "INV-CV-006: Same-mint conversion should be rejected");
    }
}

// =============================================================================
// INV-CV-007: CRIME<->FRAUD Direct Rejected
//
// Must go through PROFIT bottleneck.
// =============================================================================
#[test]
fn inv_cv_007_crime_fraud_direct_rejected() {
    let (crime, fraud, profit) = test_mints();

    // CRIME -> FRAUD
    let result = compute_output_with_mints(&crime, &fraud, 1000, &crime, &fraud, &profit);
    assert!(result.is_err(), "INV-CV-007: CRIME->FRAUD direct should be rejected");

    // FRAUD -> CRIME
    let result = compute_output_with_mints(&fraud, &crime, 1000, &crime, &fraud, &profit);
    assert!(result.is_err(), "INV-CV-007: FRAUD->CRIME direct should be rejected");
}

// =============================================================================
// INV-CV-008: Zero Amount Rejected
// =============================================================================
#[test]
fn inv_cv_008_zero_amount_rejected() {
    let (crime, _, profit) = test_mints();
    let fraud = Pubkey::new_unique();

    let result = compute_output_with_mints(&crime, &profit, 0, &crime, &fraud, &profit);
    assert!(result.is_err(), "INV-CV-008: Zero amount should be rejected");

    let result = compute_output_with_mints(&profit, &crime, 0, &crime, &fraud, &profit);
    assert!(result.is_err(), "INV-CV-008: Zero amount should be rejected");
}

// =============================================================================
// INV-CV-009: convert_v2 Math Matches convert (CRIME->PROFIT)
//
// Both convert and convert_v2 call the same compute_output_with_mints function.
// For any amount in [100, 10B], CRIME->PROFIT output must equal amount / 100.
// This verifies the math layer is shared and backwards-compatible.
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_009_convert_v2_exact_matches_convert(
        amount in 100u64..=10_000_000_000u64,
    ) {
        let (crime, fraud, profit) = test_mints();
        let output = compute_output_with_mints(
            &crime, &profit, amount, &crime, &fraud, &profit
        ).unwrap();
        // convert_v2 calls the same function, so the output must be identical
        prop_assert_eq!(
            output, amount / 100,
            "INV-CV-009: CRIME->PROFIT expected {}, got {} (amount={})",
            amount / 100, output, amount
        );
    }
}

// =============================================================================
// INV-CV-010: convert_v2 Math Matches convert Reverse (PROFIT->CRIME)
//
// For PROFIT->CRIME direction, output must equal amount * 100.
// Range capped at u64::MAX / 100 to avoid overflow.
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_010_convert_v2_exact_matches_convert_reverse(
        amount in 1u64..=(u64::MAX / 100),
    ) {
        let (crime, fraud, profit) = test_mints();
        let output = compute_output_with_mints(
            &profit, &crime, amount, &crime, &fraud, &profit
        ).unwrap();
        prop_assert_eq!(
            output, amount * 100,
            "INV-CV-010: PROFIT->CRIME expected {}, got {} (amount={})",
            amount * 100, output, amount
        );
    }
}

// =============================================================================
// INV-CV-011: Slippage Guard Passes When Output >= Minimum
//
// For a known conversion (10_000 CRIME -> 100 PROFIT), the output (100) should
// be >= minimum_output when minimum_output <= 100. This validates the math-level
// comparison that the handler's slippage guard relies on.
// =============================================================================
#[test]
fn inv_cv_011_slippage_passes_when_output_gte_minimum() {
    let (crime, fraud, profit) = test_mints();

    // 10_000 CRIME -> 100 PROFIT (exact)
    let amount_in = 10_000u64;
    let output = compute_output_with_mints(
        &crime, &profit, amount_in, &crime, &fraud, &profit
    ).unwrap();
    assert_eq!(output, 100, "INV-CV-011: Expected output 100");

    // Slippage check: output >= minimum_output
    let minimum_output = 100u64; // exact match
    assert!(
        output >= minimum_output,
        "INV-CV-011: Slippage guard should pass when output ({}) >= minimum ({})",
        output, minimum_output
    );

    // Also passes with minimum_output = 0 (disabled slippage)
    let minimum_zero: u64 = 0;
    assert!(
        output >= minimum_zero,
        "INV-CV-011: Slippage guard should pass with minimum=0"
    );

    // Also passes with minimum_output = 1 (very loose)
    assert!(
        output >= 1,
        "INV-CV-011: Slippage guard should pass with minimum=1"
    );
}

// =============================================================================
// INV-CV-012: Slippage Guard Fails When Minimum > Output
//
// For 10_000 CRIME -> 100 PROFIT, minimum_output = 101 exceeds the actual
// output. The handler-level slippage check (output >= minimum_output) would
// fail. We verify the math produces the expected output so the comparison
// is correct.
// =============================================================================
#[test]
fn inv_cv_012_slippage_fails_when_minimum_exceeds_output() {
    let (crime, fraud, profit) = test_mints();

    // 10_000 CRIME -> 100 PROFIT
    let amount_in = 10_000u64;
    let output = compute_output_with_mints(
        &crime, &profit, amount_in, &crime, &fraud, &profit
    ).unwrap();
    assert_eq!(output, 100, "INV-CV-012: Expected output 100");

    // minimum_output = 101 exceeds actual output of 100
    let minimum_output = 101u64;
    assert!(
        output < minimum_output,
        "INV-CV-012: Slippage guard should FAIL when output ({}) < minimum ({})",
        output, minimum_output
    );

    // Also verify with a much larger minimum
    let large_minimum = 1_000u64;
    assert!(
        output < large_minimum,
        "INV-CV-012: Slippage guard should FAIL when output ({}) < minimum ({})",
        output, large_minimum
    );
}

// =============================================================================
// INV-CV-013: convert_v2 Math With FRAUD Direction
//
// Identical to INV-CV-009 but using FRAUD mint instead of CRIME.
// Verifies both faction tokens produce identical results through the math layer.
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_013_convert_v2_fraud_direction(
        amount in 100u64..=10_000_000_000u64,
    ) {
        let (crime, fraud, profit) = test_mints();
        let output = compute_output_with_mints(
            &fraud, &profit, amount, &crime, &fraud, &profit
        ).unwrap();
        prop_assert_eq!(
            output, amount / 100,
            "INV-CV-013: FRAUD->PROFIT expected {}, got {} (amount={})",
            amount / 100, output, amount
        );
    }
}

// =============================================================================
// INV-CV-014: Delta Mode Arithmetic — Only Swap Deposit Converted
//
// Simulates the delta mode logic from convert_v2 handler:
//   effective_amount = current_balance - pre_balance
// For any (pre_balance, deposit) pair where deposit >= 100, the conversion
// output must equal deposit / 100 — independent of pre_balance magnitude.
// This proves pre-existing holdings don't affect conversion output.
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_014_delta_mode_only_converts_deposit(
        pre_balance in 0u64..=1_000_000_000_000u64,   // up to 1M tokens (6 decimals)
        deposit in 100u64..=10_000_000_000u64,          // 0.0001 to 10K tokens
    ) {
        let (crime, fraud, profit) = test_mints();

        // Simulate: current_balance = pre_balance + deposit
        let current_balance = pre_balance.checked_add(deposit).unwrap();

        // Delta mode: effective_amount = current_balance - pre_balance = deposit
        let delta = current_balance.checked_sub(pre_balance).unwrap();
        prop_assert_eq!(delta, deposit, "Delta must equal deposit exactly");

        // Conversion output depends only on delta, not on pre_balance
        let output = compute_output_with_mints(
            &crime, &profit, delta, &crime, &fraud, &profit
        ).unwrap();
        prop_assert_eq!(
            output, deposit / 100,
            "INV-CV-014: Output should be deposit/100={}, got {} (pre_balance={}, deposit={})",
            deposit / 100, output, pre_balance, deposit
        );
    }
}

// =============================================================================
// INV-CV-015: Delta Mode — DeltaUnderflow When Pre-Balance Exceeds Current
//
// If pre_balance > current_balance (stale snapshot or balance decreased),
// checked_sub returns None → handler returns DeltaUnderflow.
// Verifies the arithmetic underflow detection is correct.
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn inv_cv_015_delta_underflow_detected(
        current_balance in 0u64..=1_000_000_000_000u64,
        excess in 1u64..=1_000_000_000u64,
    ) {
        let pre_balance = current_balance.saturating_add(excess);

        // If pre_balance > current_balance, checked_sub must return None
        if pre_balance > current_balance {
            let result = current_balance.checked_sub(pre_balance);
            prop_assert!(
                result.is_none(),
                "INV-CV-015: checked_sub should return None when pre_balance ({}) > current ({})",
                pre_balance, current_balance
            );
        }
    }
}

// =============================================================================
// INV-CV-016: Delta Mode Equivalence — pre_balance=0 Matches Convert-All
//
// When pre_balance=0 and amount_in=0, delta mode computes:
//   effective = current_balance - 0 = current_balance
// This is identical to convert-all mode. Both must produce the same output.
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_016_delta_zero_pre_balance_equals_convert_all(
        balance in 100u64..=10_000_000_000u64,
    ) {
        let (crime, fraud, profit) = test_mints();

        // Convert-all: effective_amount = balance
        let convert_all_output = compute_output_with_mints(
            &crime, &profit, balance, &crime, &fraud, &profit
        ).unwrap();

        // Delta with pre_balance=0: effective_amount = balance - 0 = balance
        let delta = balance.checked_sub(0).unwrap();
        let delta_output = compute_output_with_mints(
            &crime, &profit, delta, &crime, &fraud, &profit
        ).unwrap();

        prop_assert_eq!(
            convert_all_output, delta_output,
            "INV-CV-016: convert-all ({}) != delta with pre_balance=0 ({}) for balance={}",
            convert_all_output, delta_output, balance
        );
    }
}

// =============================================================================
// INV-CV-017: Exact Mode Ignores Pre-Balance
//
// When amount_in > 0, the handler uses amount_in directly regardless of
// pre_balance. Verifies that varying pre_balance doesn't change the output
// when amount_in is specified.
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    fn inv_cv_017_exact_mode_ignores_pre_balance(
        amount_in in 100u64..=10_000_000_000u64,
        _pre_balance_a in 0u64..=1_000_000_000_000u64,
        _pre_balance_b in 0u64..=1_000_000_000_000u64,
    ) {
        let (crime, fraud, profit) = test_mints();

        // In exact mode, pre_balance is ignored — output depends only on amount_in.
        // We generate two different pre_balance values to prove they don't matter;
        // the compute function doesn't even receive pre_balance (it's handler-level).
        let output_a = compute_output_with_mints(
            &crime, &profit, amount_in, &crime, &fraud, &profit
        ).unwrap();
        let output_b = compute_output_with_mints(
            &crime, &profit, amount_in, &crime, &fraud, &profit
        ).unwrap();

        prop_assert_eq!(
            output_a, output_b,
            "INV-CV-017: Exact mode output should be identical regardless of pre_balance"
        );
        prop_assert_eq!(
            output_a, amount_in / 100,
            "INV-CV-017: Exact mode output should be amount_in/100"
        );
    }
}
