// Example: Generating quotes with the Dr. Fraudsworth Jupiter adapter SDK.
//
// Demonstrates:
//   1. Creating a SolPoolAmm with test reserves and getting buy/sell quotes
//   2. Creating a VaultAmm and getting a vault conversion quote
//   3. Using known_instances() and known_sol_pool_keys() for pool discovery
//
// Run: cargo run --example quote_example -p drfraudsworth-jupiter-adapter

use drfraudsworth_jupiter_adapter::{
    SolPoolAmm, VaultAmm, known_instances, known_sol_pool_keys,
    accounts::addresses::{CRIME_MINT, FRAUD_MINT, PROFIT_MINT, NATIVE_MINT},
};
use jupiter_amm_interface::{Amm, QuoteParams, SwapMode};

fn main() {
    println!("=== Dr. Fraudsworth Jupiter Adapter SDK ===\n");

    // -------------------------------------------------------------------------
    // 1. SOL Pool: Buy quote (SOL -> CRIME)
    // -------------------------------------------------------------------------
    // In production, SolPoolAmm is created via from_keyed_account() with live
    // on-chain data. For this example we use new_for_testing() with sample values.
    let sol_amm = SolPoolAmm::new_for_testing(
        true,             // is_crime = true (CRIME/SOL pool)
        50_000_000_000,   // 50 SOL reserve
        500_000_000_000,  // 500B CRIME token reserve
        400,              // 4% buy tax (cheap side)
        1400,             // 14% sell tax (expensive side)
    );

    let buy_quote = sol_amm.quote(&QuoteParams {
        amount: 1_000_000_000, // 1 SOL input
        input_mint: NATIVE_MINT,
        output_mint: CRIME_MINT,
        swap_mode: SwapMode::ExactIn,
    }).expect("buy quote failed");

    println!("1. Buy CRIME with 1 SOL:");
    println!("   Input:   {} lamports (1 SOL)", buy_quote.in_amount);
    println!("   Output:  {} CRIME tokens", buy_quote.out_amount);
    println!("   Fee:     {} lamports (SOL)", buy_quote.fee_amount);
    println!("   Fee %:   {}", buy_quote.fee_pct);
    println!();

    // -------------------------------------------------------------------------
    // 2. SOL Pool: Sell quote (CRIME -> SOL)
    // -------------------------------------------------------------------------
    let sell_quote = sol_amm.quote(&QuoteParams {
        amount: 10_000_000_000, // 10B CRIME tokens
        input_mint: CRIME_MINT,
        output_mint: NATIVE_MINT,
        swap_mode: SwapMode::ExactIn,
    }).expect("sell quote failed");

    println!("2. Sell 10B CRIME for SOL:");
    println!("   Input:   {} CRIME tokens", sell_quote.in_amount);
    println!("   Output:  {} lamports", sell_quote.out_amount);
    println!("   Fee:     {} lamports (SOL tax)", sell_quote.fee_amount);
    println!("   Fee %:   {}", sell_quote.fee_pct);
    println!();

    // -------------------------------------------------------------------------
    // 3. Vault: CRIME -> PROFIT conversion (divide by 100)
    // -------------------------------------------------------------------------
    let vault_amm = VaultAmm::new_for_testing(CRIME_MINT, PROFIT_MINT);

    let vault_quote = vault_amm.quote(&QuoteParams {
        amount: 100_000_000_000, // 100B CRIME
        input_mint: CRIME_MINT,
        output_mint: PROFIT_MINT,
        swap_mode: SwapMode::ExactIn,
    }).expect("vault quote failed");

    println!("3. Convert 100B CRIME -> PROFIT:");
    println!("   Input:   {} CRIME", vault_quote.in_amount);
    println!("   Output:  {} PROFIT (100:1 ratio)", vault_quote.out_amount);
    println!("   Fee:     {} (zero fees)", vault_quote.fee_amount);
    println!();

    // -------------------------------------------------------------------------
    // 4. Vault: PROFIT -> FRAUD conversion (multiply by 100)
    // -------------------------------------------------------------------------
    let reverse_vault = VaultAmm::new_for_testing(PROFIT_MINT, FRAUD_MINT);

    let reverse_quote = reverse_vault.quote(&QuoteParams {
        amount: 1_000_000_000, // 1B PROFIT
        input_mint: PROFIT_MINT,
        output_mint: FRAUD_MINT,
        swap_mode: SwapMode::ExactIn,
    }).expect("reverse vault quote failed");

    println!("4. Convert 1B PROFIT -> FRAUD:");
    println!("   Input:   {} PROFIT", reverse_quote.in_amount);
    println!("   Output:  {} FRAUD (1:100 ratio)", reverse_quote.out_amount);
    println!("   Fee:     {} (zero fees)", reverse_quote.fee_amount);
    println!();

    // -------------------------------------------------------------------------
    // 5. Pool Discovery: list all Amm instances
    // -------------------------------------------------------------------------
    println!("5. Pool Discovery:");
    println!();

    let sol_keys = known_sol_pool_keys();
    println!("   SOL Pools ({} instances, created via from_keyed_account):", sol_keys.len());
    for key in &sol_keys {
        println!("     - {}", key);
    }
    println!();

    let vault_instances = known_instances();
    println!("   Vault Conversions ({} instances, pre-built):", vault_instances.len());
    for (key, amm) in &vault_instances {
        let mints = amm.get_reserve_mints();
        let input_name = mint_name(&mints[0]);
        let output_name = mint_name(&mints[1]);
        println!("     - {} -> {} (key: {})", input_name, output_name, key);
    }
    println!();

    println!("   Total: {} Amm instances covering 8 swap directions", sol_keys.len() + vault_instances.len());
}

fn mint_name(mint: &solana_sdk::pubkey::Pubkey) -> &'static str {
    if *mint == CRIME_MINT { "CRIME" }
    else if *mint == FRAUD_MINT { "FRAUD" }
    else if *mint == PROFIT_MINT { "PROFIT" }
    else if *mint == NATIVE_MINT { "SOL" }
    else { "UNKNOWN" }
}
