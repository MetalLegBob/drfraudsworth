// Source: programs/conversion-vault/src/instructions/convert.rs -- MUST stay in sync
//
// SDK mirror of the on-chain vault conversion math.
// Returns Option instead of Result (no anchor errors in SDK).

use solana_sdk::pubkey::Pubkey;
use crate::accounts::addresses::{CRIME_MINT, FRAUD_MINT, PROFIT_MINT};
use crate::constants::CONVERSION_RATE;

/// Compute the output amount for a vault conversion.
///
/// # Conversion rules
/// - CRIME/FRAUD -> PROFIT: divide by CONVERSION_RATE (100)
/// - PROFIT -> CRIME/FRAUD: multiply by CONVERSION_RATE (100)
/// - CRIME <-> FRAUD: Not supported on-chain (InvalidMintPair).
///   Jupiter routes CRIME->PROFIT->FRAUD via multi-hop.
/// - Same mint: Not supported (SameMint).
/// - Zero amount: Not supported (ZeroAmount).
///
/// # Returns
/// * `Some(output)` - Converted amount
/// * `None` - Invalid pair, zero amount, zero output, or overflow
pub fn compute_vault_output(
    input_mint: &Pubkey,
    output_mint: &Pubkey,
    amount_in: u64,
) -> Option<u64> {
    if amount_in == 0 {
        return None;
    }
    if input_mint == output_mint {
        return None;
    }

    let is_input_crime_or_fraud =
        *input_mint == CRIME_MINT || *input_mint == FRAUD_MINT;
    let is_output_profit = *output_mint == PROFIT_MINT;
    let is_input_profit = *input_mint == PROFIT_MINT;
    let is_output_crime_or_fraud =
        *output_mint == CRIME_MINT || *output_mint == FRAUD_MINT;

    if is_input_crime_or_fraud && is_output_profit {
        // CRIME/FRAUD -> PROFIT: divide by 100
        let out = amount_in / CONVERSION_RATE;
        if out == 0 {
            return None; // OutputTooSmall
        }
        Some(out)
    } else if is_input_profit && is_output_crime_or_fraud {
        // PROFIT -> CRIME/FRAUD: multiply by 100
        amount_in.checked_mul(CONVERSION_RATE)
    } else {
        // CRIME<->FRAUD not supported on-chain (InvalidMintPair).
        // Jupiter routes CRIME->PROFIT->FRAUD via multi-hop.
        None
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crime_to_profit() {
        assert_eq!(
            compute_vault_output(&CRIME_MINT, &PROFIT_MINT, 10_000),
            Some(100)
        );
    }

    #[test]
    fn fraud_to_profit() {
        assert_eq!(
            compute_vault_output(&FRAUD_MINT, &PROFIT_MINT, 10_000),
            Some(100)
        );
    }

    #[test]
    fn profit_to_crime() {
        assert_eq!(
            compute_vault_output(&PROFIT_MINT, &CRIME_MINT, 100),
            Some(10_000)
        );
    }

    #[test]
    fn profit_to_fraud() {
        assert_eq!(
            compute_vault_output(&PROFIT_MINT, &FRAUD_MINT, 100),
            Some(10_000)
        );
    }

    #[test]
    fn crime_to_fraud_not_supported() {
        assert_eq!(
            compute_vault_output(&CRIME_MINT, &FRAUD_MINT, 1000),
            None
        );
    }

    #[test]
    fn fraud_to_crime_not_supported() {
        assert_eq!(
            compute_vault_output(&FRAUD_MINT, &CRIME_MINT, 1000),
            None
        );
    }

    #[test]
    fn same_mint_not_supported() {
        assert_eq!(
            compute_vault_output(&CRIME_MINT, &CRIME_MINT, 1000),
            None
        );
    }

    #[test]
    fn zero_amount() {
        assert_eq!(
            compute_vault_output(&CRIME_MINT, &PROFIT_MINT, 0),
            None
        );
    }

    #[test]
    fn crime_to_profit_dust_too_small() {
        // 99 CRIME / 100 = 0 -> None (output too small)
        assert_eq!(
            compute_vault_output(&CRIME_MINT, &PROFIT_MINT, 99),
            None
        );
    }

    #[test]
    fn profit_to_crime_overflow() {
        // Large PROFIT amount that would overflow u64 when multiplied by 100
        let large = u64::MAX / 50; // > u64::MAX / 100, so *100 overflows
        assert_eq!(
            compute_vault_output(&PROFIT_MINT, &CRIME_MINT, large),
            None
        );
    }

    #[test]
    fn profit_to_crime_max_safe() {
        // Maximum PROFIT that doesn't overflow when *100
        let max_safe = u64::MAX / 100;
        assert_eq!(
            compute_vault_output(&PROFIT_MINT, &CRIME_MINT, max_safe),
            Some(max_safe * 100)
        );
    }
}
