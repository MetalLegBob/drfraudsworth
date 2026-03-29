// Source: programs/tax-program/src/helpers/tax_math.rs -- MUST stay in sync
//
// Exact copy of pure tax calculation functions from the on-chain Tax Program.
// These functions operate on primitives only (no Solana deps).

/// Calculate tax amount from a lamport value and tax rate in basis points.
///
/// Formula: `amount_lamports * tax_bps / 10_000`
///
/// # Arguments
/// * `amount_lamports` - Amount to tax (in lamports)
/// * `tax_bps` - Tax rate in basis points (e.g., 400 = 4%, 10000 = 100%)
///
/// # Returns
/// * `Some(tax)` - Calculated tax amount in lamports
/// * `None` - If tax_bps > 10000 (invalid rate) or arithmetic overflow
pub fn calculate_tax(amount_lamports: u64, tax_bps: u16) -> Option<u64> {
    if tax_bps > 10_000 {
        return None;
    }

    let amount = amount_lamports as u128;
    let bps = tax_bps as u128;

    let tax = amount
        .checked_mul(bps)?
        .checked_div(10_000)?;

    u64::try_from(tax).ok()
}

/// Split total tax into (staking, carnage, treasury) portions.
///
/// Distribution (71/24/5 split):
/// - Staking: 71% (floor)
/// - Carnage: 24% (floor)
/// - Treasury: remainder (absorbs rounding dust)
///
/// Micro-tax edge case: If total_tax < 4 lamports, all goes to staking.
///
/// # Invariant
/// staking + carnage + treasury == total_tax (always)
pub fn split_distribution(total_tax: u64) -> Option<(u64, u64, u64)> {
    const STAKING_BPS: u128 = 7_100;
    const CARNAGE_BPS: u128 = 2_400;
    const BPS_DENOM: u128 = 10_000;

    if total_tax < 4 {
        return Some((total_tax, 0, 0));
    }

    let total = total_tax as u128;

    let staking_u128 = total.checked_mul(STAKING_BPS)?.checked_div(BPS_DENOM)?;
    let staking = u64::try_from(staking_u128).ok()?;

    let carnage_u128 = total.checked_mul(CARNAGE_BPS)?.checked_div(BPS_DENOM)?;
    let carnage = u64::try_from(carnage_u128).ok()?;

    let treasury = total_tax
        .checked_sub(staking)?
        .checked_sub(carnage)?;

    Some((staking, carnage, treasury))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tax_4pct_on_1_sol() {
        assert_eq!(calculate_tax(1_000_000_000, 400), Some(40_000_000));
    }

    #[test]
    fn tax_14pct_on_1_sol() {
        assert_eq!(calculate_tax(1_000_000_000, 1400), Some(140_000_000));
    }

    #[test]
    fn tax_4pct_on_100_lamports() {
        assert_eq!(calculate_tax(100, 400), Some(4));
    }

    #[test]
    fn tax_4pct_rounds_down() {
        assert_eq!(calculate_tax(10, 400), Some(0));
    }

    #[test]
    fn tax_100pct_on_max() {
        assert_eq!(calculate_tax(u64::MAX, 10000), Some(u64::MAX));
    }

    #[test]
    fn tax_invalid_bps_over_10000() {
        assert_eq!(calculate_tax(1_000_000_000, 10001), None);
    }

    #[test]
    fn tax_zero_input() {
        assert_eq!(calculate_tax(0, 400), Some(0));
    }

    #[test]
    fn tax_zero_bps() {
        assert_eq!(calculate_tax(1_000_000_000, 0), Some(0));
    }

    #[test]
    fn tax_max_valid_bps() {
        assert_eq!(calculate_tax(1000, 10000), Some(1000));
    }

    #[test]
    fn tax_1_bps_on_large_amount() {
        assert_eq!(calculate_tax(1_000_000_000_000, 1), Some(100_000_000));
    }

    #[test]
    fn split_100_lamports() {
        assert_eq!(split_distribution(100), Some((71, 24, 5)));
    }

    #[test]
    fn split_1000_lamports() {
        assert_eq!(split_distribution(1000), Some((710, 240, 50)));
    }

    #[test]
    fn split_10_lamports_with_remainder() {
        assert_eq!(split_distribution(10), Some((7, 2, 1)));
    }

    #[test]
    fn split_micro_tax_3_lamports() {
        assert_eq!(split_distribution(3), Some((3, 0, 0)));
    }

    #[test]
    fn split_micro_tax_1_lamport() {
        assert_eq!(split_distribution(1), Some((1, 0, 0)));
    }

    #[test]
    fn split_zero_tax() {
        assert_eq!(split_distribution(0), Some((0, 0, 0)));
    }

    #[test]
    fn split_max_u64() {
        let result = split_distribution(u64::MAX);
        assert!(result.is_some());
        let (staking, carnage, treasury) = result.unwrap();
        assert_eq!(
            staking.checked_add(carnage).and_then(|s| s.checked_add(treasury)),
            Some(u64::MAX),
        );
    }

    #[test]
    fn split_4_lamports_boundary() {
        assert_eq!(split_distribution(4), Some((2, 0, 2)));
    }

    #[test]
    fn split_5_lamports() {
        assert_eq!(split_distribution(5), Some((3, 1, 1)));
    }

    #[test]
    fn split_invariant_sum_equals_total() {
        for total in [0, 1, 2, 3, 4, 5, 10, 99, 100, 101, 1000, 10000, 1_000_000] {
            let result = split_distribution(total);
            assert!(result.is_some());
            let (staking, carnage, treasury) = result.unwrap();
            let sum = staking + carnage + treasury;
            assert_eq!(sum, total);
        }
    }
}
