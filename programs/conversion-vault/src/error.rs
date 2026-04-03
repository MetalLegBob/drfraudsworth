use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Input amount must be greater than zero")]
    ZeroAmount,

    #[msg("Output amount rounds to zero — input too small for conversion")]
    OutputTooSmall,

    #[msg("Invalid mint pair — must be CRIME<->PROFIT or FRAUD<->PROFIT")]
    InvalidMintPair,

    #[msg("Input and output mints must be different")]
    SameMint,

    #[msg("Invalid token program — must be Token-2022")]
    InvalidTokenProgram,

    #[msg("Overflow in conversion calculation")]
    MathOverflow,

    #[msg("Output below minimum — slippage protection")]
    SlippageExceeded,

    #[msg("Input account not owned by signer")]
    InvalidOwner,

    #[msg("Pre-balance exceeds current balance — stale or incorrect snapshot")]
    DeltaUnderflow,
}
