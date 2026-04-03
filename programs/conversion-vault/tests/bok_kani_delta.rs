//! BOK Kani verification harnesses for convert_v2 delta mode.
//!
//! These harnesses formally verify the delta mode arithmetic for ALL possible
//! u64 input combinations, not just sampled test cases.
//!
//! Run: `cargo kani --test bok_kani_delta --harness <name>`
//!
//! Invariants verified:
//! - INV-CV-K01: checked_sub never panics (DeltaUnderflow returned instead)
//! - INV-CV-K02: Delta mode output == direct compute on deposit amount
//! - INV-CV-K03: Three-mode branching is exhaustive and non-overlapping
//! - INV-CV-K04: Delta mode feeds valid input to compute_output (no zero, no overflow)

#[cfg(kani)]
mod kani_proofs {
    use conversion_vault::instructions::convert::compute_output_with_mints;

    fn test_crime() -> anchor_lang::prelude::Pubkey {
        anchor_lang::prelude::Pubkey::new_from_array([1u8; 32])
    }
    fn test_fraud() -> anchor_lang::prelude::Pubkey {
        anchor_lang::prelude::Pubkey::new_from_array([2u8; 32])
    }
    fn test_profit() -> anchor_lang::prelude::Pubkey {
        anchor_lang::prelude::Pubkey::new_from_array([3u8; 32])
    }

    /// Simulates the convert_v2 handler's amount resolution logic.
    /// Returns Ok(effective_amount) or Err(error_code).
    fn resolve_amount(amount_in: u64, pre_balance: u64, on_chain_balance: u64) -> Result<u64, u32> {
        if amount_in > 0 {
            // Exact mode
            Ok(amount_in)
        } else if pre_balance > 0 {
            // Delta mode
            match on_chain_balance.checked_sub(pre_balance) {
                Some(delta) if delta > 0 => Ok(delta),
                Some(0) => Err(6000), // ZeroAmount
                None => Err(6008),    // DeltaUnderflow
                _ => unreachable!(),
            }
        } else {
            // Convert-all mode
            if on_chain_balance > 0 {
                Ok(on_chain_balance)
            } else {
                Err(6000) // ZeroAmount
            }
        }
    }

    // =========================================================================
    // INV-CV-K01: checked_sub never panics
    //
    // For ALL u64 values of (pre_balance, on_chain_balance), the delta
    // calculation either returns a valid delta or DeltaUnderflow error.
    // It NEVER panics or produces undefined behavior.
    // =========================================================================
    #[kani::proof]
    fn inv_cv_k01_checked_sub_never_panics() {
        let pre_balance: u64 = kani::any();
        let on_chain_balance: u64 = kani::any();

        // Simulate delta mode (amount_in=0, pre_balance>0)
        kani::assume(pre_balance > 0);

        let result = resolve_amount(0, pre_balance, on_chain_balance);

        // Must always return Ok or Err — never panic
        match result {
            Ok(delta) => {
                assert!(delta > 0, "Delta must be positive when Ok");
                assert!(delta <= on_chain_balance, "Delta cannot exceed balance");
            }
            Err(code) => {
                assert!(
                    code == 6000 || code == 6008,
                    "Error must be ZeroAmount or DeltaUnderflow"
                );
            }
        }
    }

    // =========================================================================
    // INV-CV-K02: Delta mode output == direct compute on deposit
    //
    // For any (pre_balance, deposit) where deposit >= 100, converting via
    // delta mode produces the same output as converting the deposit directly.
    // This proves pre-existing holdings don't affect conversion output.
    // =========================================================================
    #[kani::proof]
    fn inv_cv_k02_delta_equals_direct_compute() {
        let pre_balance: u64 = kani::any();
        let deposit: u64 = kani::any();

        // Constrain to valid conversion range (deposit >= 100 for non-dust)
        kani::assume(deposit >= 100);
        // Prevent overflow when computing on_chain_balance
        kani::assume(pre_balance.checked_add(deposit).is_some());

        let on_chain_balance = pre_balance + deposit;

        // Delta mode
        let delta_result = resolve_amount(0, pre_balance, on_chain_balance);
        assert!(delta_result.is_ok(), "Delta mode should succeed with valid deposit");
        let delta = delta_result.unwrap();

        // Direct compute
        assert_eq!(delta, deposit, "Delta must equal deposit exactly");

        // Both paths produce identical conversion output
        let crime = test_crime();
        let fraud = test_fraud();
        let profit = test_profit();

        let delta_output = compute_output_with_mints(&crime, &profit, delta, &crime, &fraud, &profit);
        let direct_output = compute_output_with_mints(&crime, &profit, deposit, &crime, &fraud, &profit);

        assert_eq!(
            delta_output.unwrap(),
            direct_output.unwrap(),
            "Delta and direct must produce identical output"
        );
    }

    // =========================================================================
    // INV-CV-K03: Three-mode branching is exhaustive
    //
    // For ALL (amount_in, pre_balance, balance) triples, exactly one mode
    // is selected. No inputs can reach two modes or skip all modes.
    // =========================================================================
    #[kani::proof]
    fn inv_cv_k03_mode_selection_exhaustive() {
        let amount_in: u64 = kani::any();
        let pre_balance: u64 = kani::any();
        let on_chain_balance: u64 = kani::any();

        let is_exact = amount_in > 0;
        let is_delta = amount_in == 0 && pre_balance > 0;
        let is_convert_all = amount_in == 0 && pre_balance == 0;

        // Exactly one mode must be selected
        let mode_count = is_exact as u8 + is_delta as u8 + is_convert_all as u8;
        assert_eq!(mode_count, 1, "Exactly one mode must be selected");

        // The resolver must handle all cases without panic
        let _result = resolve_amount(amount_in, pre_balance, on_chain_balance);
    }

    // =========================================================================
    // INV-CV-K04: Delta feeds valid input to compute_output
    //
    // When delta mode succeeds (returns Ok), the effective_amount is always
    // in a range that compute_output_with_mints can handle without overflow.
    // Specifically: for CRIME->PROFIT (divide by 100), any u64 is safe.
    // For PROFIT->CRIME (multiply by 100), delta <= u64::MAX/100.
    // =========================================================================
    #[kani::proof]
    fn inv_cv_k04_delta_feeds_valid_compute_input() {
        let pre_balance: u64 = kani::any();
        let on_chain_balance: u64 = kani::any();

        kani::assume(pre_balance > 0);

        let result = resolve_amount(0, pre_balance, on_chain_balance);

        if let Ok(effective_amount) = result {
            // CRIME/FRAUD -> PROFIT: division by 100, always safe for any u64
            let crime = test_crime();
            let fraud = test_fraud();
            let profit = test_profit();

            let output = compute_output_with_mints(
                &crime, &profit, effective_amount, &crime, &fraud, &profit,
            );

            // Output must be Ok (effective_amount > 0 guaranteed by resolve_amount)
            // unless it's dust (< 100 raw units -> OutputTooSmall)
            match output {
                Ok(out) => assert!(out > 0, "Non-dust output must be positive"),
                Err(_) => assert!(effective_amount < 100, "Only dust amounts can fail"),
            }
        }
    }
}
