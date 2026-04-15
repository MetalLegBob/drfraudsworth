//! swap_sol_sell: CRIME/FRAUD -> SOL with sell tax.
//!
//! Sell flow (per Tax_Pool_Logic_Spec.md Section 14.3, updated Phase 48):
//! 1. User sends full token amount to AMM (no deduction on input)
//! 2. AMM returns gross WSOL output to user's WSOL ATA
//! 3. Tax calculated on gross output: tax = sol_gross * sell_tax_bps / 10_000
//! 4. Tax WSOL transferred from user's WSOL ATA to protocol intermediary
//! 5. Intermediary closed to swap_authority (unwraps WSOL to native SOL)
//! 6. Native SOL distributed from swap_authority: 71% staking, 24% carnage, 5% treasury
//! 7. Intermediary re-created and re-initialized for next sell
//! 8. User retains net WSOL output: sol_net = sol_gross - tax
//! 9. Slippage check: sol_net >= minimum_output (checked AFTER tax)
//!
//! Key difference from buy: Tax is applied to OUTPUT, not input.
//! Tax is deducted from WSOL swap output (not user's native SOL balance).
//!
//! Source: Tax_Pool_Logic_Spec.md Sections 10.3, 14.3

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_signed},
    rent::Rent,
    sysvar::Sysvar,
};
use anchor_lang::AccountDeserialize;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{
    amm_program_id, crime_mint, epoch_program_id, fraud_mint, staking_program_id,
    treasury_pubkey, CARNAGE_SOL_VAULT_SEED, DEPOSIT_REWARDS_DISCRIMINATOR,
    ESCROW_VAULT_SEED, MINIMUM_OUTPUT_FLOOR_BPS, STAKE_POOL_SEED, SWAP_AUTHORITY_SEED,
    TAX_AUTHORITY_SEED, WSOL_INTERMEDIARY_SEED,
};
use crate::errors::TaxError;
use crate::events::{PoolType, SwapDirection, TaxedSwap};
use crate::helpers::pool_reader::read_pool_reserves_with_token_mint;
use crate::helpers::tax_math::{calculate_output_floor, calculate_tax, split_distribution};
use crate::state::EpochState;

/// Execute CRIME/FRAUD -> SOL swap with sell tax on output.
///
/// Flow:
/// 1. Read and validate EpochState (dynamic tax rates)
/// 2. Record user's WSOL balance before swap
/// 3. Execute AMM CPI (BtoA direction: token B in, token A out)
/// 4. Calculate gross output from balance difference
/// 5. Calculate tax on gross output
/// 6. Guard: reject sells where tax >= gross_output (InsufficientOutput)
/// 7. Check slippage: net_output >= minimum_output
/// 8. Split tax into (staking, carnage, treasury) portions
/// 9. Transfer tax WSOL from user to intermediary (user signs)
/// 10. Close intermediary to swap_authority (unwraps WSOL to native SOL)
/// 11. Distribute native SOL from swap_authority to 3 destinations
/// 12. Re-create and re-initialize intermediary for next sell
/// 13. Emit TaxedSwap event
///
/// # Arguments
/// * `amount_in` - Token amount to sell (CRIME or FRAUD)
/// * `minimum_output` - Minimum SOL to receive AFTER tax
/// * `is_crime` - true = CRIME/SOL pool, false = FRAUD/SOL pool
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, SwapSolSell<'info>>,
    amount_in: u64,
    minimum_output: u64,
    is_crime: bool,
) -> Result<()> {
    // =========================================================================
    // 1. Read and validate EpochState
    // =========================================================================

    // Owner check: EpochState must be owned by Epoch Program.
    // CRITICAL: This prevents attackers from passing a fake EpochState with 0% tax.
    let epoch_program = epoch_program_id();
    require!(
        ctx.accounts.epoch_state.owner == &epoch_program,
        TaxError::InvalidEpochState
    );

    // Deserialize EpochState data.
    // try_deserialize validates the discriminator automatically (sha256("account:EpochState")[0..8]).
    let epoch_state = {
        let data = ctx.accounts.epoch_state.try_borrow_data()?;
        let mut data_slice: &[u8] = &data;
        EpochState::try_deserialize(&mut data_slice)
            .map_err(|_| error!(TaxError::InvalidEpochState))?
    };

    // Validate EpochState is initialized (defense-in-depth).
    require!(epoch_state.initialized, TaxError::InvalidEpochState);

    // =========================================================================
    // 1b. Bind tax-side identity to the validated mint_b account (Option B)
    //
    // The caller-supplied `is_crime` flag must NOT be the source of truth for
    // which tax schedule applies — it is treated as a witness and cross-checked
    // against the on-chain mint_b account. The mint_b account is itself
    // validated against pool.mint_b in step 2b below (Option C), so the chain
    // is: pool bytes -> mint_b account -> derived_is_crime -> tax schedule.
    //
    // See 122.1-CONTEXT.md §3.1.
    // =========================================================================
    let mint_b_key = ctx.accounts.mint_b.key();
    let derived_is_crime = if mint_b_key == crime_mint() {
        true
    } else if mint_b_key == fraud_mint() {
        false
    } else {
        return err!(TaxError::UnknownTaxedMint);
    };
    require!(
        is_crime == derived_is_crime,
        TaxError::TaxIdentityMismatch
    );

    // Get the appropriate tax rate (is_buy = false for sell direction).
    // Use derived_is_crime so the on-chain state is the only source of truth.
    let tax_bps = epoch_state.get_tax_bps(derived_is_crime, false);

    // =========================================================================
    // 2. Record user's WSOL balance before swap
    // =========================================================================
    let wsol_before = ctx.accounts.user_token_a.amount;

    // =========================================================================
    // 2b. Enforce protocol minimum output floor (SEC-10) AND bind pool.mint_b
    //     to the passed mint_b account (Option C, see 122.1-CONTEXT.md §3.2).
    //
    // For sell (BtoA): reserve_in = reserve_b (token), reserve_out = reserve_a (SOL).
    // The floor protects against zero-slippage sandwich attacks BEFORE CPI.
    // We verify the user's stated minimum is reasonable; the AMM then enforces
    // actual_output >= minimum during execution.
    //
    // The extended pool reader also returns the pool's stored token-side mint
    // (whichever of pool.mint_a / pool.mint_b is NOT NATIVE_MINT). We bind it
    // to the passed mint_b account so a caller cannot pair a CRIME pool with
    // the FRAUD mint account (or vice versa) to bypass the tax schedule
    // selected in step 1b.
    //
    // NOTE: minimum_output is checked (not gross_output) because this runs
    // before the CPI executes. The floor catches bots/frontends that send
    // minimum_output=0 before spending any compute on the swap.
    // =========================================================================
    let (sol_reserve, token_reserve, pool_token_mint) =
        read_pool_reserves_with_token_mint(&ctx.accounts.pool)?;
    require_keys_eq!(
        pool_token_mint,
        ctx.accounts.mint_b.key(),
        TaxError::PoolMintMismatch
    );
    let output_floor = calculate_output_floor(token_reserve, sol_reserve, amount_in, MINIMUM_OUTPUT_FLOOR_BPS)
        .ok_or(error!(TaxError::TaxOverflow))?;
    require!(
        minimum_output >= output_floor,
        TaxError::MinimumOutputFloorViolation
    );

    // =========================================================================
    // 3. Execute AMM CPI with direction = BtoA (token B in, token A out)
    //
    // We use invoke_signed with raw instruction because:
    // - AMM program may not have CPI stubs generated
    // - We need to pass remaining_accounts for transfer hooks
    // =========================================================================
    let swap_authority_seeds: &[&[u8]] = &[SWAP_AUTHORITY_SEED, &[ctx.bumps.swap_authority]];

    // Build AMM swap_sol_pool instruction
    // Instruction data: [discriminator][amount_in][direction][minimum_amount_out]
    // - discriminator: 8 bytes (anchor auto-generates from instruction name)
    // - amount_in: 8 bytes (u64)
    // - direction: 1 byte (0=AtoB, 1=BtoA)
    // - minimum_amount_out: 8 bytes (u64)
    //
    // For swap_sol_pool, the discriminator is derived from:
    // sha256("global:swap_sol_pool")[0..8]
    // Precomputed: [0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a]
    let discriminator: [u8; 8] = [
        0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a,
    ];

    // Direction: 1 = BtoA (selling token B for token A)
    let direction: u8 = 1;

    // Compute gross floor: what AMM must output so user gets >= minimum_output after tax.
    // Formula: gross_floor = ceil(minimum_output * 10000 / (10000 - tax_bps))
    // This prevents the AMM from executing swaps where the output would be too low
    // to satisfy the user's net minimum after tax deduction.
    let bps_denom: u64 = 10_000;
    let gross_floor = if minimum_output > 0 && (tax_bps as u64) < bps_denom {
        let numerator = (minimum_output as u128)
            .checked_mul(bps_denom as u128)
            .ok_or(error!(TaxError::TaxOverflow))?;
        let denominator = (bps_denom as u128)
            .checked_sub(tax_bps as u128)
            .ok_or(error!(TaxError::TaxOverflow))?;
        // Ceil division: (numerator + denominator - 1) / denominator
        let result = numerator
            .checked_add(denominator - 1)
            .ok_or(error!(TaxError::TaxOverflow))?
            / denominator;
        u64::try_from(result).map_err(|_| error!(TaxError::TaxOverflow))?
    } else {
        0
    };
    let amm_minimum: u64 = gross_floor;

    // Build instruction data
    let mut ix_data = Vec::with_capacity(25);
    ix_data.extend_from_slice(&discriminator);
    ix_data.extend_from_slice(&amount_in.to_le_bytes());
    ix_data.extend_from_slice(&[direction]);
    ix_data.extend_from_slice(&amm_minimum.to_le_bytes());

    // Build account metas for AMM swap_sol_pool
    // Order must match AMM's SwapSolPool accounts struct
    let accounts = vec![
        AccountMeta::new_readonly(ctx.accounts.swap_authority.key(), true), // swap_authority (signer)
        AccountMeta::new(ctx.accounts.pool.key(), false),                   // pool
        AccountMeta::new(ctx.accounts.pool_vault_a.key(), false),           // vault_a
        AccountMeta::new(ctx.accounts.pool_vault_b.key(), false),           // vault_b
        AccountMeta::new_readonly(ctx.accounts.mint_a.key(), false),        // mint_a
        AccountMeta::new_readonly(ctx.accounts.mint_b.key(), false),        // mint_b
        AccountMeta::new(ctx.accounts.user_token_a.key(), false),           // user_token_a
        AccountMeta::new(ctx.accounts.user_token_b.key(), false),           // user_token_b
        AccountMeta::new_readonly(ctx.accounts.user.key(), true),           // user (signer)
        AccountMeta::new_readonly(ctx.accounts.token_program_a.key(), false), // token_program_a
        AccountMeta::new_readonly(ctx.accounts.token_program_b.key(), false), // token_program_b
    ];

    // Collect remaining accounts for transfer hook (if any)
    let remaining: Vec<AccountMeta> = ctx
        .remaining_accounts
        .iter()
        .map(|acc| {
            if acc.is_writable {
                AccountMeta::new(*acc.key, acc.is_signer)
            } else {
                AccountMeta::new_readonly(*acc.key, acc.is_signer)
            }
        })
        .collect();

    let mut all_accounts = accounts;
    all_accounts.extend(remaining);

    let swap_ix = Instruction {
        program_id: ctx.accounts.amm_program.key(),
        accounts: all_accounts,
        data: ix_data,
    };

    // Collect AccountInfos for invoke_signed
    let mut account_infos = vec![
        ctx.accounts.swap_authority.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.pool_vault_a.to_account_info(),
        ctx.accounts.pool_vault_b.to_account_info(),
        ctx.accounts.mint_a.to_account_info(),
        ctx.accounts.mint_b.to_account_info(),
        ctx.accounts.user_token_a.to_account_info(),
        ctx.accounts.user_token_b.to_account_info(),
        ctx.accounts.user.to_account_info(),
        ctx.accounts.token_program_a.to_account_info(),
        ctx.accounts.token_program_b.to_account_info(),
        ctx.accounts.amm_program.to_account_info(),
    ];

    // Add remaining accounts for transfer hook
    for acc in ctx.remaining_accounts.iter() {
        account_infos.push(acc.clone());
    }

    invoke_signed(&swap_ix, &account_infos, &[swap_authority_seeds])?;

    // =========================================================================
    // 4. Calculate gross output from balance difference
    // =========================================================================
    ctx.accounts.user_token_a.reload()?;
    let wsol_after = ctx.accounts.user_token_a.amount;

    let gross_output = wsol_after
        .checked_sub(wsol_before)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // =========================================================================
    // 5. Calculate tax on gross output
    // =========================================================================
    let tax_amount = calculate_tax(gross_output, tax_bps).ok_or(error!(TaxError::TaxOverflow))?;

    // =========================================================================
    // 6. Calculate net output and check guards
    //
    // Critical: Slippage is checked AFTER tax deduction per RESEARCH.md.
    // This ensures minimum_output represents what user actually receives.
    // =========================================================================
    let net_output = gross_output
        .checked_sub(tax_amount)
        .ok_or(error!(TaxError::TaxOverflow))?;

    // Guard: reject sells where tax consumes entire output
    require!(net_output > 0, TaxError::InsufficientOutput);

    require!(net_output >= minimum_output, TaxError::SlippageExceeded);

    // =========================================================================
    // 7. Split tax distribution
    // =========================================================================
    let (staking_portion, carnage_portion, treasury_portion) =
        split_distribution(tax_amount).ok_or(error!(TaxError::TaxOverflow))?;

    // =========================================================================
    // 8. Transfer-Close-Distribute-Reinit: WSOL intermediary tax flow
    //
    // This replaces the old pattern of system_instruction::transfer from user.
    // Tax is now deducted from the WSOL swap output, not the user's native SOL.
    //
    // Steps:
    //   a) SPL Token transfer: user_token_a -> wsol_intermediary (tax_amount)
    //   b) SPL Token close_account: wsol_intermediary -> swap_authority (unwrap)
    //   c) System::transfer x3: swap_authority -> staking/carnage/treasury
    //   d) System::create_account + InitializeAccount3: recreate intermediary
    // =========================================================================

    // --- Step 8a: Transfer tax WSOL from user to intermediary ---
    // User signed the top-level TX; signature propagates via CPI.
    // Use invoke (not invoke_signed) because user is the authority.
    let transfer_tax_ix = Instruction {
        program_id: ctx.accounts.token_program_a.key(),
        accounts: vec![
            AccountMeta::new(ctx.accounts.user_token_a.key(), false),
            AccountMeta::new(ctx.accounts.wsol_intermediary.key(), false),
            AccountMeta::new_readonly(ctx.accounts.user.key(), true),
        ],
        data: {
            let mut d = vec![3u8]; // SPL Token Transfer instruction discriminator
            d.extend_from_slice(&tax_amount.to_le_bytes());
            d
        },
    };
    invoke(
        &transfer_tax_ix,
        &[
            ctx.accounts.user_token_a.to_account_info(),
            ctx.accounts.wsol_intermediary.to_account_info(),
            ctx.accounts.user.to_account_info(),
            ctx.accounts.token_program_a.to_account_info(),
        ],
    )?;

    // --- Step 8b: Close intermediary to swap_authority (unwrap WSOL) ---
    // close_account transfers ALL lamports (token balance + rent) to destination.
    // swap_authority is the owner of the intermediary, so it signs.
    let close_ix = Instruction {
        program_id: ctx.accounts.token_program_a.key(),
        accounts: vec![
            AccountMeta::new(ctx.accounts.wsol_intermediary.key(), false),
            AccountMeta::new(ctx.accounts.swap_authority.key(), false),
            AccountMeta::new_readonly(ctx.accounts.swap_authority.key(), true),
        ],
        data: vec![9u8], // CloseAccount instruction discriminator
    };
    invoke_signed(
        &close_ix,
        &[
            ctx.accounts.wsol_intermediary.to_account_info(),
            ctx.accounts.swap_authority.to_account_info(),
            ctx.accounts.token_program_a.to_account_info(),
        ],
        &[swap_authority_seeds],
    )?;

    // --- Step 8c: Distribute native SOL from swap_authority to 3 destinations ---
    // swap_authority now holds tax_amount + rent_exempt as native SOL lamports.
    // We distribute only the tax portions; rent lamports are retained for reinit.

    if staking_portion > 0 {
        invoke_signed(
            &anchor_lang::solana_program::system_instruction::transfer(
                ctx.accounts.swap_authority.key,
                ctx.accounts.staking_escrow.key,
                staking_portion,
            ),
            &[
                ctx.accounts.swap_authority.to_account_info(),
                ctx.accounts.staking_escrow.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[swap_authority_seeds],
        )?;

        // CPI to Staking Program's deposit_rewards to update pending_rewards counter.
        // The SOL is already in escrow; this just updates the state.
        let tax_authority_seeds: &[&[u8]] =
            &[TAX_AUTHORITY_SEED, &[ctx.bumps.tax_authority]];

        // Build deposit_rewards instruction data: discriminator (8) + amount (8)
        let mut deposit_ix_data = Vec::with_capacity(16);
        deposit_ix_data.extend_from_slice(&DEPOSIT_REWARDS_DISCRIMINATOR);
        deposit_ix_data.extend_from_slice(&staking_portion.to_le_bytes());

        // Build account metas for deposit_rewards:
        // Order matches Staking's DepositRewards struct: tax_authority, stake_pool, escrow_vault
        let deposit_accounts = vec![
            AccountMeta::new_readonly(ctx.accounts.tax_authority.key(), true), // signer
            AccountMeta::new(ctx.accounts.stake_pool.key(), false),
            AccountMeta::new_readonly(ctx.accounts.staking_escrow.key(), false), // escrow_vault (balance reconciliation)
        ];

        let deposit_ix = Instruction {
            program_id: ctx.accounts.staking_program.key(),
            accounts: deposit_accounts,
            data: deposit_ix_data,
        };

        invoke_signed(
            &deposit_ix,
            &[
                ctx.accounts.tax_authority.to_account_info(),
                ctx.accounts.stake_pool.to_account_info(),
                ctx.accounts.staking_escrow.to_account_info(),
                ctx.accounts.staking_program.to_account_info(),
            ],
            &[tax_authority_seeds],
        )?;
    }

    if carnage_portion > 0 {
        invoke_signed(
            &anchor_lang::solana_program::system_instruction::transfer(
                ctx.accounts.swap_authority.key,
                ctx.accounts.carnage_vault.key,
                carnage_portion,
            ),
            &[
                ctx.accounts.swap_authority.to_account_info(),
                ctx.accounts.carnage_vault.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[swap_authority_seeds],
        )?;
    }

    if treasury_portion > 0 {
        invoke_signed(
            &anchor_lang::solana_program::system_instruction::transfer(
                ctx.accounts.swap_authority.key,
                ctx.accounts.treasury.key,
                treasury_portion,
            ),
            &[
                ctx.accounts.swap_authority.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[swap_authority_seeds],
        )?;
    }

    // --- Step 8d: Recreate the intermediary at its PDA address ---
    // After close_account, swap_authority retained the rent-exempt lamports.
    // Use these to fund the new intermediary account.
    let intermediary_seeds: &[&[u8]] = &[WSOL_INTERMEDIARY_SEED, &[ctx.bumps.wsol_intermediary]];
    let rent = Rent::get()?;
    let space = 165u64; // spl_token::state::Account::LEN
    let rent_lamports = rent.minimum_balance(space as usize);

    let create_ix = anchor_lang::solana_program::system_instruction::create_account(
        ctx.accounts.swap_authority.key,
        ctx.accounts.wsol_intermediary.key,
        rent_lamports,
        space,
        &ctx.accounts.token_program_a.key(),
    );

    // Both swap_authority (funder) and wsol_intermediary (PDA) must sign.
    invoke_signed(
        &create_ix,
        &[
            ctx.accounts.swap_authority.to_account_info(),
            ctx.accounts.wsol_intermediary.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[swap_authority_seeds, intermediary_seeds],
    )?;

    // Initialize as WSOL token account using InitializeAccount3.
    // InitializeAccount3 (discriminator 18) takes owner as instruction data
    // (32 bytes after discriminator) instead of as an account. No rent sysvar needed.
    let init_ix = Instruction {
        program_id: ctx.accounts.token_program_a.key(),
        accounts: vec![
            AccountMeta::new(ctx.accounts.wsol_intermediary.key(), false),
            AccountMeta::new_readonly(ctx.accounts.mint_a.key(), false),
        ],
        data: {
            let mut d = vec![18u8]; // InitializeAccount3 discriminator
            d.extend_from_slice(&ctx.accounts.swap_authority.key().to_bytes());
            d
        },
    };

    invoke(
        &init_ix,
        &[
            ctx.accounts.wsol_intermediary.to_account_info(),
            ctx.accounts.mint_a.to_account_info(),
            ctx.accounts.token_program_a.to_account_info(),
        ],
    )?;

    // =========================================================================
    // 9. Emit TaxedSwap event
    // =========================================================================
    let clock = Clock::get()?;
    emit!(TaxedSwap {
        user: ctx.accounts.user.key(),
        pool_type: if derived_is_crime {
            PoolType::SolCrime
        } else {
            PoolType::SolFraud
        },
        direction: SwapDirection::Sell,
        input_amount: amount_in,
        output_amount: net_output,
        tax_amount,
        tax_rate_bps: tax_bps,
        staking_portion,
        carnage_portion,
        treasury_portion,
        accumulator_portion: 0,
        epoch: epoch_state.current_epoch,
        slot: clock.slot,
    });

    Ok(())
}

/// Accounts for swap_sol_sell instruction (CRIME/FRAUD -> SOL).
///
/// Sell tax is deducted from WSOL OUTPUT via a protocol-owned intermediary.
/// Direction is BtoA (Token B = CRIME/FRAUD in, Token A = WSOL out).
///
/// Tax flow: user WSOL -> intermediary -> close to swap_authority ->
/// native SOL distributed to staking/carnage/treasury -> intermediary recreated.
///
/// Account ordering matches swap_sol_buy per Tax_Pool_Logic_Spec.md Section 10.3:
/// "Same constraints as swap_sol_buy, with input/output reversed"
///
/// Source: Tax_Pool_Logic_Spec.md Section 10.3, Phase 48 WSOL intermediary
#[derive(Accounts)]
pub struct SwapSolSell<'info> {
    /// User initiating the swap - signs SPL Token transfer of tax WSOL
    #[account(mut)]
    pub user: Signer<'info>,

    /// EpochState account from Epoch Program.
    /// Provides current tax rates for the swap.
    ///
    /// CHECK: Validated manually in handler:
    /// - Owner check: must be Epoch Program (prevents fake 0% tax)
    /// - Deserialization validates discriminator
    /// - initialized flag checked
    pub epoch_state: AccountInfo<'info>,

    /// Tax Program's swap_authority PDA - signs AMM CPI and tax distribution.
    /// Mutable because it receives lamports from close_account (unwrap)
    /// and sends them to tax destinations via system transfers.
    /// CHECK: PDA derived from seeds, used as signer for CPI
    #[account(
        mut,
        seeds = [SWAP_AUTHORITY_SEED],
        bump,
    )]
    pub swap_authority: AccountInfo<'info>,

    /// Tax Program's tax_authority PDA - signs Staking Program CPI
    /// CHECK: PDA derived from seeds, used as signer for deposit_rewards CPI
    #[account(
        seeds = [TAX_AUTHORITY_SEED],
        bump,
    )]
    pub tax_authority: AccountInfo<'info>,

    // === Pool State (AMM) ===
    /// AMM pool state - mutable for reserve updates
    /// CHECK: Validated in AMM CPI
    #[account(mut)]
    pub pool: AccountInfo<'info>,

    // === Pool Vaults ===
    /// Pool's WSOL vault (Token A)
    #[account(mut)]
    pub pool_vault_a: InterfaceAccount<'info, TokenAccount>,

    /// Pool's CRIME/FRAUD vault (Token B)
    #[account(mut)]
    pub pool_vault_b: InterfaceAccount<'info, TokenAccount>,

    // === Mints ===
    /// WSOL mint
    pub mint_a: InterfaceAccount<'info, Mint>,

    /// CRIME or FRAUD mint (Token-2022)
    pub mint_b: InterfaceAccount<'info, Mint>,

    // === User Token Accounts ===
    /// User's WSOL token account - receives gross output from AMM
    #[account(mut)]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    /// User's CRIME/FRAUD token account - sends tokens to AMM
    #[account(mut)]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    // === Staking Program Integration ===
    /// Staking Program's StakePool PDA - updated by deposit_rewards CPI
    /// CHECK: PDA validated by Staking Program via seeds constraint
    #[account(
        mut,
        seeds = [STAKE_POOL_SEED],
        bump,
        seeds::program = staking_program_id(),
    )]
    pub stake_pool: AccountInfo<'info>,

    // === Tax Distribution Targets ===
    /// Staking Program escrow - receives 71% of tax (native SOL)
    /// CHECK: PDA derived from Staking Program seeds
    #[account(
        mut,
        seeds = [ESCROW_VAULT_SEED],
        bump,
        seeds::program = staking_program_id(),
        constraint = true @ TaxError::InvalidStakingEscrow,
    )]
    pub staking_escrow: AccountInfo<'info>,

    /// Carnage Fund vault - receives 24% of tax (native SOL)
    /// CHECK: PDA derived from Epoch Program seeds
    #[account(
        mut,
        seeds = [CARNAGE_SOL_VAULT_SEED],
        bump,
        seeds::program = epoch_program_id(),
        constraint = true @ TaxError::InvalidCarnageVault,
    )]
    pub carnage_vault: AccountInfo<'info>,

    /// Protocol treasury - receives 5% of tax (native SOL)
    /// CHECK: Address validated against known treasury pubkey
    #[account(
        mut,
        address = treasury_pubkey() @ TaxError::InvalidTreasury,
    )]
    pub treasury: AccountInfo<'info>,

    /// Protocol-owned WSOL intermediary for atomic tax extraction.
    /// Holds tax portion of WSOL between transfer and unwrap.
    /// Owned by swap_authority PDA.
    /// Closed and re-created each sell to convert WSOL -> native SOL.
    ///
    /// CHECK: PDA derived from known seeds. Created/closed within handler.
    /// Account may be zero-lamport (just been closed) at validation time
    /// during same-TX sequential sells, but will be recreated within the handler.
    #[account(
        mut,
        seeds = [WSOL_INTERMEDIARY_SEED],
        bump,
    )]
    pub wsol_intermediary: AccountInfo<'info>,

    // === Programs ===
    /// AMM Program for swap CPI
    /// CHECK: Address validated against known AMM program ID
    #[account(address = amm_program_id() @ TaxError::InvalidAmmProgram)]
    pub amm_program: AccountInfo<'info>,

    /// SPL Token program (for WSOL)
    pub token_program_a: Interface<'info, TokenInterface>,

    /// Token-2022 program (for CRIME/FRAUD)
    pub token_program_b: Interface<'info, TokenInterface>,

    /// System program (for native SOL transfers and account creation)
    pub system_program: Program<'info, System>,

    /// Staking Program for deposit_rewards CPI
    /// CHECK: Program ID validated in constants.rs staking_program_id()
    #[account(address = staking_program_id() @ TaxError::InvalidStakingProgram)]
    pub staking_program: AccountInfo<'info>,
}
