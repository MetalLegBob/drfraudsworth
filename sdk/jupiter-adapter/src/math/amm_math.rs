// Source: programs/amm/src/helpers/math.rs -- MUST stay in sync
//
// Exact copy of pure swap math functions from the on-chain AMM program.
// These functions operate on primitives only (no Solana deps).

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
pub fn calculate_effective_input(amount_in: u64, fee_bps: u16) -> Option<u128> {
    let amount = amount_in as u128;
    let fee_factor = 10_000u128.checked_sub(fee_bps as u128)?;
    amount.checked_mul(fee_factor)?.checked_div(10_000)
}

/// Calculate swap output using constant-product formula.
///
/// Formula: `reserve_out * effective_input / (reserve_in + effective_input)`
///
/// Integer division truncates (rounds down) -- the protocol keeps dust.
///
/// # Arguments
/// * `reserve_in` - Current reserve of the input token
/// * `reserve_out` - Current reserve of the output token
/// * `effective_input` - Post-fee input amount (from calculate_effective_input)
///
/// # Returns
/// * `Some(output)` as u64
/// * `None` if denominator is zero, arithmetic overflow, or output exceeds u64::MAX
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_100bps_on_1000() {
        assert_eq!(calculate_effective_input(1000, 100), Some(990));
    }

    #[test]
    fn fee_50bps_on_1000() {
        assert_eq!(calculate_effective_input(1000, 50), Some(995));
    }

    #[test]
    fn fee_zero_bps() {
        assert_eq!(calculate_effective_input(1000, 0), Some(1000));
    }

    #[test]
    fn fee_10000_bps() {
        assert_eq!(calculate_effective_input(1000, 10000), Some(0));
    }

    #[test]
    fn fee_over_10000_bps() {
        assert_eq!(calculate_effective_input(1000, 10001), None);
    }

    #[test]
    fn fee_on_zero_amount() {
        assert_eq!(calculate_effective_input(0, 100), Some(0));
    }

    #[test]
    fn fee_on_one() {
        assert_eq!(calculate_effective_input(1, 100), Some(0));
    }

    #[test]
    fn fee_on_u64_max() {
        let result = calculate_effective_input(u64::MAX, 100);
        assert!(result.is_some());
        let expected = (u64::MAX as u128) * 9900 / 10000;
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn swap_equal_reserves_1m() {
        assert_eq!(calculate_swap_output(1_000_000, 1_000_000, 1000), Some(999));
    }

    #[test]
    fn swap_zero_effective_input() {
        assert_eq!(calculate_swap_output(1_000_000, 1_000_000, 0), Some(0));
    }

    #[test]
    fn swap_zero_reserve_out() {
        assert_eq!(calculate_swap_output(1_000_000, 0, 1000), Some(0));
    }

    #[test]
    fn swap_zero_reserve_in_zero_effective() {
        assert_eq!(calculate_swap_output(0, 1_000_000, 0), None);
    }

    #[test]
    fn swap_zero_reserve_in_nonzero_effective() {
        assert_eq!(calculate_swap_output(0, 1_000_000, 1000), Some(1_000_000));
    }

    #[test]
    fn swap_large_input_relative_to_reserve() {
        let output = calculate_swap_output(1000, 1000, 1_000_000_000);
        assert!(output.is_some());
        assert!(output.unwrap() < 1000);
    }

    #[test]
    fn swap_u64_max_reserves_small_input() {
        let output = calculate_swap_output(u64::MAX, u64::MAX, 1000);
        assert!(output.is_some());
        assert!(output.unwrap() <= 1000);
    }

    #[test]
    fn swap_output_cannot_exceed_u64() {
        let effective = u64::MAX as u128 + 1;
        let output = calculate_swap_output(0, u64::MAX, effective);
        assert_eq!(output, Some(u64::MAX));
    }

    #[test]
    fn k_valid_swap() {
        assert_eq!(
            verify_k_invariant(1_000_000, 1_000_000, 1_001_000, 999_001),
            Some(true)
        );
    }

    #[test]
    fn k_invalid_swap() {
        assert_eq!(
            verify_k_invariant(1_000_000, 1_000_000, 1_001_000, 998_000),
            Some(false)
        );
    }

    #[test]
    fn k_equal_reserves() {
        assert_eq!(
            verify_k_invariant(1_000_000, 1_000_000, 1_000_000, 1_000_000),
            Some(true)
        );
    }
}
