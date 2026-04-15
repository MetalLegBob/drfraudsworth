//! Pure swap math functions for the Dr Fraudsworth AMM.
//!
//! This module contains ONLY pure functions operating on primitive types.
//! No anchor_lang, no solana_program, no Pubkey, no Account types.
//!
//! Why: Pure functions are testable without a Solana VM. This makes
//! proptest (10,000 iterations) run in milliseconds instead of minutes.
//!
//! Functions:
//! - calculate_effective_input: LP fee deduction (MATH-01)
//! - calculate_swap_output: Constant-product formula (MATH-02)
//! - verify_k_invariant: k_after >= k_before check (MATH-03)
//!
//! All arithmetic uses checked_add/checked_mul/checked_div/checked_sub.
//! Returns Option<T> -- never panics. None maps to AmmError::Overflow
//! in the instruction layer (not wired in this phase).
//!
//! Source: AMM_Implementation.md Section 8

/// Calculate effective input after LP fee deduction.
///
/// Formula: `amount_in * (10_000 - fee_bps) / 10_000`
///
/// # Arguments
/// * `amount_in` - Raw input amount in token base units (lamports/smallest denomination)
/// * `fee_bps` - Fee in basis points (e.g., 100 = 1.0%, 50 = 0.5%)
///
/// # Returns
/// * `Some(effective_input)` as u128 for downstream multiplication headroom
/// * `None` if fee_bps > 10_000 (underflow) or arithmetic overflow
///
/// # Examples
/// - fee_bps=100, amount=1000 -> Some(990)
/// - fee_bps=50, amount=1000 -> Some(995)
/// - fee_bps=10001 -> None (underflow on 10_000 - 10_001)
pub fn calculate_effective_input(amount_in: u64, fee_bps: u16) -> Option<u128> {
    let amount = amount_in as u128;
    let fee_factor = 10_000u128.checked_sub(fee_bps as u128)?;
    amount.checked_mul(fee_factor)?.checked_div(10_000)
}

/// Calculate swap output using constant-product formula.
///
/// Formula: `reserve_out * effective_input / (reserve_in + effective_input)`
///
/// Integer division truncates (rounds down), which is correct per
/// AMM_Implementation.md Section 8.2 -- the protocol keeps dust.
///
/// # Arguments
/// * `reserve_in` - Current reserve of the input token
/// * `reserve_out` - Current reserve of the output token
/// * `effective_input` - Post-fee input amount (from calculate_effective_input)
///
/// # Returns
/// * `Some(output)` as u64 -- amount of output token the swapper receives
/// * `None` if denominator is zero (both reserve_in and effective_input are 0),
///   arithmetic overflow, or output exceeds u64::MAX
pub fn calculate_swap_output(
    reserve_in: u64,
    reserve_out: u64,
    effective_input: u128,
) -> Option<u64> {
    let r_in = reserve_in as u128;
    let r_out = reserve_out as u128;

    let numerator = r_out.checked_mul(effective_input)?;
    let denominator = r_in.checked_add(effective_input)?;

    if denominator == 0 {
        return None;
    }

    let output = numerator.checked_div(denominator)?;

    u64::try_from(output).ok()
}

/// Verify the constant-product invariant: k_after >= k_before.
///
/// k = reserve_in * reserve_out (computed in u128 to avoid overflow).
/// A valid swap must never decrease k. Integer truncation in the output
/// calculation means k typically increases slightly (protocol profit).
///
/// # Arguments
/// * `reserve_in_before` / `reserve_out_before` - Pool reserves before swap
/// * `reserve_in_after` / `reserve_out_after` - Pool reserves after swap
///
/// # Returns
/// * `Some(true)` if k_after >= k_before (valid swap)
/// * `Some(false)` if k_after < k_before (invalid -- funds drained)
/// * `None` if u128 multiplication overflows
pub fn verify_k_invariant(
    reserve_in_before: u64,
    reserve_out_before: u64,
    reserve_in_after: u64,
    reserve_out_after: u64,
) -> Option<bool> {
    let k_before = (reserve_in_before as u128)
        .checked_mul(reserve_out_before as u128)?;
    let k_after = (reserve_in_after as u128)
        .checked_mul(reserve_out_after as u128)?;
    Some(k_after >= k_before)
}

/// Check that effective input is non-zero when amount_in is non-zero.
/// Returns false if effective_input == 0 && amount_in > 0 (fee ate everything).
///
/// # Arguments
/// * `amount_in` - Raw input amount (pre-fee)
/// * `effective_input` - Post-fee input amount (from calculate_effective_input)
///
/// # Returns
/// * `true` if safe to proceed (either effective_input > 0, or amount_in was 0)
/// * `false` if fee consumed entire input (user would burn tokens for nothing)
pub fn check_effective_input_nonzero(amount_in: u64, effective_input: u128) -> bool {
    !(effective_input == 0 && amount_in > 0)
}

/// Check that swap output is non-zero when effective_input is non-zero.
/// Returns false if amount_out == 0 && effective_input > 0 (swap produced nothing).
///
/// # Arguments
/// * `effective_input` - Post-fee input amount
/// * `amount_out` - Computed output amount (from calculate_swap_output)
///
/// # Returns
/// * `true` if safe to proceed (either amount_out > 0, or effective_input was 0)
/// * `false` if swap math produced zero output (user would burn tokens for nothing)
pub fn check_swap_output_nonzero(effective_input: u128, amount_out: u64) -> bool {
    !(amount_out == 0 && effective_input > 0)
}

/// Calculate withdrawal amounts for both sides of a pool.
///
/// Returns `(amount_a, amount_b)` proportional to `withdraw_bps`.
/// Uses u128 intermediates to prevent overflow (same pattern as `calculate_effective_input`).
///
/// The 5000 BPS (50%) cap is hardcoded here to keep this module dependency-free
/// (no `crate::constants` import). The same limit is exported as `MAX_WITHDRAW_BPS`
/// in `constants.rs` for instruction-level validation.
///
/// # Arguments
/// * `reserve_a` - Current reserve of token A
/// * `reserve_b` - Current reserve of token B
/// * `withdraw_bps` - Withdrawal fraction in basis points (max 5000 = 50%)
///
/// # Returns
/// * `Some((amount_a, amount_b))` - Proportional withdrawal amounts (may be 0 if reserve is tiny)
/// * `None` if `withdraw_bps` is 0, exceeds 5000, or arithmetic overflows
///
/// # Examples
/// - reserve_a=1_000_000, reserve_b=2_000_000, bps=5000 -> Some((500_000, 1_000_000))
/// - reserve_a=1, reserve_b=1_000_000, bps=100 -> Some((0, 10_000))  // truncation on small reserve
/// - bps=5001 -> None (exceeds 50% cap)
/// - bps=0 -> None (no-op)
pub fn calculate_withdraw_amounts(
    reserve_a: u64,
    reserve_b: u64,
    withdraw_bps: u16,
) -> Option<(u64, u64)> {
    if withdraw_bps == 0 || withdraw_bps > 5000 {
        return None;
    }
    let bps = withdraw_bps as u128;
    let amount_a = (reserve_a as u128).checked_mul(bps)?.checked_div(10_000)?;
    let amount_b = (reserve_b as u128).checked_mul(bps)?.checked_div(10_000)?;
    Some((u64::try_from(amount_a).ok()?, u64::try_from(amount_b).ok()?))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Part A: Hand-picked unit tests
    // =========================================================================

    // ---- Fee calculation tests (calculate_effective_input) -------------------

    #[test]
    fn fee_100bps_on_1000() {
        // 1% fee on 1000: effective = 1000 * 9900 / 10000 = 990
        assert_eq!(calculate_effective_input(1000, 100), Some(990));
    }

    #[test]
    fn fee_50bps_on_1000() {
        // 0.5% fee on 1000: effective = 1000 * 9950 / 10000 = 995
        assert_eq!(calculate_effective_input(1000, 50), Some(995));
    }

    #[test]
    fn fee_zero_bps() {
        // 0% fee: full amount passes through
        assert_eq!(calculate_effective_input(1000, 0), Some(1000));
    }

    #[test]
    fn fee_10000_bps() {
        // 100% fee: zero effective input (entire amount taken as fee)
        assert_eq!(calculate_effective_input(1000, 10000), Some(0));
    }

    #[test]
    fn fee_over_10000_bps() {
        // fee_bps > 10000 causes underflow in (10_000 - fee_bps) -> None
        assert_eq!(calculate_effective_input(1000, 10001), None);
    }

    #[test]
    fn fee_on_zero_amount() {
        // Zero input with any valid fee -> zero effective input
        assert_eq!(calculate_effective_input(0, 100), Some(0));
    }

    #[test]
    fn fee_on_one() {
        // 1 * 9900 / 10000 = 0 (integer truncation)
        // Why: With 100 bps fee, a single lamport rounds to zero effective input.
        // This is correct behavior -- dust amounts below fee threshold produce nothing.
        assert_eq!(calculate_effective_input(1, 100), Some(0));
    }

    #[test]
    fn fee_on_u64_max() {
        // u64::MAX * 9900 fits in u128 (no overflow)
        // u64::MAX = 18_446_744_073_709_551_615
        // * 9900 = 182_622_766_329_724_560_988_500 (well within u128 range)
        let result = calculate_effective_input(u64::MAX, 100);
        assert!(result.is_some(), "u64::MAX with 100 bps fee must not overflow");
        // Verify the exact value: u64::MAX * 9900 / 10000
        let expected = (u64::MAX as u128) * 9900 / 10000;
        assert_eq!(result, Some(expected));
    }

    // ---- Swap output tests (calculate_swap_output) --------------------------

    #[test]
    fn swap_equal_reserves_1m() {
        // reserve_in=1M, reserve_out=1M, effective=1000
        // output = 1_000_000 * 1000 / (1_000_000 + 1000)
        //        = 1_000_000_000 / 1_001_000
        //        = 999 (integer truncation)
        assert_eq!(calculate_swap_output(1_000_000, 1_000_000, 1000), Some(999));
    }

    #[test]
    fn swap_zero_effective_input() {
        // Zero input -> zero output (valid, not an error)
        assert_eq!(calculate_swap_output(1_000_000, 1_000_000, 0), Some(0));
    }

    #[test]
    fn swap_zero_reserve_out() {
        // No output tokens available -> output is always 0
        assert_eq!(calculate_swap_output(1_000_000, 0, 1000), Some(0));
    }

    #[test]
    fn swap_zero_reserve_in_zero_effective() {
        // Both reserve_in=0 and effective_input=0 -> denominator=0 -> None
        assert_eq!(calculate_swap_output(0, 1_000_000, 0), None);
    }

    #[test]
    fn swap_zero_reserve_in_nonzero_effective() {
        // reserve_in=0, effective=1000 -> denominator=1000
        // output = 1_000_000 * 1000 / (0 + 1000) = 1_000_000
        // Swapper gets ALL of reserve_out (pool was empty on input side)
        assert_eq!(calculate_swap_output(0, 1_000_000, 1000), Some(1_000_000));
    }

    #[test]
    fn swap_large_input_relative_to_reserve() {
        // Even with very large input, output cannot exceed reserve_out
        // reserve_in=1000, reserve_out=1000, effective=1_000_000_000
        // output = 1000 * 1_000_000_000 / (1000 + 1_000_000_000)
        //        = 1_000_000_000_000 / 1_000_001_000
        //        = 999 (approximately)
        let output = calculate_swap_output(1000, 1000, 1_000_000_000);
        assert!(output.is_some());
        let output = output.unwrap();
        assert!(
            output < 1000,
            "Output {} must be less than reserve_out 1000",
            output
        );
    }

    #[test]
    fn swap_u64_max_reserves_small_input() {
        // u64::MAX reserves with small effective input
        // Must handle u128 arithmetic without overflow
        let output = calculate_swap_output(u64::MAX, u64::MAX, 1000);
        assert!(
            output.is_some(),
            "u64::MAX reserves with small input must not overflow"
        );
        // Output should be very small relative to reserves
        let output = output.unwrap();
        assert!(output <= 1000, "Output {} should be <= effective input 1000", output);
    }

    #[test]
    fn swap_output_cannot_exceed_u64() {
        // Craft a scenario where u128 output would exceed u64::MAX
        // This is hard to trigger with valid inputs since output < reserve_out
        // and reserve_out is u64. But verify the u64::try_from guard works.
        // reserve_in=0, reserve_out=u64::MAX, effective=u64::MAX as u128 + 1
        // output = u64::MAX * (u64::MAX + 1) / (0 + u64::MAX + 1) = u64::MAX
        // This should fit in u64.
        let effective = u64::MAX as u128 + 1;
        let output = calculate_swap_output(0, u64::MAX, effective);
        // denominator = 0 + (u64::MAX + 1) = 2^64
        // numerator = u64::MAX * 2^64 = (2^64-1) * 2^64
        // output = (2^64-1) * 2^64 / 2^64 = 2^64 - 1 = u64::MAX
        assert_eq!(output, Some(u64::MAX));
    }

    // ---- k-invariant tests (verify_k_invariant) -----------------------------

    #[test]
    fn k_valid_swap() {
        // Before: 1M * 1M = 1_000_000_000_000
        // After swap of 1000 in, 999 out: 1_001_000 * 999_001
        // k_after = 1_001_000 * 999_001 = 1_000_000_999_001 > 1_000_000_000_000
        assert_eq!(
            verify_k_invariant(1_000_000, 1_000_000, 1_001_000, 999_001),
            Some(true)
        );
    }

    #[test]
    fn k_invalid_swap() {
        // After: 1_001_000 * 998_000 = 998_998_000_000 < 1_000_000_000_000
        // k decreased -> invalid
        assert_eq!(
            verify_k_invariant(1_000_000, 1_000_000, 1_001_000, 998_000),
            Some(false)
        );
    }

    #[test]
    fn k_equal_reserves() {
        // k unchanged -> valid (Some(true))
        assert_eq!(
            verify_k_invariant(1_000_000, 1_000_000, 1_000_000, 1_000_000),
            Some(true)
        );
    }

    #[test]
    fn k_u64_max_both_sides() {
        // u64::MAX * u64::MAX fits in u128:
        // (2^64-1)^2 = 2^128 - 2^65 + 1
        // u128::MAX  = 2^128 - 1
        // So (2^64-1)^2 < 2^128 - 1, fits.
        assert_eq!(
            verify_k_invariant(u64::MAX, u64::MAX, u64::MAX, u64::MAX),
            Some(true)
        );
    }

    #[test]
    fn k_zero_before_nonzero_after() {
        // Before: 0 * 1_000_000 = 0
        // After: 1000 * 999_000 = 999_000_000
        // k increased from 0 -> valid
        assert_eq!(
            verify_k_invariant(0, 1_000_000, 1000, 999_000),
            Some(true)
        );
    }

    #[test]
    fn k_nonzero_before_zero_after() {
        // Before: 1_000_000 * 1_000_000 = 1e12
        // After: 1_001_000 * 0 = 0
        // Pool drained -> k decreased -> invalid
        assert_eq!(
            verify_k_invariant(1_000_000, 1_000_000, 1_001_000, 0),
            Some(false)
        );
    }

    // =========================================================================
    // Part B: Proptest property-based tests (10,000 iterations each)
    // =========================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Strategy for pool reserve values.
        /// 90% realistic range (1 lamport to 1 billion tokens with 9 decimals),
        /// 10% edge cases (0, 1, u64::MAX, u64::MAX/2).
        fn reserve_strategy() -> impl Strategy<Value = u64> {
            prop_oneof![
                9 => 1u64..=1_000_000_000_000_000_000u64,
                1 => prop::sample::select(vec![0u64, 1, u64::MAX, u64::MAX / 2]),
            ]
        }

        /// Strategy for swap input amounts.
        /// 90% realistic range (up to 100M tokens),
        /// 10% edge cases.
        fn swap_amount_strategy() -> impl Strategy<Value = u64> {
            prop_oneof![
                9 => 1u64..=100_000_000_000_000_000u64,
                1 => prop::sample::select(vec![0u64, 1, u64::MAX, u64::MAX / 2]),
            ]
        }

        /// Strategy for fee basis points.
        /// 70% valid range (0-9999), 20% protocol fee (50 bps), 10% protocol fee (100 bps).
        fn fee_bps_strategy() -> impl Strategy<Value = u16> {
            prop_oneof![
                7 => 0u16..=9999u16,
                2 => Just(50u16),
                1 => Just(100u16),
            ]
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(10_000))]

            /// Property 1: k-invariant holds for all valid swaps.
            ///
            /// For any random (reserve_in, reserve_out, amount_in, fee_bps),
            /// if we can compute a valid swap output, the k-invariant must hold.
            /// This is the fundamental safety property of a constant-product AMM.
            #[test]
            fn k_invariant_holds_for_valid_swaps(
                reserve_in in reserve_strategy(),
                reserve_out in reserve_strategy(),
                amount_in in swap_amount_strategy(),
                fee_bps in fee_bps_strategy(),
            ) {
                // Skip degenerate cases that don't represent valid swaps
                prop_assume!(amount_in > 0);
                prop_assume!(reserve_in > 0);
                prop_assume!(reserve_out > 0);

                // Calculate effective input after fee
                let effective = match calculate_effective_input(amount_in, fee_bps) {
                    Some(e) => e,
                    None => return Ok(()),  // Skip if fee calc overflows
                };

                // Calculate swap output
                let output = match calculate_swap_output(reserve_in, reserve_out, effective) {
                    Some(o) => o,
                    None => return Ok(()),  // Skip if swap calc overflows
                };

                // Skip if output would drain the pool
                prop_assume!(output < reserve_out);

                // Calculate new reserves
                // Note: amount_in (pre-fee) is added to reserve_in
                let new_reserve_in = match reserve_in.checked_add(amount_in) {
                    Some(r) => r,
                    None => return Ok(()),  // Skip if reserve addition overflows u64
                };
                let new_reserve_out = reserve_out - output;  // Safe: output < reserve_out

                // THE PROPERTY: k must never decrease
                let k_ok = verify_k_invariant(
                    reserve_in, reserve_out,
                    new_reserve_in, new_reserve_out,
                );

                prop_assert_eq!(
                    k_ok,
                    Some(true),
                    "k-invariant violated! reserve_in={}, reserve_out={}, amount_in={}, fee_bps={}, output={}, new_in={}, new_out={}",
                    reserve_in, reserve_out, amount_in, fee_bps, output, new_reserve_in, new_reserve_out
                );
            }

            /// Property 2: Swap output never exceeds reserve_out.
            ///
            /// No matter what effective_input is, the swapper cannot receive
            /// more tokens than exist in the output reserve. This prevents
            /// pool draining attacks.
            #[test]
            fn output_never_exceeds_reserve_out(
                reserve_in in reserve_strategy(),
                reserve_out in reserve_strategy(),
                effective_input_raw in swap_amount_strategy(),
            ) {
                let effective_input = effective_input_raw as u128;

                if let Some(output) = calculate_swap_output(reserve_in, reserve_out, effective_input) {
                    prop_assert!(
                        output <= reserve_out,
                        "Output {} exceeds reserve_out {}! reserve_in={}, effective_input={}",
                        output, reserve_out, reserve_in, effective_input
                    );
                }
                // If None, the function correctly refused to compute (overflow/div-by-zero)
            }

            /// Property 3: Fee calculation is monotonic.
            ///
            /// Higher fee_bps must always produce less-or-equal effective input.
            /// This ensures the fee mechanism cannot be exploited by choosing
            /// a "wrong" fee tier.
            #[test]
            fn fee_calculation_is_monotonic(
                amount_in in swap_amount_strategy(),
                fee_bps_low in 0u16..=9999u16,
                fee_bps_delta in 0u16..=500u16,
            ) {
                let fee_bps_high = fee_bps_low.saturating_add(fee_bps_delta).min(10000);

                if let (Some(effective_low), Some(effective_high)) = (
                    calculate_effective_input(amount_in, fee_bps_low),
                    calculate_effective_input(amount_in, fee_bps_high),
                ) {
                    prop_assert!(
                        effective_low >= effective_high,
                        "Monotonicity violated! fee_low={} -> {}, fee_high={} -> {}",
                        fee_bps_low, effective_low, fee_bps_high, effective_high
                    );
                }
            }

            /// Property 4: Withdrawal amounts never exceed reserves.
            ///
            /// For any valid BPS (1..=5000) and any reserve values,
            /// the withdrawn amount must be <= the reserve. This prevents
            /// the Rebalancer from extracting more than exists in the pool.
            #[test]
            fn withdraw_never_exceeds_reserves(
                reserve_a in reserve_strategy(),
                reserve_b in reserve_strategy(),
                bps in 1u16..=5000u16,
            ) {
                if let Some((a, b)) = calculate_withdraw_amounts(reserve_a, reserve_b, bps) {
                    prop_assert!(a <= reserve_a, "withdraw_a {} > reserve_a {}", a, reserve_a);
                    prop_assert!(b <= reserve_b, "withdraw_b {} > reserve_b {}", b, reserve_b);
                }
            }
        }
    }

    // =========================================================================
    // Part C: Withdrawal math tests (calculate_withdraw_amounts)
    // =========================================================================

    #[test]
    fn withdraw_50pct() {
        let (a, b) = calculate_withdraw_amounts(1_000_000, 2_000_000, 5000).unwrap();
        assert_eq!(a, 500_000);
        assert_eq!(b, 1_000_000);
    }

    #[test]
    fn withdraw_10pct() {
        let (a, b) = calculate_withdraw_amounts(1_000_000, 2_000_000, 1000).unwrap();
        assert_eq!(a, 100_000);
        assert_eq!(b, 200_000);
    }

    #[test]
    fn withdraw_exceeds_max() {
        assert!(calculate_withdraw_amounts(1_000_000, 2_000_000, 5001).is_none());
    }

    #[test]
    fn withdraw_zero_bps() {
        assert!(calculate_withdraw_amounts(1_000_000, 2_000_000, 0).is_none());
    }

    #[test]
    fn withdraw_truncation_small_reserve() {
        // 1 * 100 / 10000 = 0 (truncated)
        let (a, b) = calculate_withdraw_amounts(1, 1_000_000, 100).unwrap();
        assert_eq!(a, 0);
        assert_eq!(b, 10_000);
    }

    #[test]
    fn withdraw_u64_max_reserves() {
        // Must not overflow in u128
        let result = calculate_withdraw_amounts(u64::MAX, u64::MAX, 5000);
        assert!(result.is_some());
        let (a, b) = result.unwrap();
        // u64::MAX * 5000 / 10000 = u64::MAX / 2 (integer division)
        assert_eq!(a, u64::MAX / 2);
        assert_eq!(b, u64::MAX / 2);
    }

    #[test]
    fn withdraw_zero_reserves() {
        let (a, b) = calculate_withdraw_amounts(0, 0, 5000).unwrap();
        assert_eq!(a, 0);
        assert_eq!(b, 0);
    }
}
