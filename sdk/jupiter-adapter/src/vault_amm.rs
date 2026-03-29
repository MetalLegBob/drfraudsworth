// Jupiter Amm trait implementation for Dr. Fraudsworth Conversion Vault.
//
// VaultAmm handles 4 conversion directions at fixed rates:
//   CRIME -> PROFIT (divide by 100)
//   FRAUD -> PROFIT (divide by 100)
//   PROFIT -> CRIME (multiply by 100)
//   PROFIT -> FRAUD (multiply by 100)
//
// Each VaultAmm instance is unidirectional (input -> output).
// CRIME <-> FRAUD is NOT supported directly (Jupiter routes via multi-hop).
//
// Zero fees. Deterministic output amounts. No on-chain state changes needed for quoting.

use anyhow::{anyhow, Result};
use jupiter_amm_interface::{
    AccountMap, Amm, AmmContext, KeyedAccount, Quote, QuoteParams, Swap, SwapAndAccountMetas,
    SwapMode, SwapParams, try_get_account_data,
};
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;

use crate::accounts::addresses::{
    CONVERSION_VAULT_PROGRAM_ID, CRIME_MINT, CRIME_SOL_POOL, FRAUD_MINT, FRAUD_SOL_POOL,
    PROFIT_MINT, VAULT_CONFIG_PDA,
};
use crate::accounts::vault_accounts::build_vault_account_metas;
use crate::math::vault_math::compute_vault_output;

/// Jupiter Amm implementation for Conversion Vault (fixed-rate token conversions).
///
/// Each instance represents one unidirectional conversion (e.g., CRIME -> PROFIT).
/// 4 instances total, created via `known_instances()`.
#[derive(Clone)]
pub struct VaultAmm {
    /// Synthetic unique key for this instance (PDA derived from mint pair)
    key: Pubkey,
    /// Input token mint
    input_mint: Pubkey,
    /// Output token mint
    output_mint: Pubkey,
    /// Human-readable label suffix
    _label_suffix: String,
}

impl Amm for VaultAmm {
    fn from_keyed_account(keyed_account: &KeyedAccount, _amm_context: &AmmContext) -> Result<Self>
    where
        Self: Sized,
    {
        // Resolve mint pair from params JSON or from known synthetic key.
        // Try params first (standard Jupiter pattern).
        if let Some(params) = &keyed_account.params {
            let input_mint_str = params
                .get("input_mint")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("VaultAmm params missing 'input_mint'"))?;
            let output_mint_str = params
                .get("output_mint")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("VaultAmm params missing 'output_mint'"))?;

            let input_mint: Pubkey = input_mint_str
                .parse()
                .map_err(|_| anyhow!("Invalid input_mint pubkey: {}", input_mint_str))?;
            let output_mint: Pubkey = output_mint_str
                .parse()
                .map_err(|_| anyhow!("Invalid output_mint pubkey: {}", output_mint_str))?;

            let _label_suffix = format_label(&input_mint, &output_mint);

            return Ok(Self {
                key: keyed_account.key,
                input_mint,
                output_mint,
                _label_suffix,
            });
        }

        // Fallback: resolve from known synthetic key
        for (key, instance) in known_instances() {
            if key == keyed_account.key {
                return Ok(instance);
            }
        }

        Err(anyhow!(
            "VaultAmm: unknown key {} and no params provided",
            keyed_account.key
        ))
    }

    fn label(&self) -> String {
        "Dr Fraudsworth Vault".to_string()
    }

    fn program_id(&self) -> Pubkey {
        CONVERSION_VAULT_PROGRAM_ID
    }

    fn key(&self) -> Pubkey {
        self.key
    }

    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        vec![self.input_mint, self.output_mint]
    }

    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        // VaultConfig PDA: lets Jupiter verify the vault program is initialized.
        // No state extraction needed (rates are fixed), but Jupiter requires
        // at least one account for liveness checks.
        vec![VAULT_CONFIG_PDA]
    }

    fn update(&mut self, account_map: &AccountMap) -> Result<()> {
        // Minimal validation: confirm VaultConfig account exists and has data.
        let data = try_get_account_data(account_map, &VAULT_CONFIG_PDA)?;
        if data.is_empty() {
            return Err(anyhow!("VaultConfig account has no data"));
        }
        // No state to update -- conversion rates are fixed at 100:1.
        Ok(())
    }

    fn quote(&self, quote_params: &QuoteParams) -> Result<Quote> {
        if quote_params.swap_mode == SwapMode::ExactOut {
            return Err(anyhow!("ExactOut not supported for vault conversions"));
        }

        // Each VaultAmm instance is unidirectional.
        // Verify the input mint matches our expected direction.
        if quote_params.input_mint != self.input_mint {
            return Err(anyhow!(
                "VaultAmm: expected input_mint {}, got {}",
                self.input_mint,
                quote_params.input_mint
            ));
        }

        let out_amount = compute_vault_output(
            &self.input_mint,
            &self.output_mint,
            quote_params.amount,
        )
        .ok_or_else(|| {
            anyhow!(
                "Vault conversion failed for {} -> {} with amount {}. \
                 Possible causes: zero amount, dust too small (< 100 for divide), or overflow.",
                self.input_mint,
                self.output_mint,
                quote_params.amount
            )
        })?;

        Ok(Quote {
            in_amount: quote_params.amount,
            out_amount,
            fee_amount: 0,
            fee_mint: self.input_mint,
            fee_pct: Decimal::ZERO,
        })
    }

    fn get_swap_and_account_metas(&self, swap_params: &SwapParams) -> Result<SwapAndAccountMetas> {
        let account_metas = build_vault_account_metas(
            &swap_params.token_transfer_authority,
            &swap_params.source_token_account,
            &swap_params.destination_token_account,
            &self.input_mint,
            &self.output_mint,
        );

        Ok(SwapAndAccountMetas {
            swap: Swap::TokenSwap,
            account_metas,
        })
    }

    fn supports_exact_out(&self) -> bool {
        // Integer division in *->PROFIT direction loses information
        false
    }

    fn clone_amm(&self) -> Box<dyn Amm + Send + Sync> {
        Box::new(self.clone())
    }

    fn get_accounts_len(&self) -> usize {
        17 // 9 named + 8 hook accounts
    }

    fn unidirectional(&self) -> bool {
        true
    }
}

impl VaultAmm {
    /// Create a VaultAmm directly with known values (for testing/examples).
    ///
    /// In production, use `from_keyed_account` or `known_instances()`.
    pub fn new_for_testing(input_mint: Pubkey, output_mint: Pubkey) -> Self {
        let key = derive_synthetic_key(&input_mint, &output_mint);
        let _label_suffix = format_label(&input_mint, &output_mint);
        Self {
            key,
            input_mint,
            output_mint,
            _label_suffix,
        }
    }
}

// =============================================================================
// Factory functions for Jupiter pool discovery
// =============================================================================

/// Returns all 4 known VaultAmm instances for Jupiter pool discovery.
///
/// Jupiter integrators call this during startup to register all fixed-pool
/// Amm instances. Since VaultAmm has no on-chain pool account to discover
/// via getProgramAccounts, this factory is the standard pattern for
/// fixed-rate protocols.
///
/// Returns 4 instances:
/// - CRIME -> PROFIT (divide by 100)
/// - FRAUD -> PROFIT (divide by 100)
/// - PROFIT -> CRIME (multiply by 100)
/// - PROFIT -> FRAUD (multiply by 100)
pub fn known_instances() -> Vec<(Pubkey, VaultAmm)> {
    let pairs = [
        (CRIME_MINT, PROFIT_MINT, "CRIME/PROFIT Vault"),
        (FRAUD_MINT, PROFIT_MINT, "FRAUD/PROFIT Vault"),
        (PROFIT_MINT, CRIME_MINT, "PROFIT/CRIME Vault"),
        (PROFIT_MINT, FRAUD_MINT, "PROFIT/FRAUD Vault"),
    ];

    pairs
        .iter()
        .map(|(input, output, label)| {
            let key = derive_synthetic_key(input, output);
            let instance = VaultAmm {
                key,
                input_mint: *input,
                output_mint: *output,
                _label_suffix: label.to_string(),
            };
            (key, instance)
        })
        .collect()
}

/// Returns the 2 SOL pool keys for SolPoolAmm (created via from_keyed_account).
///
/// Jupiter uses `getMultipleAccounts` on these keys to construct SolPoolAmm instances.
pub fn known_sol_pool_keys() -> Vec<Pubkey> {
    vec![CRIME_SOL_POOL, FRAUD_SOL_POOL]
}

/// Returns all 6 Amm instance keys for Jupiter integration.
///
/// - 2 SOL pool keys (for SolPoolAmm via from_keyed_account)
/// - 4 vault synthetic keys (for VaultAmm via known_instances)
pub fn all_pool_keys() -> Vec<Pubkey> {
    let mut keys = known_sol_pool_keys();
    for (key, _) in known_instances() {
        keys.push(key);
    }
    keys
}

// =============================================================================
// Helpers
// =============================================================================

/// Derive a deterministic synthetic key for a vault conversion pair.
///
/// Uses `Pubkey::find_program_address(&[b"jup_vault", input_mint, output_mint], vault_program)`.
/// These are unique per mint pair and don't need to be real on-chain accounts.
fn derive_synthetic_key(input_mint: &Pubkey, output_mint: &Pubkey) -> Pubkey {
    let (key, _bump) = Pubkey::find_program_address(
        &[b"jup_vault", input_mint.as_ref(), output_mint.as_ref()],
        &CONVERSION_VAULT_PROGRAM_ID,
    );
    key
}

/// Format a human-readable label for a vault conversion pair.
fn format_label(input_mint: &Pubkey, output_mint: &Pubkey) -> String {
    let input_name = mint_name(input_mint);
    let output_name = mint_name(output_mint);
    format!("{}/{} Vault", input_name, output_name)
}

/// Map a mint Pubkey to its human-readable name.
fn mint_name(mint: &Pubkey) -> &'static str {
    if *mint == CRIME_MINT {
        "CRIME"
    } else if *mint == FRAUD_MINT {
        "FRAUD"
    } else if *mint == PROFIT_MINT {
        "PROFIT"
    } else {
        "UNKNOWN"
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crime_to_profit_quote() {
        let instances = known_instances();
        let (_, amm) = &instances[0]; // CRIME -> PROFIT

        let quote = amm
            .quote(&QuoteParams {
                amount: 10_000,
                input_mint: CRIME_MINT,
                output_mint: PROFIT_MINT,
                swap_mode: SwapMode::ExactIn,
            })
            .unwrap();

        assert_eq!(quote.out_amount, 100); // 10000 / 100 = 100
        assert_eq!(quote.fee_amount, 0);
        assert_eq!(quote.fee_pct, Decimal::ZERO);
    }

    #[test]
    fn profit_to_fraud_quote() {
        let instances = known_instances();
        let (_, amm) = &instances[3]; // PROFIT -> FRAUD

        let quote = amm
            .quote(&QuoteParams {
                amount: 50,
                input_mint: PROFIT_MINT,
                output_mint: FRAUD_MINT,
                swap_mode: SwapMode::ExactIn,
            })
            .unwrap();

        assert_eq!(quote.out_amount, 5000); // 50 * 100 = 5000
        assert_eq!(quote.fee_amount, 0);
    }

    #[test]
    fn small_input_crime_to_profit_errors() {
        let instances = known_instances();
        let (_, amm) = &instances[0]; // CRIME -> PROFIT

        let result = amm.quote(&QuoteParams {
            amount: 99, // 99 / 100 = 0 -> error
            input_mint: CRIME_MINT,
            output_mint: PROFIT_MINT,
            swap_mode: SwapMode::ExactIn,
        });

        assert!(result.is_err(), "99 CRIME / 100 = 0 PROFIT should error");
    }

    #[test]
    fn known_instances_returns_4() {
        let instances = known_instances();
        assert_eq!(instances.len(), 4);

        // Verify all keys are unique
        let keys: Vec<Pubkey> = instances.iter().map(|(k, _)| *k).collect();
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i], keys[j], "Synthetic keys must be unique");
            }
        }
    }

    #[test]
    fn known_sol_pool_keys_returns_2() {
        let keys = known_sol_pool_keys();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], CRIME_SOL_POOL);
        assert_eq!(keys[1], FRAUD_SOL_POOL);
    }

    #[test]
    fn all_pool_keys_returns_6() {
        let keys = all_pool_keys();
        assert_eq!(keys.len(), 6);

        // First 2 are SOL pool keys
        assert_eq!(keys[0], CRIME_SOL_POOL);
        assert_eq!(keys[1], FRAUD_SOL_POOL);

        // Last 4 are synthetic vault keys
        let vault_keys: Vec<Pubkey> = known_instances().iter().map(|(k, _)| *k).collect();
        assert_eq!(keys[2], vault_keys[0]);
        assert_eq!(keys[3], vault_keys[1]);
        assert_eq!(keys[4], vault_keys[2]);
        assert_eq!(keys[5], vault_keys[3]);
    }

    #[test]
    fn synthetic_keys_are_deterministic() {
        let key1 = derive_synthetic_key(&CRIME_MINT, &PROFIT_MINT);
        let key2 = derive_synthetic_key(&CRIME_MINT, &PROFIT_MINT);
        assert_eq!(key1, key2);
    }

    #[test]
    fn reverse_direction_has_different_key() {
        let key_fwd = derive_synthetic_key(&CRIME_MINT, &PROFIT_MINT);
        let key_rev = derive_synthetic_key(&PROFIT_MINT, &CRIME_MINT);
        assert_ne!(key_fwd, key_rev);
    }

    #[test]
    fn vault_amm_is_unidirectional() {
        let instances = known_instances();
        let (_, amm) = &instances[0];
        assert!(amm.unidirectional());
    }

    #[test]
    fn label_is_dr_fraudsworth_vault() {
        let instances = known_instances();
        let (_, amm) = &instances[0];
        assert_eq!(amm.label(), "Dr Fraudsworth Vault");
    }

    #[test]
    fn program_id_is_vault() {
        let instances = known_instances();
        let (_, amm) = &instances[0];
        assert_eq!(amm.program_id(), CONVERSION_VAULT_PROGRAM_ID);
    }

    #[test]
    fn reserve_mints_match_direction() {
        let instances = known_instances();

        // CRIME -> PROFIT
        let (_, amm) = &instances[0];
        let mints = amm.get_reserve_mints();
        assert_eq!(mints, vec![CRIME_MINT, PROFIT_MINT]);

        // PROFIT -> FRAUD
        let (_, amm) = &instances[3];
        let mints = amm.get_reserve_mints();
        assert_eq!(mints, vec![PROFIT_MINT, FRAUD_MINT]);
    }

    #[test]
    fn exact_out_not_supported() {
        let instances = known_instances();
        let (_, amm) = &instances[0];

        let result = amm.quote(&QuoteParams {
            amount: 10_000,
            input_mint: CRIME_MINT,
            output_mint: PROFIT_MINT,
            swap_mode: SwapMode::ExactOut,
        });

        assert!(result.is_err());
    }

    #[test]
    fn wrong_input_mint_errors() {
        let instances = known_instances();
        let (_, amm) = &instances[0]; // CRIME -> PROFIT

        let result = amm.quote(&QuoteParams {
            amount: 10_000,
            input_mint: FRAUD_MINT, // Wrong -- expects CRIME
            output_mint: PROFIT_MINT,
            swap_mode: SwapMode::ExactIn,
        });

        assert!(result.is_err());
    }

    #[test]
    fn accounts_to_update_has_vault_config() {
        let instances = known_instances();
        let (_, amm) = &instances[0];
        let accounts = amm.get_accounts_to_update();
        assert_eq!(accounts, vec![VAULT_CONFIG_PDA]);
    }
}
