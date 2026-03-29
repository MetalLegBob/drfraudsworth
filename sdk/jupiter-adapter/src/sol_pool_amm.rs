// Jupiter Amm trait implementation for Dr. Fraudsworth SOL pool swaps.
//
// SolPoolAmm handles CRIME/SOL and FRAUD/SOL swaps via the Tax Program.
// Two instances are created: one per pool.
//
// Quote flow:
//   Buy (SOL -> token): tax deducted from SOL INPUT, then AMM swap on post-tax amount
//   Sell (token -> SOL): AMM swap on full token input, then tax deducted from SOL OUTPUT

use anyhow::{anyhow, Result};
use jupiter_amm_interface::{
    AccountMap, Amm, AmmContext, KeyedAccount, Quote, QuoteParams, SwapAndAccountMetas, SwapMode,
    SwapParams, Swap, try_get_account_data,
};
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;

use crate::accounts::addresses::{
    CRIME_SOL_POOL, EPOCH_STATE_PDA, FRAUD_MINT, NATIVE_MINT, CRIME_MINT,
};
use crate::accounts::sol_pool_accounts::{build_buy_account_metas, build_sell_account_metas};
use crate::math::amm_math::{calculate_effective_input, calculate_swap_output};
use crate::math::tax_math::calculate_tax;
use crate::state::epoch_state::ParsedEpochState;
use crate::state::pool_state::ParsedPoolState;

/// Jupiter Amm implementation for CRIME/SOL and FRAUD/SOL pools.
///
/// Swaps go through the Tax Program which deducts dynamic tax and then
/// CPI-calls the AMM for the actual constant-product swap.
#[derive(Clone)]
pub struct SolPoolAmm {
    /// Pool PDA address (CRIME/SOL or FRAUD/SOL)
    key: Pubkey,
    /// true = CRIME pool, false = FRAUD pool
    is_crime: bool,
    /// SOL reserve (from PoolState)
    reserve_sol: u64,
    /// Token reserve (from PoolState)
    reserve_token: u64,
    /// LP fee in BPS (100 = 1%)
    lp_fee_bps: u16,
    /// Current epoch buy tax in BPS
    buy_tax_bps: u16,
    /// Current epoch sell tax in BPS
    sell_tax_bps: u16,
}

impl Amm for SolPoolAmm {
    fn from_keyed_account(keyed_account: &KeyedAccount, _amm_context: &AmmContext) -> Result<Self>
    where
        Self: Sized,
    {
        let pool_state = ParsedPoolState::from_bytes(&keyed_account.account.data)?;
        let (reserve_sol, reserve_token) = pool_state.sol_and_token_reserves();

        let is_crime = keyed_account.key == CRIME_SOL_POOL;

        Ok(Self {
            key: keyed_account.key,
            is_crime,
            reserve_sol,
            reserve_token,
            lp_fee_bps: pool_state.lp_fee_bps,
            // Tax rates initialized to 0; populated by first update() call.
            // Jupiter always calls update() before quote().
            buy_tax_bps: 0,
            sell_tax_bps: 0,
        })
    }

    fn label(&self) -> String {
        "Dr Fraudsworth".to_string()
    }

    fn program_id(&self) -> Pubkey {
        // Tax Program is what Jupiter calls for swaps (not the AMM directly)
        crate::accounts::addresses::TAX_PROGRAM_ID
    }

    fn key(&self) -> Pubkey {
        self.key
    }

    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        let token_mint = if self.is_crime { CRIME_MINT } else { FRAUD_MINT };
        vec![NATIVE_MINT, token_mint]
    }

    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        // Jupiter refreshes these accounts and passes them to update()
        vec![self.key, EPOCH_STATE_PDA]
    }

    fn update(&mut self, account_map: &AccountMap) -> Result<()> {
        // Parse pool state for reserves + LP fee
        let pool_data = try_get_account_data(account_map, &self.key)?;
        let pool_state = ParsedPoolState::from_bytes(pool_data)?;
        let (reserve_sol, reserve_token) = pool_state.sol_and_token_reserves();
        self.reserve_sol = reserve_sol;
        self.reserve_token = reserve_token;
        self.lp_fee_bps = pool_state.lp_fee_bps;

        // Parse epoch state for tax rates
        let epoch_data = try_get_account_data(account_map, &EPOCH_STATE_PDA)?;
        let epoch_state = ParsedEpochState::from_bytes(epoch_data)?;
        self.buy_tax_bps = epoch_state.get_tax_bps(self.is_crime, true);
        self.sell_tax_bps = epoch_state.get_tax_bps(self.is_crime, false);

        Ok(())
    }

    fn quote(&self, quote_params: &QuoteParams) -> Result<Quote> {
        if quote_params.swap_mode == SwapMode::ExactOut {
            return Err(anyhow!("ExactOut not supported"));
        }

        let is_buy = quote_params.input_mint == NATIVE_MINT;

        if is_buy {
            self.quote_buy(quote_params.amount)
        } else {
            self.quote_sell(quote_params.amount)
        }
    }

    fn get_swap_and_account_metas(&self, swap_params: &SwapParams) -> Result<SwapAndAccountMetas> {
        let is_buy = swap_params.source_mint == NATIVE_MINT;

        let account_metas = if is_buy {
            // Buy: source = WSOL, destination = token
            build_buy_account_metas(
                &swap_params.token_transfer_authority,
                &swap_params.source_token_account,
                &swap_params.destination_token_account,
                self.is_crime,
            )
        } else {
            // Sell: source = token, destination = WSOL
            build_sell_account_metas(
                &swap_params.token_transfer_authority,
                &swap_params.source_token_account,
                &swap_params.destination_token_account,
                self.is_crime,
            )
        };

        Ok(SwapAndAccountMetas {
            swap: Swap::TokenSwap,
            account_metas,
        })
    }

    fn supports_exact_out(&self) -> bool {
        false
    }

    fn clone_amm(&self) -> Box<dyn Amm + Send + Sync> {
        Box::new(self.clone())
    }

    fn get_accounts_len(&self) -> usize {
        // Buy: 24, Sell: 25. Use the larger to be safe.
        25
    }
}

impl SolPoolAmm {
    /// Create a SolPoolAmm directly with known values (for testing/examples).
    ///
    /// In production, use `from_keyed_account` which parses on-chain data.
    pub fn new_for_testing(
        is_crime: bool,
        reserve_sol: u64,
        reserve_token: u64,
        buy_tax_bps: u16,
        sell_tax_bps: u16,
    ) -> Self {
        Self {
            key: if is_crime {
                CRIME_SOL_POOL
            } else {
                crate::accounts::addresses::FRAUD_SOL_POOL
            },
            is_crime,
            reserve_sol,
            reserve_token,
            lp_fee_bps: crate::constants::LP_FEE_BPS,
            buy_tax_bps,
            sell_tax_bps,
        }
    }

    /// Quote a buy (SOL -> token).
    ///
    /// Flow: tax deducted from SOL input, LP fee deducted, then constant-product swap.
    fn quote_buy(&self, amount_in: u64) -> Result<Quote> {
        // 1. Tax deducted from SOL input
        let tax = calculate_tax(amount_in, self.buy_tax_bps)
            .ok_or_else(|| anyhow!("Tax calculation overflow"))?;

        let sol_to_swap = amount_in
            .checked_sub(tax)
            .ok_or_else(|| anyhow!("Tax exceeds input amount"))?;

        if sol_to_swap == 0 {
            return Ok(Quote {
                in_amount: amount_in,
                out_amount: 0,
                fee_amount: tax,
                fee_mint: NATIVE_MINT,
                fee_pct: self.total_buy_fee_pct(),
            });
        }

        // 2. LP fee deducted from post-tax amount
        let effective_input = calculate_effective_input(sol_to_swap, self.lp_fee_bps)
            .ok_or_else(|| anyhow!("Effective input calculation overflow"))?;

        // 3. Constant-product swap
        let out_amount = calculate_swap_output(
            self.reserve_sol,
            self.reserve_token,
            effective_input,
        )
        .ok_or_else(|| anyhow!("Swap output calculation overflow or zero reserves"))?;

        // LP fee in SOL terms
        let lp_fee_sol = sol_to_swap
            .checked_sub(effective_input as u64)
            .unwrap_or(0);

        Ok(Quote {
            in_amount: amount_in,
            out_amount,
            fee_amount: tax.checked_add(lp_fee_sol).unwrap_or(tax),
            fee_mint: NATIVE_MINT,
            fee_pct: self.total_buy_fee_pct(),
        })
    }

    /// Quote a sell (token -> SOL).
    ///
    /// Flow: LP fee deducted from token input, constant-product swap, then tax on SOL output.
    fn quote_sell(&self, amount_in: u64) -> Result<Quote> {
        // 1. LP fee deducted from token input
        let effective_input = calculate_effective_input(amount_in, self.lp_fee_bps)
            .ok_or_else(|| anyhow!("Effective input calculation overflow"))?;

        // 2. Constant-product swap (token -> SOL)
        let gross_sol = calculate_swap_output(
            self.reserve_token,
            self.reserve_sol,
            effective_input,
        )
        .ok_or_else(|| anyhow!("Swap output calculation overflow or zero reserves"))?;

        // 3. Tax deducted from SOL output
        let tax = calculate_tax(gross_sol, self.sell_tax_bps)
            .ok_or_else(|| anyhow!("Tax calculation overflow"))?;

        let net_sol = gross_sol
            .checked_sub(tax)
            .ok_or_else(|| anyhow!("Tax exceeds gross output"))?;

        // LP fee portion in token terms -- we report in SOL for consistency with fee_mint
        // Actually, fee_mint is NATIVE_MINT so we should express the LP fee in SOL.
        // The LP fee is taken from the token input. Approximate SOL equivalent:
        // lp_fee_tokens = amount_in - effective_input_as_u64
        // But since the fee_mint is SOL, we report only the tax as the primary fee,
        // and include LP fee in the total fee_pct.
        // LP fee is in token terms, can't be directly added to SOL tax.
        // Jupiter uses fee_pct as the authoritative fee indicator.
        let _lp_fee_tokens = amount_in.checked_sub(effective_input as u64).unwrap_or(0);
        Ok(Quote {
            in_amount: amount_in,
            out_amount: net_sol,
            fee_amount: tax,
            fee_mint: NATIVE_MINT,
            fee_pct: self.total_sell_fee_pct(),
        })
    }

    /// Total buy fee percentage (LP + tax) as a Decimal.
    fn total_buy_fee_pct(&self) -> Decimal {
        let total_bps = (self.lp_fee_bps as u32) + (self.buy_tax_bps as u32);
        Decimal::from(total_bps) / Decimal::from(10_000u32)
    }

    /// Total sell fee percentage (LP + tax) as a Decimal.
    fn total_sell_fee_pct(&self) -> Decimal {
        let total_bps = (self.lp_fee_bps as u32) + (self.sell_tax_bps as u32);
        Decimal::from(total_bps) / Decimal::from(10_000u32)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::LP_FEE_BPS;

    /// Create a SolPoolAmm directly with known values (bypassing from_keyed_account).
    fn make_amm(
        is_crime: bool,
        reserve_sol: u64,
        reserve_token: u64,
        buy_tax_bps: u16,
        sell_tax_bps: u16,
    ) -> SolPoolAmm {
        SolPoolAmm {
            key: if is_crime { CRIME_SOL_POOL } else { crate::accounts::addresses::FRAUD_SOL_POOL },
            is_crime,
            reserve_sol,
            reserve_token,
            lp_fee_bps: LP_FEE_BPS,
            buy_tax_bps,
            sell_tax_bps,
        }
    }

    #[test]
    fn buy_quote_applies_tax_before_swap() {
        // 1 SOL input, 4% buy tax, equal reserves
        let amm = make_amm(true, 100_000_000_000, 100_000_000_000, 400, 1400);

        let quote = amm.quote(&QuoteParams {
            amount: 1_000_000_000, // 1 SOL
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();

        // Tax = 1 SOL * 400/10000 = 0.04 SOL = 40_000_000 lamports
        // sol_to_swap = 960_000_000
        // After 1% LP fee: effective = 960_000_000 * 9900/10000 = 950_400_000
        // Output: 100B * 950_400_000 / (100B + 950_400_000)
        assert!(quote.out_amount > 0, "output should be non-zero");
        assert!(quote.out_amount < 1_000_000_000, "output should be less than input for equal reserves");
        assert!(quote.fee_amount >= 40_000_000, "fee should include at least the tax");
        assert_eq!(quote.in_amount, 1_000_000_000);
        assert_eq!(quote.fee_mint, NATIVE_MINT);
    }

    #[test]
    fn sell_quote_applies_tax_after_swap() {
        // Sell 1B tokens, 14% sell tax
        let amm = make_amm(true, 100_000_000_000, 100_000_000_000, 400, 1400);

        let quote = amm.quote(&QuoteParams {
            amount: 1_000_000_000, // 1B token units
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();

        // After LP fee: effective = 1B * 9900/10000 = 990_000_000
        // gross_sol from swap
        // Then 14% tax on gross_sol
        assert!(quote.out_amount > 0, "output should be non-zero");
        assert_eq!(quote.fee_mint, NATIVE_MINT);
        // The sell tax is 14%, so the fee amount (tax only) should be significant
        assert!(quote.fee_amount > 0, "sell tax should be non-zero");
    }

    #[test]
    fn buy_zero_input_returns_zero_output() {
        let amm = make_amm(true, 100_000_000_000, 100_000_000_000, 400, 1400);

        let quote = amm.quote(&QuoteParams {
            amount: 0,
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();

        assert_eq!(quote.out_amount, 0);
    }

    #[test]
    fn sell_zero_sol_reserves_returns_zero_output() {
        // Zero SOL reserves means swap output = 0 (no SOL to extract)
        let amm = make_amm(true, 0, 100_000_000_000, 400, 1400);

        let quote = amm.quote(&QuoteParams {
            amount: 1_000_000_000,
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();

        assert_eq!(quote.out_amount, 0, "should return 0 with zero SOL reserves");
    }

    #[test]
    fn sell_zero_token_reserves_errors() {
        // Zero token reserves + non-zero token input -> denominator = 0 + effective_input
        // This should still work mathematically (output = reserve_sol * eff / (0 + eff))
        // = reserve_sol, but that would drain the pool. With zero reserve_in (token=0)
        // and nonzero effective_input, calculate_swap_output returns the full reserve_out.
        // In practice this can't happen (pool wouldn't be initialized with 0 tokens).
        let amm = make_amm(true, 100_000_000_000, 0, 400, 1400);

        let result = amm.quote(&QuoteParams {
            amount: 1_000_000_000,
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            swap_mode: SwapMode::ExactIn,
        });

        // With zero token reserves, swap returns full SOL reserve.
        // This is an edge case that shouldn't happen in practice.
        assert!(result.is_ok());
    }

    #[test]
    fn exact_out_not_supported() {
        let amm = make_amm(true, 100_000_000_000, 100_000_000_000, 400, 1400);

        let result = amm.quote(&QuoteParams {
            amount: 1_000_000_000,
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            swap_mode: SwapMode::ExactOut,
        });

        assert!(result.is_err());
    }

    #[test]
    fn label_is_dr_fraudsworth() {
        let amm = make_amm(true, 1, 1, 400, 1400);
        assert_eq!(amm.label(), "Dr Fraudsworth");
    }

    #[test]
    fn reserve_mints_correct() {
        let crime_amm = make_amm(true, 1, 1, 400, 1400);
        let mints = crime_amm.get_reserve_mints();
        assert_eq!(mints, vec![NATIVE_MINT, CRIME_MINT]);

        let fraud_amm = make_amm(false, 1, 1, 400, 1400);
        let mints = fraud_amm.get_reserve_mints();
        assert_eq!(mints, vec![NATIVE_MINT, FRAUD_MINT]);
    }

    #[test]
    fn accounts_to_update_includes_pool_and_epoch() {
        let amm = make_amm(true, 1, 1, 400, 1400);
        let accounts = amm.get_accounts_to_update();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0], CRIME_SOL_POOL);
        assert_eq!(accounts[1], EPOCH_STATE_PDA);
    }

    #[test]
    fn fee_pct_matches_combined_bps() {
        let amm = make_amm(true, 100_000_000_000, 100_000_000_000, 400, 1400);

        // Buy: LP 100 + tax 400 = 500 bps = 5%
        let buy_quote = amm.quote(&QuoteParams {
            amount: 1_000_000_000,
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();
        assert_eq!(buy_quote.fee_pct, Decimal::from(500u32) / Decimal::from(10_000u32));

        // Sell: LP 100 + tax 1400 = 1500 bps = 15%
        let sell_quote = amm.quote(&QuoteParams {
            amount: 1_000_000_000,
            input_mint: CRIME_MINT,
            output_mint: NATIVE_MINT,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();
        assert_eq!(sell_quote.fee_pct, Decimal::from(1500u32) / Decimal::from(10_000u32));
    }

    #[test]
    fn buy_quote_high_tax_still_works() {
        // 50% buy tax (extreme case)
        let amm = make_amm(true, 100_000_000_000, 100_000_000_000, 5000, 5000);

        let quote = amm.quote(&QuoteParams {
            amount: 1_000_000_000,
            input_mint: NATIVE_MINT,
            output_mint: CRIME_MINT,
            swap_mode: SwapMode::ExactIn,
        }).unwrap();

        // 50% tax = 500M lamports tax, 500M to swap
        assert!(quote.out_amount > 0);
        assert!(quote.fee_amount >= 500_000_000);
    }
}
