use anchor_lang::prelude::*;

#[error_code]
pub enum AmmError {
    // --- Phase 8: Swap math errors ---

    /// Arithmetic overflow in swap calculation.
    /// The math module returns None on overflow; this error is the on-chain mapping.
    #[msg("Arithmetic overflow in swap calculation")]
    Overflow,

    /// The k-invariant (reserve_a * reserve_b) decreased after swap.
    /// This indicates a bug in swap logic -- k must never decrease.
    #[msg("K-invariant violation: k decreased after swap")]
    KInvariantViolation,

    // --- Phase 9: Pool initialization errors ---

    /// Pool has already been initialized with liquidity.
    /// Each pool PDA can only be initialized once.
    #[msg("Pool is already initialized")]
    PoolAlreadyInitialized,

    /// Mints must be provided in canonical order (mint_a < mint_b by pubkey bytes).
    /// This ensures a unique PDA per unordered mint pair.
    #[msg("Mints must be in canonical order (mint_a < mint_b)")]
    MintsNotCanonicallyOrdered,

    /// The signer is not the admin stored in AdminConfig.
    #[msg("Unauthorized: signer is not the admin")]
    Unauthorized,

    /// A token program account does not match the owner of its corresponding mint.
    /// Each mint's on-chain owner must equal the token program passed for that side.
    #[msg("Token program does not match mint owner")]
    InvalidTokenProgram,

    /// Initial seed amounts must be greater than zero on both sides.
    /// Zero-liquidity pools would cause division-by-zero in swap math.
    #[msg("Initial seed amount must be greater than zero")]
    ZeroSeedAmount,

    /// Mint A and Mint B must be different tokens.
    /// A pool pairing a token with itself has no economic purpose.
    #[msg("Mint A and Mint B must be different")]
    DuplicateMints,

    // --- Phase 10: Transfer routing errors ---

    /// Transfer amount must be greater than zero.
    /// Zero-amount transfers waste compute and indicate a logic bug.
    #[msg("Transfer amount must be greater than zero")]
    ZeroAmount,

    // --- Phase 11: Swap errors ---

    /// Computed output amount is less than the user's minimum_amount_out.
    /// Triggers when price has moved unfavorably beyond the user's tolerance.
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,

    /// Pool has not been initialized with liquidity yet.
    /// Swaps cannot execute on an uninitialized pool (reserves are zero).
    #[msg("Pool is not initialized")]
    PoolNotInitialized,

    /// Pool reentrancy guard is active.
    /// A swap is already in progress; concurrent access is rejected.
    #[msg("Pool is locked")]
    PoolLocked,

    /// Vault account does not match the vault stored in pool state.
    /// Prevents substituting a fake vault to steal funds.
    #[msg("Vault does not match pool state")]
    VaultMismatch,

    /// Mint account does not match the mint stored in pool state.
    /// Prevents swapping the wrong token through a pool.
    #[msg("Mint does not match pool state")]
    InvalidMint,

    // --- Phase 12: Zero-output swap errors ---

    /// Swap fee deduction produced zero effective input.
    /// Input amount is too small for the fee rate -- all tokens would be taken as fee.
    #[msg("Input amount too small: fee deduction produces zero effective input")]
    ZeroEffectiveInput,

    /// Swap math produced zero output tokens.
    /// Effective input is too small relative to reserves to produce any output.
    #[msg("Swap produces zero output tokens")]
    ZeroSwapOutput,

    // --- Phase 13: Access control errors ---

    /// Swap instructions require swap_authority PDA signed by Tax Program.
    /// Direct calls without valid swap_authority are not allowed.
    #[msg("Swaps must go through Tax Program - direct calls not allowed")]
    InvalidSwapAuthority,

    // --- Phase 37: LP fee cap ---

    /// LP fee exceeds maximum allowed (500 bps = 5%).
    /// Prevents admin misconfiguration that could drain liquidity.
    #[msg("LP fee exceeds maximum allowed (500 bps)")]
    LpFeeExceedsMax,

    // --- Phase 97: Transfer authority errors ---

    /// New authority cannot be Pubkey::default() (use burn_admin for that).
    /// This prevents accidental authority burn via the transfer instruction.
    #[msg("Invalid authority: cannot transfer to Pubkey::default()")]
    InvalidAuthority,

    // --- Phase 126: Rebalancer integration errors ---

    /// Withdrawal BPS exceeds maximum (5000 = 50%).
    #[msg("Withdraw BPS exceeds maximum (5000)")]
    WithdrawExceedsMax,

    /// Withdrawal BPS is zero (no-op withdrawal not allowed).
    #[msg("Withdraw BPS must be greater than zero")]
    ZeroWithdrawBps,

    /// Both calculated withdrawal amounts are zero (reserves too small for BPS).
    #[msg("Withdrawal produces zero amounts on both sides")]
    ZeroWithdrawAmounts,

    /// Injection amount_a and amount_b are both zero.
    #[msg("Injection amounts must not both be zero")]
    ZeroInjectionAmounts,
}
