//! Rebalancer Program error codes.

use anchor_lang::prelude::*;

#[error_code]
pub enum RebalancerError {
    /// Admin-only instruction called by non-admin signer.
    #[msg("Unauthorized: caller is not the admin")]
    Unauthorized,

    /// RebalancerConfig has already been initialized.
    #[msg("Rebalancer config is already initialized")]
    AlreadyInitialized,

    /// USDC accumulator balance below minimum conversion threshold (1 USDC).
    #[msg("Accumulator balance below minimum convert amount")]
    BelowMinConvertAmount,

    /// Jupiter conversion cost exceeds the configured ceiling.
    #[msg("Conversion cost exceeds ceiling")]
    CostCeilingExceeded,

    /// Pool allocation delta is below the configured minimum threshold.
    #[msg("Allocation delta below rebalance threshold")]
    DeltaBelowThreshold,

    /// Arithmetic overflow in checked math.
    #[msg("Math overflow")]
    MathOverflow,

    /// target_bps must be <= 10000.
    #[msg("Invalid target_bps: must be <= 10000")]
    InvalidTargetBps,

    /// min_delta must be <= 10000.
    #[msg("Invalid min_delta: must be <= 10000")]
    InvalidMinDelta,

    /// cost_ceiling_bps must be <= 10000.
    #[msg("Invalid cost_ceiling_bps: must be <= 10000")]
    InvalidCostCeiling,

    /// jupiter_program_id must not be Pubkey::default().
    #[msg("Invalid Jupiter program ID: must not be default pubkey")]
    InvalidJupiterProgram,

    /// Jupiter CPI was skipped because this is a devnet build.
    #[msg("Jupiter CPI skipped (devnet mode)")]
    JupiterCpiSkippedDevnet,

    /// Pool AccountInfo owner does not match AMM program ID.
    #[msg("Invalid pool owner: account not owned by AMM program")]
    InvalidPoolOwner,

    /// Pool AccountInfo data too short to read PoolState fields.
    #[msg("Invalid pool data: buffer too short for PoolState")]
    InvalidPoolData,

    /// SOL price argument is invalid (must be > 0).
    #[msg("Invalid SOL price: must be greater than 0")]
    InvalidSolPrice,

    /// withdraw_bps exceeds MAX_WITHDRAW_BPS safety bound.
    #[msg("Withdraw BPS exceeds maximum (5000 = 50%)")]
    WithdrawExceedsMax,

    /// AMM CPI call failed.
    #[msg("AMM CPI failed")]
    AmmCpiFailed,

    /// Not enough remaining_accounts passed for Jupiter CPI.
    #[msg("Insufficient remaining accounts for Jupiter CPI")]
    InsufficientRemainingAccounts,

    /// Jupiter program ID in remaining_accounts doesn't match config.
    #[msg("Jupiter program ID mismatch: remaining_accounts[0] != config.jupiter_program_id")]
    JupiterProgramMismatch,
}
