# Dinh's Bulwark — Final Audit Report

**Project:** Dr. Fraudsworth's Finance Factory
**Audit Scope:** Off-chain stack (frontend, API, crank, deploy scripts, infrastructure)
**Audit Number:** 2 (stacked on Audit #1 from 2026-03-07)
**Date:** 2026-03-21
**Auditor:** Dinh's Bulwark v1.0
**Tier:** Deep (35 auditors, 142 findings)

---

## Executive Summary

Dr. Fraudsworth's Finance Factory is a Solana DeFi protocol comprising 7 on-chain Anchor programs (separately audited by SOS) with an off-chain stack consisting of a Next.js 16 frontend on Railway, a server-side WebSocket data pipeline (Helius RPC to ws-subscriber to protocolStore to SSE to browser), PostgreSQL candle/event storage, and a standalone crank runner managing VRF-driven epoch transitions and Carnage buyback-and-burn. This audit examined the entire off-chain surface: 90 security-material files across 8 API routes, 15 hooks, 17 core library files, 7 swap/curve/staking modules, 4 crank/VRF files, 17 deploy scripts, 4 provider components, and 10 configuration files.

Compared to Audit #1 (2026-03-07, 71 confirmed, 23 potential), the protocol's security posture has materially improved in several areas: webhook authentication is now fail-closed with timing-safe comparison, SSE connection caps prevent amplification DoS, the RPC proxy correctly hides the Helius API key from client bundles, and the quote engine uses BigInt throughout. However, the major DBS refactoring (moving from per-browser RPC polling to server-side WebSocket subscriptions with SSE broadcast) introduced substantial new attack surface in the data pipeline — particularly around protocol state injection via the enhanced webhook path, which lacks semantic validation and replay protection. The most critical findings remain in the secrets domain: a real Solana private key committed in `.mcp.json` (git history), 17 devnet keypairs tracked by git, the mainnet Helius API key leaked in deployment reports, and the crank wallet key stored in a world-readable working-tree file.

This audit identified 142 total findings: 73 confirmed vulnerabilities, 13 potential issues, 9 partially fixed, 22 accepted risks, and 25 cleared as not vulnerable. The highest-impact findings are concentrated in three clusters: (1) secret exposure enabling fund theft and webhook hijack, (2) data pipeline integrity gaps enabling protocol state manipulation that can nullify on-chain slippage protection, and (3) MEV extraction surface from default 5% slippage with no private transaction submission. The combination analysis reveals that several individually MEDIUM findings chain into HIGH/CRITICAL attack trees — particularly the repo-clone-to-full-devnet-takeover chain (S010) and the state-injection-to-MEV-sandwich chain (S006).

---

## Audit Scope & Methodology

**Scope:** All off-chain code including the Next.js 16 frontend (`app/`), API routes, server-side data pipeline (ws-subscriber, protocol-store, SSE manager), crank runner, deploy scripts, configuration files, and infrastructure settings. On-chain Anchor programs were excluded (covered by SOS audit) except for cross-boundary analysis.

**Methodology:** 35 parallel auditors deployed across specialized focus areas (SEC-01/02, CHAIN-01 through 06, BOT-01/02, API-01/03/04, INJ-01 through 05, DATA-01/04/05, FE-01, WEB-02/04, INFRA-03/05, ERR-01 through 03, DEP-01, CRYPTO-01, LOGIC-01/02, AUTH-01/03). Each auditor produced a context analysis document examining their focus area's files, invariants, and risk observations. 5 verification agents cross-checked findings across focus areas. The investigation phase processed 132 base hypotheses plus 10 supplemental combination strategies (142 total), with each finding verified against source code evidence.

**Stacked audit structure:** This is Audit #2, stacked on Audit #1 (commit `173de12`, 2026-03-07). The HANDOVER document identified 26 findings requiring RECHECK due to modified primary files, plus 9 false-positive entries requiring re-evaluation. All 26 RECHECK items were re-examined: 7 confirmed resolved, 5 partially fixed, 1 regression, and 13 persisting gaps carried forward.

**Coverage:** 95% of the security-material codebase (89/90 files). 100% of API routes, hooks, core libraries, and config files. 94% of deploy scripts. 7 minor gaps identified (G001-G007), none in critical financial logic or key management paths.

---

## Severity Breakdown

| Severity | Confirmed | Potential | Accepted Risk | Partially Fixed | Not Vulnerable | Total |
|----------|-----------|-----------|---------------|-----------------|----------------|-------|
| CRITICAL | 1 | 0 | 0 | 0 | 0 | 1 |
| HIGH     | 16 | 4 | 0 | 4 | 0 | 24 |
| MEDIUM   | 27 | 5 | 10 | 4 | 10 | 56 |
| LOW      | 12 | 1 | 10 | 1 | 10 | 34 |
| INFO     | 17 | 3 | 2 | 0 | 5 | 27 |

**Total:** 73 confirmed, 13 potential, 22 accepted risk, 9 partially fixed, 25 not vulnerable = 142 findings.

**Finding Evolution (Audit #1 to #2):**

| Category | Count |
|----------|-------|
| NEW findings (first identified in Audit #2) | 76 |
| RECURRENT (persisting from Audit #1) | 37 |
| REGRESSION (was fixed, now broken) | 1 |
| RESOLVED (was open, now fixed) | 7 |
| NOT_VULNERABLE (cleared) | 25 |
| Audit #1 findings re-evaluated/re-scoped | 21 |

---

## Critical Findings

### H001: Private Key Extraction from .mcp.json in Git History

**Status:** CONFIRMED | **Severity:** CRITICAL | **Evolution:** NEW

A real Solana private key (`[REDACTED-DEVNET-PRIVKEY-PREFIX]...`) is committed in `.mcp.json` line 8 and has been in git history since commit `53ca01b` (pushed to `origin/main`). The key derives to public key `8kPzhQoUPx7LYM18f9TzskW4ZgvGyq4jMPYZikqmHMH4` — the primary devnet wallet holding ~59.5 SOL. This wallet is also the upgrade authority for all 6 devnet programs. The file is actively tracked (`git ls-files` returns it) and NOT in `.gitignore`.

**Affected code:** `.mcp.json:8`, `.gitignore` (missing entry)
**Impact:** Fund theft of devnet SOL, program upgrade authority compromise, AI tool exfiltration vector (`.mcp.json` is read by Claude Code at session start).

**Recommended fix:**
1. Rotate the devnet wallet immediately — generate new keypair, transfer funds, revoke upgrade authorities from old key
2. Add `.mcp.json` to `.gitignore`, replace key with placeholder
3. Purge git history with BFG Repo-Cleaner before any public repo access
4. Install pre-commit secret scanner (gitleaks, detect-secrets)

---

## High Findings

### H004: Helius API Key Enables Webhook Hijack

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** RECURRENT (Audit #1 H004/H131)

The devnet Helius API key (`[REDACTED-DEVNET-KEY]...`) is committed in `.env.devnet:8` and `shared/programs.ts:24`. This key grants full webhook management via `api.helius.xyz/v0/webhooks` — an attacker can register shadow webhooks for passive surveillance of all protocol events, or delete the production webhook to blind the SSE/chart data pipeline. The key has never been rotated across 3 commits.

**Affected code:** `.env.devnet:8`, `shared/programs.ts:24`, `scripts/webhook-manage.ts`
**Impact:** Passive surveillance of all protocol events; data pipeline disruption via webhook deletion; chart data corruption if combined with webhook secret.
**Fix:** Rotate Helius API key immediately. Remove from version control. Use separate Helius accounts per environment.

### H005: Webhook Secret Compromise Enables Protocol State Injection

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** NEW (DBS refactor surface)

If `HELIUS_WEBHOOK_SECRET` is obtained, an attacker can inject crafted Enhanced Account Change payloads into the protocol store. The webhook handler performs Anchor structural deserialization but applies zero semantic validation — crafted `reserveA=10^18` values pass decode and propagate through SSE to all browsers. The `updatedAt: Date.now()` stamp makes injected data appear fresh. Combined with H096 (no bounds checks), this enables manipulation of displayed prices, tax rates, and reserves, ultimately nullifying `minimumOutput` slippage protection.

**Affected code:** `app/app/api/webhooks/helius/route.ts:525-633`, `app/lib/protocol-store.ts:53-65`
**Impact:** Market manipulation via false price/rate display; MEV enablement by nullifying `minimumOutput`; affects all concurrent SSE users.
**Fix:** Add semantic validation after Anchor decode (range checks on tax BPS, reserve plausibility, enum validity). Cross-verify against ws-subscriber state. Add slot-based freshness checks.

### H008: RPC Proxy Batch Amplification

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** NEW

The `/api/rpc` proxy accepts JSON-RPC batch arrays with no size limit. Rate limiting counts HTTP requests (not RPC calls), so a batch of 500 `getAccountInfo` calls consumes 1 rate-limit token but burns 500 Helius credits. At 300 req/min rate limit, an attacker can exhaust 150,000 credits/minute per IP — depleting a 1M-credit Helius plan in under 7 minutes.

**Affected code:** `app/app/api/rpc/route.ts:102-106`
**Impact:** Helius credit exhaustion causing full frontend + crank RPC denial.
**Fix:** Enforce `MAX_BATCH_SIZE = 10` (or disable batching). Rate-limit per RPC method call, not per HTTP request.

### H010: RPC Proxy No Fetch Timeout

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** NEW

The upstream `fetch()` to Helius at `rpc/route.ts:144-148` has no `AbortSignal.timeout()`. A slow/hanging upstream holds Next.js workers indefinitely, cascading to all API routes. The existing pattern at `sol-price/route.ts` correctly uses `AbortSignal.timeout(5000)` — the RPC proxy was simply missed.

**Affected code:** `app/app/api/rpc/route.ts:144-148`
**Impact:** Complete service outage (all routes) from a single attacker staying within rate limits.
**Fix:** Add `signal: AbortSignal.timeout(10_000)` to the upstream fetch call.

### H011: Enhanced Webhook Replay — Stale State Injection

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** NEW (DBS refactor surface)

Enhanced Account Change webhooks bypass the 5-minute `MAX_TX_AGE_SECONDS` replay guard applied to raw TX webhooks. A captured enhanced webhook payload can be replayed indefinitely to overwrite current state with stale data. The protocol store has no slot-based freshness check — any payload with the webhook secret is accepted.

**Affected code:** `app/app/api/webhooks/helius/route.ts:340-341`, `app/app/api/webhooks/helius/route.ts:525-633`
**Impact:** Stale price/rate display to all users; potential for misinformed trading decisions.
**Fix:** Add slot-based monotonic check: reject enhanced payloads with slots older than the last accepted slot for each account.

### H012: 17 Devnet Keypairs in Git

**Status:** CONFIRMED (ESCALATED) | **Severity:** HIGH | **Evolution:** RECURRENT (Audit #1 H005 PARTIALLY_FIXED, now escalated with Squads keys)

17 devnet keypairs remain tracked in git, including all 6 program deploy keypairs, the devnet wallet, Squads creation key, and 3 Squads signer keys (added since Audit #1). The Squads authority transfer has NOT been executed — `squadsVault: null`. Compromise of these keys enables complete devnet program takeover before governance remediation can be applied.

**Affected code:** `keypairs/*.json` (17 files, all git-tracked)
**Impact:** Devnet program compromise; Squads governance neutralization; reputational damage.
**Fix:** Transfer all devnet upgrade authorities to Squads multisig. Remove keypairs from git tracking. Purge git history.

### H013: CarnageSolVault Balance Desync

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** NEW

The webhook handler uses `nativeBalanceChange` (a delta value from Helius enhanced webhooks) as an absolute balance when storing CarnageSolVault state. After any vault transaction, the displayed Carnage fund size desyncs from reality — potentially showing wildly incorrect values.

**Affected code:** `app/app/api/webhooks/helius/route.ts:554-558`
**Impact:** Misleading Carnage fund display; misinformed user decisions.
**Fix:** Use `getAccountInfo` to fetch actual lamports balance instead of relying on `nativeBalanceChange` delta.

### H018: No MEV-Protected Submission for Any User Swap

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** RECURRENT (Audit #1 H015, NOT_FIXED)

All user swap transactions are submitted through standard RPC (Helius JSON-RPC) to the public mempool. No Jito integration, no private mempool, no staked connections exist anywhere in the codebase. Combined with the default 500 BPS (5%) slippage, every swap is sandwichable with up to 5% value extraction. The sign-then-send pattern also bypasses Phantom's built-in MEV protection.

**Affected code:** `app/hooks/useProtocolWallet.ts:111`, `app/providers/SettingsProvider.tsx:170`
**Impact:** Up to 5% value extraction per swap via MEV sandwich attacks.
**Fix:** Integrate Jito bundle submission for swap transactions. Reduce default slippage to 100 BPS (1%). Add MEV protection warning in swap UI.

### H020: Webhook-to-SSE Data Injection Chain

**Status:** CONFIRMED (CONDITIONAL) | **Severity:** HIGH | **Evolution:** NEW

The complete injection chain from webhook POST to browser display is unobstructed — no secondary validation exists between webhook receipt and SSE broadcast. Crafted data passes through protocolStore and sseManager without any schema enforcement, range checking, or plausibility filtering. Gated on webhook secret compromise (see H005).

**Affected code:** Full chain: `webhooks/helius/route.ts` -> `protocol-store.ts` -> `sse-manager.ts` -> `useProtocolState.ts`
**Impact:** Display manipulation, MEV enablement, user deception for all concurrent users.
**Fix:** See H005 and H096 recommendations — semantic validation layer is the primary fix.

### H096: Anchor Decode No Bounds Check on Account Fields

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** NEW

This is the root cause underlying H005 and H020. Anchor's `coder.accounts.decode()` performs structural deserialization only — it validates Borsh types but not business-logic constraints. Tax BPS can be 0-65535, reserves can be any u64, enum variants can be any u8. No validation layer exists between decode and protocolStore storage.

**Affected code:** `app/app/api/webhooks/helius/route.ts:589-593`
**Impact:** Enables all state injection attacks described in H005, H020, S002, S006.
**Fix:** Add per-account-type validator functions with explicit range checks (tax BPS in [0,10000], reserves > 0 and < 10^19, enum variants in valid range).

### H119: Decode Failure Broadcasts Raw Data via SSE

**Status:** CONFIRMED | **Severity:** HIGH | **Evolution:** NEW

When Anchor decode throws an error, the catch block still calls `protocolStore.setAccountState()` with the raw, unvalidated webhook payload (`item.accountData`, `item.rawAccountData`). This means even a deliberately malformed payload that fails decode is stored and broadcast to all SSE clients — a fallback injection path that bypasses the decode-time validation entirely.

**Affected code:** `app/app/api/webhooks/helius/route.ts:607-619`
**Impact:** Protocol state corruption via SSE broadcast of raw attacker-controlled data.
**Fix:** Remove `setAccountState` call from the catch block. Log the error and return without updating the store.

### H002: Mainnet Crank Wallet Key in Working Tree

**Status:** POTENTIAL | **Severity:** HIGH | **Evolution:** NEW

The mainnet crank wallet private key (`F84XUxo5VM8FJZeGvC3CrHYwLzFod3ep57CULjZ4ZXc1`) is stored in plaintext at `.env.mainnet:88` with world-readable permissions (`0644`). The file is correctly gitignored and has never been committed, but the mainnet Helius API key from the same file HAS leaked into git history in 3 committed files (deployment report, planning docs).

**Affected code:** `.env.mainnet:88`, `scripts/deploy/deployment-report.md` (Helius key in git)
**Impact:** Crank wallet SOL theft via filesystem access; Helius API key compromise via git clone.
**Fix:** Restrict `.env.mainnet` to `chmod 600`. Rotate mainnet Helius API key. Remove WALLET_KEYPAIR from disk after confirming Railway env var is set. Scrub API keys from committed deployment reports.

### H003: Supply Chain Attack via Crank npm install

**Status:** POTENTIAL | **Severity:** HIGH | **Evolution:** RECURRENT (Audit #1 CRITICAL, partially fixed)

`railway-crank.toml` still uses `buildCommand = "npm install"` despite the Audit #1 fix that added `ignore-scripts=true` and committed `package-lock.json`. The fix is incomplete: `npm install` does not guarantee lockfile enforcement and does not clean-install `node_modules`. Module-body code injection (not blocked by `ignore-scripts`) remains a viable path for exfiltrating `WALLET_KEYPAIR`.

**Affected code:** `railway-crank.toml:3`
**Impact:** Crank wallet private key exfiltration via supply chain compromise.
**Fix:** Change `buildCommand = "npm install"` to `buildCommand = "npm ci"` in `railway-crank.toml`. One-line fix.

### H006: NEXT_PUBLIC_RPC_URL Mainnet API Key — Latent Exposure

**Status:** POTENTIAL | **Severity:** HIGH | **Evolution:** NEW

The `.env.mainnet` template instructs setting `NEXT_PUBLIC_RPC_URL` with the mainnet Helius API key. No current client code references this variable (the proxy pattern works correctly), but the template creates a latent exposure risk — any future client-side reference would inline the API key into the JavaScript bundle.

**Affected code:** `app/.env.mainnet:49`, `app/lib/connection.ts:41`, `app/app/api/rpc/route.ts:131`
**Impact:** Helius credit exhaustion and proxy bypass if regression occurs.
**Fix:** Remove `NEXT_PUBLIC_RPC_URL` from the mainnet template entirely. Remove server-side fallbacks to this variable.

### H007: Dependency Confusion via Unclaimed @dr-fraudsworth Scope

**Status:** POTENTIAL | **Severity:** HIGH | **Evolution:** RECURRENT (Audit #1 H066 cleared incorrectly)

The `@dr-fraudsworth` npm scope is unregistered. `app/package.json:18` uses bare version `"0.0.1"` instead of `workspace:*`. An attacker can claim the scope and publish a malicious package that resolves if workspace context is ever lost (e.g., `cd app && npm install`).

**Affected code:** `app/package.json:18`
**Impact:** Arbitrary code execution in build/runtime, crank wallet key exfiltration.
**Fix:** Change to `"workspace:*"` and register the `@dr-fraudsworth` npm scope as a placeholder.

### H016: Default 5% Slippage — Partially Mitigated by Tax Structure

**Status:** PARTIALLY_MITIGATED | **Severity:** HIGH | **Evolution:** RECURRENT (Audit #1 H015)

Default slippage remains 500 BPS (5%). On-chain tax structure provides partial mitigation (the 1-14% tax per swap makes sandwiching less profitable), but the extractable value remains significant. SettingsProvider validates range [0, 10000] from localStorage — a value injected via browser extension could reach 100% slippage.

**Affected code:** `app/providers/SettingsProvider.tsx:170`
**Impact:** Up to 5% MEV extraction per swap.
**Fix:** Reduce default to 100 BPS (1%). Add SettingsProvider cap at 5000 BPS to match UI.

---

## Medium Findings

### H014: Webhook Body Size Guard Bypassable via Chunked Encoding
**Status:** CONFIRMED | **Severity:** MEDIUM | The webhook body size check relies on Content-Length header, bypassable via chunked transfer encoding. Requires valid webhook secret, limiting exploitability.

### H015: IP Spoofing Bypasses All Per-IP Rate Limits
**Status:** CONFIRMED | **Severity:** MEDIUM | Rate limiter trusts `x-forwarded-for` without validation. Attacker can set arbitrary IPs to get fresh rate-limit buckets, bypassing all per-IP limits on all 3 rate-limited routes.

### H019: gapFillCandles Memory Amplification
**Status:** CONFIRMED | **Severity:** MEDIUM | `/api/candles?gapfill=true&resolution=1m` with a year range generates ~525K synthetic objects in memory. No range cap on the gap-fill query parameter.

### H026: Devnet Fallback Residual Risk
**Status:** CONFIRMED (PARTIALLY MITIGATED) | **Severity:** MEDIUM | `protocol-config.ts` defaults to `"devnet"` when `NEXT_PUBLIC_CLUSTER` is unset. Production validation exists but only in `ClusterConfigProvider`. If `protocol-config.ts` is imported before the provider validates, devnet addresses are used.

### H027: SSE Connection Cap Loosened
**Status:** CONFIRMED | **Severity:** MEDIUM | SSE caps increased from 3/IP + 100 global (Audit #1 verification) to 10/IP + 5000 global. At 5000 global connections, a distributed attacker using 500 IPs can saturate the server.

### H028: Rate Limiting — 5 of 8 Routes Unprotected
**Status:** CONFIRMED | **Severity:** MEDIUM | Only `/api/rpc`, `/api/webhooks/helius`, and `/api/sol-price` have rate limits. `/api/candles`, `/api/carnage-events`, `/api/health`, and both SSE routes lack rate limiting.

### H041: Health Endpoint Expanded Information Disclosure
**Status:** CONFIRMED | **Severity:** MEDIUM | Health endpoint now exposes ws-subscriber internal state, credit counter per-method breakdown, and dependency health — zero authentication. Enables timing attacks and infrastructure fingerprinting.

### H044: WS Subscriber Poll Overlap
**Status:** CONFIRMED | **Severity:** MEDIUM | `setInterval` polls (supply 60s, stakers 30s) have no overlap guard. If a poll takes longer than the interval, concurrent polls accumulate, consuming RPC credits and potentially corrupting store state.

### H045: ws-subscriber Double-Init TOCTOU
**Status:** CONFIRMED | **Severity:** MEDIUM | Race condition on `state.initialized` flag allows double initialization of ws-subscriber if two calls to `init()` occur before the first completes batchSeed.

### H047: No Rate Limits on DB Endpoints
**Status:** CONFIRMED | **Severity:** MEDIUM | `/api/candles` and `/api/carnage-events` query PostgreSQL with no rate limiting and no `connectionTimeoutMillis`. An attacker can exhaust the 10-connection pool, blocking all DB-dependent routes.

### H048: Stale Quote-to-Execution TOCTOU
**Status:** CONFIRMED | **Severity:** MEDIUM | Reserves used for quote computation (from SSE) may be stale by up to 60 seconds versus actual on-chain state at execution time. On-chain `minimumOutput` is the backstop, but stale quotes degrade UX.

### H049: Polling Fallback Incompatible Data Shape
**Status:** CONFIRMED | **Severity:** MEDIUM | When ws-subscriber falls back from WebSocket to HTTP polling, the data shape differs from the webhook-sourced format, potentially causing downstream parsing errors in SSE consumers.

### H050: No Process-Level Error Handlers
**Status:** CONFIRMED | **Severity:** MEDIUM | Next.js server has no `process.on('unhandledRejection')` or `process.on('uncaughtException')` handlers. An unhandled async error in any server-side code crashes the process without logging.

### H051: batchSeed Partial Failure Blocks Retry
**Status:** CONFIRMED | **Severity:** MEDIUM | If batchSeed partially fails (e.g., one RPC call times out), `state.initialized` is never set to `true`, permanently blocking ws-subscriber retry logic.

### H054: Crank Carnage Recovery Skips Atomic Bundling
**Status:** CONFIRMED | **Severity:** MEDIUM | VRF recovery path sends reveal + consume as separate transactions instead of the atomic bundle used in the normal path, creating a liveness failure window.

### H056: No External Alerting on Circuit Breaker Trip
**Status:** CONFIRMED | **Severity:** MEDIUM | Crank circuit breaker logs to console but sends no external alert (no PagerDuty, no Slack, no email). Silent failure means epoch transitions stop without operator notification.

### H057: migrate.ts Missing TLS
**Status:** CONFIRMED | **Severity:** MEDIUM | Database migration script connects without TLS even when the main app uses TLS in production.

### H058: Webhook Type Confusion
**Status:** CONFIRMED | **Severity:** MEDIUM | A payload that satisfies both raw TX and enhanced account-change discriminators could bypass the stronger raw TX validation (which has replay protection).

### H059: SSE Candle Route Leaks Protocol State
**Status:** CONFIRMED | **Severity:** MEDIUM | The SSE candle route also broadcasts protocol state updates, leaking protocol account data to a route intended only for candle streaming.

### H060: RPC Proxy as Free Transaction Relay
**Status:** CONFIRMED | **Severity:** MEDIUM | `sendTransaction` in the method allowlist means anyone can submit arbitrary Solana transactions through the project's paid Helius endpoint.

### H061: BigInt Tag Injection via Crafted SSE Data
**Status:** CONFIRMED | **Severity:** MEDIUM | Custom BigInt serialization in `bigint-json.ts` could be exploited if an attacker controls SSE data (via H005) to inject values that confuse the BigInt reviver.

### H065: Health Endpoint as DoS Timing Oracle
**Status:** CONFIRMED | **Severity:** MEDIUM | Unauthenticated health endpoint response time reveals server load, enabling an attacker to time DoS attempts for maximum impact.

### H067: SSE Broadcast Fan-Out Amplification
**Status:** CONFIRMED | **Severity:** MEDIUM | Each webhook delivery triggers a broadcast to all SSE subscribers. A burst of webhook deliveries (e.g., during high swap volume) creates O(webhooks * subscribers) network I/O.

### H070: Cluster Config Poisoning via Build Cache
**Status:** CONFIRMED | **Severity:** MEDIUM | `NEXT_PUBLIC_CLUSTER` is baked at build time via Next.js static replacement. If Railway's Nixpacks caches the build layer, a stale cluster value could persist across deployments.

### H075: No unhandledRejection Handler in Crank
**Status:** CONFIRMED | **Severity:** MEDIUM | Crank runner has no `process.on('unhandledRejection')` handler. An unhandled Promise rejection in any async code path crashes the crank without logging.

### H077: No Compute Budget on BC TXs
**Status:** CONFIRMED | **Severity:** MEDIUM | Bonding curve transactions do not include `ComputeBudgetProgram.setComputeUnitLimit()` or `setComputeUnitPrice()`. During congestion, these TXs may be deprioritized or dropped.

### H093: DB TLS Only in Production
**Status:** CONFIRMED | **Severity:** MEDIUM | Database connection uses TLS only when `NODE_ENV=production`. Staging/dev environments transmit credentials in plaintext.

### H110: Shell Injection via WALLET Env Var
**Status:** CONFIRMED | **Severity:** MEDIUM | `verify-authority.ts:390` passes the wallet path to a shell command without sanitization. An environment variable containing shell metacharacters could enable command injection.

### H111: Deploy Logger Writes to Arbitrary Path
**Status:** CONFIRMED | **Severity:** MEDIUM | The deploy logger accepts an arbitrary file path parameter without validation, enabling path traversal to write log files anywhere on the filesystem.

### H120: No Secret Rotation Mechanism
**Status:** CONFIRMED | **Severity:** MEDIUM | No dual-key rotation support exists for HELIUS_WEBHOOK_SECRET. Changing the env var causes 30-120 seconds of webhook authentication failures.

---

## Low Findings

| ID | Title | Status | Location | Description |
|----|-------|--------|----------|-------------|
| H017 | Sign-Then-Send Bypasses Wallet MEV Protection | POTENTIAL | `useProtocolWallet.ts:111` | sign-then-send pattern bypasses Phantom's MEV-protected submission |
| H032 | Sentry DSN Unconfigured in Devnet | PARTIAL_REGRESSION | `app/instrumentation-client.ts` | Sentry implementation present but DSN not configured for devnet |
| H034 | Double-Submit Lack In-Function Guard | PARTIALLY_FIXED | `useSwap.ts` | executeSwap/executeRoute lack in-function mutex; UI guards only |
| H036 | Float-to-Int Precision Regression | REGRESSION | Modified hooks | `toBaseUnits` uses `Math.floor(parseFloat(amount) * 10^decimals)` introducing precision loss |
| H042 | No Minimum Sell Amount | CONFIRMED | BuySellPanel.tsx | SellForm allows dust sells (1 lamport) with no minimum |
| H043 | skipPreflight on BC TXs Unjustified | CONFIRMED | `curve-tx-builder.ts` | Bonding curve TXs skip preflight without documented justification |
| H052 | No Price Impact Rejection Threshold | CONFIRMED | `route-engine.ts` | Route engine has no cap on acceptable price impact |
| H066 | Credit Counter Exposed via Health | CONFIRMED | `/api/health` | Per-method RPC credit breakdown exposed unauthenticated |
| H068 | Protocol Store Dedup Key Ordering | POTENTIAL | `protocol-store.ts:54-58` | JSON.stringify key ordering may cause dedup bypass |
| H069 | SSE Zombie Connection via NAT/CGNAT | CONFIRMED | `sse-connections.ts` | NAT users share IP; one corporate NAT can consume all 10 per-IP SSE slots |
| H071 | Missing CORS on SSE Responses | CONFIRMED | `api/sse/protocol/route.ts` | SSE responses lack explicit CORS headers |
| H074 | Error Truncation Loses Anchor Logs | CONFIRMED | `crank-runner.ts:529` | 300-char error truncation discards Anchor program logs |
| H078 | ALT Cache Never Invalidated | CONFIRMED | `multi-hop-builder.ts:261` | Module-level ALT cache has no invalidation; stale ALT causes TX failures |
| H079 | ATA Creation TOCTOU Race | CONFIRMED | `swap-builders.ts:229-240` | Race between ATA existence check and TX execution |
| H081 | Mixed-Denomination Fee Display | CONFIRMED | Route engine sell path | Legacy sell path displays fees in mixed SOL/token denomination |
| H082 | Additive Price Impact | CONFIRMED | Route engine | Price impact summed additively instead of multiplicatively (conservative, display-only) |
| H084 | Sell Fee Calculation Zero for Dust | CONFIRMED | Reference H038 | Integer division truncates sell fees to zero for dust amounts |
| H086 | Swap State Machine No Mutex | CONFIRMED | `useSwap.ts` | No mutex on executeSwap/executeRoute concurrent invocations |
| H089 | SSE Initial-State No Freshness | CONFIRMED | `api/sse/protocol/route.ts:71-76` | Initial SSE snapshot has no staleness indicator |
| H091 | Token Balance No Staleness Detection | CONFIRMED | `useTokenBalances.ts:72-112` | Token balance polling has no monotonic freshness check |
| H092 | WebSocket Reconnection Loses Events | CONFIRMED | `ws-subscriber.ts:299-323` | Events during WS reconnection gap are lost |
| H095 | Connection Pool No Idle Timeout | CONFIRMED | `app/db/connection.ts:56-57` | No `idle_timeout` on postgres.js pool; leaked connections persist |
| H097 | anchorToJson Shallow Conversion | CONFIRMED | `bigint-json.ts:93-117` | Nested BN/PublicKey fields not converted; future risk |
| H098 | SLOT_BROADCAST_INTERVAL No Bounds | CONFIRMED | ws-subscriber | Slot broadcast interval has no minimum/maximum bounds |
| H100 | CSP script-src unsafe-inline | CONFIRMED | `next.config.ts:33` | CSP allows unsafe-inline for scripts, weakening XSS protection |
| H101 | iframe sandbox allow-same-origin + allow-scripts | CONFIRMED | Config | Combination allows sandbox escape |
| H105 | Log Injection via Webhook Pool Type | CONFIRMED | `webhooks/helius/route.ts:407-409` | Pool type interpolated into log without validation |
| H106 | Debug Logging in Production | CONFIRMED | `app/launch/page.tsx:99-107` | Console.debug statements left in production code |
| H107 | Smoke Test Logs Raw CLUSTER_URL | CONFIRMED | `scripts/e2e/smoke-test.ts:36` | Smoke test logs full RPC URL including API key |
| H108 | Crank Health Binds 0.0.0.0 | CONFIRMED | `crank-runner.ts:185` | Crank health endpoint binds all interfaces |
| H115 | Webhook Response Leaks Processing Counts | CONFIRMED | `webhooks/helius/route.ts` | Response body reveals internal event processing counts |
| H118 | Rate Limiter Memory Growth | POTENTIAL | `rate-limit.ts` | Cleanup latency allows unbounded Map growth under sustained attack |
| H121 | Deprecated npm Packages | CONFIRMED | `package-lock.json` | glob@7.x, inflight@1.x deprecated (build-time only) |

---

## Informational Findings

| ID | Title | Status | Description |
|----|-------|--------|-------------|
| H038 | Fee Calculation Zero for Dust (RECHECK) | PARTIALLY_FIXED | Buy path fixed, sell path still truncates to zero |
| H046 | SSE Connection Limit Bypass Race | NOT_VULNERABLE | Potential under architectural change only |
| H053 | Split Route Amplifies Sandwich Surface | POTENTIAL | Multi-hop splits increase MEV surface (theoretical) |
| H062 | Chained Supply Chain + Webhook (RECHECK S001) | PARTIALLY_FIXED | Core protections intact, residual npm install gap |
| H063 | Launch Day Attack Bundle (RECHECK S004) | PARTIALLY_FIXED | New combination more severe than Audit #1 set |
| H064 | Browser-Console Webhook Hijack (RECHECK S008) | CONFIRMED | Webhook secret in git history enables full injection |
| H072 | Crank Wallet Balance Logging | ACCEPTED_RISK | Public info logged; no new exploitability |
| H080 | toBaseUnits Number Precision | ACCEPTED_RISK | Documented deviation from BigInt invariant |
| H085 | Token Supply INITIAL_SUPPLY Fallback | ACCEPTED_RISK | Display-only fallback |
| H090 | Slot Estimation Drift | ACCEPTED_RISK | Display-only slot interpolation |
| H094 | Bigint mode:"number" Truncation | ACCEPTED_RISK | postgres.js bigint mode risk for large values |
| H104 | Log Injection via RPC Method | NOT_VULNERABLE | Mitigated by method allowlist |
| H109 | COMMITMENT Env Var No Validation | ACCEPTED_RISK | Invalid value causes RPC error, not security issue |
| H112 | autoConnect with Empty Adapter | ACCEPTED_RISK | Standard wallet-adapter behavior |
| H113 | Wallet Icon Tracking Pixel | NOT_VULNERABLE | CSP img-src prevents this |
| H114 | Mobile Deep Link URL Manipulation | NOT_VULNERABLE | Well-mitigated by protocol design |
| H122 | Staking Escrow Monitoring Absent | CONFIRMED | No rent monitoring for staking escrow accounts |
| H126 | EpochState Layout Coupling | CONFIRMED | Layout dependency between on-chain and off-chain |
| H127 | DB Without TLS in Staging | CONFIRMED | Staging environment lacks TLS |

---

## Accepted Risk Register

| ID | Title | Severity | Rationale |
|----|-------|----------|-----------|
| H055 | No Distributed Lock for Crank | MEDIUM | Single Railway instance assumed; rolling deploys create brief overlap window. Documented risk. |
| H072 | Crank Wallet Balance Logging | LOW | Wallet balance is public on-chain data. Logging adds convenience, not new exposure. |
| H073 | Candle Close Price Ordering | LOW | Last-write-wins for concurrent webhooks. On-chain enforcement prevents financial impact. Display-only concern. |
| H076 | skipPreflight on Multi-Hop v0 TXs | LOW | Required for devnet v0 TX simulation bugs. To be revisited for mainnet. |
| H080 | toBaseUnits Number Precision | LOW | parseFloat precision loss documented; amounts < 2^53 / 10^decimals are safe. |
| H083 | Patch-Mint Trust Amplifier | LOW | generate-constants.ts reads keypairs from controlled directory. Risk accepted given control architecture. |
| H085 | Token Supply INITIAL_SUPPLY Fallback | LOW | Fallback to compile-time constant when RPC unavailable. Display-only. |
| H090 | Slot Estimation Drift | LOW | Client-side slot interpolation drifts ~2-5 slots. Display-only, no financial impact. |
| H094 | Bigint mode:"number" Truncation | LOW | postgres.js returns bigint columns as Number. Current column values within safe range. |
| H099 | Crank Spending Cap Estimated Cost | LOW | Spending cap uses estimated TX cost. Slight over/under-count acceptable for rate limiting. |
| H102 | Cross-Program Upgrade Cascade | MEDIUM | Mitigated by sync-program-ids.ts automation. Manual step required only for new cross-program refs. |
| H103 | Sentry Wildcard CSP | LOW | `*.ingest.us.sentry.io` is required for Sentry US region. DSN is compile-time fixed, not user-supplied. |
| H109 | COMMITMENT Env Var No Validation | LOW | Invalid commitment value causes RPC error, not a security bypass. |
| H112 | autoConnect with Empty Adapter | LOW | Standard wallet-adapter behavior. No security impact. |
| H123 | No Emergency Pause | MEDIUM | On-chain programs have no pause mechanism. Design decision — permissionless protocol. |
| H124 | Graduation Irreversibility | LOW | Graduation is a one-way state transition by design. Squads timelock provides review window. |
| H132 | Railway Dashboard SPOA | INFO | Railway dashboard is single point of admin access. Mitigated by 2FA and access controls. |

---

## Combination Attack Analysis

### Chain 1: Repo Clone → Full Devnet Takeover (S010)
**Constituents:** H001 (private key in .mcp.json) + H012 (17 keypairs in git)
**Severity:** CRITICAL
**Path:** Single `git clone` → extract devnet wallet key → drain ~59.5 SOL → extract program keypairs → deploy malicious upgrades to all 6 programs → extract Squads signer keys → neutralize planned governance
**Fix node:** H001 (key rotation + git history purge blocks the entire chain)

### Chain 2: State Injection → MEV Sandwich (S006)
**Constituents:** H005 (webhook state injection) + H020 (injection chain) + H096 (no bounds checks) + H119 (decode failure fallback)
**Severity:** HIGH
**Path:** Obtain HELIUS_WEBHOOK_SECRET → POST crafted PoolState with extreme reserves → protocolStore accepts without validation → SSE broadcasts to all browsers → useSwap computes minimumOutput ≈ 0 → user signs TX with nullified slippage guard → attacker sandwiches for full extractable value
**Fix node:** H096 (semantic validation after decode blocks the injection at its root)

### Chain 3: RPC Credit Exhaustion + IP Spoofing (S005)
**Constituents:** H008 (batch amplification) + H015 (IP spoofing)
**Severity:** HIGH
**Path:** Send batch of 500 RPC calls per HTTP request → rate limiter counts 1 → spoof x-forwarded-for to cycle IPs → unlimited credit burn → Helius returns 429 → all frontend + crank RPC fails
**Fix node:** H008 (batch size limit blocks amplification regardless of IP spoofing)

### Chain 4: Helius API Key → Full Pipeline Compromise (S003)
**Constituents:** H004 (API key in git) + H064 (webhook secret in git history) + H005 (state injection)
**Severity:** HIGH
**Path:** Clone repo → extract Helius API key → enumerate webhooks → discover webhook URL → extract webhook secret from git commit `0e8ff92` → inject crafted protocol state → manipulate all SSE clients
**Fix node:** H004 (rotate Helius API key and remove from version control)

### Chain 5: Supply Chain → Crank Key Theft (S007)
**Constituents:** H003 (npm install in crank) + H007 (unclaimed npm scope) + H002 (wallet key on disk)
**Severity:** HIGH
**Path:** Claim @dr-fraudsworth npm scope → publish malicious package → wait for crank redeploy → npm install resolves malicious version → module-body code reads WALLET_KEYPAIR → exfiltrate crank wallet key
**Fix node:** H007 (register npm scope) OR H003 (switch to npm ci) — either blocks the chain

### Priority of Fix Nodes by Chain Count

| Fix Node | Chains Blocked | Priority |
|----------|---------------|----------|
| H001 (key rotation + history purge) | S010, S001 | IMMEDIATE |
| H096 (semantic validation) | S006, S002, H005, H020, H119 | IMMEDIATE |
| H008 (batch size limit) | S005, S002, S009 | IMMEDIATE |
| H004 (Helius key rotation) | S003, S001, S008 | IMMEDIATE |
| H003 (npm ci) | S007, S004 | PRE-LAUNCH |
| H007 (npm scope registration) | S007, S004 | PRE-LAUNCH |

---

## Attack Trees

### Attack Tree 1: Full Devnet Takeover (S010)

**Goal:** Complete control of all devnet protocol programs and funds.

```
git clone (repo access)
├── Extract .mcp.json private key (H001)
│   ├── Drain devnet wallet (~59.5 SOL)
│   └── Upgrade authority over all 6 programs
├── Extract program keypairs (H012)
│   └── Deploy malicious program upgrades
└── Extract Squads signer keys (H012)
    └── Neutralize governance before it's activated
```

**Prerequisites:** Repository read access (collaborator, CI system, future public release)
**Likelihood:** MEDIUM (private repo, but 1,922 commits of history)
**Impact:** CRITICAL (complete devnet protocol takeover)
**Fix priority:** IMMEDIATE — rotate devnet wallet, purge git history

### Attack Tree 2: Protocol State Manipulation → MEV Extraction (S006)

**Goal:** Extract maximum value from user swaps via manipulated minimumOutput.

```
Obtain HELIUS_WEBHOOK_SECRET
├── Via git history (H064 — commit 0e8ff92)
├── Via Railway dashboard breach
├── Via supply chain compromise (H003/H007)
│
└── POST crafted webhook (H005)
    ├── Inject PoolState: reserveA=10^18
    │   └── No bounds check (H096)
    │       └── protocolStore accepts
    │           └── SSE broadcasts to all browsers
    │               └── useSwap: minimumOutput ≈ 0
    │                   └── User signs TX with no slippage guard
    │                       └── Attacker sandwiches (H018)
    │
    └── Inject EpochState: crimeBuyTaxBps=0
        └── Users see "0% tax" → buy incentive
            └── Attacker sells into volume
```

**Prerequisites:** Webhook secret (multiple acquisition vectors available)
**Likelihood:** MEDIUM-HIGH (secret in git history)
**Impact:** HIGH (MEV extraction from every affected user's swap)
**Fix priority:** IMMEDIATE — add semantic validation (H096), rotate webhook secret

### Attack Tree 3: RPC Credit Exhaustion → Service Outage (S005)

**Goal:** Take down the entire frontend and crank by exhausting Helius RPC credits.

```
Public /api/rpc endpoint (no auth required)
├── Batch amplification (H008)
│   └── 500 getAccountInfo per HTTP request
│       └── Rate limit counts 1 (not 500)
│           └── 150,000 credits/min per IP
│
└── IP spoofing (H015)
    └── Cycle x-forwarded-for values
        └── Fresh rate-limit bucket per fake IP
            └── Unlimited credit burn
                └── Helius returns HTTP 429
                    ├── Frontend: all swaps fail
                    ├── ws-subscriber: SSE data stale
                    └── Crank: epoch/VRF/Carnage halted
```

**Prerequisites:** None (public endpoint)
**Likelihood:** HIGH (trivial to execute)
**Impact:** HIGH (full service outage)
**Fix priority:** IMMEDIATE — add batch size limit

### Attack Tree 4: Supply Chain → Crank Wallet Drain (S007)

**Goal:** Exfiltrate crank wallet private key via compromised npm dependency.

```
Attacker claims @dr-fraudsworth npm scope (H007)
├── Publish malicious @dr-fraudsworth/shared@0.0.1
│   └── Module-body code: process.env.WALLET_KEYPAIR → exfil
│
└── Crank redeploys on Railway
    └── railway-crank.toml: npm install (H003)
        └── Lockfile drift or workspace context loss
            └── Malicious package installed
                └── import @dr-fraudsworth/shared triggers
                    └── WALLET_KEYPAIR exfiltrated
                        └── Crank wallet drained
                            └── Epoch/Carnage manipulation
```

**Prerequisites:** npm scope claim ($0, instant) + crank redeploy
**Likelihood:** LOW-MEDIUM (requires lockfile drift or workspace error)
**Impact:** CRITICAL (crank wallet key theft, protocol manipulation)
**Fix priority:** PRE-LAUNCH — npm ci + register scope

### Attack Tree 5: Data Pipeline Blinding (S008)

**Goal:** Blind the frontend data pipeline, freezing all charts and real-time displays.

```
Clone repo → extract Helius API key (H004)
│
├── DELETE production webhook
│   └── Webhook pipeline severed
│       └── No new candle data
│       └── No new protocol state updates
│       └── SSE clients receive stale data
│
└── Register shadow webhook (passive)
    └── All protocol events mirrored to attacker
        └── Front-running on epoch transitions
        └── MEV timing advantage
```

**Prerequisites:** Repository read access
**Likelihood:** MEDIUM
**Impact:** HIGH (complete data pipeline disruption + passive surveillance)
**Fix priority:** IMMEDIATE — rotate Helius API key

---

## Cross-Boundary Analysis (On-Chain / Off-Chain)

### Where off-chain relies on on-chain enforcement

| Off-Chain Assumption | On-Chain Enforcement | Gap |
|---------------------|---------------------|-----|
| Quote engine may use stale reserves (up to 60s) | `minimumOutput` enforced on every swap instruction | Gap is UX (wider slippage), not security — on-chain backstop works |
| Slippage defaults to 5% (H016/H018) | On-chain enforces `minimumOutput` regardless of value | Gap: user signs a permissive guard; attacker extracts up to the guard |
| SSE data could be corrupted (H005/H096) | On-chain programs have independent state | Gap: corrupted SSE → corrupted `minimumOutput` → on-chain guard nullified |
| Bonding curve TXs skip preflight (H043) | On-chain enforces all constraints | Gap: failed TXs cost priority fees; UX impact only |

### Where off-chain corruption undermines on-chain safety

The most critical cross-boundary vulnerability is the **state injection to minimumOutput nullification chain** (S006). The on-chain `minimumOutput` enforcement is designed as the ultimate safety net for user funds. However, because `minimumOutput` is computed off-chain from SSE-delivered reserves and tax rates, corrupting the SSE data pipeline (via H005/H096) effectively disarms this safety net. The user signs a transaction with `minimumOutput ≈ 0`, and the on-chain program faithfully enforces that permissive guard — the swap executes at real reserves but the MEV attacker extracts the full value because no minimum was imposed.

### On-chain assumptions not validated off-chain

| On-Chain Assumption | Off-Chain Reality |
|---------------------|-------------------|
| Epoch transitions are permissionless (any signer) | Crank is the only practical caller; crank compromise delays but cannot steal funds |
| Transfer hook whitelist checked on every transfer | Hook accounts derived deterministically (PDA); correct and immune to spoofing |
| Bonding curve authority = ANY signer (CRITICAL in SOS) | This is an on-chain vulnerability; off-chain cannot mitigate or worsen it |
| VRF reveal is public before consume | Crank bundles reveal+consume atomically; recovery path breaks atomicity (H054) |

---

## Audit Coverage

### Component Coverage

| Category | Files in Scope | Files Covered | Coverage |
|----------|---------------|--------------|---------|
| API Routes | 8 | 8 | 100% |
| Security-material hooks | 15 | 15 | 100% |
| Core lib files | 17 | 17 | 100% |
| Swap lib | 7 | 7 | 100% |
| Curve lib | 5 | 5 | 100% |
| Staking lib | 3 | 3 | 100% |
| Deploy scripts | 17 | 16 | 94% |
| Crank / VRF | 4 | 4 | 100% |
| Provider components | 4 | 4 | 100% |
| Config files | 10 | 10 | 100% |
| **TOTAL** | **90** | **89** | **99%** |

### KB Pattern Coverage: 34/34 (100%)

### Identified Gaps (G001-G007)

| Gap | Risk | Description |
|-----|------|-------------|
| G001 | MEDIUM | Stage shell scripts (stage-0 through stage-7) not individually audited |
| G002 | LOW | useSolPrice hook only partially covered |
| G003 | LOW | useSettings hook localStorage write-path validation gap |
| G004 | LOW | Test directories not audited for hardcoded secrets |
| G005 | LOW | scripts/deploy/lib/ not fully enumerated |
| G006 | LOW | sync-idl.mjs pre-build hook not individually reviewed |
| G007 | LOW | VRF ephemeral keypair disposal not fully traced |

---

## Finding Evolution (Stacked Audit Detail)

### Audit #1 Fixes That Survived the DBS Refactoring

| Finding | Description | Status in Audit #2 |
|---------|-------------|-------------------|
| H001/H023 | Webhook auth fail-closed + timingSafeEqual | **INTACT** (RESOLVED) |
| H002/H024 | Helius API key removed from client bundle | **INTACT** (PARTIALLY_FIXED — template risk) |
| H008/H025 | SSE connection caps | **INTACT** (caps loosened but functional) |
| H009/H026 | Devnet fallback removed | **INTACT** (residual risk in protocol-config default) |
| H029 | HSTS headers | **INTACT** (RESOLVED) |
| H031 | Webhook body size limit | **INTACT** (RESOLVED) |
| H033 | RPC failover chain | **INTACT** (RESOLVED) |
| H035 | SSE single-process acknowledgment | **INTACT** (RESOLVED) |
| H039 | BuyForm BigInt conversion | **INTACT** (RESOLVED) |
| H040 | Constants drift sync | **INTACT** (RESOLVED) |

### Fixes That Regressed

| Finding | Description | Regression Details |
|---------|-------------|-------------------|
| H036 | Float-to-Int precision (H012 recheck) | **REGRESSION** — new hooks introduced `toBaseUnits` pattern using `Math.floor(parseFloat(amount) * 10^decimals)` which loses precision for amounts with >15 significant digits. Severity bumped from LOW to LOW (unchanged in practice due to token decimal constraints). |

### New Attack Surface from DBS Changes

The DBS refactoring (Phase 102) introduced 5 new files and modified 40 existing files, creating the following new attack surface:

1. **ws-subscriber.ts** — WebSocket subscription manager with 4 concurrent polling intervals, no overlap guard, reconnection logic, and batchSeed initialization. New findings: H044 (poll overlap), H045 (double-init TOCTOU), H049 (polling fallback data shape), H051 (batchSeed partial failure), H092 (reconnection event loss), H098 (slot broadcast interval).

2. **protocol-store.ts** — In-memory state store with fragile dedup (JSON.stringify key ordering). New findings: H068 (dedup bypass), H088 (unbounded growth — cleared as NOT_VULNERABLE).

3. **credit-counter.ts** — RPC credit tracking primitive. New findings: H066 (health endpoint credit disclosure).

4. **Enhanced webhook path** — Account-change handling in the webhook route. New findings: H005 (state injection), H011 (replay bypass), H020 (injection chain), H058 (type confusion), H096 (no bounds check), H119 (decode failure fallback).

5. **SSE pipeline rework** — Modified sse-manager, sse-connections, and SSE route handlers. New findings: H027 (loosened caps), H059 (candle route leaks protocol state), H067 (fan-out amplification), H069 (NAT zombie connections), H071 (missing CORS), H089 (initial-state no freshness).

### False Positive Updates

The following Audit #1 NOT_VULNERABLE findings targeting modified files were re-evaluated:

| ID | Title | Re-evaluation Result |
|----|-------|---------------------|
| H006/H032 | WebSocket Reconnection Loss | Now covered by H092 (ws-subscriber loses events during reconnection) |
| H025 | CSP unsafe-inline XSS | Remains NOT_VULNERABLE — CSP unchanged, no new injection vectors |
| H048 | Sign-Then-Send | Remains ACCEPTED_RISK — documented project decision |
| H051 | CustomEvent RPC DoS | N/A — CustomEvent pattern removed in DBS refactor |
| H065 | WSOL ATA Race Condition | Now H079 — confirmed as LOW (ATA race exists but on-chain handles) |

---

## Remediation Roadmap

### Immediate (Before Mainnet Launch)

1. **H001 — Rotate devnet wallet key**
   - Generate new keypair: `solana-keygen new -o keypairs/devnet-wallet.json`
   - Transfer funds from `8kPzhQoUPx7LYM18f9TzskW4ZgvGyq4jMPYZikqmHMH4`
   - Add `.mcp.json` to `.gitignore`
   - Purge git history with BFG Repo-Cleaner

2. **H096 — Add semantic validation after Anchor decode**
   - File: `app/app/api/webhooks/helius/route.ts:589-606`
   - Add `validateDecodedAccount()` function with per-account-type range checks
   - Tax BPS in [0, 10000], reserves > 0 and < 10^19, valid enum variants

3. **H119 — Remove setAccountState from decode catch block**
   - File: `app/app/api/webhooks/helius/route.ts:607-619`
   - Replace with error logging only; do not store or broadcast failed decodes

4. **H008 — Enforce batch size limit on RPC proxy**
   - File: `app/app/api/rpc/route.ts:102-106`
   - Add `MAX_BATCH_SIZE = 10` check before forwarding
   - Reject batches exceeding limit with 400 Bad Request

5. **H010 — Add fetch timeout to RPC proxy**
   - File: `app/app/api/rpc/route.ts:144-148`
   - Add `signal: AbortSignal.timeout(10_000)` to upstream fetch

6. **H004 — Rotate Helius API key**
   - Generate new key in Helius dashboard
   - Update Railway env vars, `.env.devnet`, `shared/programs.ts`
   - Audit existing webhooks for unauthorized registrations

7. **H002 — Secure mainnet crank wallet**
   - `chmod 600 .env.mainnet`
   - Remove WALLET_KEYPAIR from local file after confirming Railway env var
   - Rotate mainnet Helius API key (leaked in git history)

### Pre-Launch (Within 1 Week)

8. **H003 — Change crank build to npm ci**
   - File: `railway-crank.toml:3`
   - Change: `buildCommand = "npm ci"`

9. **H007 — Register @dr-fraudsworth npm scope + use workspace:***
   - Register scope on npmjs.com
   - Change `app/package.json:18` to `"workspace:*"`

10. **H011 — Add slot-based replay protection for enhanced webhooks**
    - Track last-accepted slot per account in webhook handler
    - Reject payloads with slots older than last accepted

11. **H013 — Fix CarnageSolVault balance handling**
    - Use `getAccountInfo` for absolute lamports instead of `nativeBalanceChange` delta

12. **H018 — Integrate MEV-protected submission**
    - Integrate Jito bundle submission for swap transactions
    - Reduce default slippage from 500 BPS to 100 BPS in `SettingsProvider.tsx:170`

13. **H028 — Add rate limiting to all API routes**
    - Add rate limits to `/api/candles`, `/api/carnage-events`, `/api/health`

14. **H015 — Fix IP spoofing in rate limiter**
    - Use only the rightmost (trusted proxy) IP from `x-forwarded-for`

### Post-Launch (Within 1 Month)

15. **H041/H065/H066 — Secure health endpoint**
    - Add basic authentication or IP allowlisting to `/api/health`
    - Remove credit counter breakdown and ws-subscriber internals from response

16. **H044 — Add poll overlap guard to ws-subscriber**
    - Use `isPolling` flag or cancel previous poll before starting new one

17. **H050/H075 — Add process-level error handlers**
    - Add `process.on('unhandledRejection')` and `process.on('uncaughtException')` to both Next.js and crank

18. **H056 — Add external alerting on circuit breaker trip**
    - Integrate Slack/PagerDuty webhook for crank circuit breaker events

19. **H120 — Implement dual-key secret rotation**
    - Support `HELIUS_WEBHOOK_SECRET_NEXT` for zero-downtime rotation

20. **H006 — Remove NEXT_PUBLIC_RPC_URL from templates**
    - Remove from `.env.mainnet`, `connection.ts`, `rpc/route.ts`

### Backlog

21. H047 — Add connectionTimeoutMillis to DB pool
22. H051 — Fix batchSeed partial failure retry logic
23. H057 — Add TLS to migrate.ts
24. H058 — Strengthen webhook type discrimination
25. H060 — Consider removing sendTransaction from RPC allowlist
26. H067 — Add SSE broadcast throttling
27. H069 — Document NAT/CGNAT SSE limitation
28. H077 — Add ComputeBudgetProgram to BC TXs
29. H082/H081 — Fix fee display denomination
30. H093/H127 — Enforce TLS in all environments
31. H100 — Remove unsafe-inline from CSP when feasible
32. H110/H111 — Sanitize shell command inputs and file paths
33. H121 — Upgrade deprecated packages
34. Next.js upgrade to 16.2.1 (closes H009 CVE + 4 others)

### Fix Dependencies

- H096 must be fixed BEFORE H011 (validation layer is prerequisite for replay protection to be meaningful)
- H001 and H012 should be fixed together (single git history purge operation)
- H003 and H007 should be fixed together (both address supply chain attack surface)
- H008 and H015 should be fixed together (batch limit + IP validation close the credit exhaustion chain)
- H004 rotation should trigger H064 webhook secret rotation simultaneously

---

## Methodology

35 off-chain auditors deployed in parallel:

| ID | Name | Trigger Matches |
|----|------|----------------|
| SEC-01 | Private Key & Wallet Security | 234 |
| SEC-02 | Secret & Credential Management | 229 |
| CHAIN-01 | Transaction Construction & Signing | 131 |
| CHAIN-02 | RPC Client & Node Trust | 157 |
| CHAIN-03 | Wallet Integration & Adapter Security | 55 |
| CHAIN-04 | On-Chain/Off-Chain State Synchronization | 224 |
| CHAIN-05 | MEV & Transaction Ordering | 374 |
| CHAIN-06 | Program Account & PDA Interaction | 194 |
| BOT-01 | Keeper & Crank Security | 150 |
| BOT-02 | Trading & DeFi Bot Security | 283 |
| API-01 | REST API Security | 102 |
| API-03 | WebSocket & Real-Time Security | 361 |
| API-04 | Webhook & Callback Security | 211 |
| INJ-01 | SQL & NoSQL Injection | 145 |
| INJ-02 | Command & Code Injection | 73 |
| INJ-03 | SSRF (Server-Side Request Forgery) | 166 |
| INJ-04 | Path Traversal & File Access | 87 |
| INJ-05 | Prototype Pollution & Deserialization | 202 |
| DATA-01 | Database & Query Security | 236 |
| DATA-04 | Logging & Information Disclosure | 285 |
| DATA-05 | Encryption & Data Protection | 109 |
| FE-01 | Client-Side State & Storage | 52 |
| WEB-02 | CORS, CSP & Security Headers | 3 |
| WEB-04 | Open Redirect & URL Validation | 103 |
| INFRA-03 | Cloud & Environment Configuration | 332 |
| INFRA-05 | Monitoring, Metrics & Observability | 84 |
| ERR-01 | Error Handling & Fail Modes | 474 |
| ERR-02 | Race Conditions & Concurrency | 349 |
| ERR-03 | Rate Limiting & Resource Exhaustion | 145 |
| DEP-01 | Package & Dependency Security | 36 |
| CRYPTO-01 | Random Number Generation & Nonces | 82 |
| LOGIC-01 | Business Logic & Workflow Security | 372 |
| LOGIC-02 | Financial & Economic Logic | 348 |
| AUTH-01 | Authentication Mechanisms | 117 |
| AUTH-03 | Authorization & Access Control | 205 |

**Quality gate:** Passed (0 reruns)
**Verification agents:** 5 (cross-focus verification)
**Coverage:** 95% (89/90 security-material files)
**Total findings:** 142 (132 base + 10 supplemental)
**Total output:** 1,312 KB across 40 context files

---

## Appendix A: Complete Finding Index

| ID | Title | Severity | Status | Evolution | Category |
|----|-------|----------|--------|-----------|----------|
| H001 | Private Key in .mcp.json Git History | CRITICAL | CONFIRMED | NEW | Secrets |
| H002 | Mainnet Crank Wallet Key in Working Tree | HIGH | POTENTIAL | NEW | Secrets |
| H003 | npm Supply Chain via Crank npm install | HIGH | POTENTIAL | RECURRENT | Supply Chain |
| H004 | Helius API Key Enables Webhook Hijack | HIGH | CONFIRMED | RECURRENT | Secrets |
| H005 | Webhook Secret → Protocol State Injection | HIGH | CONFIRMED | NEW | Data Pipeline |
| H006 | NEXT_PUBLIC_RPC_URL Mainnet API Key Exposure | HIGH | POTENTIAL | NEW | Secrets |
| H007 | Dependency Confusion @dr-fraudsworth Scope | HIGH | POTENTIAL | RECURRENT | Supply Chain |
| H008 | RPC Proxy Batch Amplification | HIGH | CONFIRMED | NEW | API / DoS |
| H009 | Next.js Request Smuggling (GHSA-ggv3) | INFO | NOT_VULNERABLE | NEW | Supply Chain |
| H010 | RPC Proxy No Fetch Timeout | HIGH | CONFIRMED | NEW | API / DoS |
| H011 | Enhanced Webhook Replay — Stale Injection | HIGH | CONFIRMED | NEW | Data Pipeline |
| H012 | 17 Devnet Keypairs in Git | HIGH | CONFIRMED | RECURRENT | Secrets |
| H013 | CarnageSolVault Balance Desync | HIGH | CONFIRMED | NEW | Data Integrity |
| H014 | Webhook Body Size Bypass (Chunked) | MEDIUM | CONFIRMED | NEW | DoS |
| H015 | IP Spoofing Bypasses Rate Limits | MEDIUM | CONFIRMED | NEW | Rate Limiting |
| H016 | Default 5% Slippage — Partially Mitigated | HIGH | PARTIALLY_MITIGATED | RECURRENT | MEV |
| H017 | Sign-Then-Send Bypasses MEV Protection | LOW | POTENTIAL | RECURRENT | MEV |
| H018 | No MEV-Protected Submission | HIGH | CONFIRMED | RECURRENT | MEV |
| H019 | gapFillCandles Memory Amplification | MEDIUM | CONFIRMED | NEW | DoS |
| H020 | Webhook-to-SSE Injection Chain | HIGH | CONFIRMED | NEW | Data Pipeline |
| H021 | Candle Price Manipulation via Webhook | INFO | NOT_VULNERABLE | NEW | Data Integrity |
| H022 | Next.js GHSA-mq59 CSRF | INFO | NOT_VULNERABLE | NEW | Web Security |
| H023 | Webhook Auth Regression (H001 RECHECK) | INFO | RESOLVED | RESOLVED | Auth |
| H024 | API Key in Bundle (H002 RECHECK) | MEDIUM | PARTIALLY_FIXED | RECURRENT | Secrets |
| H025 | SSE Amplification DoS (H008 RECHECK) | INFO | NOT_VULNERABLE | RESOLVED | DoS |
| H026 | Devnet Fallback Residual (H009 RECHECK) | MEDIUM | CONFIRMED | RECURRENT | Config |
| H027 | SSE Connection Caps Loosened (H023 RECHECK) | MEDIUM | CONFIRMED | RECURRENT | DoS |
| H028 | Rate Limiting Gaps (H024 RECHECK) | MEDIUM | CONFIRMED | RECURRENT | Rate Limiting |
| H029 | HSTS Regression (H026 RECHECK) | INFO | NOT_VULNERABLE | RESOLVED | Web Security |
| H030 | Enhanced Replay Persistent Gap (H049 RECHECK) | HIGH | CONFIRMED | RECURRENT | Data Pipeline |
| H031 | Webhook Body Size Fix Intact (H050 RECHECK) | INFO | RESOLVED | RESOLVED | DoS |
| H032 | Sentry DSN Unconfigured (H045 RECHECK) | LOW | PARTIAL_REGRESSION | REGRESSION | Monitoring |
| H033 | RPC Failover Intact (H047 RECHECK) | INFO | RESOLVED | RESOLVED | Reliability |
| H034 | Double-Submit Partial (H034 RECHECK) | LOW | PARTIALLY_FIXED | RECURRENT | Concurrency |
| H035 | SSE Single-Process (H092 RECHECK) | INFO | RESOLVED | RESOLVED | Architecture |
| H036 | Float-to-Int Precision (H012 RECHECK) | LOW | REGRESSION | REGRESSION | Arithmetic |
| H037 | BN.toNumber Overflow (H096 RECHECK) | INFO | NOT_VULNERABLE | RESOLVED | Arithmetic |
| H038 | Fee Calc Zero for Dust (H119 RECHECK) | LOW | PARTIALLY_FIXED | RECURRENT | Arithmetic |
| H039 | BuyForm BigInt (H124 RECHECK) | INFO | RESOLVED | RESOLVED | Arithmetic |
| H040 | Constants Drift (H084 RECHECK) | INFO | RESOLVED | RESOLVED | Config |
| H041 | Health Endpoint Expanded Disclosure | MEDIUM | CONFIRMED | RECURRENT | Info Disclosure |
| H042 | No Minimum Sell Amount | LOW | CONFIRMED | RECURRENT | Business Logic |
| H043 | skipPreflight on BC TXs | LOW | CONFIRMED | RECURRENT | TX Construction |
| H044 | WS Subscriber Poll Overlap | MEDIUM | CONFIRMED | NEW | Concurrency |
| H045 | ws-subscriber Double-Init TOCTOU | MEDIUM | CONFIRMED | NEW | Concurrency |
| H046 | SSE Limit Bypass Race | INFO | NOT_VULNERABLE | NEW | Concurrency |
| H047 | DB Endpoints No Rate Limit | MEDIUM | CONFIRMED | NEW | Rate Limiting |
| H048 | Stale Quote TOCTOU | MEDIUM | CONFIRMED | NEW | Data Integrity |
| H049 | Polling Fallback Data Shape | MEDIUM | CONFIRMED | NEW | Data Integrity |
| H050 | No Process Error Handlers | MEDIUM | CONFIRMED | NEW | Error Handling |
| H051 | batchSeed Partial Failure | MEDIUM | CONFIRMED | NEW | Error Handling |
| H052 | No Price Impact Threshold | LOW | CONFIRMED | NEW | Business Logic |
| H053 | Split Route Sandwich Surface | LOW | POTENTIAL | NEW | MEV |
| H054 | Crank Carnage Recovery Non-Atomic | MEDIUM | CONFIRMED | NEW | Crank |
| H055 | No Distributed Crank Lock | MEDIUM | ACCEPTED_RISK | RECURRENT | Crank |
| H056 | No External Alerting on CB Trip | MEDIUM | CONFIRMED | RECURRENT | Monitoring |
| H057 | migrate.ts Missing TLS | MEDIUM | CONFIRMED | NEW | Encryption |
| H058 | Webhook Type Confusion | MEDIUM | CONFIRMED | NEW | Data Pipeline |
| H059 | SSE Candle Route Leaks Protocol | MEDIUM | CONFIRMED | NEW | Info Disclosure |
| H060 | RPC Proxy Free TX Relay | MEDIUM | CONFIRMED | NEW | API |
| H061 | BigInt Tag Injection | MEDIUM | CONFIRMED | NEW | Deserialization |
| H062 | Chained Supply Chain (S001 RECHECK) | MEDIUM | PARTIALLY_FIXED | RECURRENT | Combination |
| H063 | Launch Day Attack (S004 RECHECK) | MEDIUM | PARTIALLY_FIXED | RECURRENT | Combination |
| H064 | Webhook Hijack (S008 RECHECK) | HIGH | CONFIRMED | RECURRENT | Combination |
| H065 | Health Endpoint DoS Oracle | MEDIUM | CONFIRMED | NEW | Info Disclosure |
| H066 | Credit Counter Disclosure | LOW | CONFIRMED | NEW | Info Disclosure |
| H067 | SSE Fan-Out Amplification | MEDIUM | CONFIRMED | NEW | DoS |
| H068 | Protocol Store Dedup Bypass | MEDIUM | POTENTIAL | NEW | Data Integrity |
| H069 | SSE Zombie via NAT/CGNAT | LOW | CONFIRMED | NEW | DoS |
| H070 | Cluster Config Build Cache | MEDIUM | CONFIRMED | NEW | Config |
| H071 | Missing CORS on SSE | LOW | CONFIRMED | NEW | Web Security |
| H072 | Crank Balance Logging | LOW | ACCEPTED_RISK | RECURRENT | Info Disclosure |
| H073 | Candle Close Ordering | LOW | ACCEPTED_RISK | RECURRENT | Data Integrity |
| H074 | Error Truncation Loses Logs | LOW | CONFIRMED | RECURRENT | Error Handling |
| H075 | Crank No unhandledRejection | MEDIUM | CONFIRMED | RECURRENT | Error Handling |
| H076 | skipPreflight Multi-Hop | LOW | ACCEPTED_RISK | RECURRENT | TX Construction |
| H077 | No Compute Budget on BC | MEDIUM | CONFIRMED | RECURRENT | TX Construction |
| H078 | ALT Cache Never Invalidated | LOW | CONFIRMED | NEW | Caching |
| H079 | ATA Creation TOCTOU | LOW | CONFIRMED | NEW | Concurrency |
| H080 | toBaseUnits Number Precision | LOW | ACCEPTED_RISK | RECURRENT | Arithmetic |
| H081 | Mixed-Denomination Fee Display | LOW | CONFIRMED | RECURRENT | Display |
| H082 | Additive Price Impact | LOW | CONFIRMED | RECURRENT | Display |
| H083 | Patch-Mint Trust Amplifier | LOW | ACCEPTED_RISK | RECURRENT | Build Pipeline |
| H084 | Sell Fee Zero for Dust | LOW | CONFIRMED | RECURRENT | Arithmetic |
| H085 | Token Supply Fallback | LOW | ACCEPTED_RISK | NEW | Display |
| H086 | Swap State Machine No Mutex | LOW | CONFIRMED | RECURRENT | Concurrency |
| H087 | Cross-Epoch Tax Sniping | INFO | NOT_VULNERABLE | NEW | MEV |
| H088 | Protocol Store Unbounded Growth | INFO | NOT_VULNERABLE | NEW | Memory |
| H089 | SSE Initial-State No Freshness | LOW | CONFIRMED | NEW | Data Integrity |
| H090 | Slot Estimation Drift | LOW | ACCEPTED_RISK | NEW | Display |
| H091 | Token Balance No Staleness | LOW | CONFIRMED | NEW | Display |
| H092 | WS Reconnection Loses Events | LOW | CONFIRMED | NEW | Data Integrity |
| H093 | DB TLS Only in Production | MEDIUM | CONFIRMED | NEW | Encryption |
| H094 | Bigint mode:"number" Truncation | LOW | ACCEPTED_RISK | NEW | Data Type |
| H095 | Connection Pool No Idle Timeout | LOW | CONFIRMED | NEW | Database |
| H096 | Anchor Decode No Bounds Check | HIGH | CONFIRMED | NEW | Data Pipeline |
| H097 | anchorToJson Shallow Conversion | LOW | CONFIRMED | NEW | Serialization |
| H098 | SLOT_BROADCAST_INTERVAL No Bounds | LOW | CONFIRMED | NEW | Config |
| H099 | Crank Spending Cap Estimated | LOW | ACCEPTED_RISK | NEW | Crank |
| H100 | CSP unsafe-inline | LOW | CONFIRMED | NEW | Web Security |
| H101 | iframe sandbox Escape | LOW | CONFIRMED | NEW | Web Security |
| H102 | Cross-Program Upgrade Cascade | MEDIUM | ACCEPTED_RISK | RECURRENT | Build Pipeline |
| H103 | Sentry Wildcard CSP | LOW | ACCEPTED_RISK | RECURRENT | Web Security |
| H104 | Log Injection via RPC Method | INFO | NOT_VULNERABLE | NEW | Injection |
| H105 | Log Injection via Webhook Pool | LOW | CONFIRMED | NEW | Injection |
| H106 | Debug Logging in Production | LOW | CONFIRMED | NEW | Info Disclosure |
| H107 | Smoke Test Logs CLUSTER_URL | LOW | CONFIRMED | NEW | Info Disclosure |
| H108 | Crank Health Binds 0.0.0.0 | LOW | CONFIRMED | NEW | Network |
| H109 | COMMITMENT Env No Validation | LOW | ACCEPTED_RISK | RECURRENT | Config |
| H110 | Shell Injection via WALLET Var | MEDIUM | CONFIRMED | RECURRENT | Injection |
| H111 | Deploy Logger Arbitrary Path | MEDIUM | CONFIRMED | NEW | Path Traversal |
| H112 | autoConnect Empty Adapter | LOW | ACCEPTED_RISK | RECURRENT | Wallet |
| H113 | Wallet Icon Tracking Pixel | INFO | NOT_VULNERABLE | NEW | Privacy |
| H114 | Mobile Deep Link Manipulation | INFO | NOT_VULNERABLE | NEW | URL Validation |
| H115 | Webhook Response Leaks Counts | LOW | CONFIRMED | NEW | Info Disclosure |
| H116 | Protocol Store Arbitrary Keys | INFO | NOT_VULNERABLE | NEW | Data Integrity |
| H117 | SSE Connection Double-Release | INFO | NOT_VULNERABLE | NEW | Concurrency |
| H118 | Rate Limiter Memory Growth | MEDIUM | POTENTIAL | NEW | Memory |
| H119 | Decode Failure Broadcasts Raw Data | HIGH | CONFIRMED | NEW | Data Pipeline |
| H120 | No Secret Rotation Mechanism | MEDIUM | CONFIRMED | NEW | Operations |
| H121 | Deprecated npm Packages | LOW | CONFIRMED | RECURRENT | Supply Chain |
| H122 | Staking Escrow Monitoring Absent | INFO | CONFIRMED | RECURRENT | Monitoring |
| H123 | No Emergency Pause | MEDIUM | ACCEPTED_RISK | RECURRENT | Architecture |
| H124 | Graduation Irreversibility | LOW | ACCEPTED_RISK | RECURRENT | Architecture |
| H125 | Cross-Program Cascade Mitigated | INFO | MITIGATED | RECURRENT | Build Pipeline |
| H126 | EpochState Layout Coupling | INFO | CONFIRMED | RECURRENT | Architecture |
| H127 | DB Without TLS in Staging | INFO | CONFIRMED | NEW | Encryption |
| H128 | Vault Top-Up Cap Verified | INFO | NOT_VULNERABLE | RECURRENT | Verification |
| H129 | Crank Infinite Retry Fixed | INFO | NOT_VULNERABLE | RECURRENT | Verification |
| H130 | VRF Timeout Recovery Intact | INFO | NOT_VULNERABLE | RECURRENT | Verification |
| H131 | npm Supply Chain Guard Verified | INFO | NOT_VULNERABLE | RECURRENT | Verification |
| H132 | Railway Dashboard SPOA | INFO | ACCEPTED_RISK | RECURRENT | Infrastructure |
| S001 | Repo Clone → Devnet Drain + Webhook Hijack | HIGH | CONFIRMED | NEW | Combination |
| S002 | State Injection + RPC Exhaustion → MEV | HIGH | CONFIRMED | NEW | Combination |
| S003 | API Key → Webhook URL → Secret → Injection | HIGH | CONFIRMED | NEW | Combination |
| S004 | Dependency Confusion → Crank Key Theft | HIGH | POTENTIAL | NEW | Combination |
| S005 | Batch Amplification + IP Spoofing → Credit Burn | HIGH | CONFIRMED | NEW | Combination |
| S006 | State Injection → minimumOutput → MEV | HIGH | CONFIRMED | NEW | Combination |
| S007 | npm Supply Chain → Crank Theft → Epoch Control | HIGH | POTENTIAL | NEW | Combination |
| S008 | API Key → Shadow Webhook + Pipeline Blinding | HIGH | CONFIRMED | NEW | Combination |
| S009 | NEXT_PUBLIC_RPC_URL Regression → Credit Burn | MEDIUM | POTENTIAL | NEW | Combination |
| S010 | .mcp.json + Keypairs → Full Devnet Takeover | CRITICAL | CONFIRMED | NEW | Combination |

---

## Appendix B: Files Audited

### API Routes (8/8 — 100%)
- `app/app/api/rpc/route.ts` — 7 auditors
- `app/app/api/webhooks/helius/route.ts` — 8 auditors
- `app/app/api/candles/route.ts` — 4 auditors
- `app/app/api/carnage-events/route.ts` — 3 auditors
- `app/app/api/health/route.ts` — 5 auditors
- `app/app/api/sol-price/route.ts` — 4 auditors
- `app/app/api/sse/protocol/route.ts` — 5 auditors
- `app/app/api/sse/candles/route.ts` — 2 auditors

### Core Libraries (17/17 — 100%)
- `app/lib/connection.ts` — SEC-02, CHAIN-02, API-03
- `app/lib/protocol-store.ts` — API-04, CHAIN-04, ERR-02
- `app/lib/sse-manager.ts` — API-03, API-04, ERR-02
- `app/lib/sse-connections.ts` — ERR-03, API-03, ERR-02
- `app/lib/ws-subscriber.ts` — CHAIN-04, ERR-01, ERR-02, API-03, BOT-01
- `app/lib/credit-counter.ts` — ERR-03, CHAIN-02
- `app/lib/protocol-config.ts` — SEC-02, CHAIN-02, INFRA-03
- `app/lib/bigint-json.ts` — INJ-05, CHAIN-04, ERR-01
- `app/lib/anchor.ts` — CHAIN-02, CHAIN-06
- `app/lib/sentry.ts` — SEC-02, ERR-01, INFRA-05
- `app/lib/rate-limit.ts` — ERR-03
- `app/lib/confirm-transaction.ts` — ERR-01, ERR-02
- `app/lib/event-parser.ts` — API-04, INJ-05
- `app/lib/jupiter.ts` — INJ-03, WEB-02
- `app/lib/mobile-wallets.ts` — CHAIN-03, WEB-04
- `app/lib/solscan.ts` — WEB-04
- `app/lib/swap/quote-engine.ts` — LOGIC-02, ERR-02

### Crank / VRF (4/4 — 100%)
- `scripts/crank/crank-runner.ts` — BOT-01, SEC-01, SEC-02, ERR-01, INFRA-03, INFRA-05
- `scripts/crank/crank-provider.ts` — SEC-01, SEC-02
- `scripts/vrf/lib/vrf-flow.ts` — BOT-01, CRYPTO-01, ERR-02
- VRF epoch reader — BOT-01

### Deploy Scripts (16/17 — 94%)
- `scripts/deploy/initialize.ts` — SEC-01, SEC-02, INFRA-03, INJ-02
- `scripts/deploy/generate-constants.ts` — CHAIN-06, INFRA-03
- `scripts/deploy/sync-program-ids.ts` — CHAIN-06, INFRA-03
- `scripts/deploy/upload-metadata.ts` — SEC-01, SEC-02, INJ-04
- `scripts/deploy/deploy-all.sh` — INJ-02, INFRA-03
- `scripts/deploy/verify-authority.ts` — INJ-02, SEC-01
- `scripts/deploy/transfer-authority.ts` — SEC-01, INJ-02
- And 9 additional deploy helper scripts

---

*Generated by Dinh's Bulwark v1.0 — Deep Tier Off-Chain Security Audit*
*Audit #2 (Stacked) — 2026-03-21*
*35 auditors, 5 verification agents, 142 findings*
*Previous audit archive: `.bulwark-history/2026-03-16-173de12/`*
