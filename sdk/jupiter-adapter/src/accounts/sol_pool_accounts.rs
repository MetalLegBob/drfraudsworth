// AccountMeta builders for Tax Program SOL pool swap instructions.
//
// These match the EXACT account ordering of:
// - SwapSolBuy struct (20 named accounts) in programs/tax-program/src/instructions/swap_sol_buy.rs
// - SwapSolSell struct (21 named accounts) in programs/tax-program/src/instructions/swap_sol_sell.rs
//
// After the named accounts, transfer hook remaining_accounts are appended
// (4 per T22 mint) for the CRIME or FRAUD token side.

use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use super::addresses::{
    AMM_PROGRAM_ID, CARNAGE_SOL_VAULT_PDA, CRIME_MINT, CRIME_SOL_POOL, CRIME_SOL_VAULT_A,
    CRIME_SOL_VAULT_B, EPOCH_STATE_PDA, ESCROW_VAULT_PDA, FRAUD_MINT, FRAUD_SOL_POOL,
    FRAUD_SOL_VAULT_A, FRAUD_SOL_VAULT_B, NATIVE_MINT, SPL_TOKEN_PROGRAM_ID, STAKING_PROGRAM_ID,
    STAKE_POOL_PDA, SWAP_AUTHORITY_PDA, SYSTEM_PROGRAM_ID, TAX_AUTHORITY_PDA, TOKEN_2022_PROGRAM_ID,
    TREASURY, WSOL_INTERMEDIARY_PDA,
};
use super::hook_accounts::hook_metas_for_mint;

/// Build the 20-account list for SwapSolBuy (SOL -> CRIME/FRAUD).
///
/// Plus 4 transfer hook accounts for the T22 output token mint.
/// Total: 24 AccountMetas.
///
/// # Arguments
/// * `user` - User wallet (signer, mutable)
/// * `user_wsol_ata` - User's WSOL token account
/// * `user_token_ata` - User's CRIME/FRAUD token account
/// * `is_crime` - true = CRIME pool, false = FRAUD pool
pub fn build_buy_account_metas(
    user: &Pubkey,
    user_wsol_ata: &Pubkey,
    user_token_ata: &Pubkey,
    is_crime: bool,
) -> Vec<AccountMeta> {
    let (pool, vault_a, vault_b, token_mint) = pool_addresses(is_crime);

    let mut metas = vec![
        // 1. user (signer, mut)
        AccountMeta::new(*user, true),
        // 2. epoch_state (readonly)
        AccountMeta::new_readonly(EPOCH_STATE_PDA, false),
        // 3. swap_authority (readonly)
        AccountMeta::new_readonly(SWAP_AUTHORITY_PDA, false),
        // 4. tax_authority (readonly)
        AccountMeta::new_readonly(TAX_AUTHORITY_PDA, false),
        // 5. pool (mut)
        AccountMeta::new(pool, false),
        // 6. pool_vault_a (mut) -- SOL vault
        AccountMeta::new(vault_a, false),
        // 7. pool_vault_b (mut) -- Token vault
        AccountMeta::new(vault_b, false),
        // 8. mint_a (readonly) -- NATIVE_MINT (WSOL)
        AccountMeta::new_readonly(NATIVE_MINT, false),
        // 9. mint_b (readonly) -- CRIME or FRAUD mint
        AccountMeta::new_readonly(token_mint, false),
        // 10. user_token_a (mut) -- user's WSOL ATA
        AccountMeta::new(*user_wsol_ata, false),
        // 11. user_token_b (mut) -- user's CRIME/FRAUD ATA
        AccountMeta::new(*user_token_ata, false),
        // 12. stake_pool (mut)
        AccountMeta::new(STAKE_POOL_PDA, false),
        // 13. staking_escrow (mut)
        AccountMeta::new(ESCROW_VAULT_PDA, false),
        // 14. carnage_vault (mut)
        AccountMeta::new(CARNAGE_SOL_VAULT_PDA, false),
        // 15. treasury (mut)
        AccountMeta::new(TREASURY, false),
        // 16. amm_program (readonly)
        AccountMeta::new_readonly(AMM_PROGRAM_ID, false),
        // 17. token_program_a (readonly) -- SPL Token
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        // 18. token_program_b (readonly) -- Token-2022
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM_ID, false),
        // 19. system_program (readonly)
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        // 20. staking_program (readonly)
        AccountMeta::new_readonly(STAKING_PROGRAM_ID, false),
    ];

    // Append transfer hook accounts for the T22 token mint (output side for buy).
    // For buy: SOL -> token, so hooks are for the output token mint.
    // Source = pool_vault_b (AMM sends from vault), Dest = user_token_ata (user receives)
    // BUT: Tax Program calls AMM CPI, which calls token transfer. The transfer is:
    //   pool_vault_b -> user_token_b for the output side.
    // The hook accounts use user_token_a (WSOL) for SOL side and user_token_b for token side,
    // but since SOL is SPL Token (no hooks), we only need hooks for the T22 side.
    let hook_metas = hook_metas_for_mint(&token_mint, &vault_b, user_token_ata);
    metas.extend(hook_metas);

    metas
}

/// Build the 21-account list for SwapSolSell (CRIME/FRAUD -> SOL).
///
/// Plus 4 transfer hook accounts for the T22 input token mint.
/// Total: 25 AccountMetas.
///
/// # Arguments
/// * `user` - User wallet (signer, mutable)
/// * `user_token_ata` - User's CRIME/FRAUD token account (input)
/// * `user_wsol_ata` - User's WSOL token account (output)
/// * `is_crime` - true = CRIME pool, false = FRAUD pool
pub fn build_sell_account_metas(
    user: &Pubkey,
    user_token_ata: &Pubkey,
    user_wsol_ata: &Pubkey,
    is_crime: bool,
) -> Vec<AccountMeta> {
    let (pool, vault_a, vault_b, token_mint) = pool_addresses(is_crime);

    let mut metas = vec![
        // 1. user (signer, mut)
        AccountMeta::new(*user, true),
        // 2. epoch_state (readonly)
        AccountMeta::new_readonly(EPOCH_STATE_PDA, false),
        // 3. swap_authority (mut for sell -- receives lamports from close_account)
        AccountMeta::new(SWAP_AUTHORITY_PDA, false),
        // 4. tax_authority (readonly)
        AccountMeta::new_readonly(TAX_AUTHORITY_PDA, false),
        // 5. pool (mut)
        AccountMeta::new(pool, false),
        // 6. pool_vault_a (mut) -- SOL vault
        AccountMeta::new(vault_a, false),
        // 7. pool_vault_b (mut) -- Token vault
        AccountMeta::new(vault_b, false),
        // 8. mint_a (readonly) -- NATIVE_MINT (WSOL)
        AccountMeta::new_readonly(NATIVE_MINT, false),
        // 9. mint_b (readonly) -- CRIME or FRAUD mint
        AccountMeta::new_readonly(token_mint, false),
        // 10. user_token_a (mut) -- user's WSOL ATA
        AccountMeta::new(*user_wsol_ata, false),
        // 11. user_token_b (mut) -- user's CRIME/FRAUD ATA
        AccountMeta::new(*user_token_ata, false),
        // 12. stake_pool (mut)
        AccountMeta::new(STAKE_POOL_PDA, false),
        // 13. staking_escrow (mut)
        AccountMeta::new(ESCROW_VAULT_PDA, false),
        // 14. carnage_vault (mut)
        AccountMeta::new(CARNAGE_SOL_VAULT_PDA, false),
        // 15. treasury (mut)
        AccountMeta::new(TREASURY, false),
        // 16. wsol_intermediary (mut) -- EXTRA account in sell struct
        AccountMeta::new(WSOL_INTERMEDIARY_PDA, false),
        // 17. amm_program (readonly)
        AccountMeta::new_readonly(AMM_PROGRAM_ID, false),
        // 18. token_program_a (readonly) -- SPL Token
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        // 19. token_program_b (readonly) -- Token-2022
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM_ID, false),
        // 20. system_program (readonly)
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        // 21. staking_program (readonly)
        AccountMeta::new_readonly(STAKING_PROGRAM_ID, false),
    ];

    // Append transfer hook accounts for the T22 token mint (input side for sell).
    // For sell: token -> SOL, so hooks are for the input token mint.
    // The AMM transfer is: user_token_b -> pool_vault_b for the token input side.
    let hook_metas = hook_metas_for_mint(&token_mint, user_token_ata, &vault_b);
    metas.extend(hook_metas);

    metas
}

/// Resolve pool-specific addresses based on is_crime flag.
///
/// Returns (pool, vault_a, vault_b, token_mint).
fn pool_addresses(is_crime: bool) -> (Pubkey, Pubkey, Pubkey, Pubkey) {
    if is_crime {
        (CRIME_SOL_POOL, CRIME_SOL_VAULT_A, CRIME_SOL_VAULT_B, CRIME_MINT)
    } else {
        (FRAUD_SOL_POOL, FRAUD_SOL_VAULT_A, FRAUD_SOL_VAULT_B, FRAUD_MINT)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_metas_has_20_named_accounts() {
        let user = Pubkey::new_unique();
        let wsol = Pubkey::new_unique();
        let token = Pubkey::new_unique();

        let metas = build_buy_account_metas(&user, &wsol, &token, true);

        // 20 named + 4 hook = 24 total
        assert_eq!(metas.len(), 24, "buy should have 20 named + 4 hook accounts");

        // Verify first 20 are the named accounts (no hooks yet)
        // Account 1: user is signer + mutable
        assert_eq!(metas[0].pubkey, user);
        assert!(metas[0].is_signer);
        assert!(metas[0].is_writable);

        // Account 2: epoch_state readonly
        assert_eq!(metas[1].pubkey, EPOCH_STATE_PDA);
        assert!(!metas[1].is_writable);

        // Account 16: amm_program
        assert_eq!(metas[15].pubkey, AMM_PROGRAM_ID);

        // Account 20: staking_program
        assert_eq!(metas[19].pubkey, STAKING_PROGRAM_ID);
    }

    #[test]
    fn sell_metas_has_21_named_accounts() {
        let user = Pubkey::new_unique();
        let token = Pubkey::new_unique();
        let wsol = Pubkey::new_unique();

        let metas = build_sell_account_metas(&user, &token, &wsol, true);

        // 21 named + 4 hook = 25 total
        assert_eq!(metas.len(), 25, "sell should have 21 named + 4 hook accounts");

        // Account 16: wsol_intermediary (sell-only)
        assert_eq!(metas[15].pubkey, WSOL_INTERMEDIARY_PDA);
        assert!(metas[15].is_writable);

        // Account 17: amm_program
        assert_eq!(metas[16].pubkey, AMM_PROGRAM_ID);

        // Account 21: staking_program
        assert_eq!(metas[20].pubkey, STAKING_PROGRAM_ID);
    }

    #[test]
    fn crime_pool_uses_crime_addresses() {
        let user = Pubkey::new_unique();
        let wsol = Pubkey::new_unique();
        let token = Pubkey::new_unique();

        let metas = build_buy_account_metas(&user, &wsol, &token, true);

        // pool = CRIME_SOL_POOL
        assert_eq!(metas[4].pubkey, CRIME_SOL_POOL);
        // mint_b = CRIME_MINT
        assert_eq!(metas[8].pubkey, CRIME_MINT);
    }

    #[test]
    fn fraud_pool_uses_fraud_addresses() {
        let user = Pubkey::new_unique();
        let wsol = Pubkey::new_unique();
        let token = Pubkey::new_unique();

        let metas = build_buy_account_metas(&user, &wsol, &token, false);

        // pool = FRAUD_SOL_POOL
        assert_eq!(metas[4].pubkey, FRAUD_SOL_POOL);
        // mint_b = FRAUD_MINT
        assert_eq!(metas[8].pubkey, FRAUD_MINT);
    }

    #[test]
    fn buy_hook_accounts_at_end() {
        let user = Pubkey::new_unique();
        let wsol = Pubkey::new_unique();
        let token = Pubkey::new_unique();

        let metas = build_buy_account_metas(&user, &wsol, &token, true);

        // Last 4 are hook accounts for CRIME mint
        // [20] = CRIME_HOOK_META
        assert_eq!(metas[20].pubkey, super::super::addresses::CRIME_HOOK_META);
        // [23] = TRANSFER_HOOK_PROGRAM_ID
        assert_eq!(metas[23].pubkey, super::super::addresses::TRANSFER_HOOK_PROGRAM_ID);
    }

    #[test]
    fn sell_swap_authority_is_mutable() {
        let user = Pubkey::new_unique();
        let token = Pubkey::new_unique();
        let wsol = Pubkey::new_unique();

        let metas = build_sell_account_metas(&user, &token, &wsol, true);

        // swap_authority is account 3 (index 2), mutable for sell
        assert_eq!(metas[2].pubkey, SWAP_AUTHORITY_PDA);
        assert!(metas[2].is_writable);
    }

    #[test]
    fn buy_swap_authority_is_readonly() {
        let user = Pubkey::new_unique();
        let wsol = Pubkey::new_unique();
        let token = Pubkey::new_unique();

        let metas = build_buy_account_metas(&user, &wsol, &token, true);

        // swap_authority is account 3 (index 2), readonly for buy
        assert_eq!(metas[2].pubkey, SWAP_AUTHORITY_PDA);
        assert!(!metas[2].is_writable);
    }
}
