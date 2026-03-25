# Dr. Fraudsworth's Finance Factory

## What This Is

A gamified DeFi protocol on Solana featuring a three-token system (CRIME, FRAUD, PROFIT) designed to generate persistent trading volume and SOL-denominated yield through a closed-loop market structure. The system avoids ponzinomics by funding yield exclusively from real trading friction -- asymmetric taxes, a soft peg mechanism, and controlled chaos ("Carnage") create a self-correcting equilibrium between arbitrageurs and yield holders.

The frontend is an explorable steampunk factory scene where 6 clickable machines open themed modal interfaces for all protocol features -- swapping, staking, Carnage monitoring, wallet connection, documentation, and settings. A dedicated /launch page provides the token distribution interface with dual bonding curves, real-time progress gauges, and conditional refund UI. The factory scene IS the navigation layer on desktop; mobile users get a steampunk-themed vertical nav with full feature parity. The visual layer uses zero npm dependencies beyond Next.js Image and Tailwind v4 -- CSS-only animations, custom modal system, and optimized WebP assets.

## Current State

**LIVE ON SOLANA MAINNET** since 2026-03-25. 14 milestones shipped (v0.1-v1.4), 104 phases, 349 plans, ~40K LOC Rust + ~98K LOC TypeScript. Security-hardened across 5 audits (SOS adversarial, BOK formal verification, VulnHunter variant analysis, Bulwark off-chain x2). All 6 active programs OtterSec verified. Public repo: github.com/MetalLegBob/drfraudsworth.

**Mainnet deployment (2026-03-25):**
- AMM: `5JsSB2MRxPr3mBc3j1GfuoEVoSxfNpBJMHHQFNqK5Fm2`
- Transfer Hook: `CiQPWB3VhKbMscGHkVstHRp8WdqjbxfWawjMZraTaVVP`
- Tax Program: `43fZ3VVxqZcCLGhbcvMXfCzqMiGXib2WCHgzLUw5Cto1`
- Epoch Program: `4HeqEoSyfYpeC2goFLj9eHgkKqomaK7JBLT1MhovKZBB`
- Staking: `12b3h8WRioyvYZqpaqLfNfqnKLz8nz4LnMbAbFNbpNS2`
- Conversion Vault: `5uawHrz4MCRNKHoEhQrWZ2PoTFCKEFqoFBoBv7Qezhfn`
- Mints: CRIME=`cRiMEhAxoDhcEuh3Yf7Z2QkXUXUMKbakhcVqmDsqPXc`, FRAUD=`FraUdp6YhtVJYPxC2w255yAbpTsPqd8Bfhy9rC56jau5`, PROFIT=`pRoFiTj36haRD5sG2Neqib9KoSrtdYMGrM7SEkZetfR`
- Frontend: https://fraudsworth.fun
- Crank: Railway mainnet, 24/7 epoch advancement + Carnage + rent reclaim
- Governance: Squads 2-of-3 multisig, 1hr timelock, 11 authorities transferred

**Shipped milestones:**
- ~~v0.1-v1.3~~ (documentation through hardening) SHIPPED 2026-02-03 to 2026-03-12
- ~~v1.4: Pre-Mainnet~~ SHIPPED 2026-03-25 -- mainnet deploy, governance, open-source, OtterSec verified

## Core Value

Real SOL yield from real trading friction -- not ponzinomics. Every on-chain program is spec-driven, security-tested, and CPI-validated before integration. The documentation remains the source of truth; every conflict resolved and every gap filled prevents cascading failures.

## Requirements

### Validated

**v0.1 Documentation Audit:**
- ✓ All spec docs cross-referenced (85 concepts, zero conflicts) -- v0.1
- ✓ All gaps identified and filled (24 gaps across 5 HIGH, 16 MEDIUM, 3 LOW) -- v0.1
- ✓ Switchboard VRF implementation documented from v3 archive -- v0.1
- ✓ Token program matrix validated (T22/SPL compatibility) -- v0.1
- ✓ Token naming finalized: CRIME, FRAUD, PROFIT -- v0.1
- ✓ Yield model updated to staking-based -- v0.1
- ✓ 14-document set validated with 3 clean passes -- v0.1

**v0.2 AMM Program:**
- ✓ AMM program with constant-product swap math (u128 checked arithmetic) -- v0.2
- ✓ Pool initialization with AdminConfig + PoolState PDAs -- v0.2
- ✓ Mixed T22/SPL token transfer routing with hook passthrough -- v0.2
- ✓ SOL pool swaps (100 bps fee, CEI ordering, reentrancy guard) -- v0.2
- ✓ PROFIT pool swaps (50 bps fee, dual-hook support) -- v0.2
- ✓ CPI-only access control via swap_authority PDA -- v0.2
- ✓ 85 tests (26 unit + 59 integration) all passing -- v0.2

**v0.3 Transfer Hook Program:**
- ✓ Transfer Hook program with SPL interface discriminators (Execute, InitializeExtraAccountMetaList) -- v0.3
- ✓ WhitelistAuthority with burnable authority pattern (Option<Pubkey>) -- v0.3
- ✓ WhitelistEntry existence-based PDAs with ["whitelist", address] seeds -- v0.3
- ✓ ExtraAccountMetaList with dynamic PDA resolution (Seed::AccountKey indices 0, 2) -- v0.3
- ✓ 4-layer security validation (zero amount, mint owner, transferring flag, whitelist) -- v0.3
- ✓ 10 tests (8 requirement documentation + 2 infrastructure) all passing -- v0.3

**v0.4 Tax Program:**
- ✓ Tax Program with 5 swap instructions (swap_sol_buy, swap_sol_sell, swap_profit_buy, swap_profit_sell, swap_exempt) -- v0.4
- ✓ Tax math module with u128 intermediates and micro-tax rule (< 4 lamports all to staking) -- v0.4
- ✓ Buy tax deducted from SOL input, sell tax deducted from SOL output -- v0.4
- ✓ Atomic tax distribution: 75% staking_escrow, 24% carnage_vault, 1% treasury -- v0.4
- ✓ Untaxed PROFIT pool swaps routed through AMM (50 bps LP fee only) -- v0.4
- ✓ Tax-exempt swap_exempt instruction for Carnage (seeds::program = epoch_program_id()) -- v0.4
- ✓ AMM access control verified: seeds::program = TAX_PROGRAM_ID prevents direct bypass -- v0.4
- ✓ 54 tests (27 unit + 27 integration) all passing -- v0.4

**v0.5 Epoch/VRF Program:**
- ✓ Epoch Program with EpochState PDA (slot-based timing, dynamic tax config, VRF/Carnage state) -- v0.5
- ✓ Switchboard VRF integration with anti-manipulation (anti-reroll, freshness, timeout recovery) -- v0.5
- ✓ Tax derivation via VRF bytes (75% flip probability, discrete 100-400/1100-1400 bps bands) -- v0.5
- ✓ Tax Program reads dynamic rates from EpochState (no hardcoded values) -- v0.5
- ✓ Staking CPI from consume_randomness to update_cumulative for epoch-end yield finalization -- v0.5
- ✓ Carnage Fund execution at CPI depth 4 (atomic + fallback paths) -- v0.5
- ✓ carnage_signer PDA validation with seeds::program = EPOCH_PROGRAM_ID -- v0.5
- ✓ 59 epoch-program tests, 4 stub-staking tests all passing -- v0.5

**v0.6 Staking/Yield System:**
- ✓ Staking program with stake/unstake/claim using cumulative reward-per-token math (1e18 precision) -- v0.6
- ✓ Cross-program CPI: Tax deposits 75% SOL taxes, Epoch finalizes epochs, both with seeds::program constraints -- v0.6
- ✓ Token-2022 Transfer Hook integration with custom transfer_checked_with_hook helper -- v0.6
- ✓ First-depositor attack prevention via MINIMUM_STAKE dead stake pattern -- v0.6
- ✓ Escrow solvency invariant validated: balance >= sum(all pending claims) -- v0.6
- ✓ 87 tests + 40,000 proptest iterations, 35/35 requirements satisfied -- v0.6

**v0.7 Integration + Devnet:**
- ✓ Cross-program integration: all 5 programs in one local validator, 22 integration tests -- v0.7
- ✓ Program ID reconciliation: all cross-program refs correct, 26/26 automated checks pass -- v0.7
- ✓ Automated deployment pipeline: idempotent build/deploy/init/verify, 34/34 on-chain checks -- v0.7
- ✓ All 5 programs deployed and executable on Solana devnet -- v0.7
- ✓ Switchboard VRF on devnet: real oracle randomness, timeout recovery, anti-manipulation -- v0.7
- ✓ End-to-end flows: swap + tax (75/24/1), staking + yield claim, multi-epoch VRF cycling -- v0.7
- ✓ Security hardening: all P0-P3 audit findings fixed, BPF stack overflow resolved -- v0.7
- ✓ Independent tax rolls: 4 VRF bytes for per-token tax rates, Carnage shifted to bytes 5/6/7 -- v0.7
- ✓ Carnage fully operational: dual-pool with burn/sell, depth-4 CPI at 107k CU, ALT for large TX, 6/6 paths -- v0.7
- ✓ Continuous devnet runner: 1191+ epochs, natural Carnage on epoch 1102 -- v0.7

**v0.8 Frontend Tech Foundations:**
- ✓ Next.js monorepo with Anchor client, IDL sync, shared constants -- v0.8
- ✓ Privy embedded wallets with social login (email, Google) -- v0.8
- ✓ Standard wallet connection (Phantom, Solflare, Backpack) via Privy -- v0.8
- ✓ Unified wallet abstraction (useProtocolWallet) -- v0.8
- ✓ Token balance display (CRIME, FRAUD, PROFIT, SOL) with 30s refresh -- v0.8
- ✓ Buy/Sell UI for all 4 pools, 8 token pair combinations -- v0.8
- ✓ Price quote engine mirroring on-chain math with tax/fee breakdowns -- v0.8
- ✓ Slippage tolerance and priority fee configuration -- v0.8
- ✓ Transaction status feedback with human-readable errors (32 codes) -- v0.8
- ✓ Staking/unstaking/claim interface with BigInt reward math -- v0.8
- ✓ Real-time data dashboard (epoch, taxes, pools, Carnage) -- v0.8
- ✓ Historical price charts via TradingView lightweight-charts v5 -- v0.8
- ✓ Helius webhook indexer with OHLCV candle aggregation -- v0.8
- ✓ Railway deployment with Postgres, CSP headers, Sentry -- v0.8
- ✓ Desktop-only tech prototype deployed at Railway URL -- v0.8

**v0.9 Protocol Hardening & Smart Routing:**
- ✓ All Fortress P0-P1 security audit findings fixed (account validation, CPI targets, VRF ownership) -- v0.9
- ✓ Carnage hardening (fallback path, slippage protection, atomic bundling) -- v0.9
- ✓ Sell tax WSOL intermediary (users with low SOL can sell) -- v0.9
- ✓ Protocol safety (50% minimum output floor, escrow reconciliation, event emissions) -- v0.9
- ✓ Program maintenance (feature-gated SLOTS_PER_EPOCH, VRF bounty, treasury refactor) -- v0.9
- ✓ All 5 programs rebuilt, redeployed, verified on devnet (299 Rust tests, 0 failures) -- v0.9
- ✓ Smart swap routing engine (route comparison, multi-hop, split routing, route UI) -- v0.9

**v1.0 Frontend Design & Interactive Factory:**
- ✓ Asset pipeline: 19.5MB PNGs optimized to 1.3MB WebP, steampunk @theme tokens, Cinzel + IBM Plex Mono typography -- v1.0
- ✓ Reusable modal system: iris-open animation, backdrop blur, focus trap, steampunk chrome (brass frame, bolts) -- v1.0
- ✓ Interactive factory scene: 6 positioned overlay stations, hover glow, click-to-open, contain-fit scaling 1280-3840px -- v1.0
- ✓ Swap Station: TradingView chart, market caps, tax rates, Big Red Button with 7 CSS states -- v1.0
- ✓ Carnage Cauldron: SOL balance, trigger history, recent events from Postgres API -- v1.0
- ✓ Rewards Vat: stake/unstake/claim tabs, pending rewards, total staked -- v1.0
- ✓ Connect Wallet: Privy + browser wallet, themed chrome -- v1.0
- ✓ Documentation Table: protocol explanation embedded via iframe -- v1.0
- ✓ Settings: slippage, priority fees, wallet controls (copy/disconnect/export) -- v1.0
- ✓ Full brand application: zero off-palette residuals, 32 WCAG AA contrast pairs verified -- v1.0
- ✓ RPC optimization: WebSocket subscriptions + visibility pausing, 87% credit reduction (2,590 to 332/hr) -- v1.0
- ✓ Mobile navigation: steampunk vertical nav below 1024px, fullscreen slide-up modals, 100% feature parity -- v1.0
- ✓ Onboarding: animated SplashScreen brass gear intro gate with "Push to Enter" (replaced WelcomeModal per user) -- v1.0
- ✓ Accessibility: button elements with aria-labels, focus-visible glow, WCAG AA contrast, role="img" -- v1.0

**v1.1 Modal Mastercraft, Docs & Audio:**
- ✓ Steampunk component kit: 9 primitives (Frame, Button, Input, Tabs, Toggle, Slider, Card, Divider, Scrollbar), @layer kit, WCAG AA -- v1.1
- ✓ Charts overhaul: theme constants, volume histogram, OHLC legend, log scale, Frame wrapper, gap-fill opt-out -- v1.1
- ✓ Swap Station polish: kit-frame chrome, two-column layout, dual-panel pool selector, Big Red Button, stats bar -- v1.1
- ✓ Carnage Cauldron, Rewards Vat, Connect Wallet: kit-frame chrome, themed components -- v1.1
- ✓ Settings Station: SettingsProvider with localStorage, themed controls, audio section UI -- v1.1
- ✓ Modal infrastructure: brass valve close button, overscroll containment -- v1.1
- ✓ Audio system: AudioManager singleton, dual-slot crossfade, 3 MP3 tracks, iOS Safari unlock, gesture-gated init -- v1.1
- ✓ Audio integration: QuickMuteButton floating control, SettingsProvider sync -- v1.1
- ✓ Dashboard dead code deletion (5 files removed) -- v1.1
- ✓ Devnet ecosystem relaunch: 6 programs deployed (incl. Conversion Vault), fresh mints, crank runner -- v1.1

**v1.2 Bonding Curves & Launch Page:**
- ✓ Dual linear bonding curves (CRIME + FRAUD, 460M each) with deterministic pricing P_start=0.0000009 to P_end=0.00000345 SOL/token -- v1.2
- ✓ Buy instruction with integral-based pricing, quadratic solver, minimum_tokens_out slippage -- v1.2
- ✓ Sell instruction with reverse integral, 15% tax to escrow PDA, minimum_sol_out slippage -- v1.2
- ✓ Sells disabled when Filled; 20M per-wallet cap enforced via ATA balance -- v1.2
- ✓ Coupled graduation: both curves must fill within 48 hours; permissionless triggers -- v1.2
- ✓ Success path: prepare_transition -> pool seeding (290M tokens + 1k SOL per pool) -> tax escrow to carnage -- v1.2
- ✓ Failure path: proportional refund (SOL vault + tax escrow) based on token holdings -- v1.2
- ✓ Protocol integration: Transfer Hook whitelist, deploy pipeline, graduation orchestration, ALT extension -- v1.2
- ✓ Launch page: steampunk /launch with pressure gauges, buy/sell panel, countdown, refund UI, state machine rendering -- v1.2
- ✓ 13.5M proptest iterations across buy/sell/refund math proving vault solvency -- v1.2
- ✓ Full audit: 28/28 requirements, 32/32 integration, 6/6 E2E flows, Nyquist compliant -- v1.2

**v1.3 Protocol Hardening & Polish:**
- ✓ Authority hardening across all 7 programs (BcAdminConfig, ProgramData init gating) -- v1.3
- ✓ Financial safety guards (rent-exempt drain prevention, AMM sell floor, partial fill assertions, vault solvency buffer) -- v1.3
- ✓ Defense-in-depth (pool reader ownership, checked u128→u64 casts, EpochState padding, compile-time mainnet guards) -- v1.3
- ✓ Carnage deduplication (1800-line shared module) + VRF hardening (TOCTOU recovery, binary offset consolidation) -- v1.3
- ✓ Crank mainnet-readiness (configurable epoch slots, circuit breaker, spending cap, health endpoint) -- v1.3
- ✓ Frontend hardening (RPC proxy, webhook+SSE pipeline, dynamic priority fees, environment-aware config) -- v1.3
- ✓ Mobile responsive + mobile wallet adapter (deep-link connection for Phantom/Solflare/Backpack) -- v1.3
- ✓ Launch page polish (gauge needles, background image, documentation button, cosmetic fixes) -- v1.3
- ✓ Test coverage sweep (dual-curve LiteSVM tests, edge case audit, proptest regression fix) + CI/CD pipeline -- v1.3
- ✓ Documentation overhaul (4 core spec rewrites, Nextra site rewrite, bonding curve math proofs, state machine docs) -- v1.3
- ✓ Dead code removal, error map expansion, quote-engine BigInt conversion, cross-crate serialization test -- v1.3
- ✓ All SOS/BOK/VulnHunter audit findings closed, 57/57 requirements audit-verified -- v1.3

**v1.4 Pre-Mainnet:**
- ✓ Canonical deployment.json config system with generate-constants.ts and verify.ts -- v1.4
- ✓ 7-phase deploy-all.sh pipeline with binary verification, safety gates, mainnet preflight -- v1.4
- ✓ Arweave token metadata (logos + JSON) for all 3 mints, permanent URIs -- v1.4
- ✓ Bonding curve devnet deadline feature flag (~30 min) + clean-room full deploy + graduation -- v1.4
- ✓ Protocol E2E: all 8 swap pairs, tax verification, VRF epochs, Carnage, staking lifecycle -- v1.4
- ✓ Squads 2-of-3 multisig on devnet + mainnet, 11 authorities transferred, timelocked upgrade proven -- v1.4
- ✓ Mainnet deployment checklist validated by fresh devnet deploy (25.54 SOL actual cost) -- v1.4
- ✓ Production infrastructure: Cloudflare DNS, Railway mainnet services, Helius mainnet RPC -- v1.4
- ✓ Nextra documentation: every page production-accurate with illustrated diagrams -- v1.4
- ✓ Protocol deployed to Solana mainnet: 6 programs, 3 vanity tokens, bonding curves graduated -- v1.4
- ✓ Off-chain security hardening: 7 Bulwark findings closed (RPC proxy, webhook, supply chain, rate limiting) -- v1.4
- ✓ Open-source release at MetalLegBob/drfraudsworth with 5-pass sanitization -- v1.4
- ✓ All 6 active programs OtterSec verified on Solscan -- v1.4
- ✓ 61/61 requirements functionally satisfied (audit: tech_debt) -- v1.4

### Active

(No active milestone — all development milestones complete)

### Future

**Post-Launch Operations (v2 scope):**
- Progressive timelock extension (1hr -> 24hr after stability period)
- Immunefi bug bounty program
- External audit funded from protocol revenue
- Jupiter routing integration (Tax Program CPI publishing)
- Protocol-owned arbitrage (Carnage Fund capture)
- USDC pool pairs (CRIME/USDC, FRAUD/USDC)

### Backlog

- 3 ignored LiteSVM tests (is_reversed bug, test-only)
- Refund path (CURVE-04/05) never frontend-tested
- E2E-02 requirement text says 75/24/1, actual on-chain is 71/24/5

### Out of Scope

- External integrations beyond Switchboard VRF -- minimize attack surface
- WebGL / Three.js 3D scene -- 2D CSS achieves the goal without complexity/device requirements
- Complex particle physics -- stylized CSS animations preferred
- SFX triggers (per-interaction sounds) -- user decided not to pursue
- Music ducking during SFX -- user decided not to pursue
- Ambient factory animations (steam, bubbling, gears) -- user decided not to pursue
- Guided spotlight tour -- protocol is simple enough, not needed
- Emergency pause mechanism -- trust tradeoff, admin pause perceived as rug pull vector
- Vanity program addresses -- have vanity mint CAs (cRiME, FraUd, pRoFiT), program addresses don't need branding

## Context

**Shipped v0.1** with 38,053 lines of markdown across 14 specification documents.
**Shipped v0.2** with 10,648 lines of Rust across 3 programs (AMM, Mock Tax, Fake Tax).
**Shipped v0.3** with 1,023 lines of Rust in the Transfer Hook program (4 phases, 9 plans, 10 tests).
**Shipped v0.4** with 7,044 lines of Rust in the Tax Program (4 phases, 11 plans, 54 tests).
**Shipped v0.5** with 4,311 lines of Rust in Epoch/Stub-Staking (4 phases, 16 plans, 63 tests).
**Shipped v0.6** with 2,576 lines of Rust in the Staking Program (4 phases, 17 plans, 87 tests + 40,000 proptest iterations).
**Shipped v0.7** with 28,093 lines of Rust, 12,653 lines of scripts, 10,026 lines of tests (9 phases, 25 plans, 22 integration tests + E2E devnet suite).
**Shipped v0.8** with 19,576 lines of TypeScript/TSX, 7 phases, 18 plans, 92 commits.
**Shipped v0.9** with protocol hardening + smart routing, 7 phases, 27 plans, 299 Rust tests.
**Shipped v1.0** with 16,180 lines of TypeScript/TSX + 1,389 lines CSS, 8 phases, 30 plans, 134 commits. Image optimization 19.5MB to 1.3MB. RPC credits 2,590 to 332/hr.
**Shipped v1.1** with 406 files changed (+44,554/-13,579 lines), 9 phases, 27 plans, 156 commits. Steampunk component kit (9 primitives), chart overhaul, audio system, devnet relaunch with Conversion Vault.
**Shipped v1.2** with 193 files changed (+39,747/-1,091 lines), 8 phases, 25 plans, 134 commits. 7th on-chain program (dual bonding curves), launch page frontend, 13.5M proptest iterations, 28/28 requirements audit-verified.
**Shipped v1.3** with 972 files changed (+163,086/-46,481 lines), 16 phases, 45 plans, 211 commits. Full security hardening across 3 audits (SOS/BOK/VulnHunter), 57/57 requirements verified, documentation rewritten, CI/CD pipeline, mobile responsive + wallet adapter.
**Shipped v1.4** with 685 files changed (+231,317/-10,609 lines), 16 phases, 58 plans, 324 commits. Mainnet deployment, Squads governance, Nextra docs, off-chain hardening, open-source release, OtterSec verification. 14 days (2026-03-12 → 2026-03-25).

Tech stack: Anchor 0.32.1/Rust for on-chain programs, Token-2022 standard, Switchboard VRF On-Demand.
Frontend stack: Next.js 16.1.6 + Turbopack + Tailwind CSS v4 + @solana/wallet-adapter-react + @coral-xyz/anchor client + TradingView lightweight-charts v5. Backend: Railway (Next.js API routes + Postgres) + Helius webhooks for OHLCV indexing + Drizzle ORM.
Total: 6 active on-chain programs (~40,128 LOC Rust), ~97,567 LOC TypeScript/TSX, 14 specification documents.
Frontend deployed at: https://fraudsworth.fun
Public repo: https://github.com/MetalLegBob/drfraudsworth

**14-Document Specification Set:**
- DrFraudsworth_Overview.md
- AMM_Implementation.md
- Bonding_Curve_Spec.md
- Carnage_Fund_Spec.md
- Epoch_State_Machine_Spec.md
- New_Yield_System_Spec.md (staking-based)
- Protocol_Initialzation_and_Launch_Flow.md
- Soft_Peg_Arbitrage_Spec.md
- Tax_Pool_Logic_Spec.md
- Transfer_Hook_Spec.md
- Token_Program_Reference.md
- SolanaSetup.md
- VRF_Implementation_Reference.md
- VRF_Migration_Lessons.md

**Key Architectural Constraints:**
- CPI depth-4 limit for Carnage execution (at Solana maximum)
- Two-instruction atomic bundle for VRF + Carnage
- WSOL as SPL Token requires special handling in T22/SPL mixed pools
- 14-entry transfer hook whitelist (Transfer_Hook_Spec.md Section 4 is authoritative)
- Address Lookup Table required for Carnage Sell path (23 named + 8 remaining accounts)

**v3 Reference:**
- archive-V3 branch contains working Switchboard VRF implementation (captured in VRF docs)
- v3 approach was CPI-based (deprecated); v4 uses Switchboard On-Demand client-side pattern

## Constraints

- **Tech Stack**: Anchor/Rust for on-chain programs, Token-2022 standard -- non-negotiable
- **Security**: Crypto is dangerous; every spec must consider attack vectors
- **No Single Source of Truth**: Per the Overview doc, any conflict discovered must be resolved explicitly across all affected documents before proceeding
- **Token Program Compatibility**: Must handle both Token-2022 and SPL Token (WSOL) in pool implementations
- **CPI Depth**: Carnage execution at depth 4 (Solana limit) -- no room for additional CPI layers

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Documentation-first rebuild | v3 failed due to unvalidated architectural assumption; validate specs before code | ✓ Good -- 24 gaps found and fixed before writing any code |
| Iterative convergence process | Single-pass review insufficient; new docs may introduce new conflicts | ✓ Good -- 3 clean passes achieved, documentation stable |
| Document VRF learnings from v3 | Working implementation exists but knowledge isn't captured in specs | ✓ Good -- 736-line reference + 258-line lessons doc created |
| All spec discrepancies resolved as SPEC | v4 follows spec intent, not v3 shortcuts; 7 discrepancies reviewed | ✓ Good -- clean separation between spec intent and implementation pattern |
| Two-instruction atomic bundle for VRF+Carnage | Compute budget concerns resolved by splitting into 2 instructions in same tx | ✓ Good -- documented in Carnage Section 9.5 |
| Token naming: CRIME/FRAUD/PROFIT | Replace placeholder IPA/IPB/OP4 with thematic names | ✓ Good -- zero old names remain in active docs |
| Yield model: passive -> staking-based | Staking requirement adds utility to PROFIT token | ✓ Good -- old spec archived, all references updated |
| 14-category coverage framework | Systematic gap detection across all specs | ✓ Good -- caught 24 gaps including CPI depth-4 limit |
| Foundation doc boost (+1 severity) | Foundation document issues cascade to all dependents | ✓ Good -- prioritized critical fixes first |
| 13-entry whitelist (Transfer_Hook_Spec.md authoritative) | Carnage needs 2 vaults, curves need 2 vaults, reserve whitelisted | ✓ Good -- user-confirmed, consistent across all docs |
| One program per milestone | Limits blast radius when specs meet reality; each program tested in isolation | ✓ Good -- v0.2-v0.6 all proved pattern works |
| Backend-first, no frontend until protocol works on devnet | On-chain programs are source of truth; frontend is just a client | ✓ Good -- protocol now proven on devnet, frontend is next |
| AMM first in build order | Everything else depends on AMM (pools, swaps); foundation must be solid | ✓ Good -- AMM shipped, 85 tests passing |
| litesvm with type bridge for Anchor 0.32.1 | anchor-litesvm incompatible; manual type conversion works | ✓ Good -- 59 integration tests working |
| PoolType uses 2 behavioral variants | MixedPool/PureT22Pool distinguishes transfer behavior, not protocol pools | ✓ Good -- simpler than 4-variant approach |
| seeds::program for cross-program auth | Carnage PDA must be signed by Epoch Program only | ✓ Good -- unauthorized access provably rejected |
| Synthetix-style cumulative reward-per-token | Industry-standard pattern for gas-efficient proportional distribution | ✓ Good -- 40,000 proptest iterations verify correctness |
| Custom transfer_checked_with_hook | Anchor SPL transfer_checked doesn't forward remaining_accounts | ✓ Good -- manual CPI build works correctly |
| Separate validator per test file | StakePool PDA is singleton; parallel test files need isolated state | ✓ Good -- 4 test files run independently |
| Tax Program epoch_program_id fixed | Was placeholder; updated to real Epoch Program ID | ✓ Good -- all cross-program CPI chains work (v0.7) |
| Feature-flagged Switchboard PID | #[cfg(feature = "devnet")] for compile-time PID selection | ✓ Good -- devnet and mainnet builds cleanly separated |
| CPI passthroughs as AccountInfo | Pool vaults/mints never read by Epoch, only forwarded to Tax | ✓ Good -- resolved BPF stack overflow, 741 bytes saved |
| Independent tax rolls (4 VRF bytes) | Each token gets independent magnitude roll, not shared rate | ✓ Good -- 45 unique rate combos in 50-epoch sample |
| HOOK_ACCOUNTS_PER_MINT = 4 | Token-2022 returns 4 accounts per mint (meta_list, wl_source, wl_dest, hook_program) | ✓ Good -- Sell path remaining_accounts partition correct |
| Protocol-wide Address Lookup Table | Sell path (23+8 accounts) exceeds 1232-byte TX limit | ✓ Good -- 46 addresses cached, v0 TX works |
| VRF timeout recovery over gateway rotation | Each randomness account assigned to specific oracle; rotation doesn't help | ✓ Good -- fresh randomness + retry more reliable |
| Timelock upgrade authority (48-72hr) | Keep upgradeability for SDK/bug fixes, but with user exit window | pending -- deployment policy decision |
| VRF bounty payment deferred | Treasury infrastructure not in current scope; emits 0 | pending -- pre-mainnet |
| Monorepo for frontend | IDLs, PDAs, program IDs, existing scripts all in one repo; no sync overhead | ✓ Good -- npm workspaces + transpilePackages, shared/ consumed by app/ and scripts/ |
| Railway + Postgres for backend | Predictable pricing, everything in one platform, avoids Supabase billing surprises | ✓ Good -- Nixpacks build, Drizzle migrations, health check, live at Railway URL |
| Helius webhooks for price indexing | Low effort, reliable, first-class Solana infrastructure provider | ✓ Good -- webhook receiver parsing 6 event types, OHLCV pipeline to TradingView |
| Frontend tech before design | Build all functionality with throwaway layout; plug in design assets later | ✓ Good -- all 32 requirements satisfied with unstyled but functional UI |
| Custom swap transaction builders | No existing DEX SDK maps to our Tax+AMM+Hook chain; security lives on-chain not in frontend | ✓ Good -- 8 builder functions covering all token pairs, hook resolution |
| Privy in v0.8 (before bonding curves) | Build integration early so it's ready when curves need phone-gated access | ✓ Good initially -- later migrated to @solana/wallet-adapter in v1.1 (Blowfish compatibility + simpler architecture) |
| Zero-dep Sentry over @sentry/nextjs | All @sentry/* packages break Turbopack SSR (monkey-patch webpack/bundle server code) | ✓ Good -- lib/sentry.ts with fetch() to envelope API, works in browser + Node |
| Turbopack over webpack | Faster dev builds, Next.js future direction; required resolveAlias stubs for Node modules | ✓ Good -- 10x faster HMR, worth the polyfill complexity |
| Zero npm deps for visual layer | CSS-only animations avoid Framer Motion/GSAP bundle bloat and Turbopack issues | ✓ Good -- iris animation, slide-up, glow all pure CSS |
| Re-parent components into modals | Keep existing hook logic untouched; only change container/parent elements | ✓ Good -- zero hook regressions across 6 station modals |
| Contain-fit scene scaling | min() CSS function ensures factory fits any viewport with letterbox/pillarbox | ✓ Good -- works 1280-3840px without distortion |
| Iris-open clip-path animation | Dialog-relative coordinate conversion from click origin to modal center | ✓ Good -- visceral "opening a portal" feel |
| Toast via Popover API | popover=manual renders in top layer, above dialog::backdrop blur | ✓ Good -- toasts visible during modal interactions |
| useVisibility hook for data lifecycle | Page Visibility API + modal context gating pauses data fetching when hidden | ✓ Good -- 87% RPC credit reduction |
| SplashScreen over WelcomeModal | User preferred animated brass gear intro gate over localStorage-gated text dialog | ✓ Good -- more visceral, brand-establishing first impression |
| Tooltips removed | User decided glow hover provides sufficient interactive affordance | ✓ Good -- cleaner visual, less clutter |
| Postgres-backed Carnage events | /api/carnage-events replaces client-side RPC parsing, saves ~900 credits/hr | ✓ Good -- faster, cheaper, more reliable |
| Mobile CSS-only responsive | display swap via @media width < 64rem, no JS viewport detection | ✓ Good -- works in SSR, no hydration mismatch |
| 9-slice CSS border for kit frames | Dual-mode Frame (CSS-only with border-radius, asset-based with border-image) | ✓ Good -- covers both rounded and rectangular needs |
| Component kit built upfront | Phase 60 foundation consumed by all subsequent phases (61-69) | ✓ Good -- consistent primitives, zero per-phase rework |
| HTMLAudioElement streaming over AudioBuffer | Avoids 118MB decoded PCM; streaming plays immediately | ✓ Good -- fast load, low memory |
| SettingsProvider as preference hub | Single React context for slippage, priority, audio — localStorage persistence | ✓ Good -- one source of truth, no state desync |
| Phase 66 Documentation deferred | Protocol mechanics not finalized, documenting now would create stale docs | ✓ Good -- saves rework |
| SFX triggers deferred from Phase 68 | Background music is higher impact; SFX can layer on later | ✓ Good -- ships audio value without blocking |
| Two-pass deploy pattern | Feature-flagged programs need mint addresses; init mints first, rebuild, re-deploy | ✓ Good -- solved chicken-and-egg |
| deploy-all.sh pipeline | Automated Phase 0-4 with auto-detection of cluster URL | ✓ Good -- repeatable deployments |
| Buy+sell bonding curves with 15% sell tax | Original spec was buy-only; adding sells creates richer price discovery and the sell tax funds carnage on success or refunds on failure | ✓ Good -- 13.5M proptest iterations prove solvency; sell tax creates meaningful carnage/refund pool |
| Separate tax escrow from curve reserves | Curve reserves = ∫₀^supply P(x)dx deterministically; tax tracked separately for conditional routing (carnage vs refund) | ✓ Good -- clean separation enables correct refund math and carnage routing |
| Both curves must graduate (coupled) | Creates coordination game; neither pool seeds unless both curves fill within 48hrs | ✓ Good -- 48hr deadline + permissionless triggers create fair coordination |
| Open access (no whitelist) | Privy removed in v1.1; open access for now, sybil resistance deferred | ✓ Good -- 20M per-wallet cap is sole sybil resistance; simpler UX |
| CurveState single PDA (no ParticipantState) | ATA balance reads replace per-user PDA; eliminates account bloat | ✓ Good -- simpler, cheaper, no cleanup needed |
| Multi-TX graduation (not monolithic) | Avoids 1232-byte TX limit; uses existing AMM/vault instructions via orchestration script | ✓ Good -- idempotent checkpoint/resume pattern |
| Ceil-rounded tax (protocol-favored) | (sol * 1500 + 9999) / 10000 ensures rounding never shorts the protocol | ✓ Good -- 1-lamport rounding always favors vault solvency |
| Authority hardening (Phase 78) | Prevent front-running of program initialization; single security model under Squads multisig | ✓ Good -- all 7 programs gated; see Authority Map below |
| No emergency pause | Admin pause perceived as rug pull vector; timelocked multisig upgrade is the safety net | ✓ Good -- community trust preserved |
| Hard error on insufficient staking claim | Partial claims create confusing UX; fail-fast is simpler | ✓ Good -- clear error, user retries when funds available |
| Carnage shared module extraction | 1800 lines duplicated across execute_carnage.rs and execute_carnage_atomic.rs | ✓ Good -- CU profile unchanged (binary size identical) |
| Borsh serialization test over offset_of! | #[repr(C)] alignment padding differs from Borsh packed layout | ✓ Good -- tests verify actual on-chain behavior |
| Webhook+SSE over polling | 30s polling wasteful; Helius webhooks + server-sent events for real-time data | ✓ Good -- immediate updates, lower RPC usage |
| GitHub Actions CI (2 parallel jobs) | Rust + TypeScript tests on every push to main | ✓ Good -- catches regressions automatically |
| Code-first spec rewrites | Specs must describe actual code, not stale design intent | ✓ Good -- 4 core specs verified against program source |
| Vault solvency pre-transfer buffer | BOK formal verification identified sell edge case needing guard | ✓ Good -- constant buffer prevents race condition |

### Authority Map

Every program authority, its type, current holder, and burn plan.

| Program | Authority Type | Mechanism | Current Holder | Status |
|---------|---------------|-----------|----------------|--------|
| AMM | Upgrade Authority | BPF Loader ProgramData | Squads vault | Transferred (v1.4) |
| AMM | Admin PDA (AdminConfig) | has_one = admin | Squads vault | Transferred (v1.4). Retained for future features. |
| Transfer Hook | Upgrade Authority | BPF Loader ProgramData | Squads vault | Transferred (v1.4) |
| Transfer Hook | Whitelist Authority | authority = Some(signer) | Squads vault | Transferred (v1.4). Not burned -- retained for flexibility. |
| Transfer Hook | Metadata Update Authority (x3) | Metaplex | Squads vault | Transferred (v1.4) |
| Tax Program | Upgrade Authority | BPF Loader ProgramData | Squads vault | Transferred (v1.4) |
| Epoch Program | Upgrade Authority | BPF Loader ProgramData | Squads vault | Transferred (v1.4) |
| Staking | Upgrade Authority | BPF Loader ProgramData | Squads vault | Transferred (v1.4) |
| Conversion Vault | Upgrade Authority | BPF Loader ProgramData | Squads vault | Transferred (v1.4) |
| Bonding Curve | Program Account | — | CLOSED | Program closed post-graduation (rent reclaimed ~4.73 SOL) |
| CRIME/FRAUD/PROFIT | Mint Authority | — | BURNED | Irreversible. Supply fixed. |

**Current state:** All 11 authorities held by Squads 2-of-3 multisig vault with 1hr (3600s) timelock. Mint authorities permanently burned. Bonding Curve program closed.

**No emergency pause:** Admin pause perceived as rug pull vector. Timelocked multisig upgrade is the safety net.

---
*Last updated: 2026-03-25 after v1.4 milestone*
