//! Cross-crate parity tests: SDK quote vs on-chain math for SOL pool swaps.
//!
//! These tests prove that SolPoolAmm::quote() produces IDENTICAL output to the
//! on-chain Tax Program + AMM math pipeline by importing the actual on-chain
//! functions from the program crates and comparing outputs directly.
//!
//! Verification approach:
//!   1. Import the REAL on-chain math functions from amm::helpers::math and
//!      tax_program::helpers::tax_math (same functions the deployed programs use)
//!   2. Step through the on-chain handler logic manually (same order as handler)
//!   3. Compute SDK quote via SolPoolAmm for the same inputs
//!   4. Assert: SDK output == on-chain output (exact match, zero tolerance)
//!
//! Why not full LiteSVM:
//!   Full 5-program LiteSVM deployment (AMM + Tax + Epoch + Staking + Hook) with
//!   T22 mints, transfer hooks, whitelist PDAs, and epoch state is infeasible in
//!   a single test setup (the AMM's own LiteSVM tests are still placeholders).
//!   This cross-crate approach is STRONGER than LiteSVM for math parity because
//!   it verifies the exact function calls, not just the final balance diff.
//!   The unit tests from Plan 01 additionally verify each individual function.
//!
//! Run: cargo test -p drfraudsworth-jupiter-adapter --test parity_sol_pool

use drfraudsworth_jupiter_adapter::constants::LP_FEE_BPS;
use drfraudsworth_jupiter_adapter::math::amm_math::{
    calculate_effective_input as sdk_effective_input,
    calculate_swap_output as sdk_swap_output,
};
use drfraudsworth_jupiter_adapter::math::tax_math::calculate_tax as sdk_calculate_tax;
use drfraudsworth_jupiter_adapter::sol_pool_amm::SolPoolAmm;
use drfraudsworth_jupiter_adapter::accounts::addresses::{
    CRIME_MINT, FRAUD_MINT, NATIVE_MINT,
};

use jupiter_amm_interface::{Amm, QuoteParams, SwapMode};

// On-chain math functions (imported from actual program crates)
use amm::helpers::math::{
    calculate_effective_input as onchain_effective_input,
    calculate_swap_output as onchain_swap_output,
};
use tax_program::helpers::tax_math::calculate_tax as onchain_calculate_tax;

// =============================================================================
// Helpers
// =============================================================================

/// Create a SolPoolAmm with known state for testing.
fn make_sol_pool_amm(
    is_crime: bool,
    reserve_sol: u64,
    reserve_token: u64,
    buy_tax_bps: u16,
    sell_tax_bps: u16,
) -> SolPoolAmm {
    SolPoolAmm::new_for_testing(
        is_crime,
        reserve_sol,
        reserve_token,
        buy_tax_bps,
        sell_tax_bps,
    )
}

/// Simulate on-chain buy execution: tax on input, LP fee, swap.
/// Returns (output_tokens, tax_amount).
///
/// Mirrors: programs/tax-program/src/instructions/swap_sol_buy.rs handler()
fn onchain_buy_pipeline(
    reserve_sol: u64,
    reserve_token: u64,
    amount_in: u64,
    buy_tax_bps: u16,
) -> (u64, u64) {
    // Step 1: Tax deducted from SOL input (on-chain line ~83)
    let tax = onchain_calculate_tax(amount_in, buy_tax_bps).unwrap();
    let sol_to_swap = amount_in.checked_sub(tax).unwrap();

    if sol_to_swap == 0 {
        return (0, tax);
    }

    // Step 2: AMM swap -- LP fee then constant-product (on-chain AMM handler)
    let effective = onchain_effective_input(sol_to_swap, LP_FEE_BPS).unwrap();
    let output = onchain_swap_output(reserve_sol, reserve_token, effective).unwrap();

    (output, tax)
}

/// Simulate on-chain sell execution: LP fee, swap, tax on output.
/// Returns (net_sol_output, tax_amount).
///
/// Mirrors: programs/tax-program/src/instructions/swap_sol_sell.rs handler()
fn onchain_sell_pipeline(
    reserve_sol: u64,
    reserve_token: u64,
    amount_in: u64,
    sell_tax_bps: u16,
) -> (u64, u64) {
    // Step 1: AMM swap -- LP fee then constant-product (token -> SOL)
    let effective = onchain_effective_input(amount_in, LP_FEE_BPS).unwrap();
    let gross_sol = onchain_swap_output(reserve_token, reserve_sol, effective).unwrap();

    // Step 2: Tax deducted from SOL output (on-chain line ~104 of swap_sol_sell)
    let tax = onchain_calculate_tax(gross_sol, sell_tax_bps).unwrap();
    let net_sol = gross_sol.checked_sub(tax).unwrap();

    (net_sol, tax)
}

// =============================================================================
// PART 1: Function-level parity (SDK functions == on-chain functions)
//
// These tests prove the SDK's copied math functions produce byte-identical
// results to the on-chain originals for all test inputs.
// =============================================================================

#[test]
fn parity_calculate_tax_matches_onchain() {
    let test_cases: Vec<(u64, u16)> = vec![
        (1_000_000_000, 400),    // 1 SOL, 4% tax
        (1_000_000_000, 1400),   // 1 SOL, 14% tax
        (10_000_000_000, 400),   // 10 SOL, 4% tax
        (10_000_000_000, 1400),  // 10 SOL, 14% tax
        (100, 400),              // dust
        (1, 400),                // 1 lamport
        (u64::MAX, 400),         // max amount
        (0, 400),                // zero
        (1_000_000_000, 0),      // zero tax
        (1_000_000_000, 10000),  // 100% tax
    ];

    for (amount, bps) in test_cases {
        let sdk_result = sdk_calculate_tax(amount, bps);
        let onchain_result = onchain_calculate_tax(amount, bps);
        assert_eq!(
            sdk_result, onchain_result,
            "calculate_tax mismatch for amount={}, bps={}",
            amount, bps
        );
    }
}

#[test]
fn parity_effective_input_matches_onchain() {
    let test_cases: Vec<(u64, u16)> = vec![
        (1_000_000_000, 100),   // 1 SOL, 1% LP fee
        (960_000_000, 100),     // post-tax amount
        (10_000_000_000, 100),  // 10 SOL
        (1, 100),               // dust
        (u64::MAX, 100),        // max
        (0, 100),               // zero
    ];

    for (amount, bps) in test_cases {
        let sdk_result = sdk_effective_input(amount, bps);
        let onchain_result = onchain_effective_input(amount, bps);
        assert_eq!(
            sdk_result, onchain_result,
            "calculate_effective_input mismatch for amount={}, bps={}",
            amount, bps
        );
    }
}

#[test]
fn parity_swap_output_matches_onchain() {
    let test_cases: Vec<(u64, u64, u128)> = vec![
        (100_000_000_000, 1_000_000_000_000_000_000, 950_400_000),   // realistic buy
        (1_000_000_000_000_000_000, 100_000_000_000, 9_504_000_000), // realistic sell
        (100_000_000_000, 1_000_000_000_000_000_000, 9_504_000_000_000), // 10 SOL buy
        (1_000_000, 1_000_000, 990),                                  // small equal reserves
        (u64::MAX, u64::MAX, 1000),                                   // max reserves
    ];

    for (reserve_in, reserve_out, effective) in test_cases {
        let sdk_result = sdk_swap_output(reserve_in, reserve_out, effective);
        let onchain_result = onchain_swap_output(reserve_in, reserve_out, effective);
        assert_eq!(
            sdk_result, onchain_result,
            "calculate_swap_output mismatch for r_in={}, r_out={}, eff={}",
            reserve_in, reserve_out, effective
        );
    }
}

// =============================================================================
// PART 2: Pipeline parity (SDK SolPoolAmm::quote() == on-chain handler pipeline)
//
// These tests step through the EXACT same sequence as the on-chain handler
// and prove the SDK's quote() produces identical output.
// =============================================================================

// Pool reserves: 100 SOL + 1B tokens (realistic mainnet scale)
const RESERVE_SOL: u64 = 100_000_000_000;          // 100 SOL
const RESERVE_TOKEN: u64 = 1_000_000_000_000_000_000; // 1B tokens (9 decimals)

const CRIME_BUY_TAX: u16 = 400;   // 4%
const CRIME_SELL_TAX: u16 = 400;   // 4%
const FRAUD_BUY_TAX: u16 = 1400;  // 14%
const FRAUD_SELL_TAX: u16 = 1400;  // 14%

// --- Buy parity tests ---

#[test]
fn parity_buy_crime_1sol() {
    let amount_in: u64 = 1_000_000_000; // 1 SOL
    let (onchain_output, _) = onchain_buy_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_BUY_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Buy CRIME 1 SOL: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

#[test]
fn parity_buy_crime_10sol() {
    let amount_in: u64 = 10_000_000_000; // 10 SOL
    let (onchain_output, _) = onchain_buy_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_BUY_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Buy CRIME 10 SOL: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

#[test]
fn parity_buy_fraud_1sol() {
    let amount_in: u64 = 1_000_000_000; // 1 SOL
    let (onchain_output, _) = onchain_buy_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, FRAUD_BUY_TAX,
    );

    let amm = make_sol_pool_amm(false, RESERVE_SOL, RESERVE_TOKEN, FRAUD_BUY_TAX, FRAUD_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: NATIVE_MINT,
        output_mint: FRAUD_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Buy FRAUD 1 SOL: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

#[test]
fn parity_buy_fraud_10sol() {
    let amount_in: u64 = 10_000_000_000; // 10 SOL
    let (onchain_output, _) = onchain_buy_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, FRAUD_BUY_TAX,
    );

    let amm = make_sol_pool_amm(false, RESERVE_SOL, RESERVE_TOKEN, FRAUD_BUY_TAX, FRAUD_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: NATIVE_MINT,
        output_mint: FRAUD_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Buy FRAUD 10 SOL: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

// --- Sell parity tests ---

#[test]
fn parity_sell_crime_1m_tokens() {
    let amount_in: u64 = 1_000_000_000_000_000; // 1M tokens (9 decimals)
    let (onchain_output, _) = onchain_sell_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_SELL_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Sell CRIME 1M: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

#[test]
fn parity_sell_crime_100m_tokens() {
    let amount_in: u64 = 100_000_000_000_000_000; // 100M tokens (9 decimals)
    let (onchain_output, _) = onchain_sell_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_SELL_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Sell CRIME 100M: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

#[test]
fn parity_sell_fraud_1m_tokens() {
    let amount_in: u64 = 1_000_000_000_000_000; // 1M tokens (9 decimals)
    let (onchain_output, _) = onchain_sell_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, FRAUD_SELL_TAX,
    );

    let amm = make_sol_pool_amm(false, RESERVE_SOL, RESERVE_TOKEN, FRAUD_BUY_TAX, FRAUD_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: FRAUD_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Sell FRAUD 1M: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

#[test]
fn parity_sell_fraud_100m_tokens() {
    let amount_in: u64 = 100_000_000_000_000_000; // 100M tokens (9 decimals)
    let (onchain_output, _) = onchain_sell_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, FRAUD_SELL_TAX,
    );

    let amm = make_sol_pool_amm(false, RESERVE_SOL, RESERVE_TOKEN, FRAUD_BUY_TAX, FRAUD_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: FRAUD_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Sell FRAUD 100M: SDK={}, on-chain={}, diff={}",
        sdk_quote.out_amount, onchain_output, sdk_quote.out_amount.abs_diff(onchain_output)
    );
}

// =============================================================================
// PART 3: Edge cases
// =============================================================================

#[test]
fn parity_buy_dust_amount() {
    // 100 lamports -- small enough that tax rounds to 0
    let amount_in: u64 = 100;
    let (onchain_output, _) = onchain_buy_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_BUY_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Buy CRIME dust: SDK={}, on-chain={}",
        sdk_quote.out_amount, onchain_output
    );
}

#[test]
fn parity_sell_dust_amount() {
    // 1000 token units -- very small sell
    let amount_in: u64 = 1000;
    let (onchain_output, _) = onchain_sell_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_SELL_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Sell CRIME dust: SDK={}, on-chain={}",
        sdk_quote.out_amount, onchain_output
    );
}

#[test]
fn parity_buy_max_tax_rate() {
    // 50% buy tax -- extreme edge case
    let amount_in: u64 = 1_000_000_000;
    let max_tax: u16 = 5000;
    let (onchain_output, _) = onchain_buy_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, max_tax,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, max_tax, max_tax);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Buy with 50% tax: SDK={}, on-chain={}",
        sdk_quote.out_amount, onchain_output
    );
}

#[test]
fn parity_sell_max_tax_rate() {
    // 50% sell tax -- extreme edge case
    let amount_in: u64 = 1_000_000_000_000_000;
    let max_tax: u16 = 5000;
    let (onchain_output, _) = onchain_sell_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, max_tax,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, max_tax, max_tax);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    assert_eq!(
        sdk_quote.out_amount, onchain_output,
        "Sell with 50% tax: SDK={}, on-chain={}",
        sdk_quote.out_amount, onchain_output
    );
}

// =============================================================================
// PART 4: Fee amount parity
//
// Verify that SDK's reported fee_amount matches the on-chain tax.
// =============================================================================

#[test]
fn parity_buy_fee_amount_matches_tax() {
    let amount_in: u64 = 1_000_000_000;
    let (_, onchain_tax) = onchain_buy_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_BUY_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    // SDK fee_amount includes both tax and LP fee (in SOL terms for buy)
    // On-chain tax is just the tax portion
    // Verify fee_amount >= tax (it includes LP fee as well)
    assert!(
        sdk_quote.fee_amount >= onchain_tax,
        "SDK fee_amount {} should be >= on-chain tax {}",
        sdk_quote.fee_amount, onchain_tax
    );

    // Verify the tax component is exact
    let expected_tax = sdk_calculate_tax(amount_in, CRIME_BUY_TAX).unwrap();
    assert_eq!(expected_tax, onchain_tax, "Tax calculation mismatch");
}

#[test]
fn parity_sell_fee_amount_matches_tax() {
    let amount_in: u64 = 1_000_000_000_000_000;
    let (_, onchain_tax) = onchain_sell_pipeline(
        RESERVE_SOL, RESERVE_TOKEN, amount_in, CRIME_SELL_TAX,
    );

    let amm = make_sol_pool_amm(true, RESERVE_SOL, RESERVE_TOKEN, CRIME_BUY_TAX, CRIME_SELL_TAX);
    let sdk_quote = amm.quote(&QuoteParams {
        amount: amount_in,
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).unwrap();

    // For sell, SDK fee_amount reports only the SOL tax (LP fee is in token terms)
    assert_eq!(
        sdk_quote.fee_amount, onchain_tax,
        "Sell fee_amount should equal on-chain tax: SDK={}, on-chain={}",
        sdk_quote.fee_amount, onchain_tax
    );
}
