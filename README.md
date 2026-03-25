# Dr. Fraudsworths' Fantastical Finance Factory

A tax-driven DeFi protocol on Solana where every swap fuels epochs, carnage events, and real SOL rewards for stakers.

Dr. Fraudsworths' Fantastical Finance Factory is a live, mainnet-deployed Solana protocol built around a novel economic loop: every token swap incurs a configurable tax that flows into the Carnage Fund. Epochs advance via Switchboard VRF randomness, and when Carnage strikes, accumulated taxes are redistributed back to token holders. PROFIT token stakers earn real SOL rewards from trading activity -- not inflationary token emissions.

## Live Protocol

- **Website:** [fraudsworth.fun](https://fraudsworth.fun)
- **Documentation:** [docs.fraudsworth.fun](https://docs.fraudsworth.fun)
- **X:** [@fraudsworth](https://x.com/fraudsworth)
- **Telegram:** [t.me/fraudsworth](https://t.me/fraudsworth)
- **Explorer:** See program addresses below

## Deployed Programs (Solana Mainnet)

| Program | Address | Verified |
|---------|---------|----------|
| AMM | [`5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR`](https://solscan.io/account/5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR) | [OtterSec](https://verify.osec.io/status/5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR) |
| Transfer Hook | [`CiQPQrmQh6BPhb9k7dFnsEs5gKPgdrvNKFc5xie5xVGd`](https://solscan.io/account/CiQPQrmQh6BPhb9k7dFnsEs5gKPgdrvNKFc5xie5xVGd) | [OtterSec](https://verify.osec.io/status/CiQPQrmQh6BPhb9k7dFnsEs5gKPgdrvNKFc5xie5xVGd) |
| Tax Program | [`43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj`](https://solscan.io/account/43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj) | [OtterSec](https://verify.osec.io/status/43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj) |
| Epoch Program | [`4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2`](https://solscan.io/account/4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2) | [OtterSec](https://verify.osec.io/status/4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2) |
| Staking | [`12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH`](https://solscan.io/account/12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH) | [OtterSec](https://verify.osec.io/status/12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH) |
| Conversion Vault | [`5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ`](https://solscan.io/account/5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ) | [OtterSec](https://verify.osec.io/status/5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ) |

### Token Mints

| Token | Mint Address |
|-------|-------------|
| CRIME | [`cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc`](https://solscan.io/account/cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc) |
| FRAUD | [`FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5`](https://solscan.io/account/FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5) |
| PROFIT | [`pRoFiTj36haRD5sG2Neqib9KoSrtdYMGrM7SEkZetfR`](https://solscan.io/account/pRoFiTj36haRD5sG2Neqib9KoSrtdYMGrM7SEkZetfR) |

### Treasury

| | Address |
|---|---------|
| Treasury | [`3ihhwLnEJ2duwPSLYxhLbFrdhhxXLcvcrV9rAHqMgzCv`](https://solscan.io/account/3ihhwLnEJ2duwPSLYxhLbFrdhhxXLcvcrV9rAHqMgzCv) |

### Squads Multisig (2-of-3, Timelocked)

| | Address |
|---|---------|
| Multisig | [`F7axBNUgWQQ33ZYLdenCk5SV3wBrKyYz9R7MscdPJi1A`](https://solscan.io/account/F7axBNUgWQQ33ZYLdenCk5SV3wBrKyYz9R7MscdPJi1A) |
| Vault (Authority Holder) | [`4SMcPtixKvjgj3U5N7C4kcnHYcySudLZfFWc523NAvXJ`](https://solscan.io/account/4SMcPtixKvjgj3U5N7C4kcnHYcySudLZfFWc523NAvXJ) |

## How It Works

Dr. Fraudsworth implements a closed economic loop where trading friction generates real rewards:

1. **Tax on Every Swap** -- Every token swap through the AMM incurs a configurable tax (14-25% depending on the current epoch). Tax proceeds are split: 71% to the Carnage Fund, 24% to PROFIT stakers, and 5% to the treasury.

2. **VRF-Driven Epochs** -- Epochs advance via Switchboard VRF (verifiable random function), ensuring no admin can manipulate outcomes. Each epoch sets new tax rates and determines whether Carnage triggers.

3. **Carnage Events** -- When Carnage fires, the accumulated Carnage Fund executes autonomous buy-and-burn operations, redistributing value back to token holders by permanently removing supply.

4. **PROFIT Staking** -- PROFIT token holders stake to earn real SOL rewards from the 24% staker allocation of every swap. These are real rewards from real trading activity, not inflationary token emissions.

5. **Dual-Token Economy** -- CRIME and FRAUD are the two tradeable tokens, each with their own SOL pool. PROFIT is the reward-bearing staking token earned through the Conversion Vault.

## Architecture

The protocol consists of 6 active programs working together:

- **AMM** (`amm`) -- Constant-product automated market maker with 4 trading pools (CRIME/SOL, FRAUD/SOL, CRIME/PROFIT, FRAUD/PROFIT). Supports Token-2022 transfer hooks for automatic tax collection.

- **Transfer Hook** (`transfer-hook`) -- Token-2022 transfer hook that intercepts every token transfer to enforce tax collection. Implements a whitelist system so protocol-internal transfers (staking, carnage, vault) bypass taxes.

- **Tax Program** (`tax-program`) -- Tax distribution engine that splits collected taxes across the Carnage Fund (71%), stakers (24%), and treasury (5%). Manages WSOL wrapping/unwrapping for SOL-denominated distributions.

- **Epoch Program** (`epoch-program`) -- VRF-driven epoch state machine. Manages the commit-reveal-consume cycle for Switchboard randomness. Controls tax rate rotation and Carnage event triggering.

- **Staking** (`staking`) -- PROFIT token staking with SOL reward distribution. Tracks per-user stake positions and distributes accumulated staker rewards proportionally.

- **Conversion Vault** (`conversion-vault`) -- Cross-token conversion mechanism allowing users to convert between CRIME/FRAUD and PROFIT at protocol-defined rates.

A **Bonding Curve** program was used for the initial token launch (CRIME and FRAUD curves raised ~1,000 SOL total) but has been closed post-graduation, with rent reclaimed.

## Security

This protocol has undergone extensive security review:

- **3 AI-assisted internal audits** covering on-chain programs and off-chain infrastructure:
  - **SOS Audit** (`.audit/`) -- Comprehensive Anchor program security review (132 findings analyzed)
  - **Bulwark Audit** (`.bulwark/`) -- Full-stack security assessment including frontend, API, crank bot, and infrastructure (132 findings analyzed)
  - **BOK Formal Verification** (`.bok/`) -- Kani-based formal verification of critical invariants (arithmetic bounds, state transitions)

- **OtterSec verified build badges** for all 6 deployed programs (independently reproducible builds)

- **Switchboard VRF** for verifiable on-chain randomness (no admin-controlled outcomes)

- **Squads v4 multisig governance** -- All program authorities held by a 2-of-3 timelocked multisig

- **`security.txt`** embedded in all program binaries per [Solana security.txt standard](https://github.com/nicholasgasior/solana-security-txt)

See [SECURITY_AUDIT_SUMMARY.md](./SECURITY_AUDIT_SUMMARY.md) for the complete audit findings and project response to each.

## Build Journey

This project was built over 104 phases with Claude (Anthropic's AI) as the primary developer and a solo human as the product owner. The complete build history -- every planning document, research phase, decision log, and phase summary -- is preserved in [`.planning/`](./.planning/).

**By the numbers:**
- 104 development phases across 13 milestones
- 334 individual plans executed
- 3 comprehensive security audits
- From first line of code to mainnet deployment

This represents one of the most thoroughly documented AI-assisted software development projects in the Solana ecosystem. The `.planning/` directory is included deliberately as a transparency feature -- you can trace every architectural decision, bug fix, and tradeoff back to the planning document that motivated it.

## Building from Source

### Prerequisites

- Rust + Cargo (via [rustup](https://rustup.rs/))
- Solana CLI v2.x ([install](https://solana.com/docs/intro/installation))
- Anchor CLI v0.32.x (via [AVM](https://www.anchor-lang.com/docs/installation))
- Node.js 18+ and npm
- Docker (for verified/deterministic builds)

### Build Programs

```bash
# Standard build
anchor build

# Devnet build (enables devnet feature flags)
anchor build
anchor build -p epoch_program -- --features devnet
```

### Verified Build (matches mainnet binaries)

```bash
# Requires Docker and solana-verify CLI
solana-verify build --library-name amm
solana-verify build --library-name transfer_hook
solana-verify build --library-name tax_program
solana-verify build --library-name epoch_program
solana-verify build --library-name staking
solana-verify build --library-name conversion_vault
```

### Run Tests

```bash
# Anchor integration tests (requires local validator)
anchor test

# Individual test suites
anchor test -- --test staking
anchor test -- --test token-flow
anchor test -- --test security
```

### Run Frontend

```bash
cd app && npm install && npm run dev
```

### Run Documentation Site

```bash
cd docs-site && npm install && npm run dev
```

## Project Structure

```
programs/           -- 7 Anchor programs (6 active + bonding curve)
  amm/              -- Constant-product AMM with Token-2022 hook support
  transfer-hook/    -- Token-2022 transfer hook for tax collection
  tax-program/      -- Tax distribution engine
  epoch-program/    -- VRF-driven epoch advancement and Carnage
  staking/          -- PROFIT staking with SOL rewards
  conversion-vault/ -- Cross-token conversion
  bonding_curve/    -- Token launch curves (closed post-graduation)
app/                -- Next.js 16 frontend (React, TailwindCSS, Turbopack)
docs-site/          -- Nextra documentation website
scripts/            -- Deploy, crank, graduation, and test infrastructure
  deploy/           -- Full deployment pipeline (build, deploy, initialize)
  crank/            -- Epoch advancement crank bot
  graduation/       -- Bonding curve -> AMM pool migration
  test/             -- Test helpers and validation scripts
tests/              -- Anchor integration test suites
shared/             -- TypeScript shared constants and program IDs
deployments/        -- On-chain addresses (mainnet.json, devnet.json)
Docs/               -- Protocol specifications and operational docs
.audit/             -- SOS security audit (AI-assisted)
.bulwark/           -- Bulwark security audit (AI-assisted)
.bok/               -- BOK formal verification (Kani harnesses)
.planning/          -- 104 phases of build documentation
```

## Governance

All program upgrade authorities and admin PDA authorities are held by a 2-of-3 Squads v4 multisig with timelocked upgrades. No single key can modify the protocol.

For the full governance structure, see [Docs/mainnet-governance.md](./Docs/mainnet-governance.md).

## License

MIT -- see [LICENSE](./LICENSE)

## Disclaimer

This software is provided as-is for transparency and verification purposes. DeFi protocols carry inherent financial risks including but not limited to smart contract bugs, oracle failures, and market volatility. This repository is read-only; no external contributions are accepted. Do your own research. Not financial advice.
