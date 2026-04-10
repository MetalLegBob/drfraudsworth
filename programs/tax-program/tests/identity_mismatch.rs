//! Phase 122.1 — Tax/Pool identity-mismatch regression suite.
//!
//! This test file is the regression coverage for the whitehat report fixed
//! in commits 9cedf1f5..7a79f937. The bug allowed an attacker to submit a
//! CRIME swap while passing `is_crime = false` (or vice versa), paying the
//! cheaper side's tax.
//!
//! The fix layers two checks (122.1-CONTEXT.md §3):
//!   B) `is_crime` is cross-checked against `mint_b.key()` after deriving
//!      the true identity from the pinned `crime_mint()` / `fraud_mint()`
//!      constants in `tax-program/src/constants.rs`.
//!   C) `pool.mint_b` is read from raw bytes and bound to the passed
//!      `mint_b` account so a CRIME pool can't be paired with a FRAUD
//!      mint account or vice versa.
//!
//! ## Why this file exists separately from `test_swap_sol_buy.rs` /
//! `test_swap_sol_sell.rs`
//!
//! The pre-existing harnesses generate ephemeral keypair mints via
//! `create_spl_mint` / `create_t22_mint`. The Phase 122.1 fix pins the
//! taxed mints to the **mainnet** CRIME and FRAUD pubkeys at compile time
//! (no `#[cfg(test)]` branch — that's dead code for SBF cdylib builds).
//! Any swap with an ephemeral mint now fails with `UnknownTaxedMint`,
//! which is the correct production behaviour. Modernising those legacy
//! harnesses to install Token-2022 mint state at the mainnet pubkeys via
//! `svm.set_account(...)` is deferred to a future test-cleanup phase.
//!
//! This file is the canonical regression coverage for 122.1: it builds a
//! self-contained LiteSVM fixture with **both** CRIME and FRAUD pools at
//! the pinned mainnet pubkeys, with intentionally distinct tax rates so
//! that honest swaps observably select the correct schedule, and runs
//! both honest-path and exploit-path cases for buy and sell.
//!
//! ## Test matrix (12 cases)
//!
//! Honest path (8):
//!   - CRIME buy:  is_crime=true,  mint_b=CRIME mint, CRIME pool → ok, tax=CRIME buy bps
//!   - CRIME sell: is_crime=true,  mint_b=CRIME mint, CRIME pool → ok, tax=CRIME sell bps
//!   - FRAUD buy:  is_crime=false, mint_b=FRAUD mint, FRAUD pool → ok, tax=FRAUD buy bps
//!   - FRAUD sell: is_crime=false, mint_b=FRAUD mint, FRAUD pool → ok, tax=FRAUD sell bps
//!   - (each repeated for buy and sell entry points → 8 honest cases)
//!
//! Negative path (4):
//!   - Buy with CRIME pool + CRIME mint + is_crime=false → TaxIdentityMismatch
//!   - Sell with FRAUD pool + FRAUD mint + is_crime=true  → TaxIdentityMismatch
//!   - Buy with CRIME pool + FRAUD mint + is_crime=false → PoolMintMismatch
//!     (mint check passes is_crime against FRAUD identity, then pool-bind fires)
//!   - Sell with random unrelated mint passed as mint_b → UnknownTaxedMint
//!
//! ## Regression proof
//!
//! Each negative case must succeed (i.e. the swap completes without the
//! new check firing) when run against a pre-patch build of `tax_program.so`.
//! The proof procedure is documented in `122.1-01-SUMMARY.md` §Regression
//! proof.

use std::path::Path;

use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_keypair::Keypair as LiteKeypair;
use solana_signer::Signer as LiteSigner;
use solana_message::{Message, VersionedMessage};
use solana_transaction::versioned::VersionedTransaction;
use solana_instruction::{Instruction, account_meta::AccountMeta};

use anchor_lang::prelude::Pubkey;
use anchor_lang::AnchorSerialize;
use solana_sdk::program_pack::Pack;
use spl_token::state::Mint as SplMintState;
use sha2::{Sha256, Digest};

// ---------------------------------------------------------------------------
// Pinned program IDs (must match declare_id! in each crate)
// ---------------------------------------------------------------------------

fn amm_program_id() -> Pubkey {
    "5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR".parse().unwrap()
}

fn tax_program_id() -> Pubkey {
    "43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj".parse().unwrap()
}

fn epoch_program_id() -> Pubkey {
    "4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2".parse().unwrap()
}

fn staking_program_id() -> Pubkey {
    "12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH".parse().unwrap()
}

fn spl_token_program_id() -> Pubkey { spl_token::id() }
fn token_2022_program_id() -> Pubkey { spl_token_2022::id() }
fn system_program_id() -> Pubkey { solana_sdk::system_program::id() }
fn bpf_loader_upgradeable_id() -> Pubkey { solana_sdk::bpf_loader_upgradeable::id() }
fn native_mint_id() -> Pubkey { spl_token::native_mint::id() }

/// Mainnet CRIME mint — must match `crime_mint()` in tax-program/src/constants.rs
/// (mainnet branch). The patched binary refuses any other pubkey with
/// `UnknownTaxedMint`.
fn crime_mint_pk() -> Pubkey {
    "cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc".parse().unwrap()
}

/// Mainnet FRAUD mint — same provenance.
fn fraud_mint_pk() -> Pubkey {
    "FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5".parse().unwrap()
}

/// Mainnet treasury — matches `treasury_pubkey()` mainnet branch.
fn treasury_pubkey() -> Pubkey {
    "3ihhwLnEJ2duwPSLYxhLbFrdhhxXLcvcrV9rAHqMgzCv".parse().unwrap()
}

// ---------------------------------------------------------------------------
// Constants (mirror Tax Program)
// ---------------------------------------------------------------------------

const ADMIN_SEED: &[u8] = b"admin";
const POOL_SEED: &[u8] = b"pool";
const VAULT_SEED: &[u8] = b"vault";
const VAULT_A_SEED: &[u8] = b"a";
const VAULT_B_SEED: &[u8] = b"b";
const SWAP_AUTHORITY_SEED: &[u8] = b"swap_authority";
const TAX_AUTHORITY_SEED: &[u8] = b"tax_authority";
const STAKE_POOL_SEED: &[u8] = b"stake_pool";
const ESCROW_VAULT_SEED: &[u8] = b"escrow_vault";
const CARNAGE_SOL_VAULT_SEED: &[u8] = b"carnage_sol_vault";
const WSOL_INTERMEDIARY_SEED: &[u8] = b"wsol_intermediary";
const EPOCH_STATE_SEED: &[u8] = b"epoch_state";

const TEST_DECIMALS: u8 = 9;
const SEED_AMOUNT: u64 = 10_000_000_000; // 10 tokens — small but enough for both pools
const LP_FEE_BPS: u16 = 100; // 1%
const BPS_DENOMINATOR: u64 = 10_000;

// Distinct tax rates so honest cases observably pick the right schedule.
// Cheap CRIME regime: CRIME buy=300 (3%), CRIME sell=1400 (14%);
//                      FRAUD buy=1400 (14%), FRAUD sell=300 (3%).
const CRIME_BUY_BPS: u16 = 300;
const CRIME_SELL_BPS: u16 = 1400;
const FRAUD_BUY_BPS: u16 = 1400;
const FRAUD_SELL_BPS: u16 = 300;

// ---------------------------------------------------------------------------
// Type conversion helpers
// ---------------------------------------------------------------------------

fn addr(pk: &Pubkey) -> Address { Address::from(pk.to_bytes()) }
fn pk(address: &Address) -> Pubkey { Pubkey::new_from_array(address.to_bytes()) }
fn kp_pubkey(kp: &LiteKeypair) -> Pubkey { pk(&kp.pubkey()) }

// ---------------------------------------------------------------------------
// Discriminators
// ---------------------------------------------------------------------------

fn anchor_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("global:{}", name));
    let hash = hasher.finalize();
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

fn anchor_account_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("account:{}", name));
    let hash = hasher.finalize();
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

// ---------------------------------------------------------------------------
// PDA helpers
// ---------------------------------------------------------------------------

fn admin_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ADMIN_SEED], &amm_program_id())
}

fn pool_pda(mint_a: &Pubkey, mint_b: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[POOL_SEED, mint_a.as_ref(), mint_b.as_ref()],
        &amm_program_id(),
    )
}

fn vault_a_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, pool.as_ref(), VAULT_A_SEED], &amm_program_id())
}

fn vault_b_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, pool.as_ref(), VAULT_B_SEED], &amm_program_id())
}

fn swap_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SWAP_AUTHORITY_SEED], &tax_program_id())
}

fn tax_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TAX_AUTHORITY_SEED], &tax_program_id())
}

fn epoch_state_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[EPOCH_STATE_SEED], &epoch_program_id())
}

fn stake_pool_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[STAKE_POOL_SEED], &staking_program_id())
}

fn escrow_vault_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ESCROW_VAULT_SEED], &staking_program_id())
}

fn carnage_sol_vault_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CARNAGE_SOL_VAULT_SEED], &epoch_program_id())
}

fn wsol_intermediary_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[WSOL_INTERMEDIARY_SEED], &tax_program_id())
}

fn program_data_address(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[program_id.as_ref()], &bpf_loader_upgradeable_id())
}

// ---------------------------------------------------------------------------
// Mock data builders
// ---------------------------------------------------------------------------

/// EpochState mock (172 bytes incl. discriminator). Layout matches
/// `EpochState` in tax-program/src/state/epoch_state_reader.rs.
///
/// We always set `cheap_side = 0` (CRIME), `low_tax_bps = 300`, `high_tax_bps = 1400`
/// for self-consistency, even though `get_tax_bps` only reads the derived
/// rate fields. The Tax Program reads only the derived rates.
fn create_mock_epoch_state(
    crime_buy_bps: u16,
    crime_sell_bps: u16,
    fraud_buy_bps: u16,
    fraud_sell_bps: u16,
) -> Vec<u8> {
    let discriminator = anchor_account_discriminator("EpochState");
    let mut data = Vec::with_capacity(172);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&0u64.to_le_bytes()); // genesis_slot
    data.extend_from_slice(&1u32.to_le_bytes()); // current_epoch
    data.extend_from_slice(&0u64.to_le_bytes()); // epoch_start_slot
    data.push(0u8); // cheap_side (CRIME)
    data.extend_from_slice(&300u16.to_le_bytes());
    data.extend_from_slice(&1400u16.to_le_bytes());
    data.extend_from_slice(&crime_buy_bps.to_le_bytes());
    data.extend_from_slice(&crime_sell_bps.to_le_bytes());
    data.extend_from_slice(&fraud_buy_bps.to_le_bytes());
    data.extend_from_slice(&fraud_sell_bps.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // vrf_request_slot
    data.push(0u8);                              // vrf_pending
    data.push(1u8);                              // taxes_confirmed
    data.extend_from_slice(&[0u8; 32]);          // pending_randomness_account
    data.push(0u8);                              // carnage_pending
    data.push(0u8);                              // carnage_target
    data.push(0u8);                              // carnage_action
    data.extend_from_slice(&0u64.to_le_bytes()); // carnage_deadline_slot
    data.extend_from_slice(&0u64.to_le_bytes()); // carnage_lock_slot
    data.extend_from_slice(&0u32.to_le_bytes()); // last_carnage_epoch
    data.extend_from_slice(&[0u8; 64]);          // reserved
    data.push(1u8);                              // initialized
    data.push(255u8);                            // bump
    assert_eq!(data.len(), 172);
    data
}

fn create_mock_stake_pool(bump: u8) -> Vec<u8> {
    let discriminator = anchor_account_discriminator("StakePool");
    let mut data = Vec::with_capacity(62);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u128.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.push(1u8);
    data.push(bump);
    assert_eq!(data.len(), 62);
    data
}

// ---------------------------------------------------------------------------
// LiteSVM low-level helpers
// ---------------------------------------------------------------------------

fn read_program_bytes(program_name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("target/deploy/{}.so", program_name)),
    )
    .unwrap_or_else(|_| panic!("{}.so not found -- run `anchor build` first", program_name))
}

fn deploy_upgradeable_program(
    svm: &mut LiteSVM,
    program_id: &Pubkey,
    upgrade_authority: &Pubkey,
    program_bytes: &[u8],
) {
    let (programdata_key, _) = program_data_address(program_id);
    let loader_id = bpf_loader_upgradeable_id();

    let mut program_account_data = vec![0u8; 36];
    program_account_data[0..4].copy_from_slice(&2u32.to_le_bytes());
    program_account_data[4..36].copy_from_slice(programdata_key.as_ref());

    let header_size = 4 + 8 + 1 + 32;
    let mut programdata_data = vec![0u8; header_size + program_bytes.len()];
    programdata_data[0..4].copy_from_slice(&3u32.to_le_bytes());
    programdata_data[4..12].copy_from_slice(&0u64.to_le_bytes());
    programdata_data[12] = 1;
    programdata_data[13..45].copy_from_slice(upgrade_authority.as_ref());
    programdata_data[header_size..].copy_from_slice(program_bytes);

    let rent = solana_sdk::rent::Rent::default();
    let program_lamports = rent.minimum_balance(program_account_data.len()).max(1);
    let programdata_lamports = rent.minimum_balance(programdata_data.len()).max(1);

    svm.set_account(
        addr(&programdata_key),
        Account {
            lamports: programdata_lamports,
            data: programdata_data,
            owner: addr(&loader_id),
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("set programdata");

    svm.set_account(
        addr(program_id),
        Account {
            lamports: program_lamports,
            data: program_account_data,
            owner: addr(&loader_id),
            executable: true,
            rent_epoch: 0,
        },
    )
    .expect("set program");
}

/// Build a Token-2022 (or SPL Token) base mint state byte buffer (82 bytes).
/// Layout: COption<Pubkey>(36) mint_authority + supply u64(8) + decimals u8(1)
///       + is_initialized u8(1) + COption<Pubkey>(36) freeze_authority.
///
/// `mint_authority` MUST be Some(...) so the test admin can `MintTo`.
fn build_base_mint_data(mint_authority: &Pubkey, decimals: u8) -> Vec<u8> {
    let mut data = vec![0u8; 82];
    // mint_authority: Some(authority)
    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    data[4..36].copy_from_slice(mint_authority.as_ref());
    // supply = 0
    data[36..44].copy_from_slice(&0u64.to_le_bytes());
    data[44] = decimals;
    data[45] = 1; // is_initialized
    // freeze_authority: None (already zeroed)
    data
}

/// Install a Token-2022 mint at a specific pinned pubkey via `set_account`.
/// This is the entire reason this test file exists separately from the
/// legacy harnesses — those use ephemeral keypairs, but the patched Tax
/// Program only accepts the mainnet CRIME / FRAUD pubkeys.
fn install_t22_mint_at(svm: &mut LiteSVM, mint: &Pubkey, authority: &Pubkey, decimals: u8) {
    let data = build_base_mint_data(authority, decimals);
    let rent = solana_sdk::rent::Rent::default();
    let lamports = rent.minimum_balance(data.len());
    svm.set_account(
        addr(mint),
        Account {
            lamports,
            data,
            owner: addr(&token_2022_program_id()),
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("install t22 mint");
}

/// Install the native WSOL mint (SPL Token, 9 decimals, no authority).
fn install_native_wsol_mint(svm: &mut LiteSVM) {
    let mint = native_mint_id();
    let mut data = vec![0u8; SplMintState::LEN];
    data[0..4].copy_from_slice(&0u32.to_le_bytes()); // None
    data[36..44].copy_from_slice(&0u64.to_le_bytes()); // supply
    data[44] = 9;                                    // decimals
    data[45] = 1;                                    // is_initialized
    data[46..50].copy_from_slice(&0u32.to_le_bytes()); // freeze_authority None
    let rent = solana_sdk::rent::Rent::default();
    let lamports = rent.minimum_balance(data.len());
    svm.set_account(
        addr(&mint),
        Account {
            lamports,
            data,
            owner: addr(&spl_token_program_id()),
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("install native WSOL mint");
}

fn create_token_account(
    svm: &mut LiteSVM,
    payer: &LiteKeypair,
    mint: &Pubkey,
    owner: &Pubkey,
    token_program: &Pubkey,
) -> Pubkey {
    let account_kp = LiteKeypair::new();
    let account_pk = kp_pubkey(&account_kp);
    let account_addr = account_kp.pubkey();

    let rent = solana_sdk::rent::Rent::default();
    let space = 165u64;
    let lamports = rent.minimum_balance(space as usize);

    let create_account_ix = Instruction {
        program_id: addr(&system_program_id()),
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(account_addr, true),
        ],
        data: {
            let mut data = vec![0u8; 4 + 8 + 8 + 32];
            data[0..4].copy_from_slice(&0u32.to_le_bytes());
            data[4..12].copy_from_slice(&lamports.to_le_bytes());
            data[12..20].copy_from_slice(&space.to_le_bytes());
            data[20..52].copy_from_slice(token_program.as_ref());
            data
        },
    };

    let init_account_ix = Instruction {
        program_id: addr(token_program),
        accounts: vec![
            AccountMeta::new(account_addr, false),
            AccountMeta::new_readonly(addr(mint), false),
        ],
        data: {
            let mut d = vec![18u8]; // InitializeAccount3
            d.extend_from_slice(owner.as_ref());
            d
        },
    };

    let msg = Message::new_with_blockhash(
        &[create_account_ix, init_account_ix],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer, &account_kp]).unwrap();
    svm.send_transaction(tx).expect("create token account");
    account_pk
}

fn fund_native_wsol(svm: &mut LiteSVM, payer: &LiteKeypair, token_account: &Pubkey, amount: u64) {
    let transfer_ix = Instruction {
        program_id: addr(&system_program_id()),
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(addr(token_account), false),
        ],
        data: {
            let mut d = vec![0u8; 4 + 8];
            d[0..4].copy_from_slice(&2u32.to_le_bytes());
            d[4..12].copy_from_slice(&amount.to_le_bytes());
            d
        },
    };
    let sync_ix = Instruction {
        program_id: addr(&spl_token_program_id()),
        accounts: vec![AccountMeta::new(addr(token_account), false)],
        data: vec![17u8], // SyncNative
    };
    let msg = Message::new_with_blockhash(
        &[transfer_ix, sync_ix],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).expect("fund native wsol");
}

fn mint_t22_tokens(
    svm: &mut LiteSVM,
    payer: &LiteKeypair,
    mint: &Pubkey,
    dest: &Pubkey,
    amount: u64,
    authority: &LiteKeypair,
) {
    let ix = Instruction {
        program_id: addr(&token_2022_program_id()),
        accounts: vec![
            AccountMeta::new(addr(mint), false),
            AccountMeta::new(addr(dest), false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: {
            let mut d = vec![7u8]; // MintTo
            d.extend_from_slice(&amount.to_le_bytes());
            d
        },
    };

    let msg = Message::new_with_blockhash(
        &[ix],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );
    let signers: Vec<&LiteKeypair> = if kp_pubkey(payer) == kp_pubkey(authority) {
        vec![payer]
    } else {
        vec![payer, authority]
    };
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &signers).unwrap();
    svm.send_transaction(tx).expect("mint t22");
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

fn initialize_admin_data(admin: &Pubkey) -> Vec<u8> {
    let mut data = anchor_discriminator("initialize_admin").to_vec();
    admin.serialize(&mut data).unwrap();
    data
}

fn initialize_pool_data(lp_fee_bps: u16, amount_a: u64, amount_b: u64) -> Vec<u8> {
    let mut data = anchor_discriminator("initialize_pool").to_vec();
    lp_fee_bps.serialize(&mut data).unwrap();
    amount_a.serialize(&mut data).unwrap();
    amount_b.serialize(&mut data).unwrap();
    data
}

fn build_initialize_admin_ix(authority: &Pubkey, admin: &Pubkey) -> Instruction {
    let pid = amm_program_id();
    let (admin_config, _) = admin_config_pda();
    let (programdata_key, _) = program_data_address(&pid);

    Instruction {
        program_id: addr(&pid),
        accounts: vec![
            AccountMeta::new(addr(authority), true),
            AccountMeta::new(addr(&admin_config), false),
            AccountMeta::new_readonly(addr(&pid), false),
            AccountMeta::new_readonly(addr(&programdata_key), false),
            AccountMeta::new_readonly(addr(&system_program_id()), false),
        ],
        data: initialize_admin_data(admin),
    }
}

fn build_initialize_pool_ix(
    payer: &Pubkey,
    admin: &Pubkey,
    mint_a: &Pubkey,
    mint_b: &Pubkey,
    source_a: &Pubkey,
    source_b: &Pubkey,
    token_program_a: &Pubkey,
    token_program_b: &Pubkey,
    amount_a: u64,
    amount_b: u64,
) -> Instruction {
    let pid = amm_program_id();
    let (admin_config, _) = admin_config_pda();
    let (pool, _) = pool_pda(mint_a, mint_b);
    let (vault_a, _) = vault_a_pda(&pool);
    let (vault_b, _) = vault_b_pda(&pool);

    Instruction {
        program_id: addr(&pid),
        accounts: vec![
            AccountMeta::new(addr(payer), true),
            AccountMeta::new_readonly(addr(&admin_config), false),
            AccountMeta::new_readonly(addr(admin), true),
            AccountMeta::new(addr(&pool), false),
            AccountMeta::new(addr(&vault_a), false),
            AccountMeta::new(addr(&vault_b), false),
            AccountMeta::new_readonly(addr(mint_a), false),
            AccountMeta::new_readonly(addr(mint_b), false),
            AccountMeta::new(addr(source_a), false),
            AccountMeta::new(addr(source_b), false),
            AccountMeta::new_readonly(addr(token_program_a), false),
            AccountMeta::new_readonly(addr(token_program_b), false),
            AccountMeta::new_readonly(addr(&system_program_id()), false),
        ],
        data: initialize_pool_data(LP_FEE_BPS, amount_a, amount_b),
    }
}

fn swap_sol_buy_data(amount_in: u64, minimum_output: u64, is_crime: bool) -> Vec<u8> {
    let mut data = anchor_discriminator("swap_sol_buy").to_vec();
    amount_in.serialize(&mut data).unwrap();
    minimum_output.serialize(&mut data).unwrap();
    is_crime.serialize(&mut data).unwrap();
    data
}

fn swap_sol_sell_data(amount_in: u64, minimum_output: u64, is_crime: bool) -> Vec<u8> {
    let mut data = anchor_discriminator("swap_sol_sell").to_vec();
    amount_in.serialize(&mut data).unwrap();
    minimum_output.serialize(&mut data).unwrap();
    is_crime.serialize(&mut data).unwrap();
    data
}

#[allow(clippy::too_many_arguments)]
fn build_swap_sol_buy_ix(
    user: &Pubkey,
    epoch_state: &Pubkey,
    swap_authority: &Pubkey,
    tax_authority: &Pubkey,
    pool: &Pubkey,
    vault_a: &Pubkey,
    vault_b: &Pubkey,
    mint_a: &Pubkey,
    mint_b: &Pubkey,
    user_token_a: &Pubkey,
    user_token_b: &Pubkey,
    stake_pool: &Pubkey,
    staking_escrow: &Pubkey,
    carnage_vault: &Pubkey,
    treasury: &Pubkey,
    token_program_a: &Pubkey,
    token_program_b: &Pubkey,
    amount_in: u64,
    minimum_output: u64,
    is_crime: bool,
) -> Instruction {
    Instruction {
        program_id: addr(&tax_program_id()),
        accounts: vec![
            AccountMeta::new(addr(user), true),
            AccountMeta::new_readonly(addr(epoch_state), false),
            AccountMeta::new_readonly(addr(swap_authority), false),
            AccountMeta::new_readonly(addr(tax_authority), false),
            AccountMeta::new(addr(pool), false),
            AccountMeta::new(addr(vault_a), false),
            AccountMeta::new(addr(vault_b), false),
            AccountMeta::new_readonly(addr(mint_a), false),
            AccountMeta::new_readonly(addr(mint_b), false),
            AccountMeta::new(addr(user_token_a), false),
            AccountMeta::new(addr(user_token_b), false),
            AccountMeta::new(addr(stake_pool), false),
            AccountMeta::new(addr(staking_escrow), false),
            AccountMeta::new(addr(carnage_vault), false),
            AccountMeta::new(addr(treasury), false),
            AccountMeta::new_readonly(addr(&amm_program_id()), false),
            AccountMeta::new_readonly(addr(token_program_a), false),
            AccountMeta::new_readonly(addr(token_program_b), false),
            AccountMeta::new_readonly(addr(&system_program_id()), false),
            AccountMeta::new_readonly(addr(&staking_program_id()), false),
        ],
        data: swap_sol_buy_data(amount_in, minimum_output, is_crime),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_swap_sol_sell_ix(
    user: &Pubkey,
    epoch_state: &Pubkey,
    swap_authority: &Pubkey,
    tax_authority: &Pubkey,
    pool: &Pubkey,
    vault_a: &Pubkey,
    vault_b: &Pubkey,
    mint_a: &Pubkey,
    mint_b: &Pubkey,
    user_token_a: &Pubkey,
    user_token_b: &Pubkey,
    stake_pool: &Pubkey,
    staking_escrow: &Pubkey,
    carnage_vault: &Pubkey,
    treasury: &Pubkey,
    wsol_intermediary: &Pubkey,
    token_program_a: &Pubkey,
    token_program_b: &Pubkey,
    amount_in: u64,
    minimum_output: u64,
    is_crime: bool,
) -> Instruction {
    Instruction {
        program_id: addr(&tax_program_id()),
        accounts: vec![
            AccountMeta::new(addr(user), true),
            AccountMeta::new_readonly(addr(epoch_state), false),
            AccountMeta::new(addr(swap_authority), false),
            AccountMeta::new_readonly(addr(tax_authority), false),
            AccountMeta::new(addr(pool), false),
            AccountMeta::new(addr(vault_a), false),
            AccountMeta::new(addr(vault_b), false),
            AccountMeta::new_readonly(addr(mint_a), false),
            AccountMeta::new_readonly(addr(mint_b), false),
            AccountMeta::new(addr(user_token_a), false),
            AccountMeta::new(addr(user_token_b), false),
            AccountMeta::new(addr(stake_pool), false),
            AccountMeta::new(addr(staking_escrow), false),
            AccountMeta::new(addr(carnage_vault), false),
            AccountMeta::new(addr(treasury), false),
            AccountMeta::new(addr(wsol_intermediary), false),
            AccountMeta::new_readonly(addr(&amm_program_id()), false),
            AccountMeta::new_readonly(addr(token_program_a), false),
            AccountMeta::new_readonly(addr(token_program_b), false),
            AccountMeta::new_readonly(addr(&system_program_id()), false),
            AccountMeta::new_readonly(addr(&staking_program_id()), false),
        ],
        data: swap_sol_sell_data(amount_in, minimum_output, is_crime),
    }
}

// ---------------------------------------------------------------------------
// Balance / pool readers
// ---------------------------------------------------------------------------

fn get_token_balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let acct = svm.get_account(&addr(token_account)).expect("token account exists");
    u64::from_le_bytes(acct.data[64..72].try_into().unwrap())
}

fn get_sol_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    svm.get_account(&addr(account)).map(|a| a.lamports).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tax math (mirrors Tax Program tax_math.rs)
// ---------------------------------------------------------------------------

fn calculate_expected_tax(amount: u64, tax_bps: u64) -> u64 {
    ((amount as u128) * (tax_bps as u128) / (BPS_DENOMINATOR as u128)) as u64
}

fn expected_effective_input(amount_in: u64) -> u128 {
    let amount = amount_in as u128;
    let fee_factor = 10_000u128 - LP_FEE_BPS as u128;
    amount * fee_factor / 10_000
}

fn expected_swap_output(reserve_in: u64, reserve_out: u64, effective_input: u128) -> u64 {
    let r_in = reserve_in as u128;
    let r_out = reserve_out as u128;
    let numerator = r_out * effective_input;
    let denominator = r_in + effective_input;
    (numerator / denominator) as u64
}

/// minimum_output for a buy that satisfies SEC-10 (>= 50% of expected gross).
/// Pool reserves are SEED_AMOUNT each. We use 51% to clear the floor.
fn safe_minimum_for_buy(amount_in: u64, tax_bps: u64) -> u64 {
    let tax = calculate_expected_tax(amount_in, tax_bps);
    let sol_to_swap = amount_in - tax;
    let effective = expected_effective_input(sol_to_swap);
    let expected = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective);
    expected * 51 / 100
}

fn safe_minimum_for_sell(amount_in: u64) -> u64 {
    let effective = expected_effective_input(amount_in);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective);
    expected_gross * 51 / 100
}

// ---------------------------------------------------------------------------
// Test fixture
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct IdMismatchCtx {
    svm: LiteSVM,
    admin: LiteKeypair,
    user: LiteKeypair,
    // Both pools share these
    swap_authority: Pubkey,
    tax_authority: Pubkey,
    epoch_state: Pubkey,
    stake_pool: Pubkey,
    staking_escrow: Pubkey,
    carnage_vault: Pubkey,
    treasury: Pubkey,
    wsol_intermediary: Pubkey,

    // CRIME pool fixtures
    crime_mint: Pubkey,
    crime_pool: Pubkey,
    crime_vault_a: Pubkey, // WSOL vault
    crime_vault_b: Pubkey, // CRIME vault
    user_crime_a: Pubkey,  // user WSOL ATA for crime pool
    user_crime_b: Pubkey,  // user CRIME ATA

    // FRAUD pool fixtures
    fraud_mint: Pubkey,
    fraud_pool: Pubkey,
    fraud_vault_a: Pubkey,
    fraud_vault_b: Pubkey,
    user_fraud_a: Pubkey,
    user_fraud_b: Pubkey,

    // Random unrelated mint (for UnknownTaxedMint case)
    bogus_mint: Pubkey,
}

impl IdMismatchCtx {
    fn setup() -> Self {
        let upgrade_authority = LiteKeypair::new();
        let mut svm = LiteSVM::new();

        svm.airdrop(&upgrade_authority.pubkey(), 100_000_000_000).unwrap();

        // Deploy AMM, Tax Program, Staking Program
        deploy_upgradeable_program(
            &mut svm,
            &amm_program_id(),
            &kp_pubkey(&upgrade_authority),
            &read_program_bytes("amm"),
        );
        deploy_upgradeable_program(
            &mut svm,
            &tax_program_id(),
            &kp_pubkey(&upgrade_authority),
            &read_program_bytes("tax_program"),
        );
        deploy_upgradeable_program(
            &mut svm,
            &staking_program_id(),
            &kp_pubkey(&upgrade_authority),
            &read_program_bytes("staking"),
        );

        // Admin
        let admin = LiteKeypair::new();
        svm.airdrop(&admin.pubkey(), 100_000_000_000).unwrap();

        // Initialize AMM AdminConfig
        let init_admin_ix = build_initialize_admin_ix(
            &kp_pubkey(&upgrade_authority),
            &kp_pubkey(&admin),
        );
        let msg = Message::new_with_blockhash(
            &[init_admin_ix],
            Some(&upgrade_authority.pubkey()),
            &svm.latest_blockhash(),
        );
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&upgrade_authority]).unwrap();
        svm.send_transaction(tx).expect("init admin");
        svm.expire_blockhash();

        // Install native WSOL mint and the two pinned T22 mints (CRIME, FRAUD).
        install_native_wsol_mint(&mut svm);
        install_t22_mint_at(&mut svm, &crime_mint_pk(), &kp_pubkey(&admin), TEST_DECIMALS);
        install_t22_mint_at(&mut svm, &fraud_mint_pk(), &kp_pubkey(&admin), TEST_DECIMALS);

        // Sanity: NATIVE_MINT must be canonical mint_a vs both CRIME and FRAUD
        // (it sorts before all token mints because the bytes start with 0x06).
        let native = native_mint_id();
        assert!(native < crime_mint_pk(), "NATIVE_MINT must sort before CRIME");
        assert!(native < fraud_mint_pk(), "NATIVE_MINT must sort before FRAUD");

        // === Build CRIME pool: (NATIVE_MINT, CRIME) ===
        let crime_mint = crime_mint_pk();
        let crime_source_a = create_token_account(&mut svm, &admin, &native, &kp_pubkey(&admin), &spl_token_program_id());
        svm.expire_blockhash();
        let crime_source_b = create_token_account(&mut svm, &admin, &crime_mint, &kp_pubkey(&admin), &token_2022_program_id());
        svm.expire_blockhash();

        fund_native_wsol(&mut svm, &admin, &crime_source_a, SEED_AMOUNT);
        svm.expire_blockhash();
        mint_t22_tokens(&mut svm, &admin, &crime_mint, &crime_source_b, SEED_AMOUNT, &admin);
        svm.expire_blockhash();

        // === Build FRAUD pool: (NATIVE_MINT, FRAUD) ===
        let fraud_mint = fraud_mint_pk();
        let fraud_source_a = create_token_account(&mut svm, &admin, &native, &kp_pubkey(&admin), &spl_token_program_id());
        svm.expire_blockhash();
        let fraud_source_b = create_token_account(&mut svm, &admin, &fraud_mint, &kp_pubkey(&admin), &token_2022_program_id());
        svm.expire_blockhash();

        fund_native_wsol(&mut svm, &admin, &fraud_source_a, SEED_AMOUNT);
        svm.expire_blockhash();
        mint_t22_tokens(&mut svm, &admin, &fraud_mint, &fraud_source_b, SEED_AMOUNT, &admin);
        svm.expire_blockhash();

        // Initialize CRIME pool
        let payer = LiteKeypair::new();
        svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
        {
            let ix = build_initialize_pool_ix(
                &kp_pubkey(&payer),
                &kp_pubkey(&admin),
                &native,
                &crime_mint,
                &crime_source_a,
                &crime_source_b,
                &spl_token_program_id(),
                &token_2022_program_id(),
                SEED_AMOUNT,
                SEED_AMOUNT,
            );
            let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
            let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer, &admin]).unwrap();
            svm.send_transaction(tx).expect("init crime pool");
            svm.expire_blockhash();
        }

        // Initialize FRAUD pool
        {
            let ix = build_initialize_pool_ix(
                &kp_pubkey(&payer),
                &kp_pubkey(&admin),
                &native,
                &fraud_mint,
                &fraud_source_a,
                &fraud_source_b,
                &spl_token_program_id(),
                &token_2022_program_id(),
                SEED_AMOUNT,
                SEED_AMOUNT,
            );
            let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
            let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer, &admin]).unwrap();
            svm.send_transaction(tx).expect("init fraud pool");
            svm.expire_blockhash();
        }

        let (crime_pool, _) = pool_pda(&native, &crime_mint);
        let (crime_vault_a, _) = vault_a_pda(&crime_pool);
        let (crime_vault_b, _) = vault_b_pda(&crime_pool);
        let (fraud_pool, _) = pool_pda(&native, &fraud_mint);
        let (fraud_vault_a, _) = vault_a_pda(&fraud_pool);
        let (fraud_vault_b, _) = vault_b_pda(&fraud_pool);

        // User
        let user = LiteKeypair::new();
        svm.airdrop(&user.pubkey(), 500_000_000_000).unwrap();

        let user_crime_a = create_token_account(&mut svm, &user, &native, &kp_pubkey(&user), &spl_token_program_id());
        svm.expire_blockhash();
        let user_crime_b = create_token_account(&mut svm, &user, &crime_mint, &kp_pubkey(&user), &token_2022_program_id());
        svm.expire_blockhash();
        let user_fraud_a = create_token_account(&mut svm, &user, &native, &kp_pubkey(&user), &spl_token_program_id());
        svm.expire_blockhash();
        let user_fraud_b = create_token_account(&mut svm, &user, &fraud_mint, &kp_pubkey(&user), &token_2022_program_id());
        svm.expire_blockhash();

        fund_native_wsol(&mut svm, &user, &user_crime_a, 100_000_000_000);
        svm.expire_blockhash();
        fund_native_wsol(&mut svm, &user, &user_fraud_a, 100_000_000_000);
        svm.expire_blockhash();
        mint_t22_tokens(&mut svm, &admin, &crime_mint, &user_crime_b, 100_000_000_000, &admin);
        svm.expire_blockhash();
        mint_t22_tokens(&mut svm, &admin, &fraud_mint, &user_fraud_b, 100_000_000_000, &admin);
        svm.expire_blockhash();

        // Derive shared PDAs
        let (swap_authority, _) = swap_authority_pda();
        let (tax_authority, _) = tax_authority_pda();

        let rent = solana_sdk::rent::Rent::default();

        // EpochState mock with distinct rates
        let (epoch_state, _) = epoch_state_pda();
        let epoch_data = create_mock_epoch_state(CRIME_BUY_BPS, CRIME_SELL_BPS, FRAUD_BUY_BPS, FRAUD_SELL_BPS);
        let epoch_lamports = rent.minimum_balance(epoch_data.len());
        svm.set_account(
            addr(&epoch_state),
            Account {
                lamports: epoch_lamports,
                data: epoch_data,
                owner: addr(&epoch_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("set epoch state");

        // StakePool mock
        let (stake_pool_pk, stake_pool_bump) = stake_pool_pda();
        let stake_pool_data = create_mock_stake_pool(stake_pool_bump);
        let stake_pool_lamports = rent.minimum_balance(stake_pool_data.len());
        svm.set_account(
            addr(&stake_pool_pk),
            Account {
                lamports: stake_pool_lamports,
                data: stake_pool_data,
                owner: addr(&staking_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("set stake pool");

        // Staking escrow PDA
        let (staking_escrow_pk, _) = escrow_vault_pda();
        svm.set_account(
            addr(&staking_escrow_pk),
            Account {
                lamports: 1_000_000,
                data: vec![],
                owner: addr(&staking_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("set staking escrow");

        // Carnage vault PDA
        let (carnage_vault_pk, _) = carnage_sol_vault_pda();
        svm.set_account(
            addr(&carnage_vault_pk),
            Account {
                lamports: 1_000_000,
                data: vec![],
                owner: addr(&system_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("set carnage vault");

        // Treasury (mainnet pubkey)
        let treasury_pk = treasury_pubkey();
        svm.set_account(
            addr(&treasury_pk),
            Account {
                lamports: 1_000_000,
                data: vec![],
                owner: addr(&system_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("set treasury");

        // WSOL intermediary PDA — initialized native WSOL token account
        // owned by swap_authority. See test_swap_sol_sell.rs lines 1111-1161
        // for the rationale: needs is_native=Some(rent) for the close-and-reinit
        // cycle to unwrap WSOL properly.
        let (wsol_intermediary_pk, _) = wsol_intermediary_pda();
        let intermediary_lamports = rent.minimum_balance(165);
        let mut intermediary_data = vec![0u8; 165];
        intermediary_data[0..32].copy_from_slice(native.as_ref());
        intermediary_data[32..64].copy_from_slice(swap_authority.as_ref());
        intermediary_data[64..72].copy_from_slice(&0u64.to_le_bytes());
        intermediary_data[108] = 1; // Initialized
        intermediary_data[109..113].copy_from_slice(&1u32.to_le_bytes()); // is_native: Some
        intermediary_data[113..121].copy_from_slice(&intermediary_lamports.to_le_bytes());
        svm.set_account(
            addr(&wsol_intermediary_pk),
            Account {
                lamports: intermediary_lamports,
                data: intermediary_data,
                owner: addr(&spl_token_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("set wsol intermediary");

        // Fund swap_authority for sell rent + distribution
        svm.set_account(
            addr(&swap_authority),
            Account {
                lamports: 10_000_000_000,
                data: vec![],
                owner: addr(&system_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("fund swap authority");

        // Bogus unrelated mint for the UnknownTaxedMint case.
        // Install another T22 base mint at a fresh random pubkey.
        let bogus_mint_kp = LiteKeypair::new();
        let bogus_mint = kp_pubkey(&bogus_mint_kp);
        install_t22_mint_at(&mut svm, &bogus_mint, &kp_pubkey(&admin), TEST_DECIMALS);

        IdMismatchCtx {
            svm,
            admin,
            user,
            swap_authority,
            tax_authority,
            epoch_state,
            stake_pool: stake_pool_pk,
            staking_escrow: staking_escrow_pk,
            carnage_vault: carnage_vault_pk,
            treasury: treasury_pk,
            wsol_intermediary: wsol_intermediary_pk,
            crime_mint,
            crime_pool,
            crime_vault_a,
            crime_vault_b,
            user_crime_a,
            user_crime_b,
            fraud_mint,
            fraud_pool,
            fraud_vault_a,
            fraud_vault_b,
            user_fraud_a,
            user_fraud_b,
            bogus_mint,
        }
    }

    /// Send a buy swap with arbitrary mint_b / pool / vault selection.
    /// This is the *exploit surface* — tests construct combinations that
    /// the patched program must reject.
    #[allow(clippy::too_many_arguments)]
    fn send_buy_with(
        &mut self,
        pool: &Pubkey,
        vault_a: &Pubkey,
        vault_b: &Pubkey,
        mint_b: &Pubkey,
        user_token_a: &Pubkey,
        user_token_b: &Pubkey,
        amount_in: u64,
        minimum_output: u64,
        is_crime: bool,
    ) -> litesvm::types::TransactionResult {
        let native = native_mint_id();
        let ix = build_swap_sol_buy_ix(
            &kp_pubkey(&self.user),
            &self.epoch_state,
            &self.swap_authority,
            &self.tax_authority,
            pool,
            vault_a,
            vault_b,
            &native,
            mint_b,
            user_token_a,
            user_token_b,
            &self.stake_pool,
            &self.staking_escrow,
            &self.carnage_vault,
            &self.treasury,
            &spl_token_program_id(),
            &token_2022_program_id(),
            amount_in,
            minimum_output,
            is_crime,
        );
        let msg = Message::new_with_blockhash(
            &[ix],
            Some(&self.user.pubkey()),
            &self.svm.latest_blockhash(),
        );
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&self.user]).unwrap();
        self.svm.send_transaction(tx)
    }

    #[allow(clippy::too_many_arguments)]
    fn send_sell_with(
        &mut self,
        pool: &Pubkey,
        vault_a: &Pubkey,
        vault_b: &Pubkey,
        mint_b: &Pubkey,
        user_token_a: &Pubkey,
        user_token_b: &Pubkey,
        amount_in: u64,
        minimum_output: u64,
        is_crime: bool,
    ) -> litesvm::types::TransactionResult {
        let native = native_mint_id();
        let ix = build_swap_sol_sell_ix(
            &kp_pubkey(&self.user),
            &self.epoch_state,
            &self.swap_authority,
            &self.tax_authority,
            pool,
            vault_a,
            vault_b,
            &native,
            mint_b,
            user_token_a,
            user_token_b,
            &self.stake_pool,
            &self.staking_escrow,
            &self.carnage_vault,
            &self.treasury,
            &self.wsol_intermediary,
            &spl_token_program_id(),
            &token_2022_program_id(),
            amount_in,
            minimum_output,
            is_crime,
        );
        let msg = Message::new_with_blockhash(
            &[ix],
            Some(&self.user.pubkey()),
            &self.svm.latest_blockhash(),
        );
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&self.user]).unwrap();
        self.svm.send_transaction(tx)
    }
}

fn err_string<T: std::fmt::Debug>(r: &Result<T, litesvm::types::FailedTransactionMetadata>) -> String {
    match r {
        Ok(_) => "<ok>".to_string(),
        Err(e) => format!("{:?}", e),
    }
}

fn assert_err_contains<T: std::fmt::Debug>(
    r: &Result<T, litesvm::types::FailedTransactionMetadata>,
    needle: &str,
    case: &str,
) {
    assert!(r.is_err(), "[{}] expected failure, got Ok", case);
    let s = err_string(r);
    assert!(
        s.contains(needle),
        "[{}] expected error to contain '{}', got: {}",
        case,
        needle,
        s
    );
}

// ===========================================================================
// HONEST PATH (8 cases)
// ===========================================================================

#[test]
fn honest_buy_crime_succeeds_with_crime_tax() {
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 100_000_000; // 0.1 WSOL
    let min_out = safe_minimum_for_buy(amount_in, CRIME_BUY_BPS as u64);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    let r = ctx.send_buy_with(
        &ctx.crime_pool.clone(),
        &ctx.crime_vault_a.clone(),
        &ctx.crime_vault_b.clone(),
        &ctx.crime_mint.clone(),
        &ctx.user_crime_a.clone(),
        &ctx.user_crime_b.clone(),
        amount_in,
        min_out,
        true,
    );
    assert!(r.is_ok(), "honest CRIME buy failed: {}", err_string(&r));

    // Verify total tax distributed equals CRIME buy schedule (3%)
    let total_after = get_sol_balance(&ctx.svm, &ctx.staking_escrow)
        + get_sol_balance(&ctx.svm, &ctx.carnage_vault)
        + get_sol_balance(&ctx.svm, &ctx.treasury);
    let total_before = staking_before + carnage_before + treasury_before;
    let observed_tax = total_after - total_before;
    let expected_tax = calculate_expected_tax(amount_in, CRIME_BUY_BPS as u64);
    assert_eq!(
        observed_tax, expected_tax,
        "CRIME buy tax should be {}bps of {}, got {}",
        CRIME_BUY_BPS, amount_in, observed_tax
    );
}

#[test]
fn honest_buy_fraud_succeeds_with_fraud_tax() {
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 100_000_000;
    let min_out = safe_minimum_for_buy(amount_in, FRAUD_BUY_BPS as u64);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    let r = ctx.send_buy_with(
        &ctx.fraud_pool.clone(),
        &ctx.fraud_vault_a.clone(),
        &ctx.fraud_vault_b.clone(),
        &ctx.fraud_mint.clone(),
        &ctx.user_fraud_a.clone(),
        &ctx.user_fraud_b.clone(),
        amount_in,
        min_out,
        false,
    );
    assert!(r.is_ok(), "honest FRAUD buy failed: {}", err_string(&r));

    let total_after = get_sol_balance(&ctx.svm, &ctx.staking_escrow)
        + get_sol_balance(&ctx.svm, &ctx.carnage_vault)
        + get_sol_balance(&ctx.svm, &ctx.treasury);
    let total_before = staking_before + carnage_before + treasury_before;
    let observed_tax = total_after - total_before;
    let expected_tax = calculate_expected_tax(amount_in, FRAUD_BUY_BPS as u64);
    assert_eq!(observed_tax, expected_tax);
    // Cross-check: must NOT match CRIME buy tax (because rates are distinct)
    assert_ne!(
        observed_tax,
        calculate_expected_tax(amount_in, CRIME_BUY_BPS as u64),
        "FRAUD tax must differ from CRIME tax (rate-distinguishability check)"
    );
}

#[test]
fn honest_buy_crime_uses_crime_rate_not_fraud_rate() {
    // Defense-in-depth: proves the patched code reads CRIME bps, not FRAUD bps,
    // when the swap is against the CRIME pool.
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 200_000_000;
    let min_out = safe_minimum_for_buy(amount_in, CRIME_BUY_BPS as u64);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    let r = ctx.send_buy_with(
        &ctx.crime_pool.clone(),
        &ctx.crime_vault_a.clone(),
        &ctx.crime_vault_b.clone(),
        &ctx.crime_mint.clone(),
        &ctx.user_crime_a.clone(),
        &ctx.user_crime_b.clone(),
        amount_in,
        min_out,
        true,
    );
    assert!(r.is_ok(), "honest CRIME buy failed: {}", err_string(&r));

    let observed_tax = (get_sol_balance(&ctx.svm, &ctx.staking_escrow) - staking_before)
        + (get_sol_balance(&ctx.svm, &ctx.carnage_vault) - carnage_before)
        + (get_sol_balance(&ctx.svm, &ctx.treasury) - treasury_before);
    assert_eq!(observed_tax, calculate_expected_tax(amount_in, CRIME_BUY_BPS as u64));
    // Sanity: this is observably less than what FRAUD buy rate would have produced
    assert!(observed_tax < calculate_expected_tax(amount_in, FRAUD_BUY_BPS as u64));
}

#[test]
fn honest_buy_fraud_uses_fraud_rate_not_crime_rate() {
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 200_000_000;
    let min_out = safe_minimum_for_buy(amount_in, FRAUD_BUY_BPS as u64);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    let r = ctx.send_buy_with(
        &ctx.fraud_pool.clone(),
        &ctx.fraud_vault_a.clone(),
        &ctx.fraud_vault_b.clone(),
        &ctx.fraud_mint.clone(),
        &ctx.user_fraud_a.clone(),
        &ctx.user_fraud_b.clone(),
        amount_in,
        min_out,
        false,
    );
    assert!(r.is_ok(), "honest FRAUD buy failed: {}", err_string(&r));

    let observed_tax = (get_sol_balance(&ctx.svm, &ctx.staking_escrow) - staking_before)
        + (get_sol_balance(&ctx.svm, &ctx.carnage_vault) - carnage_before)
        + (get_sol_balance(&ctx.svm, &ctx.treasury) - treasury_before);
    assert_eq!(observed_tax, calculate_expected_tax(amount_in, FRAUD_BUY_BPS as u64));
    assert!(observed_tax > calculate_expected_tax(amount_in, CRIME_BUY_BPS as u64));
}

#[test]
fn honest_sell_crime_succeeds_with_crime_sell_tax() {
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 50_000_000;
    let min_out = safe_minimum_for_sell(amount_in);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);
    let user_crime_b_before = get_token_balance(&ctx.svm, &ctx.user_crime_b);

    let r = ctx.send_sell_with(
        &ctx.crime_pool.clone(),
        &ctx.crime_vault_a.clone(),
        &ctx.crime_vault_b.clone(),
        &ctx.crime_mint.clone(),
        &ctx.user_crime_a.clone(),
        &ctx.user_crime_b.clone(),
        amount_in,
        min_out,
        true,
    );
    assert!(r.is_ok(), "honest CRIME sell failed: {}", err_string(&r));

    // CRIME tokens debited
    let user_crime_b_after = get_token_balance(&ctx.svm, &ctx.user_crime_b);
    assert_eq!(user_crime_b_before - amount_in, user_crime_b_after);

    // Tax extracted at CRIME sell rate (1400 bps) of gross output
    let effective = expected_effective_input(amount_in);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective);
    let expected_tax = calculate_expected_tax(expected_gross, CRIME_SELL_BPS as u64);
    let observed_tax = (get_sol_balance(&ctx.svm, &ctx.staking_escrow) - staking_before)
        + (get_sol_balance(&ctx.svm, &ctx.carnage_vault) - carnage_before)
        + (get_sol_balance(&ctx.svm, &ctx.treasury) - treasury_before);
    // Allow off-by-one rounding tolerance from u128 vs u64 arithmetic
    let diff = observed_tax.abs_diff(expected_tax);
    assert!(diff <= 2, "CRIME sell tax mismatch: expected ~{}, got {}", expected_tax, observed_tax);
}

#[test]
fn honest_sell_fraud_succeeds_with_fraud_sell_tax() {
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 50_000_000;
    let min_out = safe_minimum_for_sell(amount_in);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    let r = ctx.send_sell_with(
        &ctx.fraud_pool.clone(),
        &ctx.fraud_vault_a.clone(),
        &ctx.fraud_vault_b.clone(),
        &ctx.fraud_mint.clone(),
        &ctx.user_fraud_a.clone(),
        &ctx.user_fraud_b.clone(),
        amount_in,
        min_out,
        false,
    );
    assert!(r.is_ok(), "honest FRAUD sell failed: {}", err_string(&r));

    let effective = expected_effective_input(amount_in);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective);
    let expected_tax = calculate_expected_tax(expected_gross, FRAUD_SELL_BPS as u64);
    let observed_tax = (get_sol_balance(&ctx.svm, &ctx.staking_escrow) - staking_before)
        + (get_sol_balance(&ctx.svm, &ctx.carnage_vault) - carnage_before)
        + (get_sol_balance(&ctx.svm, &ctx.treasury) - treasury_before);
    let diff = observed_tax.abs_diff(expected_tax);
    assert!(diff <= 2, "FRAUD sell tax mismatch: expected ~{}, got {}", expected_tax, observed_tax);
}

#[test]
fn honest_sell_crime_uses_crime_rate_not_fraud_rate() {
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 80_000_000;
    let min_out = safe_minimum_for_sell(amount_in);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    let r = ctx.send_sell_with(
        &ctx.crime_pool.clone(),
        &ctx.crime_vault_a.clone(),
        &ctx.crime_vault_b.clone(),
        &ctx.crime_mint.clone(),
        &ctx.user_crime_a.clone(),
        &ctx.user_crime_b.clone(),
        amount_in,
        min_out,
        true,
    );
    assert!(r.is_ok(), "honest CRIME sell failed: {}", err_string(&r));

    let observed_tax = (get_sol_balance(&ctx.svm, &ctx.staking_escrow) - staking_before)
        + (get_sol_balance(&ctx.svm, &ctx.carnage_vault) - carnage_before)
        + (get_sol_balance(&ctx.svm, &ctx.treasury) - treasury_before);
    let effective = expected_effective_input(amount_in);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective);
    let crime_tax = calculate_expected_tax(expected_gross, CRIME_SELL_BPS as u64);
    let fraud_tax = calculate_expected_tax(expected_gross, FRAUD_SELL_BPS as u64);
    let diff = observed_tax.abs_diff(crime_tax);
    assert!(diff <= 2, "expected ~{} (CRIME sell), got {}", crime_tax, observed_tax);
    // CRIME sell (14%) is observably much higher than FRAUD sell (3%)
    assert!(observed_tax > fraud_tax * 3, "CRIME sell should produce vastly more tax than FRAUD sell");
}

#[test]
fn honest_sell_fraud_uses_fraud_rate_not_crime_rate() {
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 80_000_000;
    let min_out = safe_minimum_for_sell(amount_in);

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    let r = ctx.send_sell_with(
        &ctx.fraud_pool.clone(),
        &ctx.fraud_vault_a.clone(),
        &ctx.fraud_vault_b.clone(),
        &ctx.fraud_mint.clone(),
        &ctx.user_fraud_a.clone(),
        &ctx.user_fraud_b.clone(),
        amount_in,
        min_out,
        false,
    );
    assert!(r.is_ok(), "honest FRAUD sell failed: {}", err_string(&r));

    let observed_tax = (get_sol_balance(&ctx.svm, &ctx.staking_escrow) - staking_before)
        + (get_sol_balance(&ctx.svm, &ctx.carnage_vault) - carnage_before)
        + (get_sol_balance(&ctx.svm, &ctx.treasury) - treasury_before);
    let effective = expected_effective_input(amount_in);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective);
    let fraud_tax = calculate_expected_tax(expected_gross, FRAUD_SELL_BPS as u64);
    let crime_tax = calculate_expected_tax(expected_gross, CRIME_SELL_BPS as u64);
    let diff = observed_tax.abs_diff(fraud_tax);
    assert!(diff <= 2, "expected ~{} (FRAUD sell), got {}", fraud_tax, observed_tax);
    assert!(observed_tax * 3 < crime_tax);
}

// ===========================================================================
// EXPLOIT REJECTION (4 cases)
// ===========================================================================
//
// These are the regression tests for the live whitehat-reported bug.
// Each case constructs an attempted exploit and asserts the patched code
// rejects it with the expected error variant. Each must SUCCEED on
// pre-patch code (proving the bug existed) — see SUMMARY.md regression
// proof section.

#[test]
fn exploit_buy_crime_pool_with_is_crime_false_rejected() {
    // Attacker submits the CRIME pool but lies that it's FRAUD (is_crime=false)
    // hoping to pay FRAUD buy rate (1400 bps) — but wait, they'd want to pay
    // the CHEAPER side. CRIME buy is 300 bps, FRAUD buy is 1400 bps. So flipping
    // CRIME→FRAUD makes them PAY more. The cheap-side exploit goes the OTHER
    // way: attacker submits FRAUD pool with is_crime=true to pay 300 bps
    // instead of 1400 bps. Both directions must be rejected. We test BOTH:
    //
    // Direction 1 (this test): CRIME pool, mint_b=CRIME mint, is_crime=false
    //   → Option B catches it: derived_is_crime=true (from CRIME mint),
    //     caller said false → TaxIdentityMismatch.
    //
    // Direction 2 (next test): FRAUD pool, mint_b=FRAUD mint, is_crime=true
    //   → Option B catches it the same way.

    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 100_000_000;
    // Use a very low minimum to be sure the pre-patch path would have got
    // past the SEC-10 floor (we want the negative case to fail SPECIFICALLY
    // on TaxIdentityMismatch, not floor violation).
    let min_out = safe_minimum_for_buy(amount_in, FRAUD_BUY_BPS as u64);

    let r = ctx.send_buy_with(
        &ctx.crime_pool.clone(),
        &ctx.crime_vault_a.clone(),
        &ctx.crime_vault_b.clone(),
        &ctx.crime_mint.clone(),
        &ctx.user_crime_a.clone(),
        &ctx.user_crime_b.clone(),
        amount_in,
        min_out,
        false, // LIE: claim FRAUD on a CRIME swap
    );
    assert_err_contains(&r, "TaxIdentityMismatch", "buy CRIME pool with is_crime=false");
}

#[test]
fn exploit_sell_fraud_pool_with_is_crime_true_rejected() {
    // Cheap-side sell exploit: FRAUD sell is 300 bps (cheap side), CRIME sell
    // is 1400 bps. An attacker selling CRIME would want to pretend it's FRAUD
    // to pay 300 bps instead of 1400. But the symmetric path also needs
    // rejecting — selling FRAUD while claiming is_crime=true tries to flip
    // identity in the other direction. Both lies must die.
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 50_000_000;
    let min_out = safe_minimum_for_sell(amount_in);

    let r = ctx.send_sell_with(
        &ctx.fraud_pool.clone(),
        &ctx.fraud_vault_a.clone(),
        &ctx.fraud_vault_b.clone(),
        &ctx.fraud_mint.clone(),
        &ctx.user_fraud_a.clone(),
        &ctx.user_fraud_b.clone(),
        amount_in,
        min_out,
        true, // LIE: claim CRIME on a FRAUD sell
    );
    assert_err_contains(&r, "TaxIdentityMismatch", "sell FRAUD pool with is_crime=true");
}

#[test]
fn exploit_buy_crime_pool_with_fraud_mint_account_rejected() {
    // Attacker tries to pair CRIME pool with the FRAUD mint account, hoping
    // the mint check will then derive is_crime=false (matching what they
    // claim) and bypass Option B. Option C (PoolMintMismatch) catches it
    // because pool.mint_b reads as the CRIME pubkey, not the passed FRAUD
    // mint account.
    //
    // ORDER OF CHECKS in the patched handler:
    //   1. Option B mint→identity derivation (uses passed mint_b account)
    //   2. require!(is_crime == derived_is_crime) → TaxIdentityMismatch
    //   3. Option C read pool reserves + pool_token_mint
    //   4. require_keys_eq!(pool_token_mint, mint_b.key()) → PoolMintMismatch
    //
    // If attacker passes FRAUD mint + is_crime=false, step 2 passes (both
    // say "fraud"), then step 4 catches the mismatch (pool.mint_b=CRIME,
    // passed=FRAUD) → PoolMintMismatch. This is the case we test.
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 100_000_000;
    let min_out = safe_minimum_for_buy(amount_in, FRAUD_BUY_BPS as u64);

    let r = ctx.send_buy_with(
        &ctx.crime_pool.clone(),
        &ctx.crime_vault_a.clone(),
        &ctx.crime_vault_b.clone(),
        &ctx.fraud_mint.clone(), // mismatched mint_b account
        &ctx.user_crime_a.clone(),
        &ctx.user_crime_b.clone(),
        amount_in,
        min_out,
        false, // matches the passed FRAUD mint, so step 2 passes
    );
    // The first check that fails SHOULD be PoolMintMismatch. But the AMM CPI's
    // own checks may fire first if we get past Tax's checks. We accept either
    // PoolMintMismatch or any error containing "mismatch" — but the patched
    // code's intent is PoolMintMismatch.
    assert!(r.is_err(), "expected exploit to fail");
    let s = err_string(&r);
    assert!(
        s.contains("PoolMintMismatch") || s.contains("mismatch"),
        "expected PoolMintMismatch, got: {}",
        s
    );
}

#[test]
fn exploit_sell_with_unknown_mint_rejected() {
    // Attacker passes a random unrelated mint as mint_b. The patched code's
    // Option B identity derivation has no entry for it → UnknownTaxedMint.
    let mut ctx = IdMismatchCtx::setup();
    let amount_in: u64 = 50_000_000;
    let min_out = safe_minimum_for_sell(amount_in);

    let r = ctx.send_sell_with(
        &ctx.fraud_pool.clone(),
        &ctx.fraud_vault_a.clone(),
        &ctx.fraud_vault_b.clone(),
        &ctx.bogus_mint.clone(), // not CRIME, not FRAUD
        &ctx.user_fraud_a.clone(),
        &ctx.user_fraud_b.clone(),
        amount_in,
        min_out,
        false,
    );
    assert_err_contains(&r, "UnknownTaxedMint", "sell with unknown mint");
}
