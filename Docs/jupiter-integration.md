# Jupiter Integration Submission Guide

Guide for submitting the Dr. Fraudsworth Jupiter adapter SDK for integration with Jupiter's aggregator.

## Prerequisites

Before submitting to Jupiter:

1. **SDK crate published to crates.io** as `drfraudsworth-jupiter-adapter`
2. **Public GitHub repo** with IDLs at [github.com/MetalLegBob/drfraudsworth](https://github.com/MetalLegBob/drfraudsworth)
3. **Protocol has trading volume** -- Jupiter requires active pools with real liquidity
4. **All tests passing** -- `cargo test -p drfraudsworth-jupiter-adapter` (105+ tests)

## Submission Process

1. **Contact Jupiter team** via their [integration form](https://station.jup.ag/) or Discord
2. **Provide** the artifacts listed below
3. **Jupiter team reviews** the SDK code and runs their snapshot tests
4. **Jupiter assigns Swap enum variants** for Dr. Fraudsworth (replacing the placeholder `Swap::TokenSwap`)
5. **Jupiter adds to `PROGRAM_ID_TO_AMM_LABEL_WITH_AMM_FROM_KEYED_ACCOUNT`** mapping in their router
6. **Integration goes live** -- Jupiter starts routing swaps through Dr. Fraudsworth pools

## Artifacts to Provide

### Crate

- **Name:** `drfraudsworth-jupiter-adapter`
- **Registry:** [crates.io](https://crates.io/crates/drfraudsworth-jupiter-adapter)
- **Source:** [github.com/MetalLegBob/drfraudsworth/tree/main/sdk/jupiter-adapter](https://github.com/MetalLegBob/drfraudsworth/tree/main/sdk/jupiter-adapter)

### IDLs

- **Location:** [github.com/MetalLegBob/drfraudsworth/tree/main/target/idl](https://github.com/MetalLegBob/drfraudsworth/tree/main/target/idl)
- `amm.json` -- AMM Program (constant-product swap logic)
- `tax_program.json` -- Tax Program (swap entry point with tax deduction)
- `conversion_vault.json` -- Conversion Vault (fixed-rate token conversion)
- `transfer_hook.json` -- Transfer Hook (whitelist-based transfer validation)
- `epoch_program.json` -- Epoch Program (VRF-based epoch rotation)
- `staking.json` -- Staking Program (PROFIT yield distribution)

### Program IDs

| Program | Address | Solscan |
|---------|---------|---------|
| Tax Program | `43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj` | [View](https://solscan.io/account/43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj) |
| Conversion Vault | `5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ` | [View](https://solscan.io/account/5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ) |
| AMM Program | `5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR` | [View](https://solscan.io/account/5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR) |
| Transfer Hook | `CiQPQrmQh6BPhb9k7dFnsEs5gKPgdrvNKFc5xie5xVGd` | [View](https://solscan.io/account/CiQPQrmQh6BPhb9k7dFnsEs5gKPgdrvNKFc5xie5xVGd) |
| Epoch Program | `4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2` | [View](https://solscan.io/account/4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2) |
| Staking Program | `12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH` | [View](https://solscan.io/account/12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH) |

### Pool Addresses

| Pool | Address | Solscan |
|------|---------|---------|
| CRIME/SOL | `ZWUZ3PzGk6bg6g3BS3WdXKbdAecUgZxnruKXQkte7wf` | [View](https://solscan.io/account/ZWUZ3PzGk6bg6g3BS3WdXKbdAecUgZxnruKXQkte7wf) |
| FRAUD/SOL | `AngvViTVGd2zxP8KoFUjGU3TyrQjqeM1idRWiKM8p3mq` | [View](https://solscan.io/account/AngvViTVGd2zxP8KoFUjGU3TyrQjqeM1idRWiKM8p3mq) |
| VaultConfig | `8vFpSBnCVt8dfX57FKrsGwy39TEo1TjVzrj9QYGxCkcD` | [View](https://solscan.io/account/8vFpSBnCVt8dfX57FKrsGwy39TEo1TjVzrj9QYGxCkcD) |

### Pool Discovery

Jupiter should use the SDK's factory functions to discover all pools:

```rust
use drfraudsworth_jupiter_adapter::{known_instances, known_sol_pool_keys};

// 2 SOL pools (bidirectional, 4 swap directions)
let sol_pool_keys = known_sol_pool_keys();

// 4 vault conversion instances (unidirectional, 4 swap directions)
let vault_instances = known_instances();

// Total: 6 Amm instances covering 8 swap directions
```

### Token Mints

| Token | Address | Solscan |
|-------|---------|---------|
| CRIME | `cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc` | [View](https://solscan.io/token/cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc) |
| FRAUD | `FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5` | [View](https://solscan.io/token/FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5) |
| PROFIT | `pRoFiTj36haRD5sG2Neqib9KoSrtdYMGrM7SEkZetfR` | [View](https://solscan.io/token/pRoFiTj36haRD5sG2Neqib9KoSrtdYMGrM7SEkZetfR) |

All three tokens are Token-2022 with transfer hooks (whitelist-based).

## Publishing to crates.io

```bash
# 1. Login to crates.io (get token from https://crates.io/settings/tokens)
cargo login <your-crates-io-token>

# 2. Verify the crate builds and passes all tests
cd sdk/jupiter-adapter
cargo test
cargo publish --dry-run

# 3. Publish
cargo publish
```

After publishing, verify at: https://crates.io/crates/drfraudsworth-jupiter-adapter

## Amm Instances Summary

| # | Instance | Type | Program Called | Directions | Accounts |
|---|----------|------|---------------|------------|----------|
| 1 | CRIME/SOL | SolPoolAmm | Tax Program | buy + sell | 24 (buy) / 25 (sell) |
| 2 | FRAUD/SOL | SolPoolAmm | Tax Program | buy + sell | 24 (buy) / 25 (sell) |
| 3 | CRIME->PROFIT | VaultAmm | Conversion Vault | 1 direction | 17 |
| 4 | FRAUD->PROFIT | VaultAmm | Conversion Vault | 1 direction | 17 |
| 5 | PROFIT->CRIME | VaultAmm | Conversion Vault | 1 direction | 17 |
| 6 | PROFIT->FRAUD | VaultAmm | Conversion Vault | 1 direction | 17 |

**Total: 6 Amm instances covering 8 swap directions.**

## Known Limitations (v0.1.0)

1. **CRIME <-> FRAUD direct conversion not supported.** The on-chain Conversion Vault returns `InvalidMintPair` for CRIME <-> FRAUD. Jupiter routes this via multi-hop: CRIME -> PROFIT -> FRAUD (or reverse).

2. **`supports_exact_out` = false** for all instances. Integer division in vault conversions loses information, and SOL pool exact-out would require iterative solving.

3. **Swap variant is placeholder.** The SDK uses `Swap::TokenSwap` as a placeholder. Jupiter assigns the real variant during integration review.

4. **Dynamic tax rates.** Tax rates change every ~13 minutes (epoch rotation). Between quote and execution, rates may change. This is handled by on-chain slippage protection (`minimum_output` parameter) -- standard Jupiter pattern.

## Post-Integration Maintenance

- **Bump crate version** when on-chain programs are upgraded
- **Update tax rate parsing** if EpochState account layout changes
- **Add new Amm instances** if new pools are created (e.g., USDC pools planned for v1.6)
- **Update account metas** if instruction struct fields change
- **Coordinate with Jupiter** for breaking changes that require new Swap variants

## Full Address Reference

All addresses are in `deployments/mainnet.json` at the repository root.
