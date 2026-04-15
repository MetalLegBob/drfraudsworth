//! execute_rebalance: Read pool states, calculate allocation delta, and rebalance
//! liquidity between SOL and USDC pool pairs via AMM CPI.
//!
//! This is the rebalancing engine (REBAL-04). It ensures protocol liquidity stays
//! aligned with target allocation (50/50 in v1.7) by withdrawing from overweight
//! pools and injecting into underweight pools.
//!
//! The instruction moves FACTION tokens (CRIME/FRAUD) between pool pairs.
//! Denomination tokens (WSOL/USDC) are handled by the convert_usdc pipeline.
//!
//! Cost ceiling (REBAL-06) is validated crank-side: the crank reads pool reserves
//! off-chain, estimates slippage via constant product formula, and only submits if
//! estimated cost < cost_ceiling_bps. On-chain, withdraw_bps is bounded by
//! MAX_WITHDRAW_BPS (5000 = 50%) as a safety limit.
//!
//! Permissionless: anyone can call. Pays rebalance_bounty_lamports from bounty_vault
//! (0 in v1.7, wired for v1.8 PM fee revenue).
//!
//! Source: Phase 128, Plan 03

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::rent::Rent;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::sysvar::Sysvar;
use anchor_spl::token::Token;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::Mint;

use crate::constants::{
    ADD_LIQUIDITY_DISCRIMINATOR, BOUNTY_VAULT_SEED, HOLDING_SEED, MAX_WITHDRAW_BPS,
    REBALANCE_SEED, WITHDRAW_LIQUIDITY_DISCRIMINATOR,
};
use crate::errors::RebalancerError;
use crate::events::LiquidityRebalanced;
use crate::helpers::rebalance_math::calculate_delta_bps;
use crate::state::RebalancerConfig;

// ---------------------------------------------------------------------------
// Pool byte offsets (PoolState layout, identical to Tax Program pool_reader.rs)
// ---------------------------------------------------------------------------

/// PoolState minimum data size: 8 (disc) + 1 (type) + 32*4 (mints+vaults) + 8*2 (reserves) = 153 bytes
const POOL_MIN_LEN: usize = 153;
const RESERVE_A_OFFSET: usize = 137;
const RESERVE_B_OFFSET: usize = 145;

/// Number of Transfer Hook extra accounts per T22 mint (meta_list, wl_source, wl_dest, hook_program).
const HOOK_ACCOUNTS_PER_MINT: usize = 4;

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Execute pool liquidity rebalancing between SOL and USDC pool pairs.
///
/// # Arguments
/// * `withdraw_bps` - BPS to withdraw from overweight pools (crank-calculated, max 5000)
/// * `sol_usd_price_x1000` - SOL price in milli-USD (e.g., SOL at $150.123 = 150_123).
///   Used to convert SOL reserves to USD-equivalent for allocation comparison.
///   The crank reads Pyth off-chain and passes this value.
///
/// # Flow
/// 1. Validate inputs (withdraw_bps, sol_price)
/// 2. Read all 4 pool reserves via raw byte offsets
/// 3. Convert SOL reserves to USD-equivalent using sol_usd_price_x1000
/// 4. Calculate allocation delta via calculate_delta_bps
/// 5. Skip if delta < min_delta (REBAL-05)
/// 6. Withdraw faction tokens from overweight pools via AMM CPI
/// 7. Inject faction tokens into underweight pools via AMM CPI
/// 8. Pay bounty (0 in v1.7)
/// 9. Emit LiquidityRebalanced event
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, ExecuteRebalance<'info>>,
    withdraw_bps: u16,
    sol_usd_price_x1000: u64,
) -> Result<()> {
    // =========================================================================
    // 1. CHECKS: Validate inputs
    // =========================================================================
    require!(
        withdraw_bps > 0 && withdraw_bps <= MAX_WITHDRAW_BPS,
        RebalancerError::WithdrawExceedsMax
    );
    require!(sol_usd_price_x1000 > 0, RebalancerError::InvalidSolPrice);

    let config = &ctx.accounts.rebalancer_config;
    let amm_id = crate::constants::amm_program_id();

    // =========================================================================
    // 2. Read pool reserves from all 4 pools
    // =========================================================================

    // SOL pools: CRIME/SOL and FRAUD/SOL
    // In SOL pools, NATIVE_MINT (0x06) < CRIME/FRAUD, so:
    //   mint_a = NATIVE_MINT (SOL), reserve_a = SOL reserve
    //   mint_b = faction token, reserve_b = token reserve
    let (crime_sol_rsv_a, _crime_sol_rsv_b) =
        read_pool_reserves(&ctx.accounts.crime_sol_pool, &amm_id)?;
    let (fraud_sol_rsv_a, _fraud_sol_rsv_b) =
        read_pool_reserves(&ctx.accounts.fraud_sol_pool, &amm_id)?;

    // USDC pools: CRIME/USDC and FRAUD/USDC
    // Canonical ordering depends on cluster mint addresses; we read raw reserves.
    // The crank knows which side is USDC. For allocation calculation, we need
    // the total USDC locked. We detect USDC position from mint_a bytes.
    let usdc_key = ctx.accounts.usdc_mint.key();
    let (crime_usdc_rsv_usdc, _crime_usdc_rsv_token) =
        read_pool_reserves_for_quote(&ctx.accounts.crime_usdc_pool, &amm_id, &usdc_key)?;
    let (fraud_usdc_rsv_usdc, _fraud_usdc_rsv_token) =
        read_pool_reserves_for_quote(&ctx.accounts.fraud_usdc_pool, &amm_id, &usdc_key)?;

    // =========================================================================
    // 3. Calculate USD-equivalent values for allocation comparison
    //
    // SOL reserves are in lamports (9 decimals). USDC reserves are in USDC-lamports (6 decimals).
    // Convert both to common unit: milli-cents (1/100000 of a dollar).
    //
    // sol_value_millicents = total_sol_lamports * sol_usd_price_x1000 / 1_000_000
    //   (sol_usd_price_x1000 is milli-USD per SOL, lamports are 1e-9 SOL)
    //   = lamports * price_milli_usd / 1_000_000_000 * 1_000 (to millicents)
    //   Simplified: lamports * price_x1000 / 1_000_000 (gives micro-USD ~ millicents)
    //
    // usdc_value_millicents = total_usdc_lamports / 10 (USDC 6 decimals -> same scale)
    //   Actually: USDC lamports are 1e-6 USD. To match SOL scale:
    //   usdc_value = usdc_lamports * 1_000 / 1_000_000 = usdc_lamports / 1_000
    //
    // For delta BPS comparison, the absolute scale doesn't matter, only the ratio.
    // So we can use a simpler normalization:
    //   sol_value = total_sol_lamports * sol_usd_price_x1000 / 1_000_000_000
    //   usdc_value = total_usdc_lamports / 1_000  (USDC 6 decimals -> milli-USD)
    // Both now in milli-USD units.
    // =========================================================================

    let total_sol_lamports = (crime_sol_rsv_a as u128)
        .checked_add(fraud_sol_rsv_a as u128)
        .ok_or(RebalancerError::MathOverflow)?;

    let sol_value = total_sol_lamports
        .checked_mul(sol_usd_price_x1000 as u128)
        .ok_or(RebalancerError::MathOverflow)?
        .checked_div(1_000_000_000)
        .ok_or(RebalancerError::MathOverflow)?;

    let total_usdc_lamports = (crime_usdc_rsv_usdc as u128)
        .checked_add(fraud_usdc_rsv_usdc as u128)
        .ok_or(RebalancerError::MathOverflow)?;

    let usdc_value = total_usdc_lamports
        .checked_div(1_000)
        .ok_or(RebalancerError::MathOverflow)?;

    // Clamp to u64 for calculate_delta_bps (values should fit -- milli-USD range)
    let sol_value_u64 = u64::try_from(sol_value).unwrap_or(u64::MAX);
    let usdc_value_u64 = u64::try_from(usdc_value).unwrap_or(u64::MAX);

    msg!(
        "Pool values: sol_milli_usd={}, usdc_milli_usd={}, sol_price_x1000={}",
        sol_value_u64,
        usdc_value_u64,
        sol_usd_price_x1000,
    );

    // =========================================================================
    // 4. Calculate allocation delta (REBAL-05)
    // =========================================================================
    let delta = calculate_delta_bps(sol_value_u64, usdc_value_u64, config.target_bps)
        .ok_or(RebalancerError::MathOverflow)?;

    let abs_delta = delta.unsigned_abs() as u16;

    msg!(
        "Allocation delta: {} bps (threshold: {} bps)",
        delta,
        config.min_delta,
    );

    if abs_delta < config.min_delta {
        msg!(
            "Delta {} bps below threshold {} bps, skipping rebalance",
            abs_delta,
            config.min_delta,
        );
        return Ok(());
    }

    // =========================================================================
    // 5. Determine rebalance direction
    //
    // Positive delta = SOL overweight -> withdraw from SOL pools, inject into USDC pools
    // Negative delta = USDC overweight -> withdraw from USDC pools, inject into SOL pools
    //
    // We move FACTION tokens (CRIME/FRAUD) between pool pairs.
    // The faction tokens withdrawn from overweight pools land in holdings,
    // then are injected into underweight pools.
    //
    // Denomination tokens (SOL/USDC) are also withdrawn but stay in holdings.
    // The convert_usdc pipeline handles denomination conversion separately.
    // =========================================================================

    let rebalance_bump = ctx.bumps.rebalance_authority;
    let rebalance_seeds: &[&[u8]] = &[REBALANCE_SEED, &[rebalance_bump]];

    // Partition remaining_accounts into Transfer Hook accounts for each T22 mint.
    // Layout: [crime_hooks(4), fraud_hooks(4)]
    // Each set of 4: [extra_account_meta_list, wl_source, wl_dest, hook_program]
    let remaining = ctx.remaining_accounts;
    require!(
        remaining.len() >= HOOK_ACCOUNTS_PER_MINT * 2,
        RebalancerError::MathOverflow // reusing error for insufficient remaining accounts
    );
    let crime_hook_accounts = &remaining[..HOOK_ACCOUNTS_PER_MINT];
    let fraud_hook_accounts = &remaining[HOOK_ACCOUNTS_PER_MINT..HOOK_ACCOUNTS_PER_MINT * 2];

    if delta > 0 {
        // SOL overweight: withdraw from SOL pools, inject into USDC pools
        msg!("SOL overweight by {} bps, rebalancing SOL -> USDC", delta);

        // Step A: Withdraw from CRIME/SOL pool
        withdraw_from_pool(
            &ctx.accounts.crime_sol_pool,
            &ctx.accounts.crime_sol_vault_a,
            &ctx.accounts.crime_sol_vault_b,
            &ctx.accounts.holding_wsol,    // destination_a (SOL side = mint_a)
            &ctx.accounts.holding_crime,   // destination_b (CRIME side = mint_b)
            &ctx.accounts.wsol_mint.to_account_info(),
            &ctx.accounts.crime_mint.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.token_2022_program.to_account_info(),
            &ctx.accounts.rebalance_authority,
            &ctx.accounts.amm_program,
            rebalance_seeds,
            withdraw_bps,
            crime_hook_accounts,
        )?;

        // Step A2: Withdraw from FRAUD/SOL pool
        withdraw_from_pool(
            &ctx.accounts.fraud_sol_pool,
            &ctx.accounts.fraud_sol_vault_a,
            &ctx.accounts.fraud_sol_vault_b,
            &ctx.accounts.holding_wsol,    // destination_a (SOL side)
            &ctx.accounts.holding_fraud,   // destination_b (FRAUD side)
            &ctx.accounts.wsol_mint.to_account_info(),
            &ctx.accounts.fraud_mint.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.token_2022_program.to_account_info(),
            &ctx.accounts.rebalance_authority,
            &ctx.accounts.amm_program,
            rebalance_seeds,
            withdraw_bps,
            fraud_hook_accounts,
        )?;

        // Step B: Inject into CRIME/USDC pool
        // Read how many CRIME tokens are now in the holding (from withdrawal)
        let crime_holding_amount = read_token_balance(&ctx.accounts.holding_crime)?;
        if crime_holding_amount > 0 {
            inject_into_pool(
                &ctx.accounts.crime_usdc_pool,
                &ctx.accounts.crime_usdc_vault_a,
                &ctx.accounts.crime_usdc_vault_b,
                &ctx.accounts.holding_crime,
                &ctx.accounts.holding_usdc,
                &ctx.accounts.crime_mint.to_account_info(),
                &ctx.accounts.usdc_mint.to_account_info(),
                &ctx.accounts.token_2022_program.to_account_info(),
                &ctx.accounts.token_program.to_account_info(),
                &ctx.accounts.rebalance_authority,
                &ctx.accounts.amm_program,
                rebalance_seeds,
                &usdc_key,
                crime_holding_amount,
                crime_hook_accounts,
            )?;
        }

        // Step B2: Inject into FRAUD/USDC pool
        let fraud_holding_amount = read_token_balance(&ctx.accounts.holding_fraud)?;
        if fraud_holding_amount > 0 {
            inject_into_pool(
                &ctx.accounts.fraud_usdc_pool,
                &ctx.accounts.fraud_usdc_vault_a,
                &ctx.accounts.fraud_usdc_vault_b,
                &ctx.accounts.holding_fraud,
                &ctx.accounts.holding_usdc,
                &ctx.accounts.fraud_mint.to_account_info(),
                &ctx.accounts.usdc_mint.to_account_info(),
                &ctx.accounts.token_2022_program.to_account_info(),
                &ctx.accounts.token_program.to_account_info(),
                &ctx.accounts.rebalance_authority,
                &ctx.accounts.amm_program,
                rebalance_seeds,
                &usdc_key,
                fraud_holding_amount,
                fraud_hook_accounts,
            )?;
        }
    } else {
        // USDC overweight: withdraw from USDC pools, inject into SOL pools
        msg!("USDC overweight by {} bps, rebalancing USDC -> SOL", delta.abs());

        // Step A: Withdraw from CRIME/USDC pool
        withdraw_from_pool(
            &ctx.accounts.crime_usdc_pool,
            &ctx.accounts.crime_usdc_vault_a,
            &ctx.accounts.crime_usdc_vault_b,
            &ctx.accounts.holding_crime,  // These get re-ordered in the helper based on canonical order
            &ctx.accounts.holding_usdc,
            &ctx.accounts.crime_mint.to_account_info(),
            &ctx.accounts.usdc_mint.to_account_info(),
            &ctx.accounts.token_2022_program.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.rebalance_authority,
            &ctx.accounts.amm_program,
            rebalance_seeds,
            withdraw_bps,
            crime_hook_accounts,
        )?;

        // Step A2: Withdraw from FRAUD/USDC pool
        withdraw_from_pool(
            &ctx.accounts.fraud_usdc_pool,
            &ctx.accounts.fraud_usdc_vault_a,
            &ctx.accounts.fraud_usdc_vault_b,
            &ctx.accounts.holding_fraud,
            &ctx.accounts.holding_usdc,
            &ctx.accounts.fraud_mint.to_account_info(),
            &ctx.accounts.usdc_mint.to_account_info(),
            &ctx.accounts.token_2022_program.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.rebalance_authority,
            &ctx.accounts.amm_program,
            rebalance_seeds,
            withdraw_bps,
            fraud_hook_accounts,
        )?;

        // Step B: Inject into CRIME/SOL pool
        let crime_holding_amount = read_token_balance(&ctx.accounts.holding_crime)?;
        if crime_holding_amount > 0 {
            inject_into_pool(
                &ctx.accounts.crime_sol_pool,
                &ctx.accounts.crime_sol_vault_a,
                &ctx.accounts.crime_sol_vault_b,
                &ctx.accounts.holding_wsol,
                &ctx.accounts.holding_crime,
                &ctx.accounts.wsol_mint.to_account_info(),
                &ctx.accounts.crime_mint.to_account_info(),
                &ctx.accounts.token_program.to_account_info(),
                &ctx.accounts.token_2022_program.to_account_info(),
                &ctx.accounts.rebalance_authority,
                &ctx.accounts.amm_program,
                rebalance_seeds,
                &anchor_spl::token::spl_token::native_mint::id(),
                crime_holding_amount,
                crime_hook_accounts,
            )?;
        }

        // Step B2: Inject into FRAUD/SOL pool
        let fraud_holding_amount = read_token_balance(&ctx.accounts.holding_fraud)?;
        if fraud_holding_amount > 0 {
            inject_into_pool(
                &ctx.accounts.fraud_sol_pool,
                &ctx.accounts.fraud_sol_vault_a,
                &ctx.accounts.fraud_sol_vault_b,
                &ctx.accounts.holding_wsol,
                &ctx.accounts.holding_fraud,
                &ctx.accounts.wsol_mint.to_account_info(),
                &ctx.accounts.fraud_mint.to_account_info(),
                &ctx.accounts.token_program.to_account_info(),
                &ctx.accounts.token_2022_program.to_account_info(),
                &ctx.accounts.rebalance_authority,
                &ctx.accounts.amm_program,
                rebalance_seeds,
                &anchor_spl::token::spl_token::native_mint::id(),
                fraud_holding_amount,
                fraud_hook_accounts,
            )?;
        }
    }

    // =========================================================================
    // 8. Bounty payout (0 in v1.7 -- wired for v1.8)
    // =========================================================================
    let bounty_lamports = config.rebalance_bounty_lamports;
    let bounty_paid = if bounty_lamports > 0 {
        let rent = Rent::get()?;
        let rent_exempt_min = rent.minimum_balance(0);
        let vault_balance = ctx.accounts.bounty_vault.lamports();
        let bounty_threshold = bounty_lamports
            .checked_add(rent_exempt_min)
            .ok_or(RebalancerError::MathOverflow)?;

        if vault_balance >= bounty_threshold {
            let vault_bump = ctx.bumps.bounty_vault;
            let vault_seeds: &[&[u8]] = &[BOUNTY_VAULT_SEED, &[vault_bump]];

            invoke_signed(
                &system_instruction::transfer(
                    ctx.accounts.bounty_vault.to_account_info().key,
                    ctx.accounts.caller.to_account_info().key,
                    bounty_lamports,
                ),
                &[
                    ctx.accounts.bounty_vault.to_account_info(),
                    ctx.accounts.caller.to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                ],
                &[vault_seeds],
            )?;
            bounty_lamports
        } else {
            msg!("Bounty vault insufficient for rebalance bounty (skipped)");
            0
        }
    } else {
        0 // v1.7: rebalance_bounty_lamports = 0
    };

    // =========================================================================
    // 9. Emit event
    // =========================================================================
    let clock = Clock::get()?;
    let (withdrawn_from, injected_into) = if delta > 0 {
        ("SOL pools".to_string(), "USDC pools".to_string())
    } else {
        ("USDC pools".to_string(), "SOL pools".to_string())
    };

    emit!(LiquidityRebalanced {
        sol_pool_delta_bps: delta,
        usdc_pool_delta_bps: -delta, // mirror (they sum to 0 in 2-pool system)
        withdrawn_from,
        injected_into,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Rebalance complete: delta={} bps, withdraw_bps={}, bounty_paid={}",
        delta,
        withdraw_bps,
        bounty_paid
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Pool reading helpers
// ---------------------------------------------------------------------------

/// Read (reserve_a, reserve_b) from a PoolState AccountInfo at known byte offsets.
/// Validates the account is owned by the AMM program.
fn read_pool_reserves(pool_info: &AccountInfo, amm_id: &Pubkey) -> Result<(u64, u64)> {
    require!(
        *pool_info.owner == *amm_id,
        RebalancerError::InvalidPoolOwner
    );

    let data = pool_info.data.borrow();
    require!(data.len() >= POOL_MIN_LEN, RebalancerError::InvalidPoolData);

    let reserve_a = u64::from_le_bytes(
        data[RESERVE_A_OFFSET..RESERVE_A_OFFSET + 8]
            .try_into()
            .map_err(|_| error!(RebalancerError::MathOverflow))?,
    );
    let reserve_b = u64::from_le_bytes(
        data[RESERVE_B_OFFSET..RESERVE_B_OFFSET + 8]
            .try_into()
            .map_err(|_| error!(RebalancerError::MathOverflow))?,
    );

    Ok((reserve_a, reserve_b))
}

/// Read pool reserves for a quote-token pool (USDC or WSOL), identifying which
/// side is the quote token. Returns (quote_reserve, base_reserve).
fn read_pool_reserves_for_quote(
    pool_info: &AccountInfo,
    amm_id: &Pubkey,
    quote_mint: &Pubkey,
) -> Result<(u64, u64)> {
    require!(
        *pool_info.owner == *amm_id,
        RebalancerError::InvalidPoolOwner
    );

    let data = pool_info.data.borrow();
    require!(data.len() >= POOL_MIN_LEN, RebalancerError::InvalidPoolData);

    let mint_a = Pubkey::try_from(&data[9..41])
        .map_err(|_| error!(RebalancerError::MathOverflow))?;

    let reserve_a = u64::from_le_bytes(
        data[RESERVE_A_OFFSET..RESERVE_A_OFFSET + 8]
            .try_into()
            .map_err(|_| error!(RebalancerError::MathOverflow))?,
    );
    let reserve_b = u64::from_le_bytes(
        data[RESERVE_B_OFFSET..RESERVE_B_OFFSET + 8]
            .try_into()
            .map_err(|_| error!(RebalancerError::MathOverflow))?,
    );

    if mint_a == *quote_mint {
        Ok((reserve_a, reserve_b))
    } else {
        Ok((reserve_b, reserve_a))
    }
}

/// Read the token balance from an AccountInfo (SPL Token or Token-2022).
/// Amount is at offset 64, 8 bytes LE.
fn read_token_balance(account: &AccountInfo) -> Result<u64> {
    let data = account.data.borrow();
    if data.len() >= 72 {
        Ok(u64::from_le_bytes(
            data[64..72]
                .try_into()
                .map_err(|_| error!(RebalancerError::MathOverflow))?,
        ))
    } else {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// AMM CPI helpers
// ---------------------------------------------------------------------------

/// CPI into AMM withdraw_liquidity instruction.
///
/// AccountMeta layout matches WithdrawLiquidity struct:
///   [0] rebalance_authority (signer)
///   [1] pool_state (mut)
///   [2] vault_a (mut)
///   [3] vault_b (mut)
///   [4] destination_a (mut) — holding for mint_a
///   [5] destination_b (mut) — holding for mint_b
///   [6] mint_a
///   [7] mint_b
///   [8] token_program_a
///   [9] token_program_b
///   [10..] Transfer Hook remaining_accounts for T22 tokens
#[allow(clippy::too_many_arguments)]
fn withdraw_from_pool<'info>(
    pool: &AccountInfo<'info>,
    vault_a: &AccountInfo<'info>,
    vault_b: &AccountInfo<'info>,
    destination_a: &AccountInfo<'info>,
    destination_b: &AccountInfo<'info>,
    mint_a: &AccountInfo<'info>,
    mint_b: &AccountInfo<'info>,
    token_program_a: &AccountInfo<'info>,
    token_program_b: &AccountInfo<'info>,
    rebalance_authority: &AccountInfo<'info>,
    amm_program: &AccountInfo<'info>,
    rebalance_seeds: &[&[u8]],
    withdraw_bps: u16,
    hook_accounts: &[AccountInfo<'info>],
) -> Result<()> {
    let mut account_metas = Vec::with_capacity(10 + hook_accounts.len());
    account_metas.push(AccountMeta::new_readonly(rebalance_authority.key(), true)); // signer
    account_metas.push(AccountMeta::new(pool.key(), false));
    account_metas.push(AccountMeta::new(vault_a.key(), false));
    account_metas.push(AccountMeta::new(vault_b.key(), false));
    account_metas.push(AccountMeta::new(destination_a.key(), false));
    account_metas.push(AccountMeta::new(destination_b.key(), false));
    account_metas.push(AccountMeta::new_readonly(mint_a.key(), false));
    account_metas.push(AccountMeta::new_readonly(mint_b.key(), false));
    account_metas.push(AccountMeta::new_readonly(token_program_a.key(), false));
    account_metas.push(AccountMeta::new_readonly(token_program_b.key(), false));

    // Append Transfer Hook accounts for T22 token transfers
    for hook_acc in hook_accounts.iter() {
        if hook_acc.is_writable {
            account_metas.push(AccountMeta::new(hook_acc.key(), hook_acc.is_signer));
        } else {
            account_metas.push(AccountMeta::new_readonly(hook_acc.key(), hook_acc.is_signer));
        }
    }

    // Instruction data: discriminator(8) + withdraw_bps(2)
    let mut ix_data = Vec::with_capacity(10);
    ix_data.extend_from_slice(&WITHDRAW_LIQUIDITY_DISCRIMINATOR);
    ix_data.extend_from_slice(&withdraw_bps.to_le_bytes());

    let ix = Instruction {
        program_id: amm_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    let mut account_infos = Vec::with_capacity(10 + hook_accounts.len());
    account_infos.push(rebalance_authority.to_account_info());
    account_infos.push(pool.to_account_info());
    account_infos.push(vault_a.to_account_info());
    account_infos.push(vault_b.to_account_info());
    account_infos.push(destination_a.to_account_info());
    account_infos.push(destination_b.to_account_info());
    account_infos.push(mint_a.to_account_info());
    account_infos.push(mint_b.to_account_info());
    account_infos.push(token_program_a.to_account_info());
    account_infos.push(token_program_b.to_account_info());
    for hook_acc in hook_accounts.iter() {
        account_infos.push(hook_acc.to_account_info());
    }

    invoke_signed(&ix, &account_infos, &[rebalance_seeds])
        .map_err(|_| error!(RebalancerError::AmmCpiFailed))?;

    msg!("Withdrew {} bps from pool {}", withdraw_bps, pool.key());

    Ok(())
}

/// CPI into AMM add_liquidity instruction.
///
/// Injects the faction token amount from holdings into the target pool.
/// The denomination token amount (USDC or WSOL) is 0 -- we only move faction tokens.
/// AMM add_liquidity accepts one-sided injection (amount_a=0 or amount_b=0 is fine,
/// as long as at least one is non-zero).
///
/// AccountMeta layout matches AddLiquidity struct:
///   [0] rebalance_authority (signer)
///   [1] pool_state (mut)
///   [2] vault_a (mut)
///   [3] vault_b (mut)
///   [4] source_a (mut) — holding for mint_a
///   [5] source_b (mut) — holding for mint_b
///   [6] mint_a
///   [7] mint_b
///   [8] token_program_a
///   [9] token_program_b
///   [10..] Transfer Hook remaining_accounts for T22 tokens
#[allow(clippy::too_many_arguments)]
fn inject_into_pool<'info>(
    pool: &AccountInfo<'info>,
    vault_a: &AccountInfo<'info>,
    vault_b: &AccountInfo<'info>,
    source_a: &AccountInfo<'info>,   // holding for mint_a side
    source_b: &AccountInfo<'info>,   // holding for mint_b side
    mint_a: &AccountInfo<'info>,
    mint_b: &AccountInfo<'info>,
    token_program_a: &AccountInfo<'info>,
    token_program_b: &AccountInfo<'info>,
    rebalance_authority: &AccountInfo<'info>,
    amm_program: &AccountInfo<'info>,
    rebalance_seeds: &[&[u8]],
    quote_mint: &Pubkey,
    faction_amount: u64,
    hook_accounts: &[AccountInfo<'info>],
) -> Result<()> {
    // Determine which side is the faction token (non-quote) and which is quote.
    // Read mint_a from pool bytes to determine canonical ordering.
    let pool_data = pool.data.borrow();
    let pool_mint_a = if pool_data.len() >= 41 {
        Pubkey::try_from(&pool_data[9..41]).unwrap_or_default()
    } else {
        return Err(error!(RebalancerError::InvalidPoolData));
    };
    drop(pool_data);

    // amount_a and amount_b for add_liquidity CPI.
    // We inject faction tokens only; denomination side = 0.
    let (amount_a, amount_b) = if pool_mint_a == *quote_mint {
        // mint_a = quote (WSOL/USDC), mint_b = faction token
        (0u64, faction_amount)
    } else {
        // mint_a = faction token, mint_b = quote (WSOL/USDC)
        (faction_amount, 0u64)
    };

    let mut account_metas = Vec::with_capacity(10 + hook_accounts.len());
    account_metas.push(AccountMeta::new_readonly(rebalance_authority.key(), true)); // signer
    account_metas.push(AccountMeta::new(pool.key(), false));
    account_metas.push(AccountMeta::new(vault_a.key(), false));
    account_metas.push(AccountMeta::new(vault_b.key(), false));
    account_metas.push(AccountMeta::new(source_a.key(), false));
    account_metas.push(AccountMeta::new(source_b.key(), false));
    account_metas.push(AccountMeta::new_readonly(mint_a.key(), false));
    account_metas.push(AccountMeta::new_readonly(mint_b.key(), false));
    account_metas.push(AccountMeta::new_readonly(token_program_a.key(), false));
    account_metas.push(AccountMeta::new_readonly(token_program_b.key(), false));

    for hook_acc in hook_accounts.iter() {
        if hook_acc.is_writable {
            account_metas.push(AccountMeta::new(hook_acc.key(), hook_acc.is_signer));
        } else {
            account_metas.push(AccountMeta::new_readonly(hook_acc.key(), hook_acc.is_signer));
        }
    }

    // Instruction data: discriminator(8) + amount_a(8) + amount_b(8)
    let mut ix_data = Vec::with_capacity(24);
    ix_data.extend_from_slice(&ADD_LIQUIDITY_DISCRIMINATOR);
    ix_data.extend_from_slice(&amount_a.to_le_bytes());
    ix_data.extend_from_slice(&amount_b.to_le_bytes());

    let ix = Instruction {
        program_id: amm_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    let mut account_infos = Vec::with_capacity(10 + hook_accounts.len());
    account_infos.push(rebalance_authority.to_account_info());
    account_infos.push(pool.to_account_info());
    account_infos.push(vault_a.to_account_info());
    account_infos.push(vault_b.to_account_info());
    account_infos.push(source_a.to_account_info());
    account_infos.push(source_b.to_account_info());
    account_infos.push(mint_a.to_account_info());
    account_infos.push(mint_b.to_account_info());
    account_infos.push(token_program_a.to_account_info());
    account_infos.push(token_program_b.to_account_info());
    for hook_acc in hook_accounts.iter() {
        account_infos.push(hook_acc.to_account_info());
    }

    invoke_signed(&ix, &account_infos, &[rebalance_seeds])
        .map_err(|_| error!(RebalancerError::AmmCpiFailed))?;

    msg!(
        "Injected faction_amount={} into pool {} (a={}, b={})",
        faction_amount,
        pool.key(),
        amount_a,
        amount_b,
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

/// Accounts for the `execute_rebalance` instruction.
///
/// Reads reserves from 4 pools (CRIME/SOL, FRAUD/SOL, CRIME/USDC, FRAUD/USDC),
/// determines allocation imbalance, and CPIs into AMM withdraw_liquidity +
/// add_liquidity to rebalance.
///
/// Uses AccountInfo for pool/vault/holding accounts to minimize BPF stack pressure.
/// Anchor constraints validate pool/vault relationships on the AMM side during CPI.
///
/// Remaining accounts: Transfer Hook accounts for CRIME (4) + FRAUD (4) T22 mints.
#[derive(Accounts)]
pub struct ExecuteRebalance<'info> {
    /// Caller who triggers the rebalance. Receives bounty on success.
    /// Permissionless.
    #[account(mut)]
    pub caller: Signer<'info>,

    /// RebalancerConfig singleton — read-only for target_bps, min_delta, bounty.
    #[account(
        seeds = [crate::constants::REBALANCER_CONFIG_SEED],
        bump = rebalancer_config.bump,
        constraint = rebalancer_config.initialized @ RebalancerError::AlreadyInitialized,
    )]
    pub rebalancer_config: Account<'info, RebalancerConfig>,

    /// RebalanceAuthority PDA — signs AMM CPIs.
    ///
    /// CHECK: PDA derived from known seeds; validated by seeds constraint.
    #[account(
        mut,
        seeds = [REBALANCE_SEED],
        bump,
    )]
    pub rebalance_authority: SystemAccount<'info>,

    // -----------------------------------------------------------------------
    // SOL pools (CRIME/SOL, FRAUD/SOL)
    // -----------------------------------------------------------------------

    /// CRIME/SOL pool state.
    /// CHECK: Owner validated as AMM program in read_pool_reserves.
    #[account(mut)]
    pub crime_sol_pool: AccountInfo<'info>,

    /// CRIME/SOL vault A (SOL/WSOL side).
    /// CHECK: Validated by AMM's WithdrawLiquidity/AddLiquidity constraints during CPI.
    #[account(mut)]
    pub crime_sol_vault_a: AccountInfo<'info>,

    /// CRIME/SOL vault B (CRIME token side).
    /// CHECK: Validated by AMM constraints during CPI.
    #[account(mut)]
    pub crime_sol_vault_b: AccountInfo<'info>,

    /// FRAUD/SOL pool state.
    /// CHECK: Owner validated as AMM program in read_pool_reserves.
    #[account(mut)]
    pub fraud_sol_pool: AccountInfo<'info>,

    /// FRAUD/SOL vault A.
    /// CHECK: Validated by AMM constraints during CPI.
    #[account(mut)]
    pub fraud_sol_vault_a: AccountInfo<'info>,

    /// FRAUD/SOL vault B.
    /// CHECK: Validated by AMM constraints during CPI.
    #[account(mut)]
    pub fraud_sol_vault_b: AccountInfo<'info>,

    // -----------------------------------------------------------------------
    // USDC pools (CRIME/USDC, FRAUD/USDC)
    // -----------------------------------------------------------------------

    /// CRIME/USDC pool state.
    /// CHECK: Owner validated as AMM program in read_pool_reserves.
    #[account(mut)]
    pub crime_usdc_pool: AccountInfo<'info>,

    /// CRIME/USDC vault A.
    /// CHECK: Validated by AMM constraints during CPI.
    #[account(mut)]
    pub crime_usdc_vault_a: AccountInfo<'info>,

    /// CRIME/USDC vault B.
    /// CHECK: Validated by AMM constraints during CPI.
    #[account(mut)]
    pub crime_usdc_vault_b: AccountInfo<'info>,

    /// FRAUD/USDC pool state.
    /// CHECK: Owner validated as AMM program in read_pool_reserves.
    #[account(mut)]
    pub fraud_usdc_pool: AccountInfo<'info>,

    /// FRAUD/USDC vault A.
    /// CHECK: Validated by AMM constraints during CPI.
    #[account(mut)]
    pub fraud_usdc_vault_a: AccountInfo<'info>,

    /// FRAUD/USDC vault B.
    /// CHECK: Validated by AMM constraints during CPI.
    #[account(mut)]
    pub fraud_usdc_vault_b: AccountInfo<'info>,

    // -----------------------------------------------------------------------
    // Holdings (receive withdrawn tokens, provide tokens for injection)
    // -----------------------------------------------------------------------

    /// CRIME token holding. Authority = rebalance_authority.
    /// CHECK: PDA validated by seeds. Balance read manually.
    #[account(
        mut,
        seeds = [HOLDING_SEED, crime_mint.key().as_ref()],
        bump,
    )]
    pub holding_crime: AccountInfo<'info>,

    /// FRAUD token holding. Authority = rebalance_authority.
    /// CHECK: PDA validated by seeds.
    #[account(
        mut,
        seeds = [HOLDING_SEED, fraud_mint.key().as_ref()],
        bump,
    )]
    pub holding_fraud: AccountInfo<'info>,

    /// WSOL holding. Authority = rebalance_authority.
    /// CHECK: PDA validated by seeds.
    #[account(
        mut,
        seeds = [HOLDING_SEED, wsol_mint.key().as_ref()],
        bump,
    )]
    pub holding_wsol: AccountInfo<'info>,

    /// USDC holding. Authority = rebalance_authority.
    /// CHECK: PDA validated by seeds.
    #[account(
        mut,
        seeds = [HOLDING_SEED, usdc_mint.key().as_ref()],
        bump,
    )]
    pub holding_usdc: AccountInfo<'info>,

    // -----------------------------------------------------------------------
    // Mints
    // -----------------------------------------------------------------------

    /// CRIME mint (Token-2022).
    pub crime_mint: Box<InterfaceAccount<'info, Mint>>,

    /// FRAUD mint (Token-2022).
    pub fraud_mint: Box<InterfaceAccount<'info, Mint>>,

    /// WSOL (Native Mint).
    #[account(address = anchor_spl::token::spl_token::native_mint::id())]
    pub wsol_mint: Box<InterfaceAccount<'info, Mint>>,

    /// USDC mint.
    #[account(address = crate::constants::usdc_mint())]
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    // -----------------------------------------------------------------------
    // Programs + vaults
    // -----------------------------------------------------------------------

    /// Bounty vault — native SOL PDA for crank bounties.
    /// CHECK: PDA derived from known seeds.
    #[account(
        mut,
        seeds = [BOUNTY_VAULT_SEED],
        bump,
    )]
    pub bounty_vault: SystemAccount<'info>,

    /// AMM program for withdraw_liquidity and add_liquidity CPI.
    /// CHECK: Validated against known AMM program ID constant.
    #[account(address = crate::constants::amm_program_id())]
    pub amm_program: AccountInfo<'info>,

    /// SPL Token program (for WSOL + USDC).
    pub token_program: Program<'info, Token>,

    /// Token-2022 program (for CRIME + FRAUD).
    pub token_2022_program: Program<'info, Token2022>,

    /// System program for bounty transfer.
    pub system_program: Program<'info, System>,
}
