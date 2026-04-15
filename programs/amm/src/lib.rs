use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod helpers;
pub mod instructions;
pub mod state;

use instructions::*;

#[cfg(not(feature = "no-entrypoint"))]
use solana_security_txt::security_txt;

#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "Dr Fraudsworth's Finance Factory",
    project_url: "https://fraudsworth.fun",
    contacts: "email:drfraudsworth@gmail.com,twitter:@fraudsworth",
    policy: "https://fraudsworth.fun/docs/security/security-policy",
    preferred_languages: "en",
    auditors: "Internal audits: SOS, BOK, VulnHunter (v1.3)",
    expiry: "2027-03-20"
}

declare_id!("5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR");

#[program]
pub mod amm {
    use super::*;

    /// Initialize the global AdminConfig PDA.
    ///
    /// Can only be called by the program's upgrade authority (deployer).
    /// The `admin` parameter sets who can create pools -- this can be a
    /// different key from the upgrade authority (e.g., a multisig).
    pub fn initialize_admin(ctx: Context<InitializeAdmin>, admin: Pubkey) -> Result<()> {
        instructions::initialize_admin::handler(ctx, admin)
    }

    /// Transfer the admin key to a new pubkey (e.g., Squads multisig vault).
    /// Only the current admin can call this. new_admin must not be Pubkey::default().
    pub fn transfer_admin(ctx: Context<TransferAdmin>, new_admin: Pubkey) -> Result<()> {
        instructions::transfer_admin::handler(ctx, new_admin)
    }

    /// Burns the admin key, permanently preventing new pool creation.
    /// Only the current admin can call this. Irreversible.
    ///
    /// # Accounts
    /// * `admin` - Current admin signer
    /// * `admin_config` - AdminConfig PDA (admin set to Pubkey::default())
    pub fn burn_admin(ctx: Context<BurnAdmin>) -> Result<()> {
        instructions::burn_admin::handler(ctx)
    }

    /// Initialize a new AMM pool with PDA-owned vaults and seed liquidity.
    ///
    /// Creates pool state PDA, vault token accounts (owned by pool PDA),
    /// and transfers initial liquidity atomically. Pool type is inferred
    /// from token programs, not caller-declared.
    ///
    /// # Arguments
    /// * `lp_fee_bps` - LP fee in basis points
    /// * `amount_a` - Initial seed amount for token A
    /// * `amount_b` - Initial seed amount for token B
    pub fn initialize_pool<'info>(
        ctx: Context<'_, '_, 'info, 'info, InitializePool<'info>>,
        lp_fee_bps: u16,
        amount_a: u64,
        amount_b: u64,
    ) -> Result<()> {
        instructions::initialize_pool::handler(ctx, lp_fee_bps, amount_a, amount_b)
    }

    /// Execute a swap in a SOL pool (CRIME/SOL or FRAUD/SOL).
    ///
    /// Routes between Token-2022 (CRIME/FRAUD) and SPL Token (WSOL) based on
    /// swap direction. LP fee is deducted before output calculation.
    ///
    /// # Arguments
    /// * `amount_in` - Input token amount (pre-fee)
    /// * `direction` - SwapDirection::AtoB or SwapDirection::BtoA
    /// * `minimum_amount_out` - Slippage protection floor
    pub fn swap_sol_pool<'info>(
        ctx: Context<'_, '_, 'info, 'info, SwapSolPool<'info>>,
        amount_in: u64,
        direction: SwapDirection,
        minimum_amount_out: u64,
    ) -> Result<()> {
        instructions::swap_sol_pool::handler(ctx, amount_in, direction, minimum_amount_out)
    }

    /// Withdraw proportional liquidity from a pool.
    ///
    /// Only callable by the Rebalancer program via RebalanceAuthority PDA.
    /// Calculates both token amounts as a BPS fraction of current reserves.
    /// Maximum withdrawal is 50% per call (MAX_WITHDRAW_BPS = 5000).
    ///
    /// # Arguments
    /// * `withdraw_bps` - Fraction of pool to withdraw in basis points (1-5000)
    pub fn withdraw_liquidity<'info>(
        ctx: Context<'_, '_, 'info, 'info, WithdrawLiquidity<'info>>,
        withdraw_bps: u16,
    ) -> Result<()> {
        instructions::withdraw_liquidity::handler(ctx, withdraw_bps)
    }

    /// Inject liquidity into a pool.
    ///
    /// Only callable by the Rebalancer program via RebalanceAuthority PDA.
    /// Accepts arbitrary token amounts -- no proportionality enforcement.
    /// Pool price adjusts naturally; arbitrageurs correct minor imbalances.
    ///
    /// # Arguments
    /// * `amount_a` - Amount of token A to inject
    /// * `amount_b` - Amount of token B to inject
    pub fn add_liquidity<'info>(
        ctx: Context<'_, '_, 'info, 'info, AddLiquidity<'info>>,
        amount_a: u64,
        amount_b: u64,
    ) -> Result<()> {
        instructions::add_liquidity::handler(ctx, amount_a, amount_b)
    }

}
