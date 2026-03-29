# Dr. Fraudsworth Jupiter Adapter SDK

Jupiter AMM adapter for the Dr. Fraudsworth DEX protocol on Solana. Implements the `jupiter-amm-interface::Amm` trait so Jupiter's routing engine can route swaps through Dr. Fraudsworth's on-chain programs.

**Key properties:**

- Exact quote accuracy (proven by LiteSVM parity tests -- quotes match on-chain output within 1 lamport)
- Zero network calls in any method (all addresses and hook accounts are hardcoded)
- Supports all 8 swap directions across 6 Amm instances

## Pool Types

Dr. Fraudsworth exposes 6 Amm instances to Jupiter:

| # | Instance | Type | Key Source | Reserves | Fees |
|---|----------|------|------------|----------|------|
| 1 | CRIME/SOL | `SolPoolAmm` | Pool PDA | Dynamic (AMM constant-product) | 1% LP + dynamic tax (4-14%) |
| 2 | FRAUD/SOL | `SolPoolAmm` | Pool PDA | Dynamic (AMM constant-product) | 1% LP + dynamic tax (4-14%) |
| 3 | CRIME->PROFIT | `VaultAmm` | Synthetic PDA | Fixed rate (100:1) | Zero |
| 4 | FRAUD->PROFIT | `VaultAmm` | Synthetic PDA | Fixed rate (100:1) | Zero |
| 5 | PROFIT->CRIME | `VaultAmm` | Synthetic PDA | Fixed rate (1:100) | Zero |
| 6 | PROFIT->FRAUD | `VaultAmm` | Synthetic PDA | Fixed rate (1:100) | Zero |

- The 2 SOL pool instances are **bidirectional** (buy and sell), covering 4 swap directions.
- The 4 vault instances are **unidirectional**, one per conversion direction.
- **CRIME <-> FRAUD direct conversion is not supported on-chain.** Jupiter routes this via multi-hop (CRIME -> PROFIT -> FRAUD).

## Pool Discovery

Jupiter integrators call these factory functions at startup to register all Dr. Fraudsworth pools:

```rust
use drfraudsworth_jupiter_adapter::{known_instances, known_sol_pool_keys, all_pool_keys};

// SOL pools: returns 2 pool PDAs for SolPoolAmm (created via from_keyed_account)
let sol_keys: Vec<Pubkey> = known_sol_pool_keys();

// Vault instances: returns 4 pre-built (Pubkey, VaultAmm) pairs
let vault_instances: Vec<(Pubkey, VaultAmm)> = known_instances();

// All 6 pool keys in one call
let all_keys: Vec<Pubkey> = all_pool_keys();
```

- `known_sol_pool_keys()` -- Returns 2 SOL pool PDAs. Jupiter fetches account data and calls `SolPoolAmm::from_keyed_account()`.
- `known_instances()` -- Returns 4 pre-constructed `VaultAmm` instances (fixed-pool protocol, no `getProgramAccounts` needed).
- `all_pool_keys()` -- Convenience: all 6 instance keys combined.

## Fee Structure

### SOL Pools (CRIME/SOL, FRAUD/SOL)

SOL pool swaps have two fee components:

1. **LP fee:** 1% (100 BPS), fixed, deducted from swap amount
2. **Dynamic tax:** 4-14% based on current epoch, split across staking rewards, treasury, and Carnage Fund

**Buy (SOL -> token):** Tax deducted from SOL input BEFORE the AMM swap.
**Sell (token -> SOL):** Tax deducted from SOL output AFTER the AMM swap.

Tax rates change every epoch (~13 minutes). Jupiter's `update()` method refreshes EpochState to get current rates. Stale rates between quote and execution are handled by on-chain slippage protection (`minimum_output`).

### Vault Conversions

- **Zero fees**, fixed rate conversion
- CRIME/FRAUD -> PROFIT: divide by 100 (100 CRIME = 1 PROFIT)
- PROFIT -> CRIME/FRAUD: multiply by 100 (1 PROFIT = 100 CRIME)

## Epoch Dynamics

Tax rates rotate every epoch between the CRIME and FRAUD factions:

| Faction | Cheap Side | Expensive Side |
|---------|-----------|----------------|
| Buy tax | 4% | 14% |
| Sell tax | 14% | 4% |

Which faction is "cheap" alternates each epoch based on VRF randomness. This creates arbitrage opportunities between the two pools that Jupiter can route through.

The `EpochState` PDA is declared in `get_accounts_to_update()`, so Jupiter automatically refreshes it and passes the latest state to `update()`.

## Account Metas

Each instruction type requires a specific set of accounts. The SDK builds these via hardcoded mainnet addresses with zero network calls.

### SwapSolBuy (SOL -> CRIME/FRAUD)

20 named accounts + 4 transfer hook accounts = **24 total**

Named accounts: user, epoch_state, swap_authority, tax_authority, pool, pool_vault_a, pool_vault_b, mint_a (WSOL), mint_b (token), user_token_a, user_token_b, stake_pool, staking_escrow, carnage_vault, treasury, amm_program, token_program_a (SPL Token), token_program_b (Token-2022), system_program, staking_program.

### SwapSolSell (CRIME/FRAUD -> SOL)

21 named accounts + 4 transfer hook accounts = **25 total**

Same as buy, plus `wsol_intermediary` PDA (account #16). The sell path routes SOL through an intermediary WSOL account before closing it back to the user.

### Vault Convert (token <-> token)

9 named accounts + 8 transfer hook accounts = **17 total**

Named accounts: user, vault_config, user_input_account, user_output_account, input_mint, output_mint, vault_input, vault_output, token_program (Token-2022).

Hook accounts: 4 for input mint + 4 for output mint (both are Token-2022 mints with transfer hooks).

### Transfer Hook Accounts (per mint)

Each Token-2022 mint has 4 deterministic hook accounts:

1. ExtraAccountMetaList PDA
2. Whitelist entry for source token account
3. Whitelist entry for destination token account
4. Transfer Hook program ID

## Quick Start

```rust
use drfraudsworth_jupiter_adapter::{SolPoolAmm, VaultAmm, known_instances, known_sol_pool_keys};
use jupiter_amm_interface::{Amm, KeyedAccount, QuoteParams, SwapMode};

// -- SOL Pool (production: use from_keyed_account with live account data) --
// Jupiter calls from_keyed_account() automatically during pool registration.
// The SDK's update() method refreshes reserves and tax rates from account snapshots.

// -- Vault Instances (pre-built, no account data needed) --
let vault_instances = known_instances();
for (key, amm) in &vault_instances {
    let quote = amm.quote(&QuoteParams {
        amount: 100_000_000_000, // 100B tokens
        input_mint: amm.get_reserve_mints()[0],
        output_mint: amm.get_reserve_mints()[1],
        swap_mode: SwapMode::ExactIn,
    }).unwrap();
    println!("{}: {} -> {}", key, quote.in_amount, quote.out_amount);
}
```

See `examples/quote_example.rs` for a complete working example:

```bash
cargo run --example quote_example -p drfraudsworth-jupiter-adapter
```

## Program IDs

Jupiter needs to know which programs are called for each swap type:

| Program | Address | Called For |
|---------|---------|-----------|
| Tax Program | `43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj` | SOL pool swaps (CPI to AMM internally) |
| Conversion Vault | `5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ` | Vault conversions |
| AMM Program | `5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR` | Called via CPI by Tax Program (not directly by Jupiter) |
| Transfer Hook | `CiQPQrmQh6BPhb9k7dFnsEs5gKPgdrvNKFc5xie5xVGd` | Called by Token-2022 during transfers |
| Epoch Program | `4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2` | Manages epoch state (not called by Jupiter directly) |
| Staking Program | `12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH` | Receives staking rewards from tax (not called by Jupiter directly) |

## IDL Locations

Anchor IDLs are required for Jupiter's integration review:

| IDL File | Program |
|----------|---------|
| `target/idl/amm.json` | AMM Program (constant-product swap logic) |
| `target/idl/tax_program.json` | Tax Program (swap entry point with tax deduction) |
| `target/idl/conversion_vault.json` | Conversion Vault (fixed-rate token conversion) |
| `target/idl/transfer_hook.json` | Transfer Hook (whitelist-based transfer validation) |
| `target/idl/epoch_program.json` | Epoch Program (VRF-based epoch rotation) |
| `target/idl/staking.json` | Staking Program (PROFIT yield distribution) |

Public repository: [github.com/MetalLegBob/drfraudsworth](https://github.com/MetalLegBob/drfraudsworth)

IDLs are in `target/idl/` in the source repository.

## Token Mints

All three protocol tokens use Token-2022 with transfer hooks:

| Token | Mint Address | Decimals |
|-------|-------------|----------|
| CRIME | `cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc` | 9 |
| FRAUD | `FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5` | 9 |
| PROFIT | `pRoFiTj36haRD5sG2Neqib9KoSrtdYMGrM7SEkZetfR` | 9 |

## Mainnet Addresses

Full address set is in `deployments/mainnet.json`. Key addresses for Jupiter integration:

| Resource | Address |
|----------|---------|
| CRIME/SOL Pool | `ZWUZ3PzGk6bg6g3BS3WdXKbdAecUgZxnruKXQkte7wf` |
| FRAUD/SOL Pool | `AngvViTVGd2zxP8KoFUjGU3TyrQjqeM1idRWiKM8p3mq` |
| EpochState PDA | `FjJrLcmDjA8FtavGWdhJq3pdirAH889oWXc2bhEAMbDU` |
| VaultConfig PDA | `8vFpSBnCVt8dfX57FKrsGwy39TEo1TjVzrj9QYGxCkcD` |
| Swap Authority | `CoCdbornGtiZ8tLxF5HD2TdGidfgfwbbiDX79BaZGJ2D` |
| Treasury | `3ihhwLnEJ2duwPSLYxhLbFrdhhxXLcvcrV9rAHqMgzCv` |

## Jupiter Integration Notes

- **Swap variant:** Uses `Swap::TokenSwap` as placeholder. Jupiter assigns the real variant during integration review.
- **Vault instance keying:** Synthetic PDAs derived from `[b"jup_vault", input_mint, output_mint]` via `Pubkey::find_program_address`. These are not real on-chain accounts -- they exist solely to give each VaultAmm instance a unique key.
- **`supports_exact_out`:** Returns `false` for all instances. Integer division in vault conversions loses information, and SOL pool exact-out would require iterative solving.
- **No network calls:** All methods (`quote`, `get_swap_and_account_metas`, `get_accounts_to_update`) use hardcoded addresses. Jupiter handles account fetching externally.
- **WSOL wrapping:** Jupiter handles SOL <-> WSOL wrapping/unwrapping. The SDK returns only the Tax Program swap instruction.
- **`unidirectional()`:** Returns `true` for VaultAmm, `false` (default) for SolPoolAmm. Jupiter uses this to avoid routing backwards through vault instances.

## Testing

```bash
# Run all unit tests (59 math/state + 46 trait/account tests = 105 total)
cargo test -p drfraudsworth-jupiter-adapter

# Run the quote example
cargo run --example quote_example -p drfraudsworth-jupiter-adapter
```

LiteSVM parity tests deploy the actual on-chain programs, execute swaps, and compare outputs to SDK quotes -- proving exact accuracy within 1 lamport.

## License

MIT
