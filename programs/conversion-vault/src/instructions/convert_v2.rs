use anchor_lang::prelude::*;
use crate::constants::{TOKEN_DECIMALS, VAULT_CONFIG_SEED};
use crate::error::VaultError;
use crate::helpers;
use crate::instructions::convert::Convert;
#[cfg(not(feature = "localnet"))]
use crate::instructions::convert::compute_output;
#[cfg(feature = "localnet")]
use crate::instructions::convert::compute_output_with_mints;

pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, Convert<'info>>,
    amount_in: u64,
    minimum_output: u64,
    pre_balance: u64,
) -> Result<()> {
    // --- 0. Owner check for convert-all safety ---
    require!(
        ctx.accounts.user_input_account.owner == ctx.accounts.user.key(),
        VaultError::InvalidOwner
    );

    // --- 1. Resolve effective amount ---
    // Three modes:
    //   amount_in > 0              → Exact mode: convert exactly amount_in
    //   amount_in == 0, pre_balance > 0  → Delta mode: convert only what was deposited
    //                                      since pre_balance (for multi-hop routes)
    //   amount_in == 0, pre_balance == 0 → Convert-all: convert entire balance
    let effective_amount = if amount_in > 0 {
        amount_in
    } else {
        let balance = ctx.accounts.user_input_account.amount;
        if pre_balance > 0 {
            // Delta mode: convert only the tokens deposited since pre_balance.
            // In atomic multi-hop routes, this is exactly what the preceding
            // AMM swap step deposited — the user's pre-existing holdings are
            // untouched.
            let delta = balance
                .checked_sub(pre_balance)
                .ok_or(VaultError::DeltaUnderflow)?;
            require!(delta > 0, VaultError::ZeroAmount);
            delta
        } else {
            // Convert-all mode: convert the user's entire token balance.
            require!(balance > 0, VaultError::ZeroAmount);
            balance
        }
    };

    // --- 2. Compute output (validates mint pair, dust, overflow) ---
    let input_key = ctx.accounts.input_mint.key();
    let output_key = ctx.accounts.output_mint.key();

    #[cfg(feature = "localnet")]
    let amount_out = {
        let vc = &ctx.accounts.vault_config;
        compute_output_with_mints(
            &input_key, &output_key, effective_amount,
            &vc.crime_mint, &vc.fraud_mint, &vc.profit_mint,
        )?
    };
    #[cfg(not(feature = "localnet"))]
    let amount_out = compute_output(&input_key, &output_key, effective_amount)?;

    // --- 3. Slippage guard ---
    require!(amount_out >= minimum_output, VaultError::SlippageExceeded);

    // --- 4. Log for debugging/indexing ---
    // Mode: amount_in>0 = exact, amount_in=0+pre_balance>0 = delta, amount_in=0+pre_balance=0 = convert-all
    msg!(
        "convert_v2: mode={}, effective_amount={}, output={}, pre_balance={}",
        if amount_in > 0 { "exact" } else if pre_balance > 0 { "delta" } else { "convert-all" },
        effective_amount,
        amount_out,
        pre_balance
    );

    // --- 5. Transfer input: user -> vault (user-signed) ---
    let remaining = ctx.remaining_accounts;
    let mid = remaining.len() / 2;
    let (input_hooks, output_hooks) = remaining.split_at(mid);

    helpers::hook_helper::transfer_t22_checked(
        &ctx.accounts.token_program.to_account_info(),
        &ctx.accounts.user_input_account.to_account_info(),
        &ctx.accounts.input_mint.to_account_info(),
        &ctx.accounts.vault_input.to_account_info(),
        &ctx.accounts.user.to_account_info(),
        effective_amount,
        TOKEN_DECIMALS,
        &[],
        input_hooks,
    )?;

    // --- 6. Transfer output: vault -> user (PDA-signed) ---
    let vault_bump = ctx.accounts.vault_config.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[VAULT_CONFIG_SEED, &[vault_bump]]];

    helpers::hook_helper::transfer_t22_checked(
        &ctx.accounts.token_program.to_account_info(),
        &ctx.accounts.vault_output.to_account_info(),
        &ctx.accounts.output_mint.to_account_info(),
        &ctx.accounts.user_output_account.to_account_info(),
        &ctx.accounts.vault_config.to_account_info(),
        amount_out,
        TOKEN_DECIMALS,
        signer_seeds,
        output_hooks,
    )?;

    Ok(())
}
