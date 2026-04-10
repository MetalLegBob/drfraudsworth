//! Raw byte reader for AMM PoolState fields.
//!
//! Reads fields from a PoolState AccountInfo without importing the AMM crate.
//! Uses known byte offsets verified against the AMM PoolState struct layout.
//! This avoids cross-crate coupling.
//!
//! Identical approach to epoch-program/src/instructions/execute_carnage_atomic.rs
//! (lines 930-956), adapted for Tax Program error types.
//!
//! PoolState byte layout (from AMM pool.rs):
//!   [0..8]     Anchor discriminator
//!   [8]        pool_type (1 byte)
//!   [9..41]    mint_a (Pubkey, 32 bytes)
//!   [41..73]   mint_b (Pubkey, 32 bytes)
//!   [73..105]  vault_a (Pubkey, 32 bytes)
//!   [105..137] vault_b (Pubkey, 32 bytes)
//!   [137..145] reserve_a (u64, 8 bytes)
//!   [145..153] reserve_b (u64, 8 bytes)
//!
//! Source: Phase 49 (SEC-10), 49-RESEARCH.md Pattern 2

use anchor_lang::prelude::*;
use crate::constants::amm_program_id;
use crate::errors::TaxError;

/// Return NATIVE_MINT address (So11111111111111111111111111111111111111112).
/// Hardcoded to avoid pulling in spl_token as a dependency just for this constant.
/// This is the canonical WSOL mint used by all SOL pools.
fn native_mint() -> Pubkey {
    use std::str::FromStr;
    Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap()
}

/// Read pool reserves from a PoolState AccountInfo, returning (sol_reserve, token_reserve).
///
/// # Security checks (DEF-01, DEF-02)
/// 1. **Owner verification (DEF-01):** Rejects accounts not owned by AMM program.
///    Prevents spoofed pool accounts from feeding arbitrary reserve data to
///    Tax Program swap calculations and slippage floor enforcement.
/// 2. **is_reversed detection (DEF-02):** Reads mint_a from bytes [9..41] and
///    compares to NATIVE_MINT to determine canonical ordering. Returns reserves
///    in (SOL, token) order regardless of how the AMM stores them.
///
/// # Arguments
/// * `pool_info` - The AMM pool AccountInfo (must be at least 153 bytes)
///
/// # Returns
/// * `Ok((sol_reserve, token_reserve))` - Pool reserves normalized to (SOL, token)
/// * `Err(TaxError::InvalidPoolOwner)` - If account is not owned by AMM program
/// * `Err(TaxError::InvalidPoolType)` - If data is too short
/// * `Err(TaxError::TaxOverflow)` - If byte slice conversion fails
///
/// # Why raw bytes instead of PoolState deserialization
/// The Tax Program has no Cargo dependency on the AMM crate and does not
/// import PoolState. Raw byte reads at known offsets avoid cross-crate
/// coupling. This pattern is proven in Carnage code.
pub fn read_pool_reserves(pool_info: &AccountInfo) -> Result<(u64, u64)> {
    let (sol_reserve, token_reserve, _token_mint) =
        read_pool_reserves_with_token_mint(pool_info)?;
    Ok((sol_reserve, token_reserve))
}

/// Extended variant of [`read_pool_reserves`] that also returns the pool's
/// non-NATIVE token mint.
///
/// # Returns
/// * `Ok((sol_reserve, token_reserve, token_mint))` — reserves normalized to
///   (SOL, token) order plus whichever of `pool.mint_a` / `pool.mint_b` is NOT
///   `NATIVE_MINT` (i.e. the "token side" of a SOL pool).
/// * Same error variants as [`read_pool_reserves`], plus `TaxError::TaxOverflow`
///   if the `mint_b` slice fails to parse as a `Pubkey`.
///
/// # Why this exists
/// The tax-side identity (CRIME vs FRAUD) must be derived from the pool the
/// swap is actually hitting, not from caller-supplied flags or caller-supplied
/// mint accounts. Surfacing the pool's token mint from the pool-bytes reader
/// lets swap handlers bind the tax schedule to the validated on-chain state
/// as a single defense-in-depth primitive (see 122.1-CONTEXT.md §3.2).
pub fn read_pool_reserves_with_token_mint(
    pool_info: &AccountInfo,
) -> Result<(u64, u64, Pubkey)> {
    // DEF-01: Verify the pool account is owned by the AMM program.
    // Without this check, an attacker could pass a fake account with
    // arbitrary reserve values, manipulating slippage floor calculations.
    require!(
        *pool_info.owner == amm_program_id(),
        TaxError::InvalidPoolOwner
    );

    let data = pool_info.data.borrow();

    // PoolState minimum size: 8 (discriminator) + 1 (pool_type) + 32*4 (mints+vaults)
    // + 8*2 (reserves) = 153 bytes
    require!(data.len() >= 153, TaxError::InvalidPoolType);

    // DEF-02: Read mint_a and mint_b to determine canonical ordering AND to
    // surface the token-side mint to the caller. AMM stores pools with mints
    // in canonical (sorted) order. For SOL pools, NATIVE_MINT (0x06...) is
    // always mint_a because it sorts before all token mints. But for safety
    // (and future-proofing), we detect explicitly.
    let mint_a = Pubkey::try_from(&data[9..41])
        .map_err(|_| error!(TaxError::TaxOverflow))?;
    let mint_b = Pubkey::try_from(&data[41..73])
        .map_err(|_| error!(TaxError::TaxOverflow))?;

    let reserve_a = u64::from_le_bytes(
        data[137..145]
            .try_into()
            .map_err(|_| error!(TaxError::TaxOverflow))?,
    );
    let reserve_b = u64::from_le_bytes(
        data[145..153]
            .try_into()
            .map_err(|_| error!(TaxError::TaxOverflow))?,
    );

    // If mint_a == NATIVE_MINT: reserve_a is SOL, reserve_b is token (normal order).
    // If mint_a != NATIVE_MINT: pool is reversed, reserve_b is SOL, reserve_a is token.
    if mint_a == native_mint() {
        Ok((reserve_a, reserve_b, mint_b))
    } else {
        Ok((reserve_b, reserve_a, mint_a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Build a synthetic 153-byte PoolState buffer.
    /// Layout: [disc(8)][pool_type(1)][mint_a(32)][mint_b(32)][vault_a(32)][vault_b(32)][reserve_a(8)][reserve_b(8)]
    fn build_pool_data(mint_a: Pubkey, mint_b: Pubkey, reserve_a: u64, reserve_b: u64) -> Vec<u8> {
        let mut data = Vec::with_capacity(153);
        data.extend_from_slice(&[0u8; 8]); // discriminator (not validated by reader)
        data.push(0u8); // pool_type
        data.extend_from_slice(&mint_a.to_bytes());
        data.extend_from_slice(&mint_b.to_bytes());
        data.extend_from_slice(&[0u8; 32]); // vault_a (not read)
        data.extend_from_slice(&[0u8; 32]); // vault_b (not read)
        data.extend_from_slice(&reserve_a.to_le_bytes());
        data.extend_from_slice(&reserve_b.to_le_bytes());
        debug_assert_eq!(data.len(), 153);
        data
    }

    /// Construct a usable AccountInfo for the test reader. The reader checks
    /// `*pool_info.owner == amm_program_id()` and reads `pool_info.data`, so we
    /// need both fields to be valid for the lifetime of the call.
    fn with_account_info<F, R>(
        owner: Pubkey,
        mut data: Vec<u8>,
        f: F,
    ) -> R
    where
        F: for<'a> FnOnce(&AccountInfo<'a>) -> R,
    {
        let key = Pubkey::new_unique();
        let mut lamports: u64 = 1_000_000;
        // SAFETY: AccountInfo holds Rc<RefCell<&mut [u8]>> over the data slice.
        // We construct it manually here for unit tests where we don't have a
        // running runtime.
        let lamports_ref = Rc::new(RefCell::new(&mut lamports as *mut u64));
        let _ = lamports_ref; // not used directly, see below
        let owner_ref = owner;

        // The simplest portable construction is via AccountInfo::new with raw refs.
        let mut lamports_v: u64 = 1_000_000;
        let info = AccountInfo {
            key: &key,
            is_signer: false,
            is_writable: false,
            lamports: Rc::new(RefCell::new(&mut lamports_v)),
            data: Rc::new(RefCell::new(&mut data[..])),
            owner: &owner_ref,
            executable: false,
            rent_epoch: 0,
        };
        f(&info)
    }

    fn amm_id() -> Pubkey {
        amm_program_id()
    }

    fn nm() -> Pubkey {
        native_mint()
    }

    #[test]
    fn canonical_pool_returns_mint_b_as_token() {
        // mint_a = NATIVE, mint_b = arbitrary token
        let token = Pubkey::new_unique();
        let data = build_pool_data(nm(), token, 100u64, 200u64);
        let result = with_account_info(amm_id(), data, |info| {
            read_pool_reserves_with_token_mint(info)
        })
        .expect("read failed");
        assert_eq!(result.0, 100, "sol_reserve should be reserve_a");
        assert_eq!(result.1, 200, "token_reserve should be reserve_b");
        assert_eq!(result.2, token, "token mint should be mint_b");
    }

    #[test]
    fn reversed_pool_returns_mint_a_as_token() {
        // mint_a = arbitrary token, mint_b = NATIVE (reversed pool)
        let token = Pubkey::new_unique();
        let data = build_pool_data(token, nm(), 999u64, 555u64);
        let result = with_account_info(amm_id(), data, |info| {
            read_pool_reserves_with_token_mint(info)
        })
        .expect("read failed");
        assert_eq!(result.0, 555, "sol_reserve should be reserve_b");
        assert_eq!(result.1, 999, "token_reserve should be reserve_a");
        assert_eq!(result.2, token, "token mint should be mint_a");
    }

    #[test]
    fn owner_mismatch_returns_invalid_pool_owner() {
        let token = Pubkey::new_unique();
        let data = build_pool_data(nm(), token, 100u64, 200u64);
        let bogus_owner = Pubkey::new_unique();
        let result = with_account_info(bogus_owner, data, |info| {
            read_pool_reserves_with_token_mint(info)
        });
        let err = result.expect_err("expected error");
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("InvalidPoolOwner"),
            "expected InvalidPoolOwner, got {}",
            msg
        );
    }

    #[test]
    fn truncated_buffer_returns_invalid_pool_type() {
        // Only 100 bytes, well below the 153 minimum
        let data = vec![0u8; 100];
        let result = with_account_info(amm_id(), data, |info| {
            read_pool_reserves_with_token_mint(info)
        });
        let err = result.expect_err("expected error");
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("InvalidPoolType"),
            "expected InvalidPoolType, got {}",
            msg
        );
    }

    #[test]
    fn read_pool_reserves_wrapper_still_works() {
        let token = Pubkey::new_unique();
        let data = build_pool_data(nm(), token, 42u64, 84u64);
        let (sol, tok) = with_account_info(amm_id(), data, |info| {
            read_pool_reserves(info)
        })
        .expect("read failed");
        assert_eq!(sol, 42);
        assert_eq!(tok, 84);
    }
}
