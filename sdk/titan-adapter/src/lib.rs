//! Titan Pathfinder TradingVenue adapter for the Dr. Fraudsworth DEX protocol.
//!
//! This crate implements Titan's `TradingVenue` trait to enable Titan's Argos
//! routing engine to quote and route swaps through Dr. Fraudsworth's on-chain
//! programs (AMM, Tax, Conversion Vault).
//!
//! Architecture (reused from jupiter-adapter):
//! - `math` - Pure swap/tax/vault math functions (exact copies of on-chain logic)
//! - `state` - Raw byte parsers for on-chain account state (no anchor-lang dep)
//! - `accounts` - Hardcoded mainnet addresses and PDA derivation
//! - `constants` - Protocol constants (fees, rates, decimals)
//!
//! New for Titan:
//! - `sol_pool_venue` - TradingVenue impl for CRIME/SOL and FRAUD/SOL pools
//! - `vault_venue` - TradingVenue impl for vault conversions (4 unidirectional)
//! - `instruction_data` - Anchor IX discriminator + arg serialization
//! - `token_info_builder` - Hardcoded TokenInfo for protocol mints

pub mod constants;
pub mod math;
pub mod state;
pub mod accounts;
pub mod instruction_data;
pub mod token_info_builder;
pub mod sol_pool_venue;
pub mod vault_venue;

// Re-export primary types and factory functions.
pub use sol_pool_venue::SolPoolVenue;
pub use vault_venue::{VaultVenue, known_vault_venues, known_sol_pool_venues, all_venues};
