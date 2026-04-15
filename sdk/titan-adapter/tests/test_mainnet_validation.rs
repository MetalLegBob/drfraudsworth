// Phase 8.4: Mainnet RPC Validation
//
// Constructs venues from REAL mainnet account data (fetched 2026-03-30)
// and verifies quotes produce sane results.
//
// This catches:
// - Byte offset bugs that only manifest with real data
// - Stale hardcoded addresses
// - Discriminator mismatches with live accounts

use std::collections::HashMap;
use async_trait::async_trait;
use solana_sdk::account::Account;
use solana_sdk::pubkey::Pubkey;
use titan_integration_template::account_caching::{AccountCacheError, AccountsCache};
use titan_integration_template::trading_venue::{
    FromAccount, QuoteRequest, SwapType, TradingVenue,
};

use drfraudsworth_titan_adapter::accounts::addresses::*;
use drfraudsworth_titan_adapter::sol_pool_venue::SolPoolVenue;
use drfraudsworth_titan_adapter::vault_venue::VaultVenue;

// =============================================================================
// Real mainnet account data (fetched 2026-03-30)
// =============================================================================

// CRIME/SOL pool: 224 bytes, owner=AMM_PROGRAM_ID
// reserve_a=536,359,564,218 (~536 SOL), reserve_b=274,453,124,499,931 (~274T tokens)
const CRIME_POOL_HEX: &str = "f7ede3f5d7c3de4600069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f0000000000109134573ad65aad688e3a59dbac1022ea9152f3e64e6753c6c8b8f4c88d85d2700fc6d2c6f43b2c0024d48a5403929af7eedecf8aab6a1aaae0c6972d120936a571fd3b6c0f3e3ff9e8a33dd5196c7d9d0d5e04608be2397293746594a086542ba7385e17c000000db49fe189df9000064000100fffeff06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a906ddf6e1ee758fde18425dbce46ccddab61afc4d83b90d27febdf928d8a18bfc";

// FRAUD/SOL pool: 224 bytes, owner=AMM_PROGRAM_ID
// reserve_a=532,287,493,789 (~532 SOL), reserve_b=277,034,220,247,811 (~277T tokens)
const FRAUD_POOL_HEX: &str = "f7ede3f5d7c3de4600069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f00000000001dcb6edb69e6c5568401b5cbe4ef39fb349f28a21600af3ff6570c64ec4158f102aa53245d63de0e5354ee107fbe3c25b196b14d292a9e924780cd442f5888b701aa43833c243b98db7ea60a5cec425bfee39436c07cede06f7ad893b37974fb29d96ceee7b00000003db490ef6fb000064000100fffeff06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a906ddf6e1ee758fde18425dbce46ccddab61afc4d83b90d27febdf928d8a18bfc";

// EpochState: 172 bytes, owner=EPOCH_PROGRAM_ID
// crime_buy=300bps (3%), crime_sell=1100bps (11%)
// fraud_buy=1200bps (12%), fraud_sell=400bps (4%)
const EPOCH_STATE_HEX: &str = "bf3f8bed900cdfd20cf03c1800000000d40200009ca66e1800000000002c01b0042c014c04b0049001f6a66e1800000000000193caf4458e03ae8b293942525851761a4a586c84d6f968d8bef5ecd9a7ae136e000000c0ac6a1800000000c6ab6a18000000009a0200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001fe";

// VaultConfig: 9 bytes, owner=CONVERSION_VAULT_PROGRAM_ID
const VAULT_CONFIG_HEX: &str = "63562bd8b866774dff";

fn decode_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

// =============================================================================
// Mock cache with real mainnet data
// =============================================================================

struct MainnetCache {
    accounts: HashMap<Pubkey, Account>,
}

impl MainnetCache {
    fn new() -> Self {
        let mut accounts = HashMap::new();

        accounts.insert(CRIME_SOL_POOL, Account {
            lamports: 2_449_920,
            data: decode_hex(CRIME_POOL_HEX),
            owner: AMM_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        });

        accounts.insert(FRAUD_SOL_POOL, Account {
            lamports: 2_449_920,
            data: decode_hex(FRAUD_POOL_HEX),
            owner: AMM_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        });

        accounts.insert(EPOCH_STATE_PDA, Account {
            lamports: 2_088_000,
            data: decode_hex(EPOCH_STATE_HEX),
            owner: EPOCH_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        });

        accounts.insert(VAULT_CONFIG_PDA, Account {
            lamports: 953_520,
            data: decode_hex(VAULT_CONFIG_HEX),
            owner: CONVERSION_VAULT_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        });

        Self { accounts }
    }
}

#[async_trait]
impl AccountsCache for MainnetCache {
    async fn get_account(&self, pubkey: &Pubkey) -> Result<Option<Account>, AccountCacheError> {
        Ok(self.accounts.get(pubkey).cloned())
    }
    async fn get_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Account>>, AccountCacheError> {
        Ok(pubkeys.iter().map(|pk| self.accounts.get(pk).cloned()).collect())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn crime_pool_from_account_parses_real_data() {
    let data = decode_hex(CRIME_POOL_HEX);
    let account = Account {
        lamports: 2_449_920,
        data,
        owner: AMM_PROGRAM_ID,
        executable: false,
        rent_epoch: 0,
    };

    let venue = SolPoolVenue::from_account(&CRIME_SOL_POOL, &account).unwrap();
    assert_eq!(venue.market_id(), CRIME_SOL_POOL);
    // Can access token info
    assert_eq!(venue.get_token_info().len(), 2);
}

#[test]
fn fraud_pool_from_account_parses_real_data() {
    let data = decode_hex(FRAUD_POOL_HEX);
    let account = Account {
        lamports: 2_449_920,
        data,
        owner: AMM_PROGRAM_ID,
        executable: false,
        rent_epoch: 0,
    };

    let venue = SolPoolVenue::from_account(&FRAUD_SOL_POOL, &account).unwrap();
    assert_eq!(venue.market_id(), FRAUD_SOL_POOL);
}

#[tokio::test]
async fn crime_pool_update_state_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(true, CRIME_SOL_POOL, CRIME_MINT);

    venue.update_state(&cache).await.unwrap();
    assert!(venue.initialized());
}

#[tokio::test]
async fn fraud_pool_update_state_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(false, FRAUD_SOL_POOL, FRAUD_MINT);

    venue.update_state(&cache).await.unwrap();
    assert!(venue.initialized());
}

#[tokio::test]
async fn crime_pool_buy_quote_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(true, CRIME_SOL_POOL, CRIME_MINT);
    venue.update_state(&cache).await.unwrap();

    // Buy 1 SOL worth of CRIME
    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        amount: 1_000_000_000, // 1 SOL
        swap_type: SwapType::ExactIn,
    }).unwrap();

    // With ~536 SOL / ~274T tokens and 3% buy tax:
    // Should get a meaningful amount of tokens
    eprintln!("CRIME buy 1 SOL: {} tokens out", result.expected_output);
    assert!(result.expected_output > 0, "Should produce tokens");
    assert!(result.expected_output < 1_000_000_000_000, "Shouldn't exceed reasonable bounds");
    assert!(!result.not_enough_liquidity, "1 SOL should be well within pool capacity");
}

#[tokio::test]
async fn crime_pool_sell_quote_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(true, CRIME_SOL_POOL, CRIME_MINT);
    venue.update_state(&cache).await.unwrap();

    // Sell 1M CRIME tokens (~1 token at 6 decimals)
    let result = venue.quote(QuoteRequest {
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        amount: 1_000_000, // 1 CRIME token
        swap_type: SwapType::ExactIn,
    }).unwrap();

    eprintln!("CRIME sell 1 token: {} lamports out", result.expected_output);
    assert!(result.expected_output > 0, "Should produce some SOL");
    assert!(!result.not_enough_liquidity);
}

#[tokio::test]
async fn fraud_pool_buy_quote_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(false, FRAUD_SOL_POOL, FRAUD_MINT);
    venue.update_state(&cache).await.unwrap();

    let result = venue.quote(QuoteRequest {
        input_mint: NATIVE_MINT,
        output_mint: FRAUD_MINT,
        amount: 1_000_000_000,
        swap_type: SwapType::ExactIn,
    }).unwrap();

    eprintln!("FRAUD buy 1 SOL: {} tokens out", result.expected_output);
    assert!(result.expected_output > 0);
    assert!(!result.not_enough_liquidity);
}

#[tokio::test]
async fn fraud_pool_sell_quote_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(false, FRAUD_SOL_POOL, FRAUD_MINT);
    venue.update_state(&cache).await.unwrap();

    let result = venue.quote(QuoteRequest {
        input_mint: FRAUD_MINT,
        output_mint: NATIVE_MINT,
        amount: 1_000_000_000, // 1000 FRAUD tokens
        swap_type: SwapType::ExactIn,
    }).unwrap();

    eprintln!("FRAUD sell 1000 tokens: {} lamports out", result.expected_output);
    assert!(result.expected_output > 0);
}

#[tokio::test]
async fn vault_update_state_with_real_data() {
    let cache = MainnetCache::new();

    for mut venue in drfraudsworth_titan_adapter::vault_venue::known_vault_venues() {
        venue.update_state(&cache).await.unwrap();
        assert!(venue.initialized(), "Vault venue should initialize from real VaultConfig");
    }
}

#[tokio::test]
async fn vault_quotes_with_real_data() {
    // Vault doesn't need state update for quoting (fixed rate), but let's be thorough
    let venues = [
        (CRIME_MINT, PROFIT_MINT, 10_000_000u64, "CRIME→PROFIT"),
        (FRAUD_MINT, PROFIT_MINT, 10_000_000, "FRAUD→PROFIT"),
        (PROFIT_MINT, CRIME_MINT, 100_000, "PROFIT→CRIME"),
        (PROFIT_MINT, FRAUD_MINT, 100_000, "PROFIT→FRAUD"),
    ];

    for (input, output, amount, label) in &venues {
        let venue = VaultVenue::new_for_testing(*input, *output);
        let result = venue.quote(QuoteRequest {
            input_mint: *input,
            output_mint: *output,
            amount: *amount,
            swap_type: SwapType::ExactIn,
        }).unwrap();

        eprintln!("{}: {} in → {} out", label, amount, result.expected_output);
        assert!(result.expected_output > 0);
    }
}

#[tokio::test]
async fn epoch_state_discriminator_matches_live() {
    // The most critical byte-level check: our hardcoded discriminator must match
    // the live EpochState account's first 8 bytes
    let data = decode_hex(EPOCH_STATE_HEX);
    let live_disc = &data[0..8];
    let expected_disc = drfraudsworth_titan_adapter::constants::EPOCH_STATE_DISCRIMINATOR;
    assert_eq!(live_disc, expected_disc,
        "Hardcoded EpochState discriminator does not match live mainnet data!");
}

#[tokio::test]
async fn crime_pool_bounds_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(true, CRIME_SOL_POOL, CRIME_MINT);
    venue.update_state(&cache).await.unwrap();

    let (lower, upper) = venue.bounds(0, 1).unwrap(); // SOL → CRIME
    eprintln!("CRIME buy bounds: {} - {} lamports", lower, upper);
    assert!(lower > 0);
    assert!(upper > lower);
    assert!(upper > 1_000_000_000, "Upper bound should allow at least 1 SOL");
}

#[tokio::test]
async fn generate_instruction_with_real_data() {
    let cache = MainnetCache::new();
    let mut venue = SolPoolVenue::new_uninitialized(true, CRIME_SOL_POOL, CRIME_MINT);
    venue.update_state(&cache).await.unwrap();

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
    assert_eq!(ix.accounts.len(), 24);
    assert_eq!(ix.accounts[0].pubkey, user);
    assert!(ix.accounts[0].is_signer);
    eprintln!("Generated mainnet buy instruction: {} accounts, {} data bytes", ix.accounts.len(), ix.data.len());
}
