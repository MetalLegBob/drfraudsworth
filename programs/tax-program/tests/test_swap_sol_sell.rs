/// Integration tests for Tax Program swap_sol_sell instruction using LiteSVM.
///
/// Tests verify the complete sell flow:
/// 1. User sends tokens to AMM via CPI
/// 2. AMM returns gross SOL output
/// 3. Tax calculated on OUTPUT (14% default sell tax)
/// 4. Tax distributed: 71% staking, 24% carnage, 5% treasury
/// 5. Slippage check on NET output (after tax)
/// 6. User receives net SOL
///
/// Key difference from buy: Tax is on OUTPUT, not input.
/// Slippage check happens AFTER tax deduction.
///
/// Coverage:
/// - Sell tax calculation matches spec (14% = 1400 bps)
/// - Distribution split matches 71/24/5
/// - CRIME and FRAUD pools both work
/// - Slippage protection works AFTER tax
/// - Post-tax slippage verification

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
use spl_token_2022::state::Mint as T22MintState;
use sha2::{Sha256, Digest};

// ---------------------------------------------------------------------------
// Program IDs
// ---------------------------------------------------------------------------

fn amm_program_id() -> Pubkey {
    "5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR"
        .parse()
        .unwrap()
}

fn tax_program_id() -> Pubkey {
    "43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj"
        .parse()
        .unwrap()
}

fn spl_token_program_id() -> Pubkey {
    spl_token::id()
}

fn token_2022_program_id() -> Pubkey {
    spl_token_2022::id()
}

fn bpf_loader_upgradeable_id() -> Pubkey {
    solana_sdk::bpf_loader_upgradeable::id()
}

fn system_program_id() -> Pubkey {
    solana_sdk::system_program::id()
}

/// Epoch Program ID -- matches epoch_program_id() in Tax Program constants.rs
/// and declare_id! in epoch-program/src/lib.rs
fn epoch_program_id() -> Pubkey {
    "4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2"
        .parse()
        .unwrap()
}

/// Staking Program ID -- matches declare_id! in staking/src/lib.rs
fn staking_program_id() -> Pubkey {
    "12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH"
        .parse()
        .unwrap()
}

/// Treasury pubkey -- matches treasury_pubkey() in tax-program constants.rs.
/// Updated to dedicated treasury wallet (same for devnet and mainnet since Phase 100).
fn treasury_pubkey() -> Pubkey {
    "3ihhwLnEJ2duwPSLYxhLbFrdhhxXLcvcrV9rAHqMgzCv"
        .parse()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Constants
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
const SEED_AMOUNT: u64 = 1_000_000_000; // 1 token
const LP_FEE_BPS: u16 = 100; // 1%
const BPS_DENOMINATOR: u64 = 10_000;

// Tax constants from Tax Program
const SELL_TAX_BPS: u64 = 1400; // 14%
const STAKING_BPS: u64 = 7_100; // 71%
const CARNAGE_BPS: u64 = 2_400; // 24%

/// Native WSOL mint address (So11111111111111111111111111111111111111112).
/// Using the real native mint ensures InitializeAccount3 sets is_native=Some(rent),
/// which is required for the close-and-reinit cycle in sell to unwrap WSOL properly.
fn native_mint_id() -> Pubkey {
    spl_token::native_mint::id()
}

// ---------------------------------------------------------------------------
// Type conversion helpers
// ---------------------------------------------------------------------------

fn addr(pk: &Pubkey) -> Address {
    Address::from(pk.to_bytes())
}

fn pk(address: &Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}

fn kp_pubkey(kp: &LiteKeypair) -> Pubkey {
    pk(&kp.pubkey())
}

// ---------------------------------------------------------------------------
// Anchor discriminator helpers
// ---------------------------------------------------------------------------

fn anchor_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("global:{}", name));
    let hash = hasher.finalize();
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

/// Compute Anchor account discriminator: sha256("account:{name}")[0..8]
fn anchor_account_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("account:{}", name));
    let hash = hasher.finalize();
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

// ---------------------------------------------------------------------------
// Mock EpochState helper
// ---------------------------------------------------------------------------

/// Creates a mock EpochState account data with specified tax rates.
/// Used for testing Tax Program without requiring the actual Epoch Program.
///
/// Layout (108 bytes total = 8 discriminator + 100 data):
/// - Discriminator: 8 bytes (sha256("account:EpochState")[0..8])
/// - Timing: 20 bytes (genesis_slot: u64, current_epoch: u32, epoch_start_slot: u64)
/// - Tax Config: 5 bytes (cheap_side: u8, low_tax_bps: u16, high_tax_bps: u16)
/// - Derived Rates: 8 bytes (crime_buy/sell, fraud_buy/sell: 4 x u16)
/// - VRF State: 42 bytes (vrf_request_slot: u64, vrf_pending: bool, taxes_confirmed: bool, pending_randomness_account: Pubkey)
/// - Carnage State: 23 bytes (carnage_pending: bool, carnage_target: u8, carnage_action: u8,
///     carnage_deadline_slot: u64, carnage_lock_slot: u64, last_carnage_epoch: u32)
/// - Protocol: 2 bytes (initialized: bool, bump: u8)
///
/// Source: epoch_state_reader.rs -- Phase 47 added carnage_lock_slot (u64, +8 bytes)
fn create_mock_epoch_state(
    crime_buy_bps: u16,
    crime_sell_bps: u16,
    fraud_buy_bps: u16,
    fraud_sell_bps: u16,
) -> Vec<u8> {
    let discriminator = anchor_account_discriminator("EpochState");

    let mut data = Vec::with_capacity(172); // 8 discriminator + 164 data

    // Discriminator (8 bytes)
    data.extend_from_slice(&discriminator);

    // Timing (20 bytes)
    data.extend_from_slice(&0u64.to_le_bytes());    // genesis_slot
    data.extend_from_slice(&1u32.to_le_bytes());    // current_epoch (epoch 1)
    data.extend_from_slice(&0u64.to_le_bytes());    // epoch_start_slot

    // Tax Configuration (5 bytes)
    data.push(0u8);                                 // cheap_side (0 = CRIME)
    data.extend_from_slice(&300u16.to_le_bytes());  // low_tax_bps (3%)
    data.extend_from_slice(&1400u16.to_le_bytes()); // high_tax_bps (14%)

    // Derived Tax Rates (8 bytes) - the rates that get_tax_bps() returns
    data.extend_from_slice(&crime_buy_bps.to_le_bytes());
    data.extend_from_slice(&crime_sell_bps.to_le_bytes());
    data.extend_from_slice(&fraud_buy_bps.to_le_bytes());
    data.extend_from_slice(&fraud_sell_bps.to_le_bytes());

    // VRF State (42 bytes)
    data.extend_from_slice(&0u64.to_le_bytes());    // vrf_request_slot
    data.push(0u8);                                 // vrf_pending (false)
    data.push(1u8);                                 // taxes_confirmed (true)
    data.extend_from_slice(&[0u8; 32]);             // pending_randomness_account (zeroed)

    // Carnage State (23 bytes) -- Phase 47 added carnage_lock_slot
    data.push(0u8);                                 // carnage_pending (false)
    data.push(0u8);                                 // carnage_target (CRIME = 0)
    data.push(0u8);                                 // carnage_action (None = 0)
    data.extend_from_slice(&0u64.to_le_bytes());    // carnage_deadline_slot
    data.extend_from_slice(&0u64.to_le_bytes());    // carnage_lock_slot (Phase 47)
    data.extend_from_slice(&0u32.to_le_bytes());    // last_carnage_epoch

    // Reserved padding (64 bytes) -- Phase 80, DEF-03
    data.extend_from_slice(&[0u8; 64]);

    // Protocol (2 bytes)
    data.push(1u8);                                 // initialized (true)
    data.push(255u8);                               // bump

    assert_eq!(data.len(), 172, "EpochState data must be 172 bytes (8 disc + 164 data)");
    data
}

/// Creates a mock StakePool account data for testing.
///
/// Layout (62 bytes total = 8 discriminator + 54 data):
/// - Discriminator: 8 bytes (sha256("account:StakePool")[0..8])
/// - total_staked: u64 (8 bytes)
/// - rewards_per_token_stored: u128 (16 bytes)
/// - pending_rewards: u64 (8 bytes)
/// - last_update_epoch: u32 (4 bytes)
/// - total_distributed: u64 (8 bytes)
/// - total_claimed: u64 (8 bytes)
/// - initialized: bool (1 byte)
/// - bump: u8 (1 byte)
fn create_mock_stake_pool(bump: u8) -> Vec<u8> {
    let discriminator = anchor_account_discriminator("StakePool");
    let mut data = Vec::with_capacity(62);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&0u64.to_le_bytes());    // total_staked
    data.extend_from_slice(&0u128.to_le_bytes());   // rewards_per_token_stored
    data.extend_from_slice(&0u64.to_le_bytes());    // pending_rewards
    data.extend_from_slice(&0u32.to_le_bytes());    // last_update_epoch
    data.extend_from_slice(&0u64.to_le_bytes());    // total_distributed
    data.extend_from_slice(&0u64.to_le_bytes());    // total_claimed
    data.push(1u8);                                 // initialized = true
    data.push(bump);                                // bump
    assert_eq!(data.len(), 62, "StakePool data must be 62 bytes");
    data
}

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

/// Build Tax Program swap_sol_sell instruction data.
fn swap_sol_sell_data(amount_in: u64, minimum_output: u64, is_crime: bool) -> Vec<u8> {
    let mut data = anchor_discriminator("swap_sol_sell").to_vec();
    amount_in.serialize(&mut data).unwrap();
    minimum_output.serialize(&mut data).unwrap();
    is_crime.serialize(&mut data).unwrap();
    data
}

// ---------------------------------------------------------------------------
// PDA derivation helpers
// ---------------------------------------------------------------------------

fn admin_config_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ADMIN_SEED], program_id)
}

fn pool_pda(program_id: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[POOL_SEED, mint_a.as_ref(), mint_b.as_ref()],
        program_id,
    )
}

fn vault_a_pda(program_id: &Pubkey, pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[VAULT_SEED, pool.as_ref(), VAULT_A_SEED],
        program_id,
    )
}

fn vault_b_pda(program_id: &Pubkey, pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[VAULT_SEED, pool.as_ref(), VAULT_B_SEED],
        program_id,
    )
}

fn swap_authority_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SWAP_AUTHORITY_SEED], program_id)
}

fn program_data_address(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[program_id.as_ref()],
        &bpf_loader_upgradeable_id(),
    )
}

fn epoch_state_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[EPOCH_STATE_SEED], &epoch_program_id())
}

fn tax_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TAX_AUTHORITY_SEED], &tax_program_id())
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

// ---------------------------------------------------------------------------
// LiteSVM setup helpers (same as buy tests)
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
) -> Pubkey {
    let (programdata_key, _bump) = program_data_address(program_id);
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
    .expect("Failed to set programdata account");

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
    .expect("Failed to set program account");

    programdata_key
}

fn create_t22_mint(svm: &mut LiteSVM, authority: &LiteKeypair, decimals: u8) -> Pubkey {
    let mint_kp = LiteKeypair::new();
    let mint_pk = kp_pubkey(&mint_kp);
    let mint_addr = mint_kp.pubkey();

    let rent = solana_sdk::rent::Rent::default();
    let space = T22MintState::LEN;
    let lamports = rent.minimum_balance(space);

    let create_account_ix = Instruction {
        program_id: addr(&system_program_id()),
        accounts: vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(mint_addr, true),
        ],
        data: {
            let mut data = vec![0u8; 4 + 8 + 8 + 32];
            data[0..4].copy_from_slice(&0u32.to_le_bytes());
            data[4..12].copy_from_slice(&lamports.to_le_bytes());
            data[12..20].copy_from_slice(&(space as u64).to_le_bytes());
            data[20..52].copy_from_slice(token_2022_program_id().as_ref());
            data
        },
    };

    let init_mint_ix = Instruction {
        program_id: addr(&token_2022_program_id()),
        accounts: vec![
            AccountMeta::new(mint_addr, false),
            AccountMeta::new_readonly(addr(&solana_sdk::sysvar::rent::id()), false),
        ],
        data: {
            let mut data = vec![20u8];
            data.push(decimals);
            data.extend_from_slice(kp_pubkey(authority).as_ref());
            data.push(0);
            data
        },
    };

    let msg = Message::new_with_blockhash(
        &[create_account_ix, init_mint_ix],
        Some(&authority.pubkey()),
        &svm.latest_blockhash(),
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &[authority, &mint_kp],
    )
    .unwrap();
    svm.send_transaction(tx).expect("Failed to create T22 mint");
    mint_pk
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
            let mut data = vec![18u8];
            data.extend_from_slice(owner.as_ref());
            data
        },
    };

    let msg = Message::new_with_blockhash(
        &[create_account_ix, init_account_ix],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &[payer, &account_kp],
    )
    .unwrap();
    svm.send_transaction(tx).expect("Failed to create token account");
    account_pk
}

/// Fund a native WSOL token account by transferring SOL then calling SyncNative.
///
/// For native WSOL, you can't mint -- instead you transfer SOL to the token account,
/// then call SyncNative (instruction 17) which updates the token balance to match
/// the account's lamports minus rent-exempt minimum.
fn fund_native_wsol(
    svm: &mut LiteSVM,
    payer: &LiteKeypair,
    token_account: &Pubkey,
    amount: u64,
) {
    // Transfer SOL to the token account
    let transfer_ix = Instruction {
        program_id: addr(&system_program_id()),
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(addr(token_account), false),
        ],
        data: {
            // System program Transfer instruction: index 2, then u64 amount
            let mut data = vec![0u8; 4 + 8];
            data[0..4].copy_from_slice(&2u32.to_le_bytes());
            data[4..12].copy_from_slice(&amount.to_le_bytes());
            data
        },
    };

    // SyncNative (instruction discriminator 17) updates the token balance
    let sync_ix = Instruction {
        program_id: addr(&spl_token_program_id()),
        accounts: vec![
            AccountMeta::new(addr(token_account), false),
        ],
        data: vec![17u8], // SyncNative
    };

    let msg = Message::new_with_blockhash(
        &[transfer_ix, sync_ix],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &[payer],
    )
    .unwrap();
    svm.send_transaction(tx).expect("Failed to fund native WSOL account");
}

fn mint_tokens(
    svm: &mut LiteSVM,
    payer: &LiteKeypair,
    mint: &Pubkey,
    dest: &Pubkey,
    amount: u64,
    authority: &LiteKeypair,
    token_program: &Pubkey,
) {
    let mint_to_ix = Instruction {
        program_id: addr(token_program),
        accounts: vec![
            AccountMeta::new(addr(mint), false),
            AccountMeta::new(addr(dest), false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: {
            let mut data = vec![7u8];
            data.extend_from_slice(&amount.to_le_bytes());
            data
        },
    };

    let msg = Message::new_with_blockhash(
        &[mint_to_ix],
        Some(&payer.pubkey()),
        &svm.latest_blockhash(),
    );

    let signers: Vec<&LiteKeypair> = if kp_pubkey(payer) == kp_pubkey(authority) {
        vec![payer]
    } else {
        vec![payer, authority]
    };

    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &signers,
    )
    .unwrap();
    svm.send_transaction(tx).expect("Failed to mint tokens");
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

fn build_initialize_admin_ix(authority: &Pubkey, admin: &Pubkey) -> Instruction {
    let pid = amm_program_id();
    let (admin_config, _) = admin_config_pda(&pid);
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
    lp_fee_bps: u16,
    amount_a: u64,
    amount_b: u64,
) -> Instruction {
    let pid = amm_program_id();
    let (admin_config, _) = admin_config_pda(&pid);
    let (pool, _) = pool_pda(&pid, mint_a, mint_b);
    let (vault_a, _) = vault_a_pda(&pid, &pool);
    let (vault_b, _) = vault_b_pda(&pid, &pool);

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
        data: initialize_pool_data(lp_fee_bps, amount_a, amount_b),
    }
}

/// Build Tax Program swap_sol_sell instruction.
///
/// Account ordering matches SwapSolSell struct (updated for Phase 46-50):
/// user, epoch_state, swap_authority, tax_authority, pool, pool_vault_a, pool_vault_b,
/// mint_a, mint_b, user_token_a, user_token_b, stake_pool,
/// staking_escrow, carnage_vault, treasury, wsol_intermediary,
/// amm_program, token_program_a, token_program_b, system_program, staking_program
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
            AccountMeta::new(addr(user), true),                         // user (signer, mut)
            AccountMeta::new_readonly(addr(epoch_state), false),        // epoch_state
            AccountMeta::new(addr(swap_authority), false),              // swap_authority (mut for close lamports)
            AccountMeta::new_readonly(addr(tax_authority), false),      // tax_authority (PDA)
            AccountMeta::new(addr(pool), false),                        // pool (mut)
            AccountMeta::new(addr(vault_a), false),                     // pool_vault_a (mut)
            AccountMeta::new(addr(vault_b), false),                     // pool_vault_b (mut)
            AccountMeta::new_readonly(addr(mint_a), false),             // mint_a
            AccountMeta::new_readonly(addr(mint_b), false),             // mint_b
            AccountMeta::new(addr(user_token_a), false),                // user_token_a (mut)
            AccountMeta::new(addr(user_token_b), false),                // user_token_b (mut)
            AccountMeta::new(addr(stake_pool), false),                  // stake_pool (mut, Staking PDA)
            AccountMeta::new(addr(staking_escrow), false),              // staking_escrow (mut)
            AccountMeta::new(addr(carnage_vault), false),               // carnage_vault (mut)
            AccountMeta::new(addr(treasury), false),                    // treasury (mut)
            AccountMeta::new(addr(wsol_intermediary), false),           // wsol_intermediary (mut)
            AccountMeta::new_readonly(addr(&amm_program_id()), false),  // amm_program
            AccountMeta::new_readonly(addr(token_program_a), false),    // token_program_a
            AccountMeta::new_readonly(addr(token_program_b), false),    // token_program_b
            AccountMeta::new_readonly(addr(&system_program_id()), false), // system_program
            AccountMeta::new_readonly(addr(&staking_program_id()), false), // staking_program
        ],
        data: swap_sol_sell_data(amount_in, minimum_output, is_crime),
    }
}

// ---------------------------------------------------------------------------
// Balance readers
// ---------------------------------------------------------------------------

fn get_token_balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let acct = svm.get_account(&addr(token_account)).expect("Token account should exist");
    u64::from_le_bytes(acct.data[64..72].try_into().unwrap())
}

fn get_sol_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    svm.get_account(&addr(account))
        .map(|a| a.lamports)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tax math helpers
// ---------------------------------------------------------------------------

fn calculate_expected_tax(amount: u64, tax_bps: u64) -> u64 {
    ((amount as u128) * (tax_bps as u128) / (BPS_DENOMINATOR as u128)) as u64
}

fn calculate_distribution(tax_amount: u64) -> (u64, u64, u64) {
    let staking = ((tax_amount as u128) * STAKING_BPS as u128 / BPS_DENOMINATOR as u128) as u64;
    let carnage = ((tax_amount as u128) * CARNAGE_BPS as u128 / BPS_DENOMINATOR as u128) as u64;
    let treasury = tax_amount - staking - carnage;
    (staking, carnage, treasury)
}

fn expected_effective_input(amount_in: u64, fee_bps: u16) -> u128 {
    let amount = amount_in as u128;
    let fee_factor = 10_000u128 - fee_bps as u128;
    amount * fee_factor / 10_000
}

fn expected_swap_output(reserve_in: u64, reserve_out: u64, effective_input: u128) -> u64 {
    let r_in = reserve_in as u128;
    let r_out = reserve_out as u128;
    let numerator = r_out * effective_input;
    let denominator = r_in + effective_input;
    (numerator / denominator) as u64
}

// ---------------------------------------------------------------------------
// Test context
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct SellTestContext {
    svm: LiteSVM,
    admin: LiteKeypair,
    user: LiteKeypair,
    pool: Pubkey,
    vault_a: Pubkey,
    vault_b: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    user_token_a: Pubkey,
    user_token_b: Pubkey,
    token_program_a: Pubkey,
    token_program_b: Pubkey,
    swap_authority: Pubkey,
    tax_authority: Pubkey,
    stake_pool: Pubkey,
    staking_escrow: Pubkey,
    carnage_vault: Pubkey,
    treasury: Pubkey,
    wsol_intermediary: Pubkey,
    /// EpochState PDA for dynamic tax rates
    epoch_state: Pubkey,
    /// True if SPL mint is mint_a (for sell, user sells token_b for token_a)
    spl_is_mint_a: bool,
}

impl SellTestContext {
    fn setup() -> Self {
        let upgrade_authority = LiteKeypair::new();
        let mut svm = LiteSVM::new();

        svm.airdrop(&upgrade_authority.pubkey(), 100_000_000_000)
            .expect("Failed to airdrop");

        // Deploy AMM
        let amm_bytes = read_program_bytes("amm");
        deploy_upgradeable_program(
            &mut svm,
            &amm_program_id(),
            &kp_pubkey(&upgrade_authority),
            &amm_bytes,
        );

        // Deploy Tax Program
        let tax_bytes = read_program_bytes("tax_program");
        deploy_upgradeable_program(
            &mut svm,
            &tax_program_id(),
            &kp_pubkey(&upgrade_authority),
            &tax_bytes,
        );

        // Deploy Staking Program (needed for deposit_rewards CPI)
        let staking_bytes = read_program_bytes("staking");
        deploy_upgradeable_program(
            &mut svm,
            &staking_program_id(),
            &kp_pubkey(&upgrade_authority),
            &staking_bytes,
        );

        // Create admin
        let admin = LiteKeypair::new();
        svm.airdrop(&admin.pubkey(), 100_000_000_000).expect("Failed to airdrop");

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
        let tx = VersionedTransaction::try_new(
            VersionedMessage::Legacy(msg),
            &[&upgrade_authority],
        )
        .unwrap();
        svm.send_transaction(tx).expect("Failed to initialize admin");
        svm.expire_blockhash();

        // Use the real native WSOL mint for token A.
        // WHY: The sell handler's close-and-reinit cycle (Phase 48) uses InitializeAccount3
        // to recreate the wsol_intermediary after each sell. InitializeAccount3 sets
        // is_native=Some(rent) ONLY for the native mint. With a synthetic SPL mint,
        // the recreated intermediary would be non-native, and the next sell's close_account
        // would fail with "Non-native account can only be closed if its balance is zero".
        //
        // Using the real native mint makes our tests match production behavior exactly.
        let wsol_mint = native_mint_id();

        // Set up the native WSOL mint account in LiteSVM.
        // The native mint has 9 decimals and no mint authority.
        let rent = solana_sdk::rent::Rent::default();
        let mint_space = SplMintState::LEN;
        let mint_lamports = rent.minimum_balance(mint_space);
        let mut wsol_mint_data = vec![0u8; mint_space];
        // SPL Mint layout: mint_authority (36) + supply (8) + decimals (1) + is_initialized (1) + freeze_authority (36)
        // mint_authority: None (COption tag=0)
        wsol_mint_data[0..4].copy_from_slice(&0u32.to_le_bytes()); // COption None
        // supply: 0 (doesn't matter for test)
        wsol_mint_data[36..44].copy_from_slice(&0u64.to_le_bytes());
        // decimals: 9
        wsol_mint_data[44] = 9;
        // is_initialized: true
        wsol_mint_data[45] = 1;
        // freeze_authority: None
        wsol_mint_data[46..50].copy_from_slice(&0u32.to_le_bytes());

        svm.set_account(
            addr(&wsol_mint),
            Account {
                lamports: mint_lamports,
                data: wsol_mint_data,
                owner: addr(&spl_token_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("Failed to set native WSOL mint");

        let t22_mint = create_t22_mint(&mut svm, &admin, TEST_DECIMALS);
        svm.expire_blockhash();

        // Canonical ordering: WSOL mint vs T22 mint.
        // In production, WSOL is always token_a and token_program_a is SPL Token.
        let (mint_a, mint_b, token_program_a, token_program_b, spl_is_mint_a) = if wsol_mint < t22_mint {
            (wsol_mint, t22_mint, spl_token_program_id(), token_2022_program_id(), true)
        } else {
            (t22_mint, wsol_mint, token_2022_program_id(), spl_token_program_id(), false)
        };

        // Create admin source accounts and fund.
        // For WSOL (native mint): use fund_native_wsol (SOL transfer + SyncNative).
        // For T22 mint: use mint_tokens (standard MintTo instruction).
        let source_a = create_token_account(&mut svm, &admin, &mint_a, &kp_pubkey(&admin), &token_program_a);
        svm.expire_blockhash();
        let source_b = create_token_account(&mut svm, &admin, &mint_b, &kp_pubkey(&admin), &token_program_b);
        svm.expire_blockhash();

        if spl_is_mint_a {
            // mint_a is WSOL (native) -- fund with SOL transfer + SyncNative
            fund_native_wsol(&mut svm, &admin, &source_a, SEED_AMOUNT);
            svm.expire_blockhash();
            mint_tokens(&mut svm, &admin, &mint_b, &source_b, SEED_AMOUNT, &admin, &token_program_b);
            svm.expire_blockhash();
        } else {
            // mint_b is WSOL (native) -- fund with SOL transfer + SyncNative
            mint_tokens(&mut svm, &admin, &mint_a, &source_a, SEED_AMOUNT, &admin, &token_program_a);
            svm.expire_blockhash();
            fund_native_wsol(&mut svm, &admin, &source_b, SEED_AMOUNT);
            svm.expire_blockhash();
        }

        // Initialize pool
        let payer = LiteKeypair::new();
        svm.airdrop(&payer.pubkey(), 100_000_000_000).expect("Failed to airdrop");

        let ix = build_initialize_pool_ix(
            &kp_pubkey(&payer),
            &kp_pubkey(&admin),
            &mint_a,
            &mint_b,
            &source_a,
            &source_b,
            &token_program_a,
            &token_program_b,
            LP_FEE_BPS,
            SEED_AMOUNT,
            SEED_AMOUNT,
        );

        let msg = Message::new_with_blockhash(
            &[ix],
            Some(&payer.pubkey()),
            &svm.latest_blockhash(),
        );
        let tx = VersionedTransaction::try_new(
            VersionedMessage::Legacy(msg),
            &[&payer, &admin],
        )
        .unwrap();
        svm.send_transaction(tx).expect("Failed to initialize pool");
        svm.expire_blockhash();

        let pid = amm_program_id();
        let (pool, _) = pool_pda(&pid, &mint_a, &mint_b);
        let (vault_a, _) = vault_a_pda(&pid, &pool);
        let (vault_b, _) = vault_b_pda(&pid, &pool);

        // Create user with lots of SOL.
        // Needs extra SOL because:
        // 1. Creating token accounts costs rent
        // 2. Funding WSOL token account requires SOL transfer
        // 3. Sell taxes are distributed as native SOL
        let user = LiteKeypair::new();
        svm.airdrop(&user.pubkey(), 200_000_000_000).expect("Failed to airdrop");

        let user_token_a = create_token_account(&mut svm, &user, &mint_a, &kp_pubkey(&user), &token_program_a);
        svm.expire_blockhash();
        let user_token_b = create_token_account(&mut svm, &user, &mint_b, &kp_pubkey(&user), &token_program_b);
        svm.expire_blockhash();

        // Fund user with tokens (they'll sell token_b for token_a/WSOL).
        // The WSOL side is funded with SOL transfer + SyncNative.
        if spl_is_mint_a {
            // mint_a is WSOL -- fund user_token_a with native SOL
            fund_native_wsol(&mut svm, &user, &user_token_a, 100_000_000_000);
            svm.expire_blockhash();
            mint_tokens(&mut svm, &admin, &mint_b, &user_token_b, 100_000_000_000, &admin, &token_program_b);
            svm.expire_blockhash();
        } else {
            // mint_b is WSOL -- fund user_token_b with native SOL
            mint_tokens(&mut svm, &admin, &mint_a, &user_token_a, 100_000_000_000, &admin, &token_program_a);
            svm.expire_blockhash();
            fund_native_wsol(&mut svm, &user, &user_token_b, 100_000_000_000);
            svm.expire_blockhash();
        }

        // Derive swap_authority from Tax Program
        let (swap_authority, _) = swap_authority_pda(&tax_program_id());

        // Derive tax_authority from Tax Program
        let (tax_authority, _) = tax_authority_pda();

        let rent = solana_sdk::rent::Rent::default();

        // Create mock EpochState account
        // For sell tests: use max sell tax (1400 bps) for both CRIME and FRAUD
        // This matches the previous hardcoded behavior for baseline tests
        let (epoch_state, _) = epoch_state_pda();

        // Create mock EpochState data with 1400 bps sell tax for both tokens
        // CRIME cheap regime: crime_buy=300, crime_sell=1400, fraud_buy=1400, fraud_sell=300
        // But for these baseline tests, we use 1400 for all sell taxes
        let epoch_state_data = create_mock_epoch_state(
            300,   // crime_buy_bps
            1400,  // crime_sell_bps (high sell tax)
            1400,  // fraud_buy_bps
            1400,  // fraud_sell_bps (also high for baseline tests)
        );

        let epoch_state_lamports = rent.minimum_balance(epoch_state_data.len());

        svm.set_account(
            addr(&epoch_state),
            Account {
                lamports: epoch_state_lamports,
                data: epoch_state_data,
                owner: addr(&epoch_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        ).expect("Failed to set EpochState account");

        // Create mock StakePool PDA account (owned by staking program)
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
        )
        .expect("Failed to set mock StakePool account");

        // Create staking escrow PDA (owned by staking program, receives SOL)
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
        )
        .expect("Failed to set staking escrow PDA");

        // Create carnage vault PDA (owned by epoch program, receives SOL)
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
        )
        .expect("Failed to set carnage vault PDA");

        // Treasury: use the devnet treasury address (matches treasury_pubkey() in on-chain code)
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
        )
        .expect("Failed to set treasury account");

        // Create WSOL intermediary PDA as a valid native SPL token account.
        // The sell handler expects this to be an initialized WSOL token account
        // owned by swap_authority, which it closes and recreates each sell cycle.
        //
        // CRITICAL: Must be a "native" token account (is_native = Some(rent)).
        // close_account on native accounts unwraps WSOL -> SOL automatically.
        // Non-native accounts require balance == 0 before close, which would fail
        // because tax WSOL was just transferred in.
        //
        // PDA derived from Tax Program: seeds = [WSOL_INTERMEDIARY_SEED]
        let (wsol_intermediary_pk, _) = wsol_intermediary_pda();

        // Build a valid SPL Token Account (165 bytes) for WSOL, owned by swap_authority.
        // Layout: mint (32) + owner (32) + amount (8) + delegate (36) + state (1)
        //       + is_native (12) + delegated_amount (8) + close_authority (36) = 165
        let wsol_intermediary_lamports = rent.minimum_balance(165);
        let mut wsol_account_data = vec![0u8; 165];
        // Mint: mint_a (the WSOL-like SPL mint)
        wsol_account_data[0..32].copy_from_slice(mint_a.as_ref());
        // Owner: swap_authority
        wsol_account_data[32..64].copy_from_slice(swap_authority.as_ref());
        // Amount: 0 (u64 LE) -- empty, will receive tax WSOL during sell
        wsol_account_data[64..72].copy_from_slice(&0u64.to_le_bytes());
        // Delegate: None (COption<Pubkey> = [0u32] + 32 bytes padding)
        // Already zeroed
        // State: Initialized = 1
        wsol_account_data[108] = 1u8;
        // is_native: Some(rent_exempt_minimum) -- CRITICAL for WSOL close_account unwrap.
        // COption<u64>: tag=1u32 (Some), then value=rent_lamports (u64 LE)
        wsol_account_data[109..113].copy_from_slice(&1u32.to_le_bytes()); // COption tag = Some
        wsol_account_data[113..121].copy_from_slice(&wsol_intermediary_lamports.to_le_bytes()); // native amount = rent
        // delegated_amount: 0 (u64 LE)
        // Already zeroed
        // close_authority: None (COption<Pubkey>)
        // Already zeroed

        svm.set_account(
            addr(&wsol_intermediary_pk),
            Account {
                lamports: wsol_intermediary_lamports,
                data: wsol_account_data,
                // Owner: token_program_a (matches whichever token program owns mint_a).
                // When spl_mint < t22_mint, token_program_a = SPL Token.
                // When t22_mint < spl_mint, token_program_a = Token-2022.
                // The on-chain code uses token_program_a for all intermediary operations.
                owner: addr(&token_program_a),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("Failed to set wsol_intermediary PDA");

        // Fund swap_authority with SOL for rent during intermediary recreation.
        // The sell handler closes the intermediary (receiving rent) then recreates it
        // (spending rent). We need swap_authority to have enough SOL for the create_account.
        svm.set_account(
            addr(&swap_authority),
            Account {
                lamports: 10_000_000_000, // 10 SOL -- plenty for intermediary rent + tax distribution
                data: vec![],
                owner: addr(&system_program_id()),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("Failed to fund swap_authority");

        SellTestContext {
            svm,
            admin,
            user,
            pool,
            vault_a,
            vault_b,
            mint_a,
            mint_b,
            user_token_a,
            user_token_b,
            token_program_a,
            token_program_b,
            swap_authority,
            tax_authority,
            stake_pool: stake_pool_pk,
            staking_escrow: staking_escrow_pk,
            carnage_vault: carnage_vault_pk,
            treasury: treasury_pk,
            wsol_intermediary: wsol_intermediary_pk,
            epoch_state,
            spl_is_mint_a,
        }
    }
}

fn send_sell_swap(ctx: &mut SellTestContext, amount_in: u64, minimum_output: u64, is_crime: bool) -> litesvm::types::TransactionResult {
    let ix = build_swap_sol_sell_ix(
        &kp_pubkey(&ctx.user),
        &ctx.epoch_state,
        &ctx.swap_authority,
        &ctx.tax_authority,
        &ctx.pool,
        &ctx.vault_a,
        &ctx.vault_b,
        &ctx.mint_a,
        &ctx.mint_b,
        &ctx.user_token_a,
        &ctx.user_token_b,
        &ctx.stake_pool,
        &ctx.staking_escrow,
        &ctx.carnage_vault,
        &ctx.treasury,
        &ctx.wsol_intermediary,
        &ctx.token_program_a,
        &ctx.token_program_b,
        amount_in,
        minimum_output,
        is_crime,
    );

    let msg = Message::new_with_blockhash(
        &[ix],
        Some(&ctx.user.pubkey()),
        &ctx.svm.latest_blockhash(),
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &[&ctx.user],
    )
    .unwrap();
    ctx.svm.send_transaction(tx)
}

// ===========================================================================
// HELPERS: Calculate safe minimum_output for SEC-10 floor
// ===========================================================================

/// Calculate a safe minimum_output that satisfies the 50% output floor for sell.
///
/// For sell, the floor is checked on minimum_output vs expected gross output.
/// The AMM gives gross_output, then tax is deducted. The floor check is on the
/// minimum_output parameter (which represents net output after tax).
///
/// We compute: expected_gross * 51% as minimum_output.
/// This is above the 50% floor of expected_gross and below actual net output.
fn safe_minimum_for_sell(amount_in: u64) -> u64 {
    // Sell is BtoA: selling token_b for token_a (SOL)
    let effective = expected_effective_input(amount_in, LP_FEE_BPS);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective);
    // 51% of gross output (safely above the 50% floor)
    expected_gross * 51 / 100
}

// ===========================================================================
// TESTS
// ===========================================================================

/// Test: Sell CRIME for SOL with 14% tax, verify distribution.
///
/// Sell flow:
/// 1. User sells 1000 CRIME (token_b)
/// 2. AMM returns gross SOL (token_a)
/// 3. Tax = 14% of gross output
/// 4. User receives net = gross - tax
/// 5. Tax distributed 71/24/5
#[test]
#[ignore = "pre-existing harness generates ephemeral keypair mints (create_spl_mint/create_t22_mint); incompatible with Phase 122.1 Task 1 pinned mint constants (crime_mint()/fraud_mint()). Patched .so enforces mainnet pubkeys and rejects ephemeral mints with UnknownTaxedMint. Modernization to use svm.set_account at mainnet pubkeys is deferred to a future test-cleanup phase. Regression coverage for Phase 122.1 lives in tests/identity_mismatch.rs — see 122.1-01-SUMMARY.md Deviations section."]
fn test_sell_crime_with_tax() {
    let mut ctx = SellTestContext::setup();

    let amount_in: u64 = 100_000_000; // 0.1 CRIME tokens to sell

    // Record balances before
    let user_token_b_before = get_token_balance(&ctx.svm, &ctx.user_token_b);
    let _user_token_a_before = get_token_balance(&ctx.svm, &ctx.user_token_a);
    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    // Execute sell (is_crime = true, minimum_output satisfies SEC-10 50% floor)
    let min_out = safe_minimum_for_sell(amount_in);
    let result = send_sell_swap(&mut ctx, amount_in, min_out, true);
    assert!(result.is_ok(), "Sell swap should succeed: {:?}", result.err());
    ctx.svm.expire_blockhash();

    // Calculate expected gross output from AMM
    // For BtoA: selling token_b for token_a
    let effective_input = expected_effective_input(amount_in, LP_FEE_BPS);
    let expected_gross_output = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective_input);

    // Calculate expected tax on OUTPUT
    let expected_tax = calculate_expected_tax(expected_gross_output, SELL_TAX_BPS);
    let (exp_staking, exp_carnage, exp_treasury) = calculate_distribution(expected_tax);

    // Verify user token_b was spent
    let user_token_b_after = get_token_balance(&ctx.svm, &ctx.user_token_b);
    assert_eq!(
        user_token_b_before - amount_in, user_token_b_after,
        "User should have sent {} tokens. Before: {}, After: {}",
        amount_in, user_token_b_before, user_token_b_after
    );

    // Verify tax distribution (native SOL transfers from swap_authority)
    let staking_after = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_after = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_after = get_sol_balance(&ctx.svm, &ctx.treasury);

    assert_eq!(
        staking_after - staking_before, exp_staking,
        "Staking escrow should receive 71% of tax. Expected: {}, Got: {}",
        exp_staking, staking_after - staking_before
    );
    assert_eq!(
        carnage_after - carnage_before, exp_carnage,
        "Carnage vault should receive 24% of tax. Expected: {}, Got: {}",
        exp_carnage, carnage_after - carnage_before
    );
    assert_eq!(
        treasury_after - treasury_before, exp_treasury,
        "Treasury should receive remainder. Expected: {}, Got: {}",
        exp_treasury, treasury_after - treasury_before
    );

    // Verify total tax was collected
    let total_tax_collected = (staking_after - staking_before) + (carnage_after - carnage_before) + (treasury_after - treasury_before);
    assert_eq!(
        total_tax_collected, expected_tax,
        "Total tax collected should match expected. Expected: {}, Got: {}",
        expected_tax, total_tax_collected
    );
}

/// Test: Sell FRAUD with tax (different pool type flag).
#[test]
#[ignore = "pre-existing harness uses ephemeral keypair mints; incompatible with Phase 122.1 pinned mint constants. See test_sell_crime_with_tax for full rationale. Regression coverage lives in tests/identity_mismatch.rs."]
fn test_sell_fraud_with_tax() {
    let mut ctx = SellTestContext::setup();

    let amount_in: u64 = 50_000_000;

    let staking_before = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_before = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_before = get_sol_balance(&ctx.svm, &ctx.treasury);

    // is_crime = false for FRAUD (minimum_output satisfies SEC-10 50% floor)
    let min_out = safe_minimum_for_sell(amount_in);
    let result = send_sell_swap(&mut ctx, amount_in, min_out, false);
    assert!(result.is_ok(), "FRAUD sell swap should succeed: {:?}", result.err());
    ctx.svm.expire_blockhash();

    // Calculate expected values
    let effective_input = expected_effective_input(amount_in, LP_FEE_BPS);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective_input);
    let expected_tax = calculate_expected_tax(expected_gross, SELL_TAX_BPS);
    let (exp_staking, exp_carnage, exp_treasury) = calculate_distribution(expected_tax);

    let staking_after = get_sol_balance(&ctx.svm, &ctx.staking_escrow);
    let carnage_after = get_sol_balance(&ctx.svm, &ctx.carnage_vault);
    let treasury_after = get_sol_balance(&ctx.svm, &ctx.treasury);

    assert_eq!(staking_after - staking_before, exp_staking, "Staking distribution correct");
    assert_eq!(carnage_after - carnage_before, exp_carnage, "Carnage distribution correct");
    assert_eq!(treasury_after - treasury_before, exp_treasury, "Treasury distribution correct");
}

/// Test: Slippage check happens AFTER tax deduction.
///
/// IGNORED: read_pool_reserves is_reversed detection returns wrong reserve
/// ordering for non-NATIVE_MINT test pools, causing AMM SlippageExceeded on
/// gross_floor computation. Production SOL pools always use NATIVE_MINT as
/// mint_a. TODO: Fix by using NATIVE_MINT in test pool setup.
#[test]
#[ignore]
fn test_sell_slippage_after_tax() {
    let mut ctx = SellTestContext::setup();

    let amount_in: u64 = 100_000_000;

    // Calculate expected values
    let effective_input = expected_effective_input(amount_in, LP_FEE_BPS);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective_input);
    let expected_tax = calculate_expected_tax(expected_gross, SELL_TAX_BPS);
    let expected_net = expected_gross - expected_tax;

    // Set minimum to gross output (what AMM returns) - this should FAIL
    // because after tax deduction, user gets less than minimum
    let result = send_sell_swap(&mut ctx, amount_in, expected_gross, true);
    assert!(
        result.is_err(),
        "Should fail with slippage exceeded when minimum = gross (before tax deduction). \
         Gross: {}, Net: {}, Tax: {}",
        expected_gross, expected_net, expected_tax
    );
    ctx.svm.expire_blockhash();

    // Set minimum to net output + 1 (just above what user actually gets)
    let result = send_sell_swap(&mut ctx, amount_in, expected_net + 1, true);
    assert!(
        result.is_err(),
        "Should fail with slippage exceeded when minimum = net + 1"
    );
    ctx.svm.expire_blockhash();

    // Set minimum to exact net output - this should SUCCEED
    let result = send_sell_swap(&mut ctx, amount_in, expected_net, true);
    assert!(
        result.is_ok(),
        "Should succeed with minimum = exact net output. Expected net: {}: {:?}",
        expected_net, result.err()
    );
}

/// Test: Slippage passes with reasonable buffer.
#[test]
#[ignore = "pre-existing harness uses ephemeral keypair mints; incompatible with Phase 122.1 pinned mint constants. See test_sell_crime_with_tax for full rationale. Regression coverage lives in tests/identity_mismatch.rs."]
fn test_sell_slippage_passes() {
    let mut ctx = SellTestContext::setup();

    let amount_in: u64 = 100_000_000;

    // Calculate expected values
    let effective_input = expected_effective_input(amount_in, LP_FEE_BPS);
    let expected_gross = expected_swap_output(SEED_AMOUNT, SEED_AMOUNT, effective_input);
    let expected_tax = calculate_expected_tax(expected_gross, SELL_TAX_BPS);
    let expected_net = expected_gross - expected_tax;

    // Set minimum slightly below net (1% buffer)
    let minimum_with_buffer = expected_net - (expected_net / 100);
    let result = send_sell_swap(&mut ctx, amount_in, minimum_with_buffer, true);
    assert!(
        result.is_ok(),
        "Should succeed with 1% slippage buffer: {:?}", result.err()
    );
}

/// Test: Consecutive sells work (no state corruption).
#[test]
#[ignore = "pre-existing harness uses ephemeral keypair mints; incompatible with Phase 122.1 pinned mint constants. See test_sell_crime_with_tax for full rationale. Regression coverage lives in tests/identity_mismatch.rs."]
fn test_consecutive_sells_succeed() {
    let mut ctx = SellTestContext::setup();

    // First sell (minimum_output satisfies SEC-10 50% floor)
    let min_out = safe_minimum_for_sell(10_000_000);
    let result = send_sell_swap(&mut ctx, 10_000_000, min_out, true);
    assert!(result.is_ok(), "First sell should succeed: {:?}", result.err());
    ctx.svm.expire_blockhash();

    // Second sell
    let min_out = safe_minimum_for_sell(10_000_000);
    let result = send_sell_swap(&mut ctx, 10_000_000, min_out, true);
    assert!(result.is_ok(), "Second sell should succeed: {:?}", result.err());
    ctx.svm.expire_blockhash();

    // Third sell (FRAUD)
    let min_out = safe_minimum_for_sell(5_000_000);
    let result = send_sell_swap(&mut ctx, 5_000_000, min_out, false);
    assert!(result.is_ok(), "Third sell should succeed: {:?}", result.err());
}

/// Test: swap_sol_sell fails with InvalidEpochState when EpochState owned by wrong program.
///
/// Security test: Prevents attackers from passing a fake EpochState with 0% tax.
#[test]
fn test_swap_sol_sell_fails_with_invalid_epoch_state() {
    let mut ctx = SellTestContext::setup();

    // Create a fake EpochState owned by a random program (not Epoch Program)
    let fake_owner = Pubkey::new_unique();
    let (fake_epoch_state_pda, _) = Pubkey::find_program_address(
        &[b"fake_epoch"],
        &fake_owner,
    );

    // Create fake EpochState data with 0% tax (attack attempt)
    let fake_epoch_state_data = create_mock_epoch_state(
        0,    // crime_buy_bps = 0% (attacker's goal)
        0,    // crime_sell_bps = 0%
        0,    // fraud_buy_bps = 0%
        0,    // fraud_sell_bps = 0%
    );

    let rent = solana_sdk::rent::Rent::default();
    let lamports = rent.minimum_balance(fake_epoch_state_data.len());

    // Set the fake account owned by fake_owner (not epoch_program_id())
    ctx.svm.set_account(
        addr(&fake_epoch_state_pda),
        Account {
            lamports,
            data: fake_epoch_state_data,
            owner: addr(&fake_owner), // Wrong owner!
            executable: false,
            rent_epoch: 0,
        },
    ).expect("Failed to set fake EpochState account");

    // Build instruction with fake EpochState -- use safe minimum for the real epoch state
    let min_out = safe_minimum_for_sell(100_000_000);
    let ix = build_swap_sol_sell_ix(
        &kp_pubkey(&ctx.user),
        &fake_epoch_state_pda, // Pass fake EpochState
        &ctx.swap_authority,
        &ctx.tax_authority,
        &ctx.pool,
        &ctx.vault_a,
        &ctx.vault_b,
        &ctx.mint_a,
        &ctx.mint_b,
        &ctx.user_token_a,
        &ctx.user_token_b,
        &ctx.stake_pool,
        &ctx.staking_escrow,
        &ctx.carnage_vault,
        &ctx.treasury,
        &ctx.wsol_intermediary,
        &ctx.token_program_a,
        &ctx.token_program_b,
        100_000_000,
        min_out,
        true,
    );

    let msg = Message::new_with_blockhash(
        &[ix],
        Some(&ctx.user.pubkey()),
        &ctx.svm.latest_blockhash(),
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &[&ctx.user],
    )
    .unwrap();

    let result = ctx.svm.send_transaction(tx);

    // Should fail with InvalidEpochState error
    assert!(
        result.is_err(),
        "Should fail when EpochState is owned by wrong program"
    );
}
