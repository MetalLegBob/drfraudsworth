//! update_config: Admin-gated configuration parameter update.
//!
//! Allows the admin (Squads multisig) to update any combination of
//! RebalancerConfig fields. Each field is optional -- only provided
//! fields are updated. Emits a ConfigUpdated event per changed field.
//!
//! Source: Phase 128, Plan 01

use anchor_lang::prelude::*;

use crate::constants::REBALANCER_CONFIG_SEED;
use crate::errors::RebalancerError;
use crate::events::ConfigUpdated;
use crate::state::RebalancerConfig;

/// Handler for update_config.
///
/// Validates each provided parameter and updates the config field if valid.
/// Emits a ConfigUpdated event for each changed field.
pub fn handler(
    ctx: Context<UpdateConfig>,
    target_bps: Option<u16>,
    min_delta: Option<u16>,
    cost_ceiling_bps: Option<u16>,
    jupiter_program_id: Option<Pubkey>,
    bounty_lamports: Option<u64>,
    rebalance_bounty_lamports: Option<u64>,
) -> Result<()> {
    let config = &mut ctx.accounts.rebalancer_config;
    let clock = Clock::get()?;
    let ts = clock.unix_timestamp;

    // =========================================================================
    // Update each field if provided, with validation
    // =========================================================================

    if let Some(new_target_bps) = target_bps {
        require!(new_target_bps <= 10_000, RebalancerError::InvalidTargetBps);
        let old = config.target_bps;
        config.target_bps = new_target_bps;
        emit!(ConfigUpdated {
            field: "target_bps".to_string(),
            old_value: old as u64,
            new_value: new_target_bps as u64,
            timestamp: ts,
        });
    }

    if let Some(new_min_delta) = min_delta {
        require!(new_min_delta <= 10_000, RebalancerError::InvalidMinDelta);
        let old = config.min_delta;
        config.min_delta = new_min_delta;
        emit!(ConfigUpdated {
            field: "min_delta".to_string(),
            old_value: old as u64,
            new_value: new_min_delta as u64,
            timestamp: ts,
        });
    }

    if let Some(new_cost_ceiling) = cost_ceiling_bps {
        require!(new_cost_ceiling <= 10_000, RebalancerError::InvalidCostCeiling);
        let old = config.cost_ceiling_bps;
        config.cost_ceiling_bps = new_cost_ceiling;
        emit!(ConfigUpdated {
            field: "cost_ceiling_bps".to_string(),
            old_value: old as u64,
            new_value: new_cost_ceiling as u64,
            timestamp: ts,
        });
    }

    if let Some(new_jup_id) = jupiter_program_id {
        require!(
            new_jup_id != Pubkey::default(),
            RebalancerError::InvalidJupiterProgram
        );
        let old = config.jupiter_program_id;
        config.jupiter_program_id = new_jup_id;
        emit!(ConfigUpdated {
            field: "jupiter_program_id".to_string(),
            old_value: u64::from_le_bytes(old.to_bytes()[0..8].try_into().unwrap()),
            new_value: u64::from_le_bytes(new_jup_id.to_bytes()[0..8].try_into().unwrap()),
            timestamp: ts,
        });
    }

    if let Some(new_bounty) = bounty_lamports {
        let old = config.bounty_lamports;
        config.bounty_lamports = new_bounty;
        emit!(ConfigUpdated {
            field: "bounty_lamports".to_string(),
            old_value: old,
            new_value: new_bounty,
            timestamp: ts,
        });
    }

    if let Some(new_rebalance_bounty) = rebalance_bounty_lamports {
        let old = config.rebalance_bounty_lamports;
        config.rebalance_bounty_lamports = new_rebalance_bounty;
        emit!(ConfigUpdated {
            field: "rebalance_bounty_lamports".to_string(),
            old_value: old,
            new_value: new_rebalance_bounty,
            timestamp: ts,
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Account struct
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    /// Admin signer. Must match config.admin.
    pub admin: Signer<'info>,

    /// RebalancerConfig singleton PDA. Mutable for field updates.
    #[account(
        mut,
        seeds = [REBALANCER_CONFIG_SEED],
        bump = rebalancer_config.bump,
        constraint = admin.key() == rebalancer_config.admin @ RebalancerError::Unauthorized,
    )]
    pub rebalancer_config: Account<'info, RebalancerConfig>,
}
