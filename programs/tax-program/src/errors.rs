//! Tax Program error codes.
//!
//! Source: Tax_Pool_Logic_Spec.md Section 19

use anchor_lang::prelude::*;

#[error_code]
pub enum TaxError {
    /// Invalid pool type for this operation (e.g., PROFIT pool in SOL swap instruction)
    #[msg("Invalid pool type for this operation")]
    InvalidPoolType,

    /// Tax calculation resulted in arithmetic overflow
    #[msg("Tax calculation overflow")]
    TaxOverflow,

    /// Output amount is less than user's minimum_output parameter
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,

    /// EpochState account is invalid or cannot provide tax rates
    #[msg("Invalid epoch state - cannot determine tax rates")]
    InvalidEpochState,

    /// Input amount is too small for a meaningful swap
    #[msg("Insufficient input amount for swap")]
    InsufficientInput,

    /// Net output after tax is below minimum
    #[msg("Output amount below minimum")]
    OutputBelowMinimum,

    /// The swap_authority PDA derivation is incorrect
    #[msg("Invalid swap authority PDA")]
    InvalidSwapAuthority,

    /// Expected SPL Token program for WSOL operations
    #[msg("Token program mismatch - expected SPL Token for WSOL")]
    WsolProgramMismatch,

    /// Expected Token-2022 program for CRIME/FRAUD/PROFIT operations
    #[msg("Token program mismatch - expected Token-2022 for CRIME/FRAUD/PROFIT")]
    Token2022ProgramMismatch,

    /// Token account owner is not the expected user
    #[msg("Invalid token account owner")]
    InvalidTokenOwner,

    /// Carnage-exempt instruction called by non-Carnage authority
    #[msg("Carnage-only instruction called by non-Carnage authority")]
    UnauthorizedCarnageCall,

    /// Staking escrow PDA does not match expected derivation
    #[msg("Staking escrow PDA mismatch")]
    InvalidStakingEscrow,

    /// Carnage vault PDA does not match expected derivation
    #[msg("Carnage vault PDA mismatch")]
    InvalidCarnageVault,

    /// Treasury address does not match expected pubkey
    #[msg("Treasury address mismatch")]
    InvalidTreasury,

    /// AMM program address does not match expected program ID
    #[msg("AMM program address mismatch")]
    InvalidAmmProgram,

    /// Staking program address does not match expected program ID
    #[msg("Staking program address mismatch")]
    InvalidStakingProgram,

    /// Tax amount equals or exceeds gross swap output.
    /// Reject the sell -- net output would be zero or negative.
    #[msg("Tax exceeds gross output -- sell amount too small")]
    InsufficientOutput,

    /// User's minimum_amount_out is below the protocol-enforced floor.
    /// The floor is 50% of constant-product expected output.
    /// Set minimum_amount_out to at least the floor value.
    ///
    /// Source: Phase 49 (SEC-10) -- prevents zero-slippage sandwich attacks
    #[msg("Minimum output below protocol floor (50% of expected)")]
    MinimumOutputFloorViolation,

    /// Pool account is not owned by AMM program.
    /// Prevents spoofed pool accounts from feeding arbitrary reserve data
    /// to swap calculations and slippage floor enforcement.
    ///
    /// Source: Phase 80 (DEF-01) -- pool account ownership verification
    #[msg("Pool account is not owned by AMM program")]
    InvalidPoolOwner,

    /// Caller-supplied tax-side flag (is_crime) does not match the identity
    /// derived from the validated mint_b account. Runtime bindings now treat
    /// the caller-supplied arg as a witness cross-checked against the on-chain
    /// mint; any disagreement reverts the swap.
    #[msg("Tax identity (is_crime) does not match the validated mint")]
    TaxIdentityMismatch,

    /// The pool account's stored mint_b does not match the mint_b account
    /// passed to the instruction. Closes the residual gap where a caller
    /// could pair a pool with an unrelated mint account.
    #[msg("Pool mint_b does not match the passed mint_b account")]
    PoolMintMismatch,

    /// The mint_b account passed to the instruction is not one of the two
    /// recognized taxed token mints (CRIME / FRAUD) pinned in the Tax Program
    /// constants for the active cluster.
    #[msg("mint_b is not a recognized taxed token mint")]
    UnknownTaxedMint,

    /// The USDC mint account does not match the expected USDC mint for this cluster
    #[msg("The USDC mint account does not match the expected USDC mint for this cluster")]
    InvalidUsdcMint,

    /// USDC accumulator PDA does not match expected derivation
    #[msg("USDC accumulator PDA does not match expected derivation")]
    InvalidAccumulator,

    /// Rebalancer program address mismatch
    #[msg("Rebalancer program address mismatch")]
    InvalidRebalancerProgram,
}
