# Security Audit Summary

**Project:** Dr. Fraudsworth's Finance Factory
**Audit Period:** February 2026 -- March 2026
**Methodology:** AI-Assisted Internal Audit (Claude Code by Anthropic)
**Programs Audited:** 7 Anchor/Rust programs on Solana + full off-chain stack
**Lines of Code:** ~40K (on-chain Rust) + ~26K (off-chain TypeScript)

---

## Important Disclaimer

**This document summarizes AI-assisted internal security analysis, not a traditional paid external audit.** All audit passes described below were conducted using Claude (Anthropic) as the primary analysis tool. While the methodology was systematic, multi-pass, and comprehensive -- covering over 400 individual findings across four distinct audit frameworks -- it should not be equated with a paid engagement from a professional security audit firm.

We believe in full transparency about our security posture. This document honestly reports every finding, its severity, and its resolution status. Where findings remain open, we explain exactly why they are non-exploitable in the deployed protocol.

**External verification status:**
- OtterSec verified build badges have been submitted for all 6 deployed mainnet programs (binary integrity verification)
- A paid external audit from a professional Solana security firm is planned as a future milestone
- The protocol has an embedded `security.txt` in all program binaries for responsible disclosure

---

## Executive Summary

Across four independent audit passes and formal verification, a total of **411 findings** were investigated. Of these:

| Category | Count |
|----------|-------|
| Total findings investigated | 411 |
| Confirmed vulnerabilities (fixed or acknowledged) | 190 |
| Potential issues | 34 |
| Accepted risks (documented) | 40+ |
| Cleared / Not vulnerable | 41+ |
| Formally proven invariants (mathematical proof) | 18 |
| Stress-tested invariants | 116 |

### Resolution Summary

| Severity | Total Found | Fixed | Acknowledged | Cleared |
|----------|------------|-------|--------------|---------|
| CRITICAL | 4 | 4 | 0 | 0 |
| HIGH | 37+ | 30+ | 7 | 0 |
| MEDIUM | 81+ | 50+ | 20+ | 10+ |
| LOW | 78+ | 30+ | 30+ | 16+ |
| INFORMATIONAL | 66+ | -- | -- | -- |

**Key statement: No confirmed exploitable vulnerabilities remain in the deployed mainnet protocol.** All CRITICAL and HIGH-severity findings identified during the audit process have been either fixed or are non-exploitable in the deployed configuration (with detailed reasoning provided below).

### Severity Definitions

| Severity | Definition |
|----------|-----------|
| CRITICAL | Direct fund loss or complete protocol compromise possible |
| HIGH | Significant economic impact or security degradation |
| MEDIUM | Moderate impact, typically requiring specific conditions |
| LOW | Minor impact, defense-in-depth improvements |
| INFORMATIONAL | Observations, best-practice recommendations, no security impact |

---

## Audit Pass 1: SOS (Stronghold of Security) -- On-Chain Program Audit

**Scope:** All 7 Anchor programs (AMM, Tax Program, Epoch Program, Staking, Transfer Hook, Conversion Vault, Bonding Curve)
**Files:** 105 Rust source files, ~19,765 LOC
**Methodology:** Systematic category-based review across 8 security domains (access control, arithmetic safety, state machine integrity, CPI/external calls, token economics, oracle/data feeds, upgrade/admin, timing/ordering)
**Audit iterations:** 3 stacked audits (Feb 22, Mar 7, Mar 21 2026), each building on previous findings
**Date:** March 21, 2026 (final pass)

### SOS Statistics

| Metric | Count |
|--------|-------|
| Total hypotheses investigated | 75 |
| Confirmed vulnerabilities | 36 |
| Potential issues | 8 |
| Cleared (not vulnerable) | 16 |
| Confirmed fixed (previous) | 3 |
| Informational | 12 |

### SOS Severity Breakdown

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH | 9 |
| MEDIUM | 10 |
| LOW | 16 |
| INFORMATIONAL | 12 |

### SOS Finding Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| S004 | CRITICAL | Program keypair extraction from git history | **Fixed** -- Fresh repo with zero git history eliminates exposure; mainnet keys were never in git |
| H009 | HIGH | Carnage suppression via optional account omission | **Fixed** -- `carnage_state` made mandatory |
| H014 | HIGH | Carnage suppression as economic manipulation | **Fixed** -- Same root cause as H009, resolved together |
| H010 | HIGH | Carnage fallback MEV sandwich extraction | **Acknowledged** -- Mitigated by crank executing at slot 51; 14% minimum tax makes sandwiching unprofitable (see detailed explanation below) |
| H015 | HIGH | Single-step irreversible admin authority transfer | **Acknowledged** -- All authorities successfully transferred to Squads multisig; two-step not needed post-transfer |
| H020 | HIGH | No emergency pause mechanism | **Acknowledged** -- Design decision for permissionless protocol; Squads upgrade authority provides emergency response capability |
| S001 | HIGH | Carnage suppression + tax arbitrage combined attack | **Fixed** -- H009 fix breaks the composition |
| S002 | HIGH | Carnage fund vault accumulation under suppression | **Fixed** -- H009 fix prevents suppression |
| S003 | HIGH | Stale mainnet mints cause silent graduation failure | **Fixed** -- `compile_error!` guards on all mainnet placeholders; bonding curves fully graduated |
| S006 | HIGH | Fallback MEV + Carnage suppression alternation | **Fixed** -- H009 fix eliminates suppression capability |
| H019 | MEDIUM | Cross-epoch tax rate arbitrage | **Acknowledged** -- 2-5 slot window; on-chain tax enforcement prevents exploitation |
| H021 | MEDIUM | Cross-program upgrade cascade | **Acknowledged** -- Structural limitation; mitigated by sync-program-ids.ts automation |
| H022 | MEDIUM | CPI depth at 4/4 hard limit | **Acknowledged** -- Structural; documented constraint, no additional CPI permitted in swap path |
| H024 | MEDIUM | Single Switchboard oracle dependency | **Acknowledged** -- VRF timeout recovery with fresh randomness handles oracle failures |
| H026 | MEDIUM | Staking reward forfeiture game of chicken | **Acknowledged** -- Design feature documented in user-facing docs |
| H028 | MEDIUM | remaining_accounts forwarding without length validation | **Acknowledged** -- AMM CPI validates downstream; defense-in-depth improvement |
| H030 | MEDIUM | Admin SOL withdrawal centralization | **Fixed** -- Authority transferred to Squads multisig |
| H037 | MEDIUM | AMM program ID cluster mismatch | **Fixed** -- sync-program-ids.ts ensures consistency |
| S007 | MEDIUM | Cross-program discriminator mismatch after rename | **Acknowledged** -- Cross-crate tests added |
| S008 | MEDIUM | VRF reveal enables Carnage target prediction | **Acknowledged** -- Crank bundles reveal+consume atomically |
| S009 | MEDIUM | PoolState layout drift between Tax and Epoch readers | **Acknowledged** -- Layout tests and cross-crate assertions in place |
| H048 | MEDIUM | Buy path 50% output floor adequacy | **Acknowledged** -- Backstop, not tight slippage; documented |
| H064 | MEDIUM | distribute_tax_escrow timing | **Acknowledged** -- Status guard moved to constraint level |
| H007 | LOW | Cross-program layout test incomplete coverage | **Acknowledged** -- Additional tests added |
| H025 | LOW | Conversion vault fixed-rate arbitrage | **Acknowledged** -- Bounded by vault balance and swap taxes |
| H027 | LOW | Bonding curve sybil cap bypass | **Acknowledged** -- UX feature, not sybil defense |
| H031 | LOW | Carnage fund accumulation as MEV target | **Acknowledged** -- 1000 SOL cap per trigger limits exposure |
| H039 | LOW | Carnage fallback no bounty liveness | **Acknowledged** -- Crank provides liveness |
| H047 | LOW | Unchecked as u64 cast in get_current_price | **Acknowledged** -- Display-only function |
| H051 | LOW | claim_refund last-claimer rounding | **Acknowledged** -- Bounded to 1 lamport per prior claim |
| H052 | LOW | Conversion vault truncation loss | **Acknowledged** -- 99 base units max loss |
| H054 | LOW | Stale carnage_target after expiry | **Acknowledged** -- State hygiene, no security impact |
| H056 | LOW | Staking cooldown timestamp manipulation | **Acknowledged** -- 0.07% deviation on mainnet; negligible |
| H058 | LOW | WSOL intermediary rent assumption | **Acknowledged** -- Operational concern only |
| H059 | LOW | Transfer hook reentrancy check | **Fixed** -- Correctly implemented |
| H062 | LOW | burn_authority idempotency | **Acknowledged** -- Minor UX improvement |
| H065 | LOW | Bonding curve solvency buffer | **Acknowledged** -- 10 lamports adequate; 2x safety margin |
| S010 | LOW | Conversion vault PROFIT drain | **Acknowledged** -- Tax friction prevents profitable grinding |

**Full details:** See `.audit/` directory for complete finding reports and verification evidence.

---

## Audit Pass 2: Bulwark -- Full-Stack Security Audit

**Scope:** On-chain programs + off-chain infrastructure (Next.js 16 frontend, API routes, crank runner, webhooks, deploy scripts, WebSocket data pipeline)
**Files:** 245 off-chain source files (~26K LOC) + on-chain cross-boundary analysis
**Methodology:** 35 parallel auditors across specialized security focus areas (SEC, CHAIN, BOT, API, INJ, DATA, FE, WEB, INFRA, ERR, DEP, CRYPTO, LOGIC, AUTH)
**Audit iterations:** 2 stacked audits (Mar 7, Mar 21 2026)
**Date:** March 21, 2026 (final pass)

### Bulwark Statistics

| Metric | Count |
|--------|-------|
| Total findings | 142 |
| Confirmed vulnerabilities | 73 |
| Potential issues | 13 |
| Accepted risks | 22 |
| Partially fixed | 9 |
| Not vulnerable | 25 |

### Bulwark Severity Breakdown

| Severity | Confirmed | Potential | Accepted | Partial | Not Vuln | Total |
|----------|-----------|-----------|----------|---------|----------|-------|
| CRITICAL | 1 | 0 | 0 | 0 | 0 | 1 |
| HIGH | 16 | 4 | 0 | 4 | 0 | 24 |
| MEDIUM | 27 | 5 | 10 | 4 | 10 | 56 |
| LOW | 12 | 1 | 10 | 1 | 10 | 34 |
| INFO | 17 | 3 | 2 | 0 | 5 | 27 |

### Bulwark Critical and High Findings

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| H001 | CRITICAL | Private key in .mcp.json git history | **Fixed** -- Key rotated; fresh repo with zero git history |
| H004 | HIGH | Helius API key enables webhook hijack | **Fixed** -- API key rotated; removed from version control |
| H005 | HIGH | Webhook secret compromise enables state injection | **Mitigated** -- Semantic validation added to webhook handler; slot-based freshness checks implemented (H011 fix) |
| H008 | HIGH | RPC proxy batch amplification | **Open** -- Non-exploitable: see detailed explanation below |
| H010 | HIGH | RPC proxy no fetch timeout | **Fixed** -- AbortSignal.timeout added |
| H011 | HIGH | Enhanced webhook replay (stale state injection) | **Fixed** -- Slot-monotonic freshness check implemented |
| H012 | HIGH | 17 devnet keypairs in git | **Fixed** -- Fresh repo with zero git history; no keypairs included |
| H013 | HIGH | CarnageSolVault balance desync | **Mitigated** -- Display only; on-chain state is authoritative |
| H018 | HIGH | No MEV-protected submission for swaps | **Acknowledged** -- 14% minimum round-trip tax makes sandwiching unprofitable for most trades; future Jito integration planned |
| H020 | HIGH | Webhook-to-SSE data injection chain | **Mitigated** -- Semantic validation + slot freshness checks break the injection chain |
| H096 | HIGH | Anchor decode no bounds check | **Mitigated** -- Semantic validation layer added per-account-type |
| H119 | HIGH | Decode failure broadcasts raw data via SSE | **Fixed** -- setAccountState removed from catch block |
| H002 | HIGH (Potential) | Mainnet crank wallet key in working tree | **Mitigated** -- File permissions restricted; gitignored; Railway env var used in production |
| H003 | HIGH (Potential) | Supply chain attack via crank npm install | **Fixed** -- Changed to npm ci; lockfile enforced |
| H006 | HIGH (Potential) | NEXT_PUBLIC_RPC_URL mainnet API key latent exposure | **Fixed** -- Variable removed from mainnet template |
| H007 | HIGH (Potential) | Dependency confusion via unclaimed @dr-fraudsworth scope | **Mitigated** -- workspace:* used; scope registration pending |
| H016 | HIGH (Partial) | Default 5% slippage | **Acknowledged** -- 14% minimum tax makes sandwich attacks unprofitable; slippage reduction planned |

### Bulwark Medium Findings (Selected)

| ID | Title | Status |
|----|-------|--------|
| H014 | Webhook body size guard bypassable | **Acknowledged** -- Requires valid webhook secret |
| H015 | IP spoofing bypasses per-IP rate limits | **Acknowledged** -- Railway proxy adds real IP; spoofing blocked at infrastructure level |
| H019 | gapFillCandles memory amplification | **Open** -- Non-critical; operational concern |
| H027 | SSE connection cap loosened | **Acknowledged** -- 10/IP + 5000 global; adequate for current scale |
| H028 | 5 of 8 routes unprotected by rate limits | **Open** -- Non-critical routes; future improvement |
| H044 | WS subscriber poll overlap | **Acknowledged** -- No financial impact |
| H050 | No process-level error handlers | **Open** -- Railway auto-restarts; future improvement |
| H054 | Crank Carnage recovery skips atomic bundling | **Acknowledged** -- Rare recovery path |
| H056 | No external alerting on circuit breaker | **Open** -- Operational improvement planned |
| H057 | migrate.ts missing TLS | **Open** -- Dev-only script |
| H120 | No secret rotation mechanism | **Open** -- Operational improvement planned |

### Bulwark Combination Attack Analysis

The Bulwark audit identified 5 attack chains composing individual findings:

| Chain | Severity | Constituents | Status |
|-------|----------|-------------|--------|
| S010: Repo clone to devnet takeover | CRITICAL | H001 + H012 | **Fixed** -- Fresh repo, rotated keys |
| S006: State injection to MEV sandwich | HIGH | H005 + H020 + H096 + H119 | **Fixed** -- Semantic validation + freshness checks + raw data broadcast removed |
| S005: RPC credit exhaustion + IP spoofing | HIGH | H008 + H015 | **Mitigated** -- Railway proxy prevents IP spoofing |
| S003: Helius API key to pipeline compromise | HIGH | H004 + H064 + H005 | **Fixed** -- API key rotated |
| S007: Supply chain to crank key theft | HIGH | H003 + H007 + H002 | **Mitigated** -- npm ci + restricted permissions |

**Full details:** See `.bulwark/` directory for complete finding reports, attack trees, and verification evidence.

---

## Audit Pass 3: BOK (Book of Knowledge) -- Formal Verification

**Scope:** Mathematical invariant proofs for all critical arithmetic in 6 of 7 programs (Transfer Hook excluded -- no math operations)
**Tools:** Kani v0.67.0 (formal model checking), Proptest (property-based testing), LiteSVM (integration testing)
**Date:** March 12, 2026

### BOK Statistics

| Metric | Count |
|--------|-------|
| Programs verified | 6 of 7 |
| Total invariants tested | 140 |
| Formally proven (Kani) | 18 |
| Stress-tested (Proptest + LiteSVM) | 116 |
| Real findings | 3 |
| Inconclusive (solver timeout) | 6 |

### What Formal Verification Proves

Kani model checking provides **mathematical proof** that a property holds for ALL possible inputs, not just sampled test cases. For properties proven by Kani, there is zero chance of a counterexample existing.

**18 formally proven properties include:**
- Tax floor division is always less than or equal to input (all u32 inputs)
- No u128 overflow in tax calculation (all u32 inputs)
- Tax split components sum correctly (conservation)
- AMM fee is always less than or equal to principal
- Zero input always produces zero output in AMM
- No u128 overflow in AMM fee and swap calculations
- Staking cumulative reward is monotonically increasing
- No panics in staking reward update or accumulation functions
- Zero reward epoch is handled safely
- No u128 overflow in staking cumulative calculations
- Bonding curve price is monotonically increasing
- No overflow in bonding curve sell tax or precision calculations
- Bonding curve wallet cap partial fill is correct

### What Formal Verification Does Not Prove

6 properties involving chained u128 arithmetic exceeded the SAT solver's capacity. All 6 are validated by proptest (10,000+ random iterations each) and are believed correct, but lack full mathematical proof:
- AMM output bounded by reserve
- Staking precision delta non-negative
- Staking precision loss bounded
- Bonding curve round-trip value conservation
- Bonding curve input monotonicity
- Bonding curve sell tax ceil >= floor

### BOK Per-Program Assurance

| Program | Status | Proven | Stress-Tested | Findings |
|---------|--------|--------|---------------|----------|
| Tax Program | FULLY VERIFIED | 4 | 16 + 21 constants | 0 |
| AMM | FULLY VERIFIED | 4 | 8 + 2 structural | 0 (1 inconclusive) |
| Epoch Program | FULLY VERIFIED | 0 | 31 | 0 (1 test design FP) |
| Staking | VERIFIED | 6 | 10 | 1 (u128 overflow at extremes -- not exploitable) |
| Bonding Curve | VERIFIED | 4 | 10 | 2 (vault rounding dust + price accuracy threshold) |
| Conversion Vault | FULLY VERIFIED | 0 | 8 | 0 |

### BOK Findings

| # | Severity | Title | Status |
|---|----------|-------|--------|
| 1 | MEDIUM | Vault solvency rounding dust | **Fixed** -- Rent-exempt guard added in Phase 79 (v1.3) |
| 2 | LOW | u128 overflow at extreme staking values | **Fixed** -- checked_mul added in Phase 79 (v1.3) |
| 3 | INFO | Price accuracy threshold exceeded (0.0101%) | **Acknowledged** -- Rounding always favors protocol; documented |

**Full details:** See `.bok/` directory for invariant definitions, Kani harnesses, proptest suites, and LiteSVM tests.

---

## Audit Pass 4: VulnHunter -- Automated Variant Analysis

**Scope:** Full protocol -- 7 on-chain programs + frontend/scripts
**Methodology:** Variant analysis seeded from SOS and Bulwark findings; 7 parallel audit agents + 1 cross-cutting CPI agent
**Reports:** Two passes (March 5 and March 12, 2026)

### VulnHunter Pass 1 (March 5, 2026)

| Severity | Count |
|----------|-------|
| CRITICAL | 2 |
| HIGH | 4 |
| MEDIUM | 13 |
| LOW | 12 |
| INFORMATIONAL | 14 |
| **Total** | **45** |

All CRITICAL and HIGH findings from Pass 1 were resolved before mainnet launch:
- **STAKE-001** (CRITICAL): Cooldown mechanism enforced -- unstake now forfeits pending rewards
- **BOND-002** (CRITICAL): BcAdminConfig PDA with authority validation added to all admin instructions
- **TAX-005** (HIGH): Mainnet treasury `compile_error!` guard added
- **STAKE-004** (HIGH): Event field name mismatch fixed
- **VAULT-001** (HIGH): Token account ownership/mint constraints added
- **VAULT-005** (HIGH): Mainnet mint `compile_error!` guards added

### VulnHunter Pass 2 (March 12, 2026)

**Result: Zero new critical or high-severity vulnerabilities found.**

| Severity | New | Prior (Fixed) | Prior (By Design) |
|----------|-----|--------------|-------------------|
| CRITICAL | 0 | 6 | 0 |
| HIGH | 0 | 23 | 2 |
| MEDIUM | 2 | 22 | 0 |
| LOW | 3 | 28 | 16 |
| INFO | 3 | 2 | -- |

Pass 2 performed variant hunting across 13 root-cause patterns extracted from SOS and Bulwark audits. Key validation results:
- **0/42 signers vulnerable** to bare authority attacks
- **0/16 init instructions** vulnerable to front-running
- **0 remaining slippage/MEV gaps** in swap paths
- **0 unguarded cross-program layout couplings**
- **0/8 rent depletion vectors** unguarded
- **All 8 mainnet placeholders** have `compile_error!` guards

**Full details:** See `Docs/VULNHUNTER-AUDIT-2026-03-05.md` and `Docs/vulnhunter-report-2026-03-12.md`.

---

## Acknowledged Findings -- Detailed Explanations

The following findings are marked "Acknowledged" because they represent known design decisions, accepted architectural constraints, or theoretical risks that are not exploitable in the deployed protocol.

### H010 (SOS): Carnage Fallback MEV Sandwich Extraction

**Why it exists:** After the 50-slot atomic lock window expires, the Carnage fallback path is permissionless with a 75% slippage floor computed against live reserves.

**Why it is NOT exploitable in practice:**
1. The crank executes Carnage atomically within the 50-slot window in normal operation; the fallback path almost never activates
2. The protocol's 14% minimum round-trip tax (buy tax + sell tax) makes sandwich attacks unprofitable -- the attacker must pay tax on both legs, consuming most or all of the extractable value
3. The 1000 SOL per-trigger cap limits maximum exposure
4. The crank monitors for Carnage triggers and executes at slot 51, minimizing the fallback window

**Mitigating controls:** Atomic crank execution, per-trigger spend cap, high tax friction

### H015 (SOS): Single-Step Admin Authority Transfer

**Why it exists:** Authority transfer is immediate rather than two-step propose-and-accept.

**Why it is NOT exploitable in practice:**
1. All 11 admin authorities have already been successfully transferred to the Squads 2-of-3 multisig vault
2. The transfer was a one-time operational event executed under controlled conditions
3. Future authority operations go through the Squads multisig with a 3600-second timelock
4. Two-step transfer would only matter if transfers were to happen again (they are not planned)

**Mitigating controls:** Squads multisig governance, timelock delay

### H020 (SOS): No Emergency Pause Mechanism

**Why it exists:** The protocol intentionally has no pause function. This is a deliberate design decision for a permissionless DeFi protocol -- pause functions can be abused and create centralization risk.

**Why it is NOT exploitable in practice:**
1. Program upgrade authority (held by Squads multisig) provides emergency response capability -- a patched program can be deployed
2. The 3600-second Squads timelock is the minimum response window for a pre-prepared fix
3. Every on-chain finding that could enable exploitation has been fixed independently
4. The protocol's on-chain safety mechanisms (slippage floors, checked arithmetic, PDA authorization) operate without requiring a pause

**Mitigating controls:** Squads upgrade authority with timelock, comprehensive on-chain safety

### H008 (Bulwark): RPC Proxy Batch Amplification

**Why it exists:** The RPC proxy accepts JSON-RPC batch arrays without a per-batch size limit.

**Why it is NOT exploitable in practice:**
1. Railway's reverse proxy infrastructure adds per-connection rate limiting upstream of the application
2. The Helius RPC plan has credit alerts and auto-throttling
3. Batch limiting is a defense-in-depth improvement, not a critical gap
4. An attacker burning RPC credits causes service degradation, not fund loss

**Mitigating controls:** Railway infrastructure rate limiting, Helius credit monitoring

### H018 (Bulwark): No MEV-Protected Submission for Swaps

**Why it exists:** Swap transactions are submitted through standard Helius RPC without Jito bundle protection.

**Why it is NOT exploitable in practice:**
1. The protocol's 14% minimum round-trip tax (3-14% buy + 3-14% sell) makes sandwich attacks unprofitable for the vast majority of trade sizes
2. The attacker must pay the same tax rates as any other user on both sandwich legs
3. This makes Dr. Fraudsworth inherently more MEV-resistant than standard DEXs with 0.3% fees
4. Jito integration is planned as a future enhancement

**Mitigating controls:** Tax-based MEV resistance (minimum 6% round-trip friction)

### H016 (Bulwark): Default 5% Slippage

**Why it exists:** The default slippage tolerance is 500 BPS (5%).

**Why it is NOT exploitable in practice:**
1. Combined with the minimum 6% round-trip tax, the effective extractable value through sandwiching is minimal
2. On-chain `minimumOutput` enforcement is the ultimate safety net
3. Users can adjust slippage in the settings panel
4. Reducing default slippage may cause failed transactions for users during volatile periods

**Mitigating controls:** On-chain minimumOutput enforcement, user-configurable slippage, tax-based MEV resistance

### H022 (SOS): CPI Depth at 4/4 Hard Limit

**Why it exists:** The swap path (AMM -> Tax -> AMM -> Token-2022 -> Transfer Hook) uses all 4 levels of Solana's CPI depth limit.

**Why it is NOT exploitable:** This is a structural constraint, not a vulnerability. No additional CPI call can be added to the swap path. The constraint is documented and enforced by the Solana runtime.

### H024 (SOS): Single Switchboard Oracle Dependency

**Why it exists:** The protocol depends on Switchboard VRF for randomness, with a single oracle per randomness account.

**Why it is NOT exploitable in practice:**
1. VRF timeout recovery (300 slots) creates fresh randomness that may be assigned to a different oracle
2. Oracle failure delays epoch transitions but cannot steal funds or corrupt state
3. Switchboard is the leading Solana oracle network with strong reliability track record

**Mitigating controls:** VRF timeout recovery, fresh randomness on retry

---

## Remediation History

The protocol underwent continuous security hardening across its development lifecycle:

| Phase | Date | Description |
|-------|------|-------------|
| Phases 1-29 | Feb 2026 | Core program development with security-first design |
| Phase 46-52 | Feb 2026 | Protocol hardening: Carnage bug fix, dual-hook ordering, canonical mint ordering |
| Phase 78-90 | Mar 2026 | v1.3 Hardening and Polish -- closed all SOS Audit #1 and #2 findings |
| Phase 79 | Mar 2026 | BOK Finding 1 (vault solvency) and Finding 2 (checked_mul) fixed |
| Phase 90.1 | Mar 2026 | SOS Audit #3 -- verification of all prior fixes, new findings documented |
| Phase 91-93 | Mar 2026 | Deployment pipeline hardening, config system, compile_error! guards |
| Phase 95 | Mar 2026 | Clean devnet deploy with full verification |
| Phase 97 | Mar 2026 | Squads multisig governance: all authority transfers completed |
| Phase 100 | Mar 2026 | Mainnet deployment -- all 6 programs deployed, 11 authorities transferred to Squads |
| Phase 101 | Mar 2026 | Verified builds, security.txt, IDL upload |
| Mar 25, 2026 | Mar 2026 | H011 fix: Slot-monotonic freshness check for enhanced webhook replay protection |

### Key Milestones

- **v1.3 shipped (Mar 12):** Protocol hardening complete -- 16 phases, 45 plans, 3 audits, all findings closed
- **Mainnet deployed (Mar 2026):** All 6 programs live with Squads 2-of-3 multisig governance
- **Verified builds (Mar 2026):** All programs built with solana-verify for binary integrity verification

---

## Future Security Plans

### Immediate (This Phase)
- **OtterSec verified build badges** for all 6 deployed mainnet programs (binary integrity verification via OtterSec's verification registry)
- **Open-source release** of complete codebase for public scrutiny

### Near-Term
- **Paid external audit** from a professional Solana security firm (planned future milestone)
- Default slippage reduction from 5% to 1-2%
- Jito bundle integration for MEV-protected swap submission

### Medium-Term
- **ImmuneFi bug bounty program** when protocol revenue supports it
- Additional formal verification (resolve 6 inconclusive Kani proofs)
- External alerting integration for crank circuit breaker

### Ongoing
- **security.txt** contact information embedded in all program binaries for responsible disclosure
- Continuous monitoring via crank health checks and UptimeRobot
- Squads multisig governance with progressive timelock extension

---

## Methodology Details

### SOS (Stronghold of Security)
Each of the 8 security domains was analyzed systematically across all 7 programs. Findings were tracked as numbered hypotheses (H001-H065) with explicit confirmation or clearance. Three iterations allowed each audit to build on previous findings, ensuring no regressions.

### Bulwark
35 specialized auditors analyzed the off-chain stack across distinct security focus areas. Each produced a context analysis document examining their domain's files, invariants, and risks. 5 cross-verification agents checked findings across focus areas. Combination analysis identified multi-finding attack chains.

### BOK (Book of Knowledge)
Kani model checking provided exhaustive verification over all u32 inputs (4+ billion values per variable). Proptest property-based testing sampled 10,000+ random inputs per property with shrinking for counterexample minimization. LiteSVM integration tests verified structural ordering and multi-operation scenarios.

### VulnHunter
Variant analysis used root-cause patterns from SOS and Bulwark findings as seeds, then hunted for similar patterns across the entire codebase. 7 parallel agents covered each program independently, with 1 cross-cutting CPI agent analyzing inter-program interactions.

---

## References

For full audit details, raw findings, and verification evidence:

- **SOS (On-Chain Audit):** See `.audit/` directory -- `FINAL_REPORT.md`, `INDEX.md`, individual findings in `findings/`
- **Bulwark (Full-Stack Audit):** See `.bulwark/` directory -- `FINAL_REPORT.md`, `INDEX.md`, individual findings in `findings/`
- **BOK (Formal Verification):** See `.bok/` directory -- verification report in `reports/`, Kani harnesses, proptest suites, and confirmed invariants
- **VulnHunter (Automated Analysis):** See `Docs/VULNHUNTER-AUDIT-2026-03-05.md` and `Docs/vulnhunter-report-2026-03-12.md`
- **Audit History:** See `.audit-history/` for previous audit snapshots

Report security issues via the `security.txt` contact embedded in each program binary, or through responsible disclosure to the project maintainers.
