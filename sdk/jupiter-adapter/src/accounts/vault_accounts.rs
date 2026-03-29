// AccountMeta builder for Conversion Vault convert_v2 instruction.
//
// Matches the EXACT account ordering of the Convert struct (9 named accounts)
// in programs/conversion-vault/src/instructions/convert.rs.
//
// After the named accounts, transfer hook remaining_accounts are appended:
// [input_hooks(4), output_hooks(4)] = 8 hook accounts.
// Total: 9 named + 8 hooks = 17 AccountMetas.

use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use super::addresses::{
    CRIME_MINT, FRAUD_MINT, PROFIT_MINT, TOKEN_2022_PROGRAM_ID,
    VAULT_CONFIG_PDA, VAULT_CRIME, VAULT_FRAUD, VAULT_PROFIT,
};
use super::hook_accounts::hook_metas_for_mint;

/// Build the 9 named accounts + 8 hook remaining accounts for convert_v2.
///
/// # Arguments
/// * `user` - User wallet (signer)
/// * `user_input_ata` - User's input token account
/// * `user_output_ata` - User's output token account
/// * `input_mint` - Input mint (CRIME, FRAUD, or PROFIT)
/// * `output_mint` - Output mint (CRIME, FRAUD, or PROFIT)
///
/// # Returns
/// Vec of 17 AccountMetas (9 named + 4 input hooks + 4 output hooks).
pub fn build_vault_account_metas(
    user: &Pubkey,
    user_input_ata: &Pubkey,
    user_output_ata: &Pubkey,
    input_mint: &Pubkey,
    output_mint: &Pubkey,
) -> Vec<AccountMeta> {
    let vault_input = vault_token_account(input_mint);
    let vault_output = vault_token_account(output_mint);

    let mut metas = vec![
        // 1. user (signer)
        AccountMeta::new(*user, true),
        // 2. vault_config (readonly)
        AccountMeta::new_readonly(VAULT_CONFIG_PDA, false),
        // 3. user_input_account (mut)
        AccountMeta::new(*user_input_ata, false),
        // 4. user_output_account (mut)
        AccountMeta::new(*user_output_ata, false),
        // 5. input_mint (readonly)
        AccountMeta::new_readonly(*input_mint, false),
        // 6. output_mint (readonly)
        AccountMeta::new_readonly(*output_mint, false),
        // 7. vault_input (mut)
        AccountMeta::new(vault_input, false),
        // 8. vault_output (mut)
        AccountMeta::new(vault_output, false),
        // 9. token_program (readonly) -- Token-2022
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM_ID, false),
    ];

    // Append remaining_accounts: [input_hooks(4), output_hooks(4)]
    // Input hooks: transfer from user_input_ata -> vault_input
    let input_hooks = hook_metas_for_mint(input_mint, user_input_ata, &vault_input);
    // Output hooks: transfer from vault_output -> user_output_ata
    let output_hooks = hook_metas_for_mint(output_mint, &vault_output, user_output_ata);

    metas.extend(input_hooks);
    metas.extend(output_hooks);

    metas
}

/// Map a token mint to its corresponding vault token account.
///
/// # Panics
/// Panics if mint is not CRIME, FRAUD, or PROFIT (should never happen
/// since VaultAmm only supports these three mints).
pub fn vault_token_account(mint: &Pubkey) -> Pubkey {
    if *mint == CRIME_MINT {
        VAULT_CRIME
    } else if *mint == FRAUD_MINT {
        VAULT_FRAUD
    } else if *mint == PROFIT_MINT {
        VAULT_PROFIT
    } else {
        panic!("vault_token_account: unknown mint {}", mint);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_metas_has_9_named_plus_8_hooks() {
        let user = Pubkey::new_unique();
        let user_input = Pubkey::new_unique();
        let user_output = Pubkey::new_unique();

        let metas = build_vault_account_metas(
            &user,
            &user_input,
            &user_output,
            &CRIME_MINT,
            &PROFIT_MINT,
        );

        // 9 named + 4 input hooks + 4 output hooks = 17
        assert_eq!(metas.len(), 17, "vault should have 9 named + 8 hook accounts");
    }

    #[test]
    fn vault_account_ordering_correct() {
        let user = Pubkey::new_unique();
        let user_input = Pubkey::new_unique();
        let user_output = Pubkey::new_unique();

        let metas = build_vault_account_metas(
            &user,
            &user_input,
            &user_output,
            &CRIME_MINT,
            &PROFIT_MINT,
        );

        // 1. user (signer)
        assert_eq!(metas[0].pubkey, user);
        assert!(metas[0].is_signer);

        // 2. vault_config (readonly)
        assert_eq!(metas[1].pubkey, VAULT_CONFIG_PDA);
        assert!(!metas[1].is_writable);

        // 3. user_input_account (mut)
        assert_eq!(metas[2].pubkey, user_input);
        assert!(metas[2].is_writable);

        // 5. input_mint (readonly)
        assert_eq!(metas[4].pubkey, CRIME_MINT);
        assert!(!metas[4].is_writable);

        // 6. output_mint (readonly)
        assert_eq!(metas[5].pubkey, PROFIT_MINT);

        // 7. vault_input (CRIME vault, mut)
        assert_eq!(metas[6].pubkey, VAULT_CRIME);
        assert!(metas[6].is_writable);

        // 8. vault_output (PROFIT vault, mut)
        assert_eq!(metas[7].pubkey, VAULT_PROFIT);
        assert!(metas[7].is_writable);

        // 9. token_program
        assert_eq!(metas[8].pubkey, TOKEN_2022_PROGRAM_ID);
    }

    #[test]
    fn vault_token_account_crime() {
        assert_eq!(vault_token_account(&CRIME_MINT), VAULT_CRIME);
    }

    #[test]
    fn vault_token_account_fraud() {
        assert_eq!(vault_token_account(&FRAUD_MINT), VAULT_FRAUD);
    }

    #[test]
    fn vault_token_account_profit() {
        assert_eq!(vault_token_account(&PROFIT_MINT), VAULT_PROFIT);
    }

    #[test]
    fn profit_to_crime_has_correct_vaults() {
        let user = Pubkey::new_unique();
        let user_input = Pubkey::new_unique();
        let user_output = Pubkey::new_unique();

        let metas = build_vault_account_metas(
            &user,
            &user_input,
            &user_output,
            &PROFIT_MINT,
            &CRIME_MINT,
        );

        // vault_input = VAULT_PROFIT (holds PROFIT)
        assert_eq!(metas[6].pubkey, VAULT_PROFIT);
        // vault_output = VAULT_CRIME (sends CRIME)
        assert_eq!(metas[7].pubkey, VAULT_CRIME);
    }
}
