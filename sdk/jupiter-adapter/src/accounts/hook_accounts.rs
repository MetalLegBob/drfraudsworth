// Transfer hook extra account metas for Token-2022 mints.
//
// Each T22 mint (CRIME, FRAUD, PROFIT) requires 4 extra accounts for
// transfer_checked calls via the Transfer Hook:
//   1. ExtraAccountMetaList PDA (readonly)
//   2. Whitelist source PDA (readonly)
//   3. Whitelist dest PDA (readonly)
//   4. Transfer Hook Program (readonly)
//
// For NATIVE_MINT (SPL Token, no hooks), returns empty vec.

use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use super::addresses::{
    CRIME_HOOK_META, CRIME_MINT, FRAUD_HOOK_META, FRAUD_MINT, NATIVE_MINT, PROFIT_HOOK_META,
    PROFIT_MINT, TRANSFER_HOOK_PROGRAM_ID,
};

/// Build the 4 transfer hook extra AccountMetas for a Token-2022 mint.
///
/// For NATIVE_MINT (SPL Token), returns empty vec (no hooks).
///
/// Whitelist PDAs are deterministic: `PDA(["whitelist", token_account], TRANSFER_HOOK_PROGRAM_ID)`.
/// No network calls needed.
///
/// # Arguments
/// * `mint` - The token mint (CRIME, FRAUD, PROFIT, or NATIVE_MINT)
/// * `source_token_account` - Source token account for the transfer
/// * `dest_token_account` - Destination token account for the transfer
///
/// # Returns
/// Vec of 4 AccountMetas for T22 mints, empty for NATIVE_MINT.
pub fn hook_metas_for_mint(
    mint: &Pubkey,
    source_token_account: &Pubkey,
    dest_token_account: &Pubkey,
) -> Vec<AccountMeta> {
    // NATIVE_MINT = SPL Token, no transfer hook
    if *mint == NATIVE_MINT {
        return vec![];
    }

    // Resolve the ExtraAccountMetaList PDA for this mint
    let meta_list = if *mint == CRIME_MINT {
        CRIME_HOOK_META
    } else if *mint == FRAUD_MINT {
        FRAUD_HOOK_META
    } else if *mint == PROFIT_MINT {
        PROFIT_HOOK_META
    } else {
        // Unknown mint -- return empty (safety fallback)
        return vec![];
    };

    // Derive whitelist PDAs
    let (wl_source, _) = Pubkey::find_program_address(
        &[b"whitelist", source_token_account.as_ref()],
        &TRANSFER_HOOK_PROGRAM_ID,
    );
    let (wl_dest, _) = Pubkey::find_program_address(
        &[b"whitelist", dest_token_account.as_ref()],
        &TRANSFER_HOOK_PROGRAM_ID,
    );

    vec![
        AccountMeta::new_readonly(meta_list, false),
        AccountMeta::new_readonly(wl_source, false),
        AccountMeta::new_readonly(wl_dest, false),
        AccountMeta::new_readonly(TRANSFER_HOOK_PROGRAM_ID, false),
    ]
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_mint_returns_empty() {
        let src = Pubkey::new_unique();
        let dst = Pubkey::new_unique();
        let metas = hook_metas_for_mint(&NATIVE_MINT, &src, &dst);
        assert!(metas.is_empty());
    }

    #[test]
    fn crime_mint_returns_4_metas() {
        let src = Pubkey::new_unique();
        let dst = Pubkey::new_unique();
        let metas = hook_metas_for_mint(&CRIME_MINT, &src, &dst);
        assert_eq!(metas.len(), 4);

        // First meta is the ExtraAccountMetaList PDA
        assert_eq!(metas[0].pubkey, CRIME_HOOK_META);
        assert!(!metas[0].is_signer);
        assert!(!metas[0].is_writable);

        // Last meta is the Transfer Hook Program
        assert_eq!(metas[3].pubkey, TRANSFER_HOOK_PROGRAM_ID);
        assert!(!metas[3].is_signer);
        assert!(!metas[3].is_writable);
    }

    #[test]
    fn fraud_mint_returns_4_metas() {
        let src = Pubkey::new_unique();
        let dst = Pubkey::new_unique();
        let metas = hook_metas_for_mint(&FRAUD_MINT, &src, &dst);
        assert_eq!(metas.len(), 4);
        assert_eq!(metas[0].pubkey, FRAUD_HOOK_META);
    }

    #[test]
    fn profit_mint_returns_4_metas() {
        let src = Pubkey::new_unique();
        let dst = Pubkey::new_unique();
        let metas = hook_metas_for_mint(&PROFIT_MINT, &src, &dst);
        assert_eq!(metas.len(), 4);
        assert_eq!(metas[0].pubkey, PROFIT_HOOK_META);
    }

    #[test]
    fn unknown_mint_returns_empty() {
        let unknown = Pubkey::new_unique();
        let src = Pubkey::new_unique();
        let dst = Pubkey::new_unique();
        let metas = hook_metas_for_mint(&unknown, &src, &dst);
        assert!(metas.is_empty());
    }

    #[test]
    fn whitelist_pdas_are_deterministic() {
        let src = Pubkey::new_unique();
        let dst = Pubkey::new_unique();
        let metas1 = hook_metas_for_mint(&CRIME_MINT, &src, &dst);
        let metas2 = hook_metas_for_mint(&CRIME_MINT, &src, &dst);
        assert_eq!(metas1[1].pubkey, metas2[1].pubkey);
        assert_eq!(metas1[2].pubkey, metas2[2].pubkey);
    }

    #[test]
    fn different_accounts_produce_different_whitelist_pdas() {
        let src1 = Pubkey::new_unique();
        let dst1 = Pubkey::new_unique();
        let src2 = Pubkey::new_unique();
        let dst2 = Pubkey::new_unique();
        let metas1 = hook_metas_for_mint(&CRIME_MINT, &src1, &dst1);
        let metas2 = hook_metas_for_mint(&CRIME_MINT, &src2, &dst2);
        // Whitelist source PDAs should differ for different source accounts
        assert_ne!(metas1[1].pubkey, metas2[1].pubkey);
        // Whitelist dest PDAs should differ for different dest accounts
        assert_ne!(metas1[2].pubkey, metas2[2].pubkey);
    }
}
