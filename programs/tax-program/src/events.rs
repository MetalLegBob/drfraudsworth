//! Tax Program events.
//!
//! Source: Tax_Pool_Logic_Spec.md Section 20

use anchor_lang::prelude::*;

/// Pool type identifier for events.
///
/// Identifies which AMM pool was used in a swap operation.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PoolType {
    /// SOL-CRIME pool (taxed)
    SolCrime,
    /// SOL-FRAUD pool (taxed)
    SolFraud,
    /// USDC-CRIME pool (taxed)
    UsdcCrime,
    /// USDC-FRAUD pool (taxed)
    UsdcFraud,
}

/// Swap direction for events.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwapDirection {
    /// Buying tokens with SOL (SOL -> CRIME/FRAUD)
    Buy,
    /// Selling tokens for SOL (CRIME/FRAUD -> SOL)
    Sell,
}

/// Emitted after every taxed swap operation.
///
/// Contains full breakdown of the swap including tax calculation and distribution.
/// Used by off-chain analytics and frontends.
#[event]
pub struct TaxedSwap {
    /// User who initiated the swap
    pub user: Pubkey,
    /// Which pool was used
    pub pool_type: PoolType,
    /// Buy or Sell direction
    pub direction: SwapDirection,
    /// Amount user put in (SOL for buy, tokens for sell)
    pub input_amount: u64,
    /// Amount user received (tokens for buy, SOL for sell)
    pub output_amount: u64,
    /// Total tax collected (in SOL lamports)
    pub tax_amount: u64,
    /// Tax rate applied (in basis points)
    pub tax_rate_bps: u16,
    /// SOL sent to staking escrow (71%)
    pub staking_portion: u64,
    /// SOL sent to carnage fund (24%)
    pub carnage_portion: u64,
    /// SOL sent to treasury (5%, remainder)
    pub treasury_portion: u64,
    /// USDC tax sent to Rebalancer accumulator (0 for SOL swaps)
    pub accumulator_portion: u64,
    /// Epoch number when swap occurred
    pub epoch: u32,
    /// Slot when swap occurred
    pub slot: u64,
}

/// Emitted after every tax-exempt Carnage swap operation.
///
/// Carnage swaps bypass tax calculation entirely. This event enables
/// off-chain monitoring of Carnage rebalancing activity that was previously
/// invisible (only AMM's SwapEvent was emitted).
///
/// Source: Phase 37 audit finding -- swap_exempt had no event emission
#[event]
pub struct ExemptSwap {
    /// Carnage authority PDA that initiated the swap
    pub authority: Pubkey,
    /// AMM pool used for the swap
    pub pool: Pubkey,
    /// Amount swapped (SOL for buy, token for sell)
    pub amount_a: u64,
    /// Swap direction: 0 = buy (AtoB), 1 = sell (BtoA)
    pub direction: u8,
    /// Slot when swap occurred
    pub slot: u64,
}
