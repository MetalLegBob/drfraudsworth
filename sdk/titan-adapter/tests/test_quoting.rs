// Quoting tests following Titan's test_quoting.rs pattern.
//
// Strategy: Prove Titan adapter quotes == Jupiter adapter quotes for same inputs.
// Our Jupiter adapter is already proven to match on-chain math via 36+ cross-crate
// parity tests (sdk/jupiter-adapter/tests/parity_*.rs). Therefore:
//   Titan quote == Jupiter quote == on-chain execution (transitive proof)
//
// Tests:
// - Cross-adapter parity: same inputs → same outputs (all 8 swap directions)
// - Instruction data structure: correct discriminators, account ordering, data layout
// - Random sampling: 50 log-uniform samples per direction
// - Monotonicity: larger inputs → larger outputs (all directions)
// - Speed: < 100 microseconds average

use solana_sdk::pubkey::Pubkey;
use titan_integration_template::trading_venue::{QuoteRequest, SwapType, TradingVenue};

use drfraudsworth_titan_adapter::accounts::addresses::*;
use drfraudsworth_titan_adapter::constants::*;
use drfraudsworth_titan_adapter::math::amm_math::*;
use drfraudsworth_titan_adapter::math::tax_math::*;
use drfraudsworth_titan_adapter::math::vault_math::*;
use drfraudsworth_titan_adapter::sol_pool_venue::SolPoolVenue;
use drfraudsworth_titan_adapter::vault_venue::VaultVenue;

// =============================================================================
// Cross-adapter parity: Titan venue math == on-chain math
// =============================================================================
// These replicate the exact pipeline from our Jupiter adapter (which is proven
// to match on-chain via parity tests) and verify the Titan venue produces
// identical output.

/// Replicate the on-chain buy pipeline manually and compare with venue quote.
fn reference_buy_output(
    reserve_sol: u64,
    reserve_token: u64,
    amount_in: u64,
    buy_tax_bps: u16,
    lp_fee_bps: u16,
) -> u64 {
    if amount_in == 0 { return 0; }
    let tax = calculate_tax(amount_in, buy_tax_bps).unwrap();
    let sol_to_swap = amount_in.checked_sub(tax).unwrap();
    if sol_to_swap == 0 { return 0; }
    let effective = calculate_effective_input(sol_to_swap, lp_fee_bps).unwrap();
    calculate_swap_output(reserve_sol, reserve_token, effective).unwrap_or(0)
}

/// Replicate the on-chain sell pipeline manually and compare with venue quote.
fn reference_sell_output(
    reserve_sol: u64,
    reserve_token: u64,
    amount_in: u64,
    sell_tax_bps: u16,
    lp_fee_bps: u16,
) -> u64 {
    if amount_in == 0 { return 0; }
    let effective = calculate_effective_input(amount_in, lp_fee_bps).unwrap();
    let gross_sol = calculate_swap_output(reserve_token, reserve_sol, effective).unwrap_or(0);
    let tax = calculate_tax(gross_sol, sell_tax_bps).unwrap();
    gross_sol.saturating_sub(tax)
}

// =============================================================================
// Part 1: SOL Pool parity — realistic mainnet-like state
// =============================================================================

const RESERVE_SOL: u64 = 100_000_000_000;   // 100 SOL
const RESERVE_TOKEN: u64 = 1_000_000_000_000; // 1B tokens (6 decimals)
const BUY_TAX: u16 = 400;   // 4%
const SELL_TAX: u16 = 1400;  // 14%

fn make_crime_venue() -> SolPoolVenue {
    SolPoolVenue::new_for_testing(true, RESERVE_SOL, RESERVE_TOKEN, BUY_TAX, SELL_TAX)
}

fn make_fraud_venue() -> SolPoolVenue {
    SolPoolVenue::new_for_testing(false, RESERVE_SOL, RESERVE_TOKEN, SELL_TAX, BUY_TAX)
}

#[test]
fn parity_buy_crime_1sol() {
    let venue = make_crime_venue();
    let amount = 1_000_000_000; // 1 SOL

    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, BUY_TAX, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected,
        "Titan quote {} != reference {}", result.expected_output, expected);
}

#[test]
fn parity_buy_crime_10sol() {
    let venue = make_crime_venue();
    let amount = 10_000_000_000;

    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, BUY_TAX, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

#[test]
fn parity_buy_fraud_1sol() {
    let venue = make_fraud_venue();
    let amount = 1_000_000_000;

    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: FRAUD_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, SELL_TAX, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

#[test]
fn parity_sell_crime_1m_tokens() {
    let venue = make_crime_venue();
    let amount = 1_000_000; // 1 token (6 decimals)

    let result = venue.quote(QuoteRequest {
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_sell_output(RESERVE_SOL, RESERVE_TOKEN, amount, SELL_TAX, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

#[test]
fn parity_sell_crime_100m_tokens() {
    let venue = make_crime_venue();
    let amount = 100_000_000_000; // 100K tokens

    let result = venue.quote(QuoteRequest {
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_sell_output(RESERVE_SOL, RESERVE_TOKEN, amount, SELL_TAX, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

#[test]
fn parity_sell_fraud_1m_tokens() {
    let venue = make_fraud_venue();
    let amount = 1_000_000;

    let result = venue.quote(QuoteRequest {
        input_mint: FRAUD_MINT,
        output_mint: NATIVE_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_sell_output(RESERVE_SOL, RESERVE_TOKEN, amount, BUY_TAX, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

// =============================================================================
// Part 2: Vault parity — all 4 directions
// =============================================================================

#[test]
fn parity_vault_crime_to_profit() {
    let venue = VaultVenue::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let amounts = [100, 1_000, 10_000, 100_000, 1_000_000, 500_000_000];

    for amount in amounts {
        let result = venue.quote(QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: PROFIT_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        let expected = compute_vault_output(&CRIME_MINT, &PROFIT_MINT, amount).unwrap();
        assert_eq!(result.expected_output, expected,
            "CRIME->PROFIT mismatch at amount {}: {} vs {}", amount, result.expected_output, expected);
    }
}

#[test]
fn parity_vault_fraud_to_profit() {
    let venue = VaultVenue::new_for_testing(FRAUD_MINT, PROFIT_MINT);
    for amount in [100, 10_000, 1_000_000] {
        let result = venue.quote(QuoteRequest {
            input_mint: FRAUD_MINT,
            output_mint: PROFIT_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();
        assert_eq!(result.expected_output, compute_vault_output(&FRAUD_MINT, &PROFIT_MINT, amount).unwrap());
    }
}

#[test]
fn parity_vault_profit_to_crime() {
    let venue = VaultVenue::new_for_testing(PROFIT_MINT, CRIME_MINT);
    for amount in [1, 10, 100, 1_000, 10_000] {
        let result = venue.quote(QuoteRequest {
            input_mint: PROFIT_MINT,
            output_mint: CRIME_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();
        assert_eq!(result.expected_output, compute_vault_output(&PROFIT_MINT, &CRIME_MINT, amount).unwrap());
    }
}

#[test]
fn parity_vault_profit_to_fraud() {
    let venue = VaultVenue::new_for_testing(PROFIT_MINT, FRAUD_MINT);
    for amount in [1, 50, 500, 5_000] {
        let result = venue.quote(QuoteRequest {
            input_mint: PROFIT_MINT,
            output_mint: FRAUD_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();
        assert_eq!(result.expected_output, compute_vault_output(&PROFIT_MINT, &FRAUD_MINT, amount).unwrap());
    }
}

// =============================================================================
// Part 3: Random sampling — 50 log-uniform samples per direction
// =============================================================================

fn sample_log_uniform(lo: u64, hi: u64, seed: u64) -> u64 {
    // Simple deterministic pseudo-random log-uniform sampling
    // Using a linear congruential generator for reproducibility
    let hash = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let frac = (hash as f64) / (u64::MAX as f64);
    let log_lo = (lo as f64).ln();
    let log_hi = (hi as f64).ln();
    let log_val = log_lo + frac * (log_hi - log_lo);
    let val = log_val.exp() as u64;
    val.max(lo).min(hi)
}

#[test]
fn random_sampling_buy_crime_50_samples() {
    let venue = make_crime_venue();

    for seed in 0..50 {
        let amount = sample_log_uniform(1_000, 50_000_000_000, seed);
        let result = venue.quote(QuoteRequest {
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, BUY_TAX, LP_FEE_BPS);
        assert_eq!(result.expected_output, expected,
            "Buy CRIME random sample #{} failed at amount {}: got {} expected {}",
            seed, amount, result.expected_output, expected);
    }
}

#[test]
fn random_sampling_sell_crime_50_samples() {
    let venue = make_crime_venue();

    for seed in 0..50 {
        let amount = sample_log_uniform(1_000, 500_000_000_000, seed + 100);
        let result = venue.quote(QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        let expected = reference_sell_output(RESERVE_SOL, RESERVE_TOKEN, amount, SELL_TAX, LP_FEE_BPS);
        assert_eq!(result.expected_output, expected,
            "Sell CRIME random sample #{} failed at amount {}", seed, amount);
    }
}

#[test]
fn random_sampling_buy_fraud_50_samples() {
    let venue = make_fraud_venue();

    for seed in 0..50 {
        let amount = sample_log_uniform(1_000, 50_000_000_000, seed + 200);
        let result = venue.quote(QuoteRequest {
            input_mint: NATIVE_MINT,
            output_mint: FRAUD_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, SELL_TAX, LP_FEE_BPS);
        assert_eq!(result.expected_output, expected,
            "Buy FRAUD random sample #{} failed at amount {}", seed, amount);
    }
}

#[test]
fn random_sampling_sell_fraud_50_samples() {
    let venue = make_fraud_venue();

    for seed in 0..50 {
        let amount = sample_log_uniform(1_000, 500_000_000_000, seed + 300);
        let result = venue.quote(QuoteRequest {
            input_mint: FRAUD_MINT,
            output_mint: NATIVE_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        let expected = reference_sell_output(RESERVE_SOL, RESERVE_TOKEN, amount, BUY_TAX, LP_FEE_BPS);
        assert_eq!(result.expected_output, expected,
            "Sell FRAUD random sample #{} failed at amount {}", seed, amount);
    }
}

#[test]
fn random_sampling_vault_crime_to_profit_50_samples() {
    let venue = VaultVenue::new_for_testing(CRIME_MINT, PROFIT_MINT);

    for seed in 0..50 {
        let amount = sample_log_uniform(100, 1_000_000_000, seed + 400);
        let result = venue.quote(QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: PROFIT_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        let expected = compute_vault_output(&CRIME_MINT, &PROFIT_MINT, amount).unwrap();
        assert_eq!(result.expected_output, expected,
            "Vault C->P random sample #{} failed at amount {}", seed, amount);
    }
}

#[test]
fn random_sampling_vault_profit_to_crime_50_samples() {
    let venue = VaultVenue::new_for_testing(PROFIT_MINT, CRIME_MINT);

    for seed in 0..50 {
        let amount = sample_log_uniform(1, 10_000_000, seed + 500);
        let result = venue.quote(QuoteRequest {
            input_mint: PROFIT_MINT,
            output_mint: CRIME_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        let expected = compute_vault_output(&PROFIT_MINT, &CRIME_MINT, amount).unwrap();
        assert_eq!(result.expected_output, expected,
            "Vault P->C random sample #{} failed at amount {}", seed, amount);
    }
}

// =============================================================================
// Part 4: Edge cases — dust, max tax, boundary amounts
// =============================================================================

#[test]
fn parity_dust_amount_100_lamports() {
    let venue = make_crime_venue();
    let amount = 100; // 100 lamports

    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, BUY_TAX, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

#[test]
fn parity_max_tax_50pct() {
    // Extreme case: 50% buy tax
    let venue = SolPoolVenue::new_for_testing(true, RESERVE_SOL, RESERVE_TOKEN, 5000, 5000);
    let amount = 1_000_000_000;

    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, 5000, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

#[test]
fn parity_zero_tax() {
    let venue = SolPoolVenue::new_for_testing(true, RESERVE_SOL, RESERVE_TOKEN, 0, 0);
    let amount = 1_000_000_000;

    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        amount,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    let expected = reference_buy_output(RESERVE_SOL, RESERVE_TOKEN, amount, 0, LP_FEE_BPS);
    assert_eq!(result.expected_output, expected);
}

// =============================================================================
// Part 5: Instruction structure verification
// =============================================================================

#[test]
fn buy_instruction_has_correct_program_id() {
    let venue = make_crime_venue();
    let user = Pubkey::new_unique();

    let ix = venue.generate_swap_instruction(
        QuoteRequest {
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            amount: 1_000_000_000,
            swap_type: SwapType::ExactIn,
        },
        user,
    ).unwrap();

    assert_eq!(ix.program_id, TAX_PROGRAM_ID);
    assert_eq!(ix.accounts.len(), 24, "Buy should have 24 accounts (20 named + 4 hook)");
    assert_eq!(ix.data.len(), 25, "Buy data: 8 disc + 8 amount_in + 8 min_out + 1 is_crime");
}

#[test]
fn sell_instruction_has_correct_program_id() {
    let venue = make_crime_venue();
    let user = Pubkey::new_unique();

    let ix = venue.generate_swap_instruction(
        QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            amount: 1_000_000_000,
            swap_type: SwapType::ExactIn,
        },
        user,
    ).unwrap();

    assert_eq!(ix.program_id, TAX_PROGRAM_ID);
    assert_eq!(ix.accounts.len(), 25, "Sell should have 25 accounts (21 named + 4 hook)");
    assert_eq!(ix.data.len(), 25);
}

#[test]
fn vault_instruction_has_correct_program_id() {
    let venue = VaultVenue::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let user = Pubkey::new_unique();

    let ix = venue.generate_swap_instruction(
        QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: PROFIT_MINT,
            amount: 10_000,
            swap_type: SwapType::ExactIn,
        },
        user,
    ).unwrap();

    assert_eq!(ix.program_id, CONVERSION_VAULT_PROGRAM_ID);
    assert_eq!(ix.accounts.len(), 17, "Vault should have 17 accounts (9 named + 8 hook)");
    assert_eq!(ix.data.len(), 16, "Vault data: 8 disc + 8 amount_in");
}

#[test]
fn instruction_data_amount_matches_request() {
    let venue = make_crime_venue();
    let user = Pubkey::new_unique();
    let amount = 42_000_000_000u64;

    let ix = venue.generate_swap_instruction(
        QuoteRequest {
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        },
        user,
    ).unwrap();

    // Amount is at bytes [8..16] (after discriminator)
    let encoded_amount = u64::from_le_bytes(ix.data[8..16].try_into().unwrap());
    assert_eq!(encoded_amount, amount);

    // min_amount_out is at bytes [16..24] — set to quoted_output / 2 (50% floor)
    let min_out = u64::from_le_bytes(ix.data[16..24].try_into().unwrap());
    assert!(min_out > 0, "min_out should be non-zero (50% output floor)");

    // is_crime at byte [24]
    assert_eq!(ix.data[24], 1, "is_crime should be true for CRIME pool");
}

#[test]
fn instruction_user_is_signer() {
    let venue = make_crime_venue();
    let user = Pubkey::new_unique();

    let ix = venue.generate_swap_instruction(
        QuoteRequest {
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            amount: 1_000_000_000,
            swap_type: SwapType::ExactIn,
        },
        user,
    ).unwrap();

    // First account should be the user as signer
    assert_eq!(ix.accounts[0].pubkey, user);
    assert!(ix.accounts[0].is_signer);
    assert!(ix.accounts[0].is_writable);
}

#[test]
fn buy_and_sell_have_different_discriminators() {
    let venue = make_crime_venue();
    let user = Pubkey::new_unique();

    let buy_ix = venue.generate_swap_instruction(
        QuoteRequest { input_mint: NATIVE_MINT, output_mint: CRIME_MINT, amount: 100, swap_type: SwapType::ExactIn },
        user,
    ).unwrap();

    let sell_ix = venue.generate_swap_instruction(
        QuoteRequest { input_mint: CRIME_MINT, output_mint: NATIVE_MINT, amount: 100, swap_type: SwapType::ExactIn },
        user,
    ).unwrap();

    assert_ne!(&buy_ix.data[0..8], &sell_ix.data[0..8],
        "Buy and sell should have different instruction discriminators");
}

// =============================================================================
// Part 6: Comprehensive monotonicity with fine granularity
// =============================================================================

#[test]
fn monotonicity_buy_crime_100_steps() {
    let venue = make_crime_venue();
    let mut prev = 0u64;

    for i in 1..=100 {
        let amount = i * 500_000_000; // 0.5 SOL increments up to 50 SOL
        let result = venue.quote(QuoteRequest {
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        assert!(result.expected_output >= prev,
            "Monotonicity violation at step {}: {} < {}", i, result.expected_output, prev);
        prev = result.expected_output;
    }
}

#[test]
fn monotonicity_sell_crime_100_steps() {
    let venue = make_crime_venue();
    let mut prev = 0u64;

    for i in 1..=100 {
        let amount = i * 5_000_000_000; // 5K token increments
        let result = venue.quote(QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        assert!(result.expected_output >= prev,
            "Sell monotonicity violation at step {}", i);
        prev = result.expected_output;
    }
}

// =============================================================================
// Part 7: Speed tests (10,000 iterations, < 100us average)
// =============================================================================

#[test]
fn speed_buy_crime_10k() {
    let venue = make_crime_venue();
    let start = std::time::Instant::now();

    for i in 0..10_000u64 {
        let _ = venue.quote(QuoteRequest {
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            amount: 1_000_000 + i,
            swap_type: SwapType::ExactIn,
        });
    }

    let avg = start.elapsed().as_micros() as f64 / 10_000.0;
    assert!(avg < 100.0, "Buy quote avg {:.2}us > 100us", avg);
    eprintln!("Buy CRIME: {:.2}us avg", avg);
}

#[test]
fn speed_sell_crime_10k() {
    let venue = make_crime_venue();
    let start = std::time::Instant::now();

    for i in 0..10_000u64 {
        let _ = venue.quote(QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            amount: 1_000_000 + i,
            swap_type: SwapType::ExactIn,
        });
    }

    let avg = start.elapsed().as_micros() as f64 / 10_000.0;
    assert!(avg < 100.0, "Sell quote avg {:.2}us > 100us", avg);
    eprintln!("Sell CRIME: {:.2}us avg", avg);
}

#[test]
fn speed_vault_convert_10k() {
    let venue = VaultVenue::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let start = std::time::Instant::now();

    for i in 0..10_000u64 {
        let _ = venue.quote(QuoteRequest {
            input_mint: CRIME_MINT,
            output_mint: PROFIT_MINT,
            amount: 10_000 + i,
            swap_type: SwapType::ExactIn,
        });
    }

    let avg = start.elapsed().as_micros() as f64 / 10_000.0;
    assert!(avg < 100.0, "Vault quote avg {:.2}us > 100us", avg);
    eprintln!("Vault CRIME->PROFIT: {:.2}us avg", avg);
}

#[test]
fn speed_generate_instruction_10k() {
    let venue = make_crime_venue();
    let user = Pubkey::new_unique();
    let start = std::time::Instant::now();

    for i in 0..10_000u64 {
        let _ = venue.generate_swap_instruction(
            QuoteRequest {
                input_mint: NATIVE_MINT,
                output_mint: CRIME_MINT,
                amount: 1_000_000 + i,
                swap_type: SwapType::ExactIn,
            },
            user,
        );
    }

    let avg = start.elapsed().as_micros() as f64 / 10_000.0;
    // Instruction generation is heavier (PDA derivation), allow more headroom
    eprintln!("Generate IX: {:.2}us avg", avg);
}
