use anchor_lang::prelude::*;

/// Emitted when a new pool is initialized with its first liquidity.
///
/// `pool_type` is serialized as u8 for client compatibility:
/// - 0 = MixedPool (one SPL Token + one Token-2022)
/// - 1 = PureT22Pool (both Token-2022)
#[event]
pub struct PoolInitializedEvent {
    /// The pool PDA address.
    pub pool: Pubkey,
    /// Pool type as u8 (0 = MixedPool, 1 = PureT22Pool).
    pub pool_type: u8,
    /// Canonical mint A (the "smaller" pubkey).
    pub mint_a: Pubkey,
    /// Canonical mint B (the "larger" pubkey).
    pub mint_b: Pubkey,
    /// Vault A PDA address.
    pub vault_a: Pubkey,
    /// Vault B PDA address.
    pub vault_b: Pubkey,
    /// Initial reserve of token A after seeding.
    pub reserve_a: u64,
    /// Initial reserve of token B after seeding.
    pub reserve_b: u64,
    /// LP fee in basis points.
    pub lp_fee_bps: u16,
}

/// Emitted when a swap executes successfully in a SOL pool.
///
/// Contains all information needed for indexers and frontends to track
/// swap activity without additional RPC lookups. Direction is encoded
/// as u8 for client compatibility (0 = AtoB, 1 = BtoA).
///
/// `lp_fee_bps` is intentionally omitted -- it is immutable on pool state,
/// so clients can query it once and cache. See 11-CONTEXT.md.
#[event]
pub struct SwapEvent {
    /// The pool PDA address.
    pub pool: Pubkey,
    /// The user who initiated the swap.
    pub user: Pubkey,
    /// Mint of the input token.
    pub input_mint: Pubkey,
    /// Mint of the output token.
    pub output_mint: Pubkey,
    /// Amount of input token (pre-fee).
    pub amount_in: u64,
    /// Amount of output token sent to user.
    pub amount_out: u64,
    /// LP fee deducted (in input token units).
    pub lp_fee: u64,
    /// Post-swap reserve of token A.
    pub reserve_a: u64,
    /// Post-swap reserve of token B.
    pub reserve_b: u64,
    /// Swap direction (0 = AtoB, 1 = BtoA).
    pub direction: u8,
    /// Unix timestamp from Clock sysvar.
    pub timestamp: i64,
    /// Slot from Clock sysvar.
    pub slot: u64,
}

/// Emitted when the admin key is permanently burned.
/// After this event, no new pools can be created through the AMM.
/// This is irreversible.
#[event]
pub struct AdminBurned {
    /// The admin who burned their own key.
    pub burned_by: Pubkey,
    /// Slot when the burn occurred.
    pub slot: u64,
}

// ---------------------------------------------------------------------------
// Phase 126: Rebalancer liquidity events
// ---------------------------------------------------------------------------

/// Emitted when liquidity is withdrawn from a pool by the Rebalancer.
///
/// Contains all information needed for monitoring and indexing rebalance
/// activity. `withdraw_bps` is the requested fraction; `amount_a`/`amount_b`
/// are the actual computed amounts (may differ from BPS due to truncation).
#[event]
pub struct LiquidityWithdrawnEvent {
    /// The pool PDA address.
    pub pool: Pubkey,
    /// Requested withdrawal fraction in basis points.
    pub withdraw_bps: u16,
    /// Actual amount of token A withdrawn.
    pub amount_a: u64,
    /// Actual amount of token B withdrawn.
    pub amount_b: u64,
    /// Reserve A after withdrawal.
    pub reserve_a_after: u64,
    /// Reserve B after withdrawal.
    pub reserve_b_after: u64,
    /// Unix timestamp from Clock sysvar.
    pub timestamp: i64,
    /// Slot from Clock sysvar.
    pub slot: u64,
}

/// Emitted when liquidity is injected into a pool by the Rebalancer.
///
/// Contains post-injection reserve state for monitoring. No BPS field
/// because injection uses absolute amounts.
#[event]
pub struct LiquidityAddedEvent {
    /// The pool PDA address.
    pub pool: Pubkey,
    /// Amount of token A injected.
    pub amount_a: u64,
    /// Amount of token B injected.
    pub amount_b: u64,
    /// Reserve A after injection.
    pub reserve_a_after: u64,
    /// Reserve B after injection.
    pub reserve_b_after: u64,
    /// Unix timestamp from Clock sysvar.
    pub timestamp: i64,
    /// Slot from Clock sysvar.
    pub slot: u64,
}
