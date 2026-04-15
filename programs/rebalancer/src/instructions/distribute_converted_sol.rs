//! distribute_converted_sol: Unwrap WSOL holding, skim bounty, split SOL 71/24/5.
//!
//! Step 2 of the USDC conversion pipeline (REBAL-02):
//!   convert_usdc (USDC -> WSOL) -> distribute_converted_sol (WSOL -> SOL split)
//!
//! Follows the close-distribute-reinit pattern from Tax Program's swap_sol_sell.rs:
//! 1. Close WSOL account (unwraps to rebalance_authority as native SOL)
//! 2. Skim bounty refill to bounty_vault (self-sustaining funding)
//! 3. Split remaining SOL 71/24/5 (staking/carnage/treasury)
//! 4. Recreate WSOL holding account at same PDA
//!
//! Permissionless: anyone can call after convert_usdc has deposited WSOL.
//!
//! Source: Phase 128, Plan 02

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_lang::solana_program::rent::Rent;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::sysvar::Sysvar;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_spl::token::Token;
use anchor_spl::token_interface::Mint;

use crate::constants::{
    BOUNTY_SKIM_LAMPORTS, BOUNTY_VAULT_SEED, HOLDING_SEED, REBALANCE_SEED,
};
use crate::errors::RebalancerError;
use crate::events::SolDistributed;
use crate::helpers::rebalance_math::split_distribution;
use crate::state::RebalancerConfig;

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Unwrap WSOL holding, skim bounty refill, and distribute SOL 71/24/5.
///
/// # Flow
/// 1. Read WSOL balance — skip if 0
/// 2. Close WSOL account (unwrap to rebalance_authority)
/// 3. Skim bounty refill to bounty_vault
/// 4. Split remaining: 71% staking, 24% carnage, 5% treasury
/// 5. Recreate WSOL holding at same PDA address
/// 6. Emit SolDistributed event
pub fn handler(ctx: Context<DistributeConvertedSol>) -> Result<()> {
    // =========================================================================
    // 1. Read WSOL balance before close
    // =========================================================================
    let holding_info = ctx.accounts.holding_wsol.to_account_info();
    let wsol_balance = {
        let data = holding_info.try_borrow_data()?;
        // SPL Token account: amount at offset 64, 8 bytes little-endian
        if data.len() >= 72 {
            u64::from_le_bytes(data[64..72].try_into().unwrap())
        } else {
            0
        }
    };

    if wsol_balance == 0 {
        msg!("No WSOL to distribute (holding balance = 0)");
        return Ok(());
    }

    msg!("Distributing {} WSOL lamports", wsol_balance);

    let rebalance_bump = ctx.bumps.rebalance_authority;
    let rebalance_seeds: &[&[u8]] = &[REBALANCE_SEED, &[rebalance_bump]];

    // =========================================================================
    // 2. Close WSOL account (unwrap to rebalance_authority)
    //
    // close_account transfers ALL lamports (token balance + rent) to destination.
    // After close, rebalance_authority receives: wsol_balance + rent_exempt.
    // =========================================================================
    let close_ix = Instruction {
        program_id: ctx.accounts.token_program.key(),
        accounts: vec![
            AccountMeta::new(ctx.accounts.holding_wsol.key(), false),
            AccountMeta::new(ctx.accounts.rebalance_authority.key(), false),
            AccountMeta::new_readonly(ctx.accounts.rebalance_authority.key(), true),
        ],
        data: vec![9u8], // CloseAccount instruction discriminator
    };

    invoke_signed(
        &close_ix,
        &[
            ctx.accounts.holding_wsol.to_account_info(),
            ctx.accounts.rebalance_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
        &[rebalance_seeds],
    )?;

    msg!("WSOL holding closed, {} lamports unwrapped", wsol_balance);

    // =========================================================================
    // 3. Calculate rent for recreate (reserve from distributable amount)
    //
    // Distributable = wsol_balance (token amount).
    // The rent-exempt lamports from the closed account will fund recreate.
    // =========================================================================
    let rent = Rent::get()?;
    let rent_for_token_account = rent.minimum_balance(165); // SPL token account = 165 bytes
    let _ = rent_for_token_account; // rent is retained in rebalance_authority for recreate

    // Distributable is exactly the WSOL token balance.
    // The rent lamports are used to recreate the account.
    let distributable = wsol_balance;

    // =========================================================================
    // 4. Bounty skim — refill bounty vault for self-sustainability
    // =========================================================================
    let bounty_skim = if distributable > BOUNTY_SKIM_LAMPORTS {
        let vault_bump = ctx.bumps.bounty_vault;

        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.rebalance_authority.key,
                ctx.accounts.bounty_vault.key,
                BOUNTY_SKIM_LAMPORTS,
            ),
            &[
                ctx.accounts.rebalance_authority.to_account_info(),
                ctx.accounts.bounty_vault.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[rebalance_seeds],
        )?;

        msg!(
            "Bounty skim: {} lamports to bounty_vault",
            BOUNTY_SKIM_LAMPORTS
        );
        BOUNTY_SKIM_LAMPORTS
    } else {
        msg!(
            "Distributable {} too small for bounty skim {} (skipped)",
            distributable,
            BOUNTY_SKIM_LAMPORTS
        );
        0
    };

    let remaining = distributable
        .checked_sub(bounty_skim)
        .ok_or(RebalancerError::MathOverflow)?;

    // =========================================================================
    // 5. 71/24/5 split via rebalance_math::split_distribution
    // =========================================================================
    let (staking_amount, carnage_amount, treasury_amount) =
        split_distribution(remaining).ok_or(RebalancerError::MathOverflow)?;

    if staking_amount > 0 {
        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.rebalance_authority.key,
                ctx.accounts.staking_escrow.key,
                staking_amount,
            ),
            &[
                ctx.accounts.rebalance_authority.to_account_info(),
                ctx.accounts.staking_escrow.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[rebalance_seeds],
        )?;
    }

    if carnage_amount > 0 {
        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.rebalance_authority.key,
                ctx.accounts.carnage_sol_vault.key,
                carnage_amount,
            ),
            &[
                ctx.accounts.rebalance_authority.to_account_info(),
                ctx.accounts.carnage_sol_vault.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[rebalance_seeds],
        )?;
    }

    if treasury_amount > 0 {
        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.rebalance_authority.key,
                ctx.accounts.treasury.key,
                treasury_amount,
            ),
            &[
                ctx.accounts.rebalance_authority.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[rebalance_seeds],
        )?;
    }

    msg!(
        "SOL distributed: staking={}, carnage={}, treasury={}",
        staking_amount,
        carnage_amount,
        treasury_amount
    );

    // =========================================================================
    // 6. Recreate WSOL holding at same PDA address
    //
    // Same close-distribute-reinit pattern as Tax Program's swap_sol_sell.rs.
    // rebalance_authority retained the rent-exempt lamports from close.
    // =========================================================================
    let holding_bump = ctx.bumps.holding_wsol;
    let holding_seeds: &[&[u8]] = &[
        HOLDING_SEED,
        ctx.accounts.wsol_mint.to_account_info().key.as_ref(),
        &[holding_bump],
    ];

    let space = 165u64; // spl_token::state::Account::LEN
    let rent_lamports = rent.minimum_balance(space as usize);

    let create_ix = system_instruction::create_account(
        ctx.accounts.rebalance_authority.key,
        ctx.accounts.holding_wsol.key,
        rent_lamports,
        space,
        &ctx.accounts.token_program.key(),
    );

    // Both rebalance_authority (funder) and holding_wsol (PDA) must sign.
    invoke_signed(
        &create_ix,
        &[
            ctx.accounts.rebalance_authority.to_account_info(),
            ctx.accounts.holding_wsol.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[rebalance_seeds, holding_seeds],
    )?;

    // Initialize as WSOL token account using InitializeAccount3.
    // InitializeAccount3 (discriminator 18) takes owner as instruction data
    // (32 bytes after discriminator) instead of as an account. No rent sysvar needed.
    let init_ix = Instruction {
        program_id: ctx.accounts.token_program.key(),
        accounts: vec![
            AccountMeta::new(ctx.accounts.holding_wsol.key(), false),
            AccountMeta::new_readonly(ctx.accounts.wsol_mint.key(), false),
        ],
        data: {
            let mut d = vec![18u8]; // InitializeAccount3 discriminator
            d.extend_from_slice(&ctx.accounts.rebalance_authority.key().to_bytes());
            d
        },
    };

    invoke(
        &init_ix,
        &[
            ctx.accounts.holding_wsol.to_account_info(),
            ctx.accounts.wsol_mint.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;

    msg!("WSOL holding recreated at PDA");

    // =========================================================================
    // 7. Emit event
    // =========================================================================
    let clock = Clock::get()?;

    emit!(SolDistributed {
        total_sol: distributable,
        staking: staking_amount,
        carnage: carnage_amount,
        treasury: treasury_amount,
        bounty_skim,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Distribution complete: total={}, staking={}, carnage={}, treasury={}, bounty_skim={}",
        distributable,
        staking_amount,
        carnage_amount,
        treasury_amount,
        bounty_skim
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct DistributeConvertedSol<'info> {
    /// Caller who triggers the distribution. Permissionless.
    #[account(mut)]
    pub caller: Signer<'info>,

    /// RebalancerConfig singleton — read-only for validation.
    #[account(
        seeds = [crate::constants::REBALANCER_CONFIG_SEED],
        bump = rebalancer_config.bump,
        constraint = rebalancer_config.initialized @ RebalancerError::AlreadyInitialized,
    )]
    pub rebalancer_config: Account<'info, RebalancerConfig>,

    /// RebalanceAuthority PDA — owns the WSOL holding, receives unwrapped SOL,
    /// signs close/create/transfer operations.
    ///
    /// CHECK: PDA derived from known seeds; validated by seeds constraint.
    /// Mutable because it receives SOL from WSOL close and sends SOL distributions.
    #[account(
        mut,
        seeds = [REBALANCE_SEED],
        bump,
    )]
    pub rebalance_authority: SystemAccount<'info>,

    /// WSOL holding — closed to unwrap, then recreated.
    /// Uses AccountInfo (not InterfaceAccount) because the account is closed
    /// and recreated within this instruction — Anchor can't deserialize a
    /// closed account.
    ///
    /// CHECK: PDA derived from known seeds (HOLDING_SEED + wsol_mint).
    /// Balance read manually from raw bytes before close.
    #[account(
        mut,
        seeds = [HOLDING_SEED, wsol_mint.key().as_ref()],
        bump,
    )]
    pub holding_wsol: AccountInfo<'info>,

    /// Bounty vault — receives skim refill for self-sustainability.
    ///
    /// CHECK: PDA derived from known seeds; lamports managed via system_program transfer.
    #[account(
        mut,
        seeds = [BOUNTY_VAULT_SEED],
        bump,
    )]
    pub bounty_vault: SystemAccount<'info>,

    /// Staking escrow — receives 71% of distributed SOL.
    ///
    /// CHECK: Validated against feature-gated constant address.
    /// External PDA owned by Staking Program.
    #[account(
        mut,
        address = crate::constants::staking_escrow_address()
    )]
    pub staking_escrow: SystemAccount<'info>,

    /// Carnage SOL vault — receives 24% of distributed SOL.
    ///
    /// CHECK: Validated against feature-gated constant address.
    /// External PDA owned by Epoch Program.
    #[account(
        mut,
        address = crate::constants::carnage_sol_vault_address()
    )]
    pub carnage_sol_vault: SystemAccount<'info>,

    /// Treasury wallet — receives 5% of distributed SOL.
    ///
    /// CHECK: Validated against feature-gated constant address.
    #[account(
        mut,
        address = crate::constants::treasury_address()
    )]
    pub treasury: SystemAccount<'info>,

    /// WSOL (Native Mint) — validated against spl_token native mint.
    /// Needed for holding_wsol PDA derivation and InitializeAccount3.
    #[account(
        address = anchor_spl::token::spl_token::native_mint::id()
    )]
    pub wsol_mint: Box<InterfaceAccount<'info, Mint>>,

    /// SPL Token program for close and initialize operations.
    pub token_program: Program<'info, Token>,

    /// System program for create_account and transfer.
    pub system_program: Program<'info, System>,
}
