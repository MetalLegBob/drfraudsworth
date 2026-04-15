//! Tax Program instructions.

pub mod initialize_wsol_intermediary;
pub mod swap_exempt;
pub mod swap_sol_buy;
pub mod swap_sol_sell;
pub mod swap_usdc_buy;
pub mod swap_usdc_sell;

pub use initialize_wsol_intermediary::*;
pub use swap_exempt::*;
pub use swap_sol_buy::*;
pub use swap_sol_sell::*;
pub use swap_usdc_buy::*;
pub use swap_usdc_sell::*;
