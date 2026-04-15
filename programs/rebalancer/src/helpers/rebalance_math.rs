//! Pure math functions for the Rebalancer program.
//!
//! NO anchor_lang dependency -- keeps tests fast and the module dependency-free.
//! Functions operate on primitive types only, return Option<T>, never panic.
//!
//! Adapted from tax-program/src/helpers/tax_math.rs for distribution split.

// ---------------------------------------------------------------------------
// Distribution BPS constants (inline -- no crate dependency)
// ---------------------------------------------------------------------------

/// Staking escrow: 71% (7100 bps).
const STAKING_BPS: u128 = 7_100;

/// Carnage fund: 24% (2400 bps).
const CARNAGE_BPS: u128 = 2_400;

/// BPS denominator (10,000 = 100%).
const BPS_DENOM: u128 = 10_000;

/// Micro-tax threshold: below this, all goes to staking.
const MICRO_TAX_THRESHOLD: u64 = 4;

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Split a SOL amount into (staking, carnage, treasury) portions.
///
/// Distribution: 71% staking (floor), 24% carnage (floor), remainder to treasury.
/// Below MICRO_TAX_THRESHOLD (4 lamports), all goes to staking to avoid dust.
///
/// # Invariant
/// staking + carnage + treasury == total (always)
///
/// # Returns
/// `Some((staking, carnage, treasury))` or `None` on overflow.
pub fn split_distribution(total: u64) -> Option<(u64, u64, u64)> {
    if total < MICRO_TAX_THRESHOLD {
        return Some((total, 0, 0));
    }

    let t = total as u128;
    let staking = u64::try_from(t.checked_mul(STAKING_BPS)?.checked_div(BPS_DENOM)?).ok()?;
    let carnage = u64::try_from(t.checked_mul(CARNAGE_BPS)?.checked_div(BPS_DENOM)?).ok()?;
    let treasury = total.checked_sub(staking)?.checked_sub(carnage)?;

    Some((staking, carnage, treasury))
}

/// Calculate allocation delta in basis points.
///
/// Measures how far the current SOL allocation deviates from the target.
/// Positive = SOL overweight, negative = USDC overweight.
///
/// # Arguments
/// * `sol_value_usd` - USD value of all SOL-denominated pool liquidity
/// * `usdc_value` - USD value of all USDC-denominated pool liquidity
/// * `target_bps` - Target SOL allocation in BPS (5000 = 50%)
///
/// # Returns
/// Signed delta in BPS. 0 when perfectly balanced or total is 0.
pub fn calculate_delta_bps(
    sol_value_usd: u64,
    usdc_value: u64,
    target_bps: u16,
) -> Option<i32> {
    let total = (sol_value_usd as u128).checked_add(usdc_value as u128)?;
    if total == 0 {
        return Some(0);
    }
    let sol_bps = (sol_value_usd as u128)
        .checked_mul(10_000)?
        .checked_div(total)? as i32;
    Some(sol_bps - target_bps as i32)
}

/// Check whether a USDC->SOL conversion cost exceeds the configured ceiling.
///
/// cost_bps = (in_amount - out_value) * 10000 / in_amount
///
/// # Arguments
/// * `in_amount` - USDC sent to Jupiter (6 decimals)
/// * `out_value` - USD-equivalent value of SOL received
/// * `ceiling_bps` - Maximum acceptable cost in BPS
///
/// # Returns
/// `Some(true)` if cost exceeds ceiling, `Some(false)` otherwise.
/// Returns `Some(false)` if in_amount is 0 (no conversion).
/// Returns `None` if out_value > in_amount (negative cost -- should not happen).
pub fn cost_exceeds_ceiling(
    in_amount: u64,
    out_value: u64,
    ceiling_bps: u16,
) -> Option<bool> {
    if in_amount == 0 {
        return Some(false);
    }
    let cost = (in_amount as u128).checked_sub(out_value as u128)?;
    let cost_bps = cost.checked_mul(10_000)?.checked_div(in_amount as u128)?;
    Some(cost_bps > ceiling_bps as u128)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // split_distribution unit tests
    // =========================================================================

    #[test]
    fn split_100_lamports() {
        assert_eq!(split_distribution(100), Some((71, 24, 5)));
    }

    #[test]
    fn split_1000_lamports() {
        assert_eq!(split_distribution(1000), Some((710, 240, 50)));
    }

    #[test]
    fn split_10_lamports() {
        // floor(10*0.71)=7, floor(10*0.24)=2, 10-7-2=1
        assert_eq!(split_distribution(10), Some((7, 2, 1)));
    }

    #[test]
    fn split_micro_tax_3() {
        assert_eq!(split_distribution(3), Some((3, 0, 0)));
    }

    #[test]
    fn split_micro_tax_1() {
        assert_eq!(split_distribution(1), Some((1, 0, 0)));
    }

    #[test]
    fn split_zero() {
        assert_eq!(split_distribution(0), Some((0, 0, 0)));
    }

    #[test]
    fn split_4_boundary() {
        // 4 is >= threshold, normal split
        assert_eq!(split_distribution(4), Some((2, 0, 2)));
    }

    #[test]
    fn split_max_u64() {
        let result = split_distribution(u64::MAX);
        assert!(result.is_some());
        let (s, c, t) = result.unwrap();
        assert_eq!(s.checked_add(c).and_then(|x| x.checked_add(t)), Some(u64::MAX));
    }

    // =========================================================================
    // calculate_delta_bps unit tests
    // =========================================================================

    #[test]
    fn delta_balanced() {
        // 50/50 with target 50% -> delta 0
        assert_eq!(calculate_delta_bps(500, 500, 5000), Some(0));
    }

    #[test]
    fn delta_sol_overweight() {
        // 70% SOL with target 50% -> +2000 bps
        assert_eq!(calculate_delta_bps(700, 300, 5000), Some(2000));
    }

    #[test]
    fn delta_usdc_overweight() {
        // 30% SOL with target 50% -> -2000 bps
        assert_eq!(calculate_delta_bps(300, 700, 5000), Some(-2000));
    }

    #[test]
    fn delta_zero_total() {
        assert_eq!(calculate_delta_bps(0, 0, 5000), Some(0));
    }

    #[test]
    fn delta_all_sol() {
        // 100% SOL with target 50% -> +5000 bps
        assert_eq!(calculate_delta_bps(1000, 0, 5000), Some(5000));
    }

    #[test]
    fn delta_all_usdc() {
        // 0% SOL with target 50% -> -5000 bps
        assert_eq!(calculate_delta_bps(0, 1000, 5000), Some(-5000));
    }

    #[test]
    fn delta_large_values() {
        let result = calculate_delta_bps(u64::MAX / 2, u64::MAX / 2, 5000);
        assert!(result.is_some());
        // Approximately balanced -> delta near 0
        assert!(result.unwrap().abs() <= 1);
    }

    // =========================================================================
    // cost_exceeds_ceiling unit tests
    // =========================================================================

    #[test]
    fn cost_zero_input() {
        assert_eq!(cost_exceeds_ceiling(0, 0, 50), Some(false));
    }

    #[test]
    fn cost_no_loss() {
        // out_value == in_amount -> 0 cost
        assert_eq!(cost_exceeds_ceiling(1000, 1000, 50), Some(false));
    }

    #[test]
    fn cost_gain() {
        // out_value > in_amount -> negative cost (None because checked_sub fails)
        assert_eq!(cost_exceeds_ceiling(1000, 1100, 50), None);
    }

    #[test]
    fn cost_below_ceiling() {
        // 10 / 10000 * 10000 = 100 bps cost, ceiling 200 -> false
        assert_eq!(cost_exceeds_ceiling(10000, 9900, 200), Some(false));
    }

    #[test]
    fn cost_above_ceiling() {
        // 500 / 10000 * 10000 = 500 bps cost, ceiling 50 -> true
        assert_eq!(cost_exceeds_ceiling(10000, 9500, 50), Some(true));
    }

    #[test]
    fn cost_exactly_at_ceiling() {
        // 50 / 10000 * 10000 = 50 bps cost, ceiling 50 -> false (not strictly above)
        assert_eq!(cost_exceeds_ceiling(10000, 9950, 50), Some(false));
    }

    // =========================================================================
    // Proptest property-based tests
    // =========================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(10_000))]

            /// Conservation law: staking + carnage + treasury == total
            #[test]
            fn split_sum_equals_input(total in 0u64..=u64::MAX) {
                if let Some((staking, carnage, treasury)) = split_distribution(total) {
                    let sum = staking.saturating_add(carnage).saturating_add(treasury);
                    prop_assert_eq!(sum, total);
                }
            }

            /// Each component is within expected BPS range
            #[test]
            fn split_components_in_range(total in 4u64..=u64::MAX) {
                if let Some((staking, carnage, _treasury)) = split_distribution(total) {
                    // Staking should be ~71% (allow floor rounding)
                    let min_staking = (total as u128 * 7000 / 10000) as u64;
                    let max_staking = (total as u128 * 7200 / 10000) as u64;
                    prop_assert!(staking >= min_staking, "staking {} < min {}", staking, min_staking);
                    prop_assert!(staking <= max_staking, "staking {} > max {}", staking, max_staking);

                    // Carnage should be ~24% (allow floor rounding)
                    let min_carnage = (total as u128 * 2300 / 10000) as u64;
                    let max_carnage = (total as u128 * 2500 / 10000) as u64;
                    prop_assert!(carnage >= min_carnage, "carnage {} < min {}", carnage, min_carnage);
                    prop_assert!(carnage <= max_carnage, "carnage {} > max {}", carnage, max_carnage);
                }
            }

            /// Micro-tax sends all to staking
            #[test]
            fn split_micro_tax(total in 0u64..4u64) {
                if let Some((staking, carnage, treasury)) = split_distribution(total) {
                    prop_assert_eq!(staking, total);
                    prop_assert_eq!(carnage, 0);
                    prop_assert_eq!(treasury, 0);
                }
            }

            /// Balanced allocation returns delta 0
            #[test]
            fn delta_balanced_is_zero(value in 1u64..=1_000_000_000u64) {
                let delta = calculate_delta_bps(value, value, 5000);
                prop_assert_eq!(delta, Some(0));
            }

            /// SOL overweight gives positive delta
            #[test]
            fn delta_positive_when_sol_heavy(
                sol in 501u64..=10_000u64,
                usdc in 1u64..=500u64,
            ) {
                if let Some(delta) = calculate_delta_bps(sol, usdc, 5000) {
                    prop_assert!(delta > 0, "Expected positive delta, got {}", delta);
                }
            }

            /// USDC overweight gives negative delta
            #[test]
            fn delta_negative_when_usdc_heavy(
                sol in 1u64..=500u64,
                usdc in 501u64..=10_000u64,
            ) {
                if let Some(delta) = calculate_delta_bps(sol, usdc, 5000) {
                    prop_assert!(delta < 0, "Expected negative delta, got {}", delta);
                }
            }

            /// No-loss conversion never exceeds ceiling
            #[test]
            fn cost_no_loss_never_exceeds(
                amount in 1u64..=u64::MAX,
                ceiling in 0u16..=10000u16,
            ) {
                // out_value == amount -> 0 cost -> never exceeds
                if let Some(exceeds) = cost_exceeds_ceiling(amount, amount, ceiling) {
                    prop_assert!(!exceeds, "0% cost should never exceed ceiling");
                }
            }

            /// Total loss always exceeds non-zero ceiling check:
            /// Actually, total loss = 10000 bps cost, which exceeds any ceiling < 10000.
            #[test]
            fn cost_total_loss_exceeds_reasonable_ceiling(
                amount in 1u64..=u64::MAX,
                ceiling in 0u16..=9999u16,
            ) {
                if let Some(exceeds) = cost_exceeds_ceiling(amount, 0, ceiling) {
                    prop_assert!(exceeds, "100% cost should exceed ceiling {}", ceiling);
                }
            }
        }
    }
}
