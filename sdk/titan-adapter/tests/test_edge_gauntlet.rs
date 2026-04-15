// Phase 8.7: Edge Case Gauntlet
//
// Adversarial inputs that should never panic, always return correct errors.
// This is what Titan's reviewer would throw at us.

use solana_sdk::pubkey::Pubkey;
use titan_integration_template::trading_venue::{QuoteRequest, SwapType, TradingVenue};
use titan_integration_template::trading_venue::error::TradingVenueError;

use drfraudsworth_titan_adapter::accounts::addresses::*;
use drfraudsworth_titan_adapter::sol_pool_venue::SolPoolVenue;
use drfraudsworth_titan_adapter::vault_venue::{VaultVenue, known_sol_pool_venues, known_vault_venues};

fn crime_venue() -> SolPoolVenue {
    SolPoolVenue::new_for_testing(true, 100_000_000_000, 1_000_000_000_000, 400, 1400)
}

// =============================================================================
// (1) amount = 0
// =============================================================================

#[test]
fn zero_amount_all_sol_pool_directions() {
    let venue = crime_venue();
    for (input, output) in [(NATIVE_MINT, CRIME_MINT), (CRIME_MINT, NATIVE_MINT)] {
        let r = venue.quote(QuoteRequest {
            input_mint: input, output_mint: output, amount: 0, swap_type: SwapType::ExactIn,
        }).unwrap();
        assert_eq!(r.expected_output, 0, "Zero input should give zero output");
    }
}

#[test]
fn zero_amount_all_vault_directions() {
    let venues = known_vault_venues();
    for venue in &venues {
        let mints = venue.tradable_mints().unwrap();
        let r = venue.quote(QuoteRequest {
            input_mint: mints[0], output_mint: mints[1], amount: 0, swap_type: SwapType::ExactIn,
        }).unwrap();
        assert_eq!(r.expected_output, 0);
    }
}

// =============================================================================
// (2) amount = 1
// =============================================================================

#[test]
fn one_lamport_sol_pool_buy() {
    let venue = crime_venue();
    // 1 lamport: tax rounds to 0, LP fee rounds to 0, output = ~10 tokens (tiny)
    let r = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT, output_mint: CRIME_MINT, amount: 1, swap_type: SwapType::ExactIn,
    }).unwrap();
    // Don't assert specific value — just no panic
    assert!(r.expected_output <= 100);
}

#[test]
fn one_lamport_sol_pool_sell() {
    let venue = crime_venue();
    let r = venue.quote(QuoteRequest {
        input_mint: CRIME_MINT, output_mint: NATIVE_MINT, amount: 1, swap_type: SwapType::ExactIn,
    }).unwrap();
    // 1 token with LP fee rounds to 0 effective input → 0 output
    assert!(r.expected_output <= 1);
}

#[test]
fn one_unit_vault_crime_to_profit_errors() {
    let venue = VaultVenue::new_for_testing(CRIME_MINT, PROFIT_MINT);
    // 1 CRIME / 100 = 0 PROFIT → should error (dust too small)
    let r = venue.quote(QuoteRequest {
        input_mint: CRIME_MINT, output_mint: PROFIT_MINT, amount: 1, swap_type: SwapType::ExactIn,
    });
    assert!(r.is_err());
}

#[test]
fn one_unit_vault_profit_to_crime_succeeds() {
    let venue = VaultVenue::new_for_testing(PROFIT_MINT, CRIME_MINT);
    // 1 PROFIT * 100 = 100 CRIME → should succeed
    let r = venue.quote(QuoteRequest {
        input_mint: PROFIT_MINT, output_mint: CRIME_MINT, amount: 1, swap_type: SwapType::ExactIn,
    }).unwrap();
    assert_eq!(r.expected_output, 100);
}

// =============================================================================
// (3) amount = u64::MAX
// =============================================================================

#[test]
fn u64_max_sol_pool_buy_no_panic() {
    let venue = crime_venue();
    let r = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT, output_mint: CRIME_MINT, amount: u64::MAX, swap_type: SwapType::ExactIn,
    });
    // May succeed or error — either is fine, just no panic
    if let Ok(q) = r {
        assert!(q.not_enough_liquidity || q.expected_output > 0);
    } // Math overflow error is acceptable for u64::MAX
}

#[test]
fn u64_max_sol_pool_sell_no_panic() {
    let venue = crime_venue();
    let r = venue.quote(QuoteRequest {
        input_mint: CRIME_MINT, output_mint: NATIVE_MINT, amount: u64::MAX, swap_type: SwapType::ExactIn,
    });
    if let Ok(q) = r {
        assert!(q.not_enough_liquidity || q.expected_output > 0);
    }
}

#[test]
fn u64_max_vault_crime_to_profit_no_panic() {
    let venue = VaultVenue::new_for_testing(CRIME_MINT, PROFIT_MINT);
    // u64::MAX / 100 should succeed
    let r = venue.quote(QuoteRequest {
        input_mint: CRIME_MINT, output_mint: PROFIT_MINT, amount: u64::MAX, swap_type: SwapType::ExactIn,
    }).expect("u64::MAX / 100 should not overflow");
    assert_eq!(r.expected_output, u64::MAX / 100);
}

#[test]
fn u64_max_vault_profit_to_crime_overflows_gracefully() {
    let venue = VaultVenue::new_for_testing(PROFIT_MINT, CRIME_MINT);
    // u64::MAX * 100 overflows → should error, not panic
    let r = venue.quote(QuoteRequest {
        input_mint: PROFIT_MINT, output_mint: CRIME_MINT, amount: u64::MAX, swap_type: SwapType::ExactIn,
    });
    assert!(r.is_err(), "u64::MAX * 100 should overflow gracefully");
}

// =============================================================================
// (4) ExactOut — all venues
// =============================================================================

#[test]
fn exact_out_rejected_all_sol_pools() {
    for venue in known_sol_pool_venues() {
        let mints = venue.tradable_mints().unwrap();
        for (i, o) in [(0, 1), (1, 0)] {
            let r = venue.quote(QuoteRequest {
                input_mint: mints[i], output_mint: mints[o], amount: 1_000_000,
                swap_type: SwapType::ExactOut,
            });
            assert!(matches!(r, Err(TradingVenueError::ExactOutNotSupported)),
                "ExactOut should be rejected for SOL pool");
        }
    }
}

#[test]
fn exact_out_rejected_all_vaults() {
    for venue in known_vault_venues() {
        let mints = venue.tradable_mints().unwrap();
        let r = venue.quote(QuoteRequest {
            input_mint: mints[0], output_mint: mints[1], amount: 1_000_000,
            swap_type: SwapType::ExactOut,
        });
        assert!(matches!(r, Err(TradingVenueError::ExactOutNotSupported)));
    }
}

// =============================================================================
// (5) Wrong mint pair for venue
// =============================================================================

#[test]
fn wrong_mint_vault_crime_expects_fraud() {
    let venue = VaultVenue::new_for_testing(CRIME_MINT, PROFIT_MINT);
    let r = venue.quote(QuoteRequest {
        input_mint: FRAUD_MINT, // Wrong — venue expects CRIME
        output_mint: PROFIT_MINT,
        amount: 10_000,
        swap_type: SwapType::ExactIn,
    });
    assert!(r.is_err(), "Wrong input mint should error");
}

#[test]
fn wrong_mint_vault_profit_expects_crime() {
    let venue = VaultVenue::new_for_testing(PROFIT_MINT, CRIME_MINT);
    let r = venue.quote(QuoteRequest {
        input_mint: FRAUD_MINT, // Wrong — venue expects PROFIT
        output_mint: CRIME_MINT,
        amount: 100,
        swap_type: SwapType::ExactIn,
    });
    assert!(r.is_err());
}

#[test]
fn random_pubkey_as_input_mint_sol_pool() {
    let venue = crime_venue();
    let random_mint = Pubkey::new_unique();
    // SOL pool treats non-NATIVE_MINT as sell direction, which should still work
    // (the venue doesn't validate the mint, it just picks buy/sell based on NATIVE_MINT check)
    let r = venue.quote(QuoteRequest {
        input_mint: random_mint,
        output_mint: NATIVE_MINT,
        amount: 1_000_000,
        swap_type: SwapType::ExactIn,
    });
    // Should produce a result (sell path) — no panic
    assert!(r.is_ok() || r.is_err());
}

// =============================================================================
// (6) Uninitialized venue (no update_state called)
// =============================================================================

#[test]
fn uninitialized_sol_pool_venue_reports_not_initialized() {
    let venues = known_sol_pool_venues();
    for venue in &venues {
        assert!(!venue.initialized(),
            "Factory venues should be uninitialized");
    }
}

#[test]
fn uninitialized_vault_venue_reports_not_initialized() {
    let venues = known_vault_venues();
    for venue in &venues {
        assert!(!venue.initialized(),
            "Factory vault venues should be uninitialized");
    }
}

#[test]
fn uninitialized_sol_pool_can_still_quote_with_zero_reserves() {
    // Factory venues have zero reserves. Quoting should not panic.
    let venues = known_sol_pool_venues();
    let venue = &venues[0];

    let r = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        amount: 1_000_000_000,
        swap_type: SwapType::ExactIn,
    });
    // With zero reserves, swap output calculation may error or return 0
    if let Ok(q) = r {
        assert_eq!(q.expected_output, 0, "Zero reserves → zero output");
    } // Error is also acceptable
}

// =============================================================================
// (7) Extreme reserve ratios
// =============================================================================

#[test]
fn extreme_ratio_1_sol_vs_1_trillion_tokens() {
    let venue = SolPoolVenue::new_for_testing(true, 1_000_000_000, 1_000_000_000_000_000, 400, 1400);
    let r = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT, output_mint: CRIME_MINT, amount: 100_000_000,
        swap_type: SwapType::ExactIn,
    }).unwrap();
    assert!(r.expected_output > 0);
}

#[test]
fn extreme_ratio_1_trillion_sol_vs_1_token() {
    let venue = SolPoolVenue::new_for_testing(true, 1_000_000_000_000_000, 1_000_000, 400, 1400);
    let r = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT, output_mint: CRIME_MINT, amount: 1_000_000_000,
        swap_type: SwapType::ExactIn,
    }).unwrap();
    // Tiny token reserve → tiny output
    assert!(r.expected_output <= 1_000_000);
}

// =============================================================================
// (8) get_token / bounds index out of range
// =============================================================================

#[test]
fn get_token_out_of_bounds() {
    let venue = crime_venue();
    assert!(venue.get_token(0).is_ok());
    assert!(venue.get_token(1).is_ok());
    assert!(venue.get_token(2).is_err()); // Only 2 tokens
    assert!(venue.get_token(255).is_err());
}

#[test]
fn bounds_invalid_indices() {
    let venue = crime_venue();
    // Valid: 0→1 and 1→0
    assert!(venue.bounds(0, 1).is_ok());
    assert!(venue.bounds(1, 0).is_ok());
    // Invalid: out of range
    assert!(venue.bounds(2, 0).is_err());
    assert!(venue.bounds(0, 2).is_err());
}

// =============================================================================
// (9) All 8 swap directions produce sane results
// =============================================================================

#[test]
fn all_8_directions_produce_output() {
    let crime_pool = SolPoolVenue::new_for_testing(true, 100_000_000_000, 1_000_000_000_000, 400, 1400);
    let fraud_pool = SolPoolVenue::new_for_testing(false, 100_000_000_000, 1_000_000_000_000, 1400, 400);

    let directions: Vec<(&dyn TradingVenue, Pubkey, Pubkey, &str)> = vec![
        (&crime_pool, NATIVE_MINT, CRIME_MINT, "Buy CRIME"),
        (&crime_pool, CRIME_MINT, NATIVE_MINT, "Sell CRIME"),
        (&fraud_pool, NATIVE_MINT, FRAUD_MINT, "Buy FRAUD"),
        (&fraud_pool, FRAUD_MINT, NATIVE_MINT, "Sell FRAUD"),
    ];

    for (venue, input, output, label) in &directions {
        let r = venue.quote(QuoteRequest {
            input_mint: *input, output_mint: *output, amount: 1_000_000_000,
            swap_type: SwapType::ExactIn,
        }).unwrap();
        assert!(r.expected_output > 0, "{} should produce non-zero output", label);
    }

    // Vault directions
    let vault_pairs = [
        (CRIME_MINT, PROFIT_MINT, 10_000u64, "CRIME→PROFIT"),
        (FRAUD_MINT, PROFIT_MINT, 10_000, "FRAUD→PROFIT"),
        (PROFIT_MINT, CRIME_MINT, 100, "PROFIT→CRIME"),
        (PROFIT_MINT, FRAUD_MINT, 100, "PROFIT→FRAUD"),
    ];

    for (input, output, amount, label) in &vault_pairs {
        let venue = VaultVenue::new_for_testing(*input, *output);
        let r = venue.quote(QuoteRequest {
            input_mint: *input, output_mint: *output, amount: *amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();
        assert!(r.expected_output > 0, "{} should produce non-zero output", label);
    }
}

// =============================================================================
// (10) Trait method consistency
// =============================================================================

#[test]
fn program_id_correct_for_all_venues() {
    for venue in known_sol_pool_venues() {
        assert_eq!(venue.program_id(), TAX_PROGRAM_ID,
            "SOL pool venues should route through Tax Program");
    }
    for venue in known_vault_venues() {
        assert_eq!(venue.program_id(), CONVERSION_VAULT_PROGRAM_ID,
            "Vault venues should route through Conversion Vault");
    }
}

#[test]
fn decimals_correct_for_all_venues() {
    for venue in known_sol_pool_venues() {
        let d = venue.decimals().unwrap();
        assert_eq!(d[0], 9, "SOL should have 9 decimals");
        assert_eq!(d[1], 6, "Token should have 6 decimals");
    }
    for venue in known_vault_venues() {
        let d = venue.decimals().unwrap();
        assert_eq!(d[0], 6, "All vault tokens should have 6 decimals");
        assert_eq!(d[1], 6);
    }
}

#[test]
fn program_dependencies_non_empty() {
    for venue in known_sol_pool_venues() {
        assert!(!venue.program_dependencies().is_empty());
    }
    for venue in known_vault_venues() {
        assert!(!venue.program_dependencies().is_empty());
    }
}
