# Off-Chain Attack Strategy Catalog

**Project:** Dr. Fraudsworth's Finance Factory
**Generated:** 2026-03-21
**Total Strategies:** 132
**Tier:** Deep
**Stacked Audit:** #2 (26 RECHECK findings from Audit #1)

---

## Strategy Generation Sources

- 35 focus area off-chain context analyses (1312 KB total output)
- 312 historical off-chain exploit patterns (OC-001 through OC-312)
- HANDOVER.md with 26 RECHECK + false positive log from Audit #1
- SOS ARCHITECTURE.md (on-chain trust model, 7 programs)
- Off-chain incident timeline (Bybit $1.5B, DMM $305M, Ledger Connect Kit, etc.)
- DBS change context (ws-subscriber, credit-counter, protocol-config, SSE rework)

---

## Tier 1 — CRITICAL Potential (22 strategies)

---

### H001: Private Key Extraction from .mcp.json in Git History
**Category:** Secrets & Key Management
**Origin:** KB (OC-001, OC-016)
**Estimated Priority:** Tier 1
**Hypothesis:** Attacker extracts committed Solana private key from `.mcp.json` in git history and drains any funds sent to that wallet.
**Attack Vector:** Clone repo → `git log -p .mcp.json` → extract base58 key → import to wallet → steal SOL/tokens.
**Target Code:** `.mcp.json:8` (key: `[REDACTED-DEVNET-PRIVKEY-PREFIX]...`), `.gitignore` (missing `.mcp.json`)
**Potential Impact:** CRITICAL — fund theft if wallet ever receives mainnet SOL
**Requires:** Repository read access (public or collaborator)
**Investigation Approach:** Verify key is real, check wallet balance, confirm git tracking status.

---

### H002: Mainnet Crank Wallet Key in Working Tree
**Category:** Secrets & Key Management
**Origin:** KB (OC-001, OC-002)
**Estimated Priority:** Tier 1
**Hypothesis:** Attacker obtains mainnet crank wallet private key from `.env.mainnet` file and drains crank SOL balance or submits malicious epoch transitions.
**Attack Vector:** Access to working tree (collaborator, backup, CI artifact) → extract WALLET_KEYPAIR JSON array → reconstruct Keypair → sign arbitrary TXs.
**Target Code:** `.env.mainnet:88` (WALLET_KEYPAIR), `.gitignore` (should be blocking)
**Potential Impact:** CRITICAL — crank wallet SOL theft (~6.87 SOL deployer + crank funds)
**Requires:** Working tree access (filesystem, backup, or accidental git add)
**Investigation Approach:** Verify `.env.mainnet` exists in working tree. Confirm gitignore is effective. Check git status for accidental tracking.

---

### H003: Supply Chain Attack via Crank npm install
**Category:** Supply Chain
**Origin:** KB (OC-235, OC-240)
**Estimated Priority:** Tier 1
**Hypothesis:** Crank Railway build uses `npm install` instead of `npm ci`, allowing lockfile modification during build. A supply chain attack could inject malicious code that exfiltrates WALLET_KEYPAIR env var.
**Attack Vector:** Compromise or publish a malicious version of any crank dependency → npm install resolves the malicious version → code reads process.env.WALLET_KEYPAIR → exfiltrates to attacker.
**Target Code:** `railway-crank.toml:3` (buildCommand: "npm install")
**Potential Impact:** CRITICAL — crank wallet private key theft
**Requires:** Malicious package in dependency tree (or lockfile modification)
**Investigation Approach:** Verify railway-crank.toml buildCommand. Check if .npmrc ignore-scripts prevents postinstall hooks.

---

### H004: Helius API Key Enables Webhook Hijack
**Category:** Secrets & Key Management
**Origin:** KB (OC-005, OC-016)
**Estimated Priority:** Tier 1
**Hypothesis:** Committed Helius API key in `.env.devnet` enables attacker to create shadow webhooks for passive surveillance or delete the production webhook to blind the data pipeline.
**Attack Vector:** Extract HELIUS_API_KEY from git → POST `https://api.helius.xyz/v0/webhooks?api-key=...` → register attacker-controlled webhook URL for same program IDs.
**Target Code:** `.env.devnet:8` (HELIUS_API_KEY=[REDACTED-DEVNET-KEY]...), `scripts/webhook-manage.ts`
**Potential Impact:** HIGH — passive surveillance of all protocol events OR data pipeline disruption
**Requires:** Repository read access
**Investigation Approach:** Verify API key scope (webhook CRUD vs RPC-only). Check if key works for mainnet webhooks.

---

### H005: Webhook Secret Compromise → Protocol State Injection
**Category:** Authentication × Data Pipeline
**Origin:** Novel
**Estimated Priority:** Tier 1
**Hypothesis:** If HELIUS_WEBHOOK_SECRET is obtained (Railway breach, git history, social engineering), attacker injects crafted Enhanced Account Change payloads into the protocol store, corrupting all SSE clients' view of pool reserves, epoch state, and staking data.
**Attack Vector:** Obtain webhook secret → POST crafted enhanced webhook payloads with fake EpochState/PoolState data → protocolStore stores it → SSE broadcasts to all browsers → users see manipulated prices/rates → make incorrect trading decisions.
**Target Code:** `webhooks/helius/route.ts:525-633` (enhanced path), `protocol-store.ts:53-65`
**Potential Impact:** CRITICAL — market manipulation via false price/rate display to all users
**Requires:** Webhook secret
**Investigation Approach:** Verify enhanced webhook path has no independent validation (slot check, range check on decoded values). Check if protocol store validates data plausibility.

---

### H006: NEXT_PUBLIC_RPC_URL Mainnet API Key in Client Bundle
**Category:** Secrets & Key Management
**Origin:** KB (OC-004, OC-011) + RECHECK (H002/H009)
**Estimated Priority:** Tier 1
**Hypothesis:** If mainnet deployment follows `.env.mainnet` template, NEXT_PUBLIC_RPC_URL with Helius API key is baked into the client JS bundle, bypassing the /api/rpc proxy entirely.
**Attack Vector:** View page source → extract NEXT_PUBLIC_RPC_URL → call any Helius RPC method directly (bypassing allowlist + rate limits) → burn credits, use for arbitrary TX relay.
**Target Code:** `app/.env.mainnet:49`, `app/lib/connection.ts:41`, `app/.env.local:1`
**Potential Impact:** HIGH — Helius credit exhaustion, proxy bypass
**Requires:** Mainnet deployment following template instructions
**Investigation Approach:** Check Railway mainnet env vars. Verify if NEXT_PUBLIC_RPC_URL is set.

---

### H007: Dependency Confusion via @dr-fraudsworth Scope
**Category:** Supply Chain
**Origin:** KB (OC-237)
**Estimated Priority:** Tier 1
**Hypothesis:** The @dr-fraudsworth npm scope is unregistered. An attacker claims it and publishes `@dr-fraudsworth/shared@0.0.1`, which could be resolved by misconfigured workspace setups.
**Attack Vector:** Register @dr-fraudsworth scope on npm → publish malicious @dr-fraudsworth/shared@0.0.1 → wait for CI/developer to resolve from public registry instead of workspace.
**Target Code:** `app/package.json:18` (uses bare "0.0.1" not "workspace:*")
**Potential Impact:** CRITICAL — arbitrary code execution in build/runtime
**Requires:** npm scope registration, misconfigured workspace
**Investigation Approach:** Check if scope is still unclaimed. Verify workspace resolution prevents registry fallback.

---

### H008: RPC Proxy Batch Amplification → Credit Exhaustion
**Category:** API Security × Resource Exhaustion
**Origin:** KB (OC-135, OC-283)
**Estimated Priority:** Tier 1
**Hypothesis:** Attacker sends single HTTP request with N-element JSON-RPC batch array through /api/rpc, consuming N Helius credits per rate-limit token.
**Attack Vector:** POST /api/rpc with `[{method:"getAccountInfo",...} × 500]` → rate limiter counts 1 request → Helius processes 500 calls → 150,000 credits/minute from single IP.
**Target Code:** `app/app/api/rpc/route.ts:102-106`
**Potential Impact:** HIGH — Helius credit exhaustion → full frontend + crank RPC denial
**Requires:** HTTP access to /api/rpc
**Investigation Approach:** Check if batch array size is limited. Check if per-batch credit counting exists.

---

### H009: Next.js HTTP Request Smuggling (CVE)
**Category:** Supply Chain × Web Security
**Origin:** KB (OC-232)
**Estimated Priority:** Tier 1
**Hypothesis:** next@16.1.6 has HTTP request smuggling vulnerability (GHSA-ggv3). App uses rewrites/headers configuration. Attacker could smuggle requests to bypass webhook auth or access internal routes.
**Attack Vector:** Craft HTTP request exploiting next.js rewrite handling → smuggle secondary request past webhook auth → inject data into pipeline.
**Target Code:** `app/package.json:29` (next@16.1.6), `app/next.config.ts` (rewrites/headers)
**Potential Impact:** HIGH — authentication bypass on webhook endpoint
**Requires:** Network access, specific request smuggling technique
**Investigation Approach:** Check CVE applicability to this app's next.config. Verify if Railway proxy prevents smuggling.

---

### H010: RPC Proxy No Fetch Timeout → Full Service DoS
**Category:** Error Handling × Availability
**Origin:** KB (OC-283)
**Estimated Priority:** Tier 1
**Hypothesis:** RPC proxy fetch() to Helius has no AbortSignal.timeout(). A slow/hanging upstream holds Next.js workers indefinitely, cascading to all API routes.
**Attack Vector:** If Helius endpoint hangs (overload, DNS issue, TLS negotiation stall) → 300 req/min rate limit means 300+ concurrent hanging requests within minutes → all Next.js workers blocked → health check, SSE, webhook all fail → Railway may not restart (health returns 200).
**Target Code:** `app/app/api/rpc/route.ts:144-148`
**Potential Impact:** HIGH — complete service outage (all routes, not just RPC)
**Requires:** Helius upstream slowdown (natural or induced)
**Investigation Approach:** Verify no AbortSignal.timeout or explicit timeout wrapper on fetch. Check Next.js worker pool size.

---

### H011: Enhanced Webhook Replay → Stale State Injection
**Category:** Webhook Security × Data Integrity
**Origin:** KB (OC-145) + RECHECK (H049)
**Estimated Priority:** Tier 1
**Hypothesis:** Enhanced Account Change webhooks bypass the 5-minute blockTime replay guard. A captured webhook payload can be replayed indefinitely to overwrite current state with stale data.
**Attack Vector:** Capture one enhanced webhook payload (via network intercept or compromised Helius account) → replay it after state changes → protocol store overwrites current data with old state → SSE broadcasts stale prices to all clients.
**Target Code:** `webhooks/helius/route.ts:340-341` (enhanced path skips MAX_TX_AGE_SECONDS), `webhooks/helius/route.ts:525-633`
**Potential Impact:** HIGH — stale price/rate display to all users, potential for misinformed trades
**Requires:** One captured webhook payload + webhook secret
**Investigation Approach:** Verify enhanced path has timestamp/slot-based freshness check. Check if protocolStore rejects older-than-current data.

---

### H012: 17 Devnet Keypairs in Git → Program Upgrade Authority
**Category:** Secrets & Key Management
**Origin:** KB (OC-016) + RECHECK (H005)
**Estimated Priority:** Tier 1
**Hypothesis:** Committed devnet program keypairs (amm, bonding-curve, epoch, staking, tax, transfer-hook, vault) control devnet program upgrade authority. Attacker upgrades devnet programs to malicious versions.
**Attack Vector:** Extract program keypairs from git → `solana program deploy --keypair <stolen-keypair>` → replace devnet programs → inject drainer logic.
**Target Code:** `keypairs/*.json` (17 files, git-tracked)
**Potential Impact:** HIGH — devnet program compromise, reputational damage, social engineering vector
**Requires:** Repository read access
**Investigation Approach:** Verify devnet upgrade authorities match keypair public keys. Check if any have been burned/transferred to Squads.

---

### H013: CarnageSolVault Balance Desync → User Manipulation
**Category:** Data Integrity × Business Logic
**Origin:** Novel
**Estimated Priority:** Tier 1
**Hypothesis:** CarnageSolVault webhook handler uses `nativeBalanceChange` (a delta) as absolute balance, causing the displayed Carnage fund size to desync from reality after any vault transaction.
**Attack Vector:** Natural protocol operation: each vault deposit/withdrawal overwrites absolute balance with a delta value → users see wildly incorrect Carnage fund balance → make trading decisions based on false data.
**Target Code:** `webhooks/helius/route.ts:554-558`
**Potential Impact:** HIGH — misleading Carnage fund display → misinformed user decisions
**Requires:** Any natural vault transaction
**Investigation Approach:** Check if nativeBalanceChange is indeed a delta. Verify the initial batchSeed sets correct absolute value. Check post-webhook behavior.

---

### H014: Webhook Content-Length Bypass → Memory Exhaustion
**Category:** Rate Limiting × DoS
**Origin:** KB (OC-135, OC-280) + RECHECK (H050)
**Estimated Priority:** Tier 1
**Hypothesis:** Webhook body size check relies on Content-Length header, which can be omitted via chunked transfer encoding. Multi-MB body is fully parsed by `req.json()`, exhausting server memory.
**Attack Vector:** POST to webhook with chunked transfer encoding (no Content-Length) + 100MB JSON body → bypasses 1MB guard → `req.json()` buffers entire body → server OOM.
**Target Code:** `webhooks/helius/route.ts:309-315`
**Potential Impact:** HIGH — server crash via OOM, service disruption
**Requires:** Valid webhook secret (to pass auth)
**Investigation Approach:** Verify Content-Length check is the only body size enforcement. Check if Next.js has its own limit. Test chunked encoding.

---

### H015: IP Spoofing Bypasses All Rate Limits
**Category:** Rate Limiting × Authentication
**Origin:** KB (OC-133)
**Estimated Priority:** Tier 1
**Hypothesis:** Rate limiter trusts x-forwarded-for first entry without proxy validation. Attacker spoofs arbitrary IPs to get fresh rate limit buckets on every request.
**Attack Vector:** Set `X-Forwarded-For: <random-ip>` header on each request → rate limiter creates fresh bucket → bypass all per-IP limits (webhook 120/min, RPC 300/min, sol-price 30/min).
**Target Code:** `app/lib/rate-limit.ts:129-151`
**Potential Impact:** HIGH — all rate limits ineffective
**Requires:** Ability to set custom headers (depends on Railway proxy behavior)
**Investigation Approach:** Verify if Railway strips/overwrites x-forwarded-for. Test with custom header.

---

### H016: Default 5% Slippage → Systematic Sandwich Extraction
**Category:** MEV × Trading
**Origin:** KB (OC-128, OC-129) + RECHECK (H015)
**Estimated Priority:** Tier 1
**Hypothesis:** Default 500 BPS slippage on all swaps enables systematic sandwich attack extraction of up to 5% per trade for any MEV bot monitoring the public mempool.
**Attack Vector:** Monitor Helius RPC mempool for Dr. Fraudsworth swap TXs → front-run with buy → let user TX execute at worse price → back-run with sell → profit up to 5% of swap amount.
**Target Code:** `SettingsProvider.tsx:170` (default 500), `useProtocolWallet.ts:111` (standard RPC, no Jito)
**Potential Impact:** HIGH — systematic value extraction from all default-slippage users
**Requires:** MEV bot monitoring public mempool
**Investigation Approach:** Confirm default is still 500 BPS. Confirm no Jito/private mempool integration.

---

### H017: Sign-Then-Send Bypasses Wallet MEV Protection
**Category:** MEV × Wallet Security
**Origin:** Novel
**Estimated Priority:** Tier 1
**Hypothesis:** The sign-then-send pattern (`signTransaction` + `sendRawTransaction`) explicitly bypasses wallet-level MEV protection features (Phantom's MEV-protected RPC) and simulation preview.
**Attack Vector:** User's wallet (e.g., Phantom) may offer MEV protection when using `sendTransaction()`. By using `signTransaction()`, the app forces all TXs through the public Helius RPC with zero MEV protection.
**Target Code:** `useProtocolWallet.ts:87-121`, MEMORY.md "Revisit for mainnet"
**Potential Impact:** HIGH — users lose wallet-native MEV protection
**Requires:** Mainnet deployment with sign-then-send pattern unchanged
**Investigation Approach:** Verify pattern is still sign-then-send. Check MEMORY.md note about mainnet revisit.

---

### H018: No MEV-Protected Submission for Any Swap
**Category:** MEV × Infrastructure
**Origin:** KB (OC-127, OC-258)
**Estimated Priority:** Tier 1
**Hypothesis:** Zero Jito bundle, private mempool, or MEV-protected RPC integration exists anywhere in the swap pipeline. All user swaps are fully visible in the public mempool.
**Attack Vector:** Standard sandwich attack on any swap TX visible in mempool.
**Target Code:** `useSwap.ts:763-766`, `multi-hop-builder.ts:378-381`, `useProtocolWallet.ts:111`
**Potential Impact:** HIGH — all swaps sandwichable
**Requires:** MEV bot
**Investigation Approach:** Grep for Jito, Nozomi, private_transaction, or any MEV protection keyword.

---

### H019: gapFillCandles Memory Amplification
**Category:** Resource Exhaustion
**Origin:** Novel
**Estimated Priority:** Tier 1
**Hypothesis:** `/api/candles?gapfill=true&resolution=1m` with a large time range generates ~525K synthetic candle objects in memory, potentially crashing the server.
**Attack Vector:** GET `/api/candles?pool=X&resolution=1m&from=0&to=<now>&gapfill=true` → server generates 525K objects → JSON serialization of massive array → OOM or long-running response blocking worker.
**Target Code:** `app/app/api/candles/route.ts:94-167`
**Potential Impact:** HIGH — server crash or worker exhaustion
**Requires:** HTTP access (no rate limit on this endpoint)
**Investigation Approach:** Verify gapFill loop has no cap on synthetic candles. Check if `from` parameter is bounded.

---

### H020: Webhook-to-SSE Data Injection Chain
**Category:** Authentication × Data Pipeline
**Origin:** Novel
**Estimated Priority:** Tier 1
**Hypothesis:** A compromised webhook secret enables injection of arbitrary protocol state that propagates to ALL connected browser clients via SSE, with no secondary validation layer.
**Attack Vector:** Obtain secret → craft Enhanced Account Change payload with extreme values (e.g., crimeBuyTaxBps=0, reserveA=1) → POST to webhook → Anchor decode succeeds → protocolStore stores → SSE broadcasts → all users see manipulated state.
**Target Code:** `webhooks/helius/route.ts:588-606`, `protocol-store.ts:53-65`, `sse/protocol/route.ts:71-76`
**Potential Impact:** CRITICAL — market manipulation affecting all connected users simultaneously
**Requires:** Webhook secret
**Investigation Approach:** Check if any bounds validation exists post-Anchor decode. Check if protocolStore validates data plausibility.

---

### H021: Candle Price Manipulation via Crafted Webhook
**Category:** Data Integrity × Webhook Security
**Origin:** Novel
**Estimated Priority:** Tier 1
**Hypothesis:** With webhook secret, attacker sends crafted TaxedSwap events with extreme prices. OHLCV candles use `GREATEST`/`LEAST` for high/low — a single extreme-price event permanently corrupts the candle.
**Attack Vector:** POST webhook with TaxedSwap event containing price=999999999 → candle upsert sets high=999999999 via SQL GREATEST → chart permanently shows extreme high → user confusion.
**Target Code:** `webhooks/helius/route.ts:664-681`, `candle-aggregator.ts:121-128`
**Potential Impact:** HIGH — permanent chart data corruption
**Requires:** Webhook secret
**Investigation Approach:** Verify candle upsert uses GREATEST/LEAST without bounds check. Check if extreme values are rejected.

---

### H022: Next.js CSRF Bypass (CVE in next@16.1.6)
**Category:** Supply Chain × Web Security
**Origin:** KB (OC-095, OC-232)
**Estimated Priority:** Tier 1
**Hypothesis:** next@16.1.6 has a CSRF bypass vulnerability (GHSA-mq59). The webhook endpoint processes POST requests — a CSRF attack could trigger webhook processing with attacker-controlled payload.
**Attack Vector:** Exploit next.js CSRF bypass → send POST to /api/webhooks/helius from cross-origin context → if auth check is bypassed by the CSRF vulnerability, inject data.
**Target Code:** `app/package.json:29` (next@16.1.6)
**Potential Impact:** HIGH — webhook auth bypass
**Requires:** Cross-origin request capability, specific CVE exploitation
**Investigation Approach:** Read GHSA-mq59 details. Determine if webhook route is affected.

---

## Tier 2 — HIGH Potential (42 strategies)

---

### H023: Webhook Auth Regression (RECHECK H001)
**Category:** Authentication
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H001 (FIXED) — file `webhooks/helius/route.ts` modified in delta
**Hypothesis:** Webhook auth fix may have regressed during the DBS refactor.
**Target Code:** `webhooks/helius/route.ts:266-302`
**Investigation Approach:** Verify fail-closed production guard intact. Verify timingSafeEqual still used. Check for new code paths that skip auth.

---

### H024: Helius API Key Still in Client Bundle (RECHECK H002)
**Category:** Secrets
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H002/S001 (FIXED) — `shared/constants.ts` modified
**Hypothesis:** API key may have been reintroduced in constants or env vars during refactor.
**Target Code:** `shared/constants.ts`, `shared/programs.ts:23-24`, `app/lib/connection.ts:41`
**Investigation Approach:** Grep for Helius URL patterns in shared/. Check NEXT_PUBLIC_RPC_URL usage.

---

### H025: SSE Amplification DoS Regression (RECHECK H008)
**Category:** DoS
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H008 (FIXED) — `sse-manager.ts` modified
**Hypothesis:** SSE connection limits may have regressed during DBS rework.
**Target Code:** `sse-manager.ts`, `sse-connections.ts`, `sse/protocol/route.ts`
**Investigation Approach:** Verify acquireConnection() still called before stream creation. Verify limits are same values.

---

### H026: Devnet Fallback in Production (RECHECK H009)
**Category:** Configuration
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H009 (FIXED) — `connection.ts` modified
**Hypothesis:** Cluster-aware connection may have devnet fallback paths reintroduced.
**Target Code:** `app/lib/connection.ts`, `app/lib/protocol-config.ts:25`
**Investigation Approach:** Verify NEXT_PUBLIC_CLUSTER default behavior. Check for hardcoded devnet endpoints.

---

### H027: SSE Connection Exhaustion Regression (RECHECK H023)
**Category:** DoS
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H023 (FIXED) — `sse-manager.ts` modified
**Target Code:** `sse-connections.ts:49-57`
**Investigation Approach:** Verify connection tracking is atomic. Check for race in acquireConnection.

---

### H028: Rate Limiting Regression (RECHECK H024)
**Category:** Rate Limiting
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H024 (FIXED) — API routes modified
**Target Code:** All `app/app/api/*/route.ts`
**Investigation Approach:** Verify checkRateLimit called in all modified routes. Check for new unprotected routes.

---

### H029: HSTS Regression (RECHECK H026)
**Category:** Web Security
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H026 (FIXED) — `next.config.ts` modified
**Target Code:** `app/next.config.ts:109-113`
**Investigation Approach:** Verify HSTS header still present with max-age, includeSubDomains, preload.

---

### H030: Webhook Replay Protection Regression (RECHECK H049)
**Category:** Webhook Security
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H049 (FIXED) — webhook route modified
**Target Code:** `webhooks/helius/route.ts:381-388`
**Investigation Approach:** Verify MAX_TX_AGE_SECONDS check still in raw TX path.

---

### H031: Webhook Body Size Limit Regression (RECHECK H050)
**Category:** DoS
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H050 (FIXED) — webhook route modified
**Target Code:** `webhooks/helius/route.ts:309-315`
**Investigation Approach:** Verify body size check still present. Note Content-Length-only limitation.

---

### H032: Error Reporting Regression (RECHECK H045)
**Category:** Monitoring
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H045 (FIXED) — `instrumentation.ts` modified
**Target Code:** `app/instrumentation.ts`, `app/lib/sentry.ts`
**Investigation Approach:** Verify Sentry integration still functional. Check captureException calls.

---

### H033: RPC Failover Regression (RECHECK H047)
**Category:** Availability
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H047 (FIXED) — `connection.ts` modified
**Target Code:** `app/lib/connection.ts`, `app/app/api/rpc/route.ts:128-137`
**Investigation Approach:** Verify failover chain still intact. Check sticky routing logic.

---

### H034: Double-Submit Regression (RECHECK H034)
**Category:** Business Logic
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H034 (FIXED) — swap/launch components modified
**Target Code:** `useSwap.ts:690-698`, `BuySellPanel.tsx`, `SwapForm.tsx`
**Investigation Approach:** Verify status guard exists in executeSwap/executeRoute. Check for new execution paths.

---

### H035: SSE Single-Process Regression (RECHECK H092)
**Category:** Architecture
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H092 (FIXED) — `sse-manager.ts` modified
**Target Code:** `sse-manager.ts`, `protocol-store.ts`
**Investigation Approach:** Verify globalThis singleton pattern intact. Check for horizontal scaling assumptions.

---

### H036: Float-to-Int Precision in Modified Hooks (RECHECK H012)
**Category:** Financial Logic
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H012 (FIXED) — multiple hooks modified
**Target Code:** `useSwap.ts:301-307`, `useStaking.ts`, frontend hooks
**Investigation Approach:** Verify toBaseUnits still uses parseFloat path. Check for new Number→BigInt conversions.

---

### H037: BN.toNumber Overflow in Modified Hooks (RECHECK H096)
**Category:** Financial Logic
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H096 (ACCEPTED_RISK) — hooks modified
**Target Code:** `usePoolPrices.ts:29-31`, `useCarnageData.ts:51-55`, `ws-subscriber.ts:219`
**Investigation Approach:** Check for new BN.toNumber() calls in modified hooks. Verify values within safe range.

---

### H038: Fee Calculation Zero for Dust (RECHECK H119)
**Category:** Financial Logic
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H119 (FIXED) — hooks modified
**Target Code:** Frontend hooks
**Investigation Approach:** Verify dust amount handling. Check minimum amount enforcement.

---

### H039: BuyForm BigInt Conversion (RECHECK H124)
**Category:** Financial Logic
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H124 (FIXED) — launch components modified
**Target Code:** `BuySellPanel.tsx`, `BuyForm.tsx`, `SellForm.tsx`
**Investigation Approach:** Verify BigInt conversion uses safe pattern.

---

### H040: Constants Drift (RECHECK H084)
**Category:** Configuration
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H084 (NOT_FIXED) — `shared/constants.ts` modified
**Target Code:** `shared/constants.ts`
**Investigation Approach:** Compare constants against deployments/*.json. Check sync-program-ids.ts pipeline.

---

### H041: Health Info Disclosure (RECHECK H028/H085)
**Category:** Information Disclosure
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H028 (NOT_FIXED) + H085 (ACCEPTED_RISK) — health route modified
**Target Code:** `app/app/api/health/route.ts:32-73`
**Investigation Approach:** Verify expanded health response. Catalog new fields exposed.

---

### H042: Minimum Sell Amount Still Missing (RECHECK H069)
**Category:** Business Logic
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H069 (NOT_FIXED) — launch components modified
**Target Code:** `BuySellPanel.tsx`, `SellForm.tsx`
**Investigation Approach:** Verify if minimum sell amount was added.

---

### H043: skipPreflight on Bonding Curve TXs (RECHECK H039)
**Category:** Transaction Construction
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: H039 (NOT_FIXED) — `BuySellPanel.tsx` modified
**Target Code:** `BuyForm.tsx:191`, `SellForm.tsx:200`
**Investigation Approach:** Verify skipPreflight:true still present on BC TXs.

---

### H044: WS Subscriber Reconnect Storm
**Category:** Resource Exhaustion × Data Pipeline
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** WS subscriber setInterval polls (supply 60s, staker gPA 30s) lack overlap protection. Slow RPC → concurrent poll calls pile up → unbounded parallel RPC consumption.
**Target Code:** `ws-subscriber.ts:360,430`
**Investigation Approach:** Verify polls use bare setInterval. Check for isRunning guard.

---

### H045: ws-subscriber Double-Init via TOCTOU
**Category:** Race Condition
**Origin:** KB (OC-271)
**Estimated Priority:** Tier 2
**Hypothesis:** Check-then-set gap on `state.initialized` allows concurrent callers to double-init, creating duplicate WS subscriptions and poll timers → doubled RPC credit consumption.
**Target Code:** `ws-subscriber.ts:456-474`
**Investigation Approach:** Verify init guard is check-then-set (not atomic). Check if instrumentation.ts can call twice.

---

### H046: SSE Connection Limit Bypass via Race
**Category:** Race Condition × DoS
**Origin:** KB (OC-271)
**Estimated Priority:** Tier 2
**Hypothesis:** Two simultaneous SSE connections from same IP can both pass ipCount < MAX_PER_IP check, exceeding the per-IP limit.
**Target Code:** `sse-connections.ts:49-57`
**Investigation Approach:** Verify non-atomic read-check-write in acquireConnection.

---

### H047: No Unprotected DB Endpoint Rate Limits
**Category:** Resource Exhaustion
**Origin:** KB (OC-133)
**Estimated Priority:** Tier 2
**Hypothesis:** /api/candles, /api/carnage-events, /api/health have no rate limiting. Rapid requests exhaust PostgreSQL connection pool (max 10).
**Target Code:** `candles/route.ts:186`, `carnage-events/route.ts:32`, `health/route.ts:32`
**Investigation Approach:** Verify checkRateLimit is absent from these routes.

---

### H048: Stale Quote-to-Execution TOCTOU
**Category:** Business Logic × MEV
**Origin:** KB (OC-122, OC-271)
**Estimated Priority:** Tier 2
**Hypothesis:** Quote computed with 300ms-debounced SSE data. User think time + signing + RPC round trip = 5-15s gap. Pool reserves change. minimumOutput computed from stale data is the only protection.
**Target Code:** `useSwap.ts:690-808`, `useRoutes.ts:582-592`
**Investigation Approach:** Measure actual staleness. Check if quote auto-refreshes before execution.

---

### H049: Polling Fallback Produces Incompatible Data
**Category:** Data Pipeline
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** When SSE is down >30s, useProtocolState polls getMultipleAccountsInfo but stores raw `{lamports, owner, dataLength}` instead of Anchor-decoded fields. All downstream hooks receive wrong data shape.
**Target Code:** `useProtocolState.ts:196-212`
**Investigation Approach:** Verify pollViaRpc stores raw accountInfo. Check downstream type guards.

---

### H050: No Process-Level Error Handlers
**Category:** Error Handling
**Origin:** KB (OC-268, OC-269)
**Estimated Priority:** Tier 2
**Hypothesis:** No `process.on("unhandledRejection")` in Next.js server. Unhandled async error crashes container silently.
**Target Code:** Project-wide (absent)
**Investigation Approach:** Grep for process.on("unhandledRejection"). Verify Railway restart behavior.

---

### H051: batchSeed Partial Failure Leaves Empty Store
**Category:** Error Handling × Data Pipeline
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** If getTokenSupply/getSlot/gPA fails during batchSeed, all previously-seeded account data is retained but state.initialized is never set true, and supply/slot/staker data is empty until next poll cycle.
**Target Code:** `ws-subscriber.ts:168-207`, `instrumentation.ts:17-28`
**Investigation Approach:** Verify batchSeed has no per-call try/catch around supply/slot/gPA.

---

### H052: No Price Impact Rejection Threshold
**Category:** Business Logic × Trading
**Origin:** KB (OC-255)
**Estimated Priority:** Tier 2
**Hypothesis:** Route engine computes price impact but never blocks execution. User can submit 50%+ price impact swaps with no blocking guard.
**Target Code:** `route-engine.ts:351-354`, `useSwap.ts:415-431`
**Investigation Approach:** Verify no rejection threshold exists. Check if only UI warning (red color) is shown.

---

### H053: Split Route Amplifies Sandwich Surface
**Category:** MEV × Trading
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** Split routes (SOL→PROFIT via CRIME+FRAUD pools) create two AMM swap instructions in one TX. MEV bot can sandwich both legs, doubling extraction.
**Target Code:** `split-router.ts`, `multi-hop-builder.ts`
**Investigation Approach:** Verify split routes produce multiple swap instructions visible in mempool.

---

### H054: Crank Carnage Recovery Skips Atomic Bundling
**Category:** MEV × Crank
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** VRF recovery path (stale/timeout) returns `carnageExecutedAtomically: false`. If Carnage is triggered during recovery, the carnage_pending flag is visible on-chain, and MEV bots can sandwich the subsequent execute_carnage call.
**Target Code:** `vrf-flow.ts:644`
**Investigation Approach:** Verify recovery path skips executeCarnageAtomic. Check if carnage_pending is set.

---

### H055: No Distributed Lock for Crank (H091)
**Category:** Crank Security
**Origin:** KB (OC-264) + VERIFY (H091)
**Estimated Priority:** Tier 2
**Hypothesis:** Two simultaneous crank instances double-spend on VRF TXs, create duplicate randomness accounts, and race on epoch transitions.
**Target Code:** `crank-runner.ts` (no lock mechanism)
**Investigation Approach:** Verify no distributed lock exists. Check Railway scaling config.

---

### H056: No External Alerting on Circuit Breaker (H004)
**Category:** Monitoring × Crank
**Origin:** KB (OC-251) + VERIFY (H004)
**Estimated Priority:** Tier 2
**Hypothesis:** Circuit breaker logs to console only. If crank halts overnight, epochs stop, stakers stop earning, nobody notified.
**Target Code:** `crank-runner.ts:535-539`
**Investigation Approach:** Verify no webhook/email/Slack notification on circuit breaker trip.

---

### H057: migrate.ts Missing TLS
**Category:** Data Security
**Origin:** KB (OC-156)
**Estimated Priority:** Tier 2
**Hypothesis:** DB migration script creates postgres client without SSL. Railway preDeployCommand runs this in production, potentially transmitting credentials and SQL in plaintext.
**Target Code:** `app/db/migrate.ts:42`
**Investigation Approach:** Verify no ssl option in migrate.ts. Check if Railway internal network is encrypted.

---

### H058: Webhook Type Confusion
**Category:** Webhook Security
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** A payload with both `accountData` and `signature` fields could be routed to the enhanced handler (weaker validation, no replay protection) instead of the raw TX handler.
**Target Code:** `webhooks/helius/route.ts:329-341` (type discrimination)
**Investigation Approach:** Check type discrimination logic. Verify if ambiguous payloads are handled.

---

### H059: SSE Candle Route Leaks Protocol State
**Category:** Information Disclosure
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** `/api/sse/candles` subscriber receives ALL sseManager events (protocol-update AND candle-update) without filtering.
**Target Code:** `sse/candles/route.ts:71-76`
**Investigation Approach:** Verify subscriber callback has no event type filter.

---

### H060: RPC Proxy as Free Transaction Relay
**Category:** Resource Abuse
**Origin:** Novel
**Estimated Priority:** Tier 2
**Hypothesis:** `sendTransaction` in method allowlist means any client can use /api/rpc as a free Solana TX relay, burning Helius credits.
**Target Code:** `rpc/route.ts:43-44`
**Investigation Approach:** Verify sendTransaction is in allowlist. Check if additional per-method rate limits exist.

---

### H061: BigInt Tag Injection via Crafted SSE Data
**Category:** Deserialization × Data Integrity
**Origin:** KB (OC-066)
**Estimated Priority:** Tier 2
**Hypothesis:** If attacker controls webhook data, a field with value `{ __bigint: "0" }` bypasses normal type expectations after JSON round-trip, converting object to BigInt.
**Target Code:** `bigint-json.ts:46-51`
**Investigation Approach:** Verify if crafted account data could produce __bigint tag after Anchor decode.

---

### H062: Chained Supply Chain + Webhook Attack (RECHECK S001)
**Category:** Combination
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: S001 (FIXED) — constituent files modified
**Target Code:** `shared/constants.ts`, webhook route, deploy pipeline
**Investigation Approach:** Verify supply chain protections intact. Check for new attack surface from modified constants.

---

### H063: Launch Day Attack Bundle (RECHECK S004)
**Category:** Combination
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: S004 (FIXED) — multiple constituent files modified
**Target Code:** Multiple modified files from delta
**Investigation Approach:** Verify individual constituent fixes still intact.

---

### H064: Browser-Console Webhook Hijack (RECHECK S008)
**Category:** Combination
**Origin:** RECHECK
**Estimated Priority:** Tier 2
**Previous Audit Note:** RECHECK: S008 (FIXED) — webhook route modified
**Target Code:** `webhooks/helius/route.ts`
**Investigation Approach:** Verify webhook URL not discoverable from client-side code.

---

## Tier 3 — MEDIUM-LOW Potential (68 strategies)

---

### H065: Health Endpoint as DoS Timing Oracle
**Origin:** Novel | **Target:** `health/route.ts:66-72`
Attacker polls /api/health to detect WS degradation, times DDoS for maximum impact.

### H066: Credit Counter as Budget Exhaustion Indicator
**Origin:** Novel | **Target:** `health/route.ts:64`, `credit-counter.ts:41-47`
Monitor credit consumption rate to estimate when Helius limits are near.

### H067: SSE Broadcast Fan-Out Amplification
**Origin:** KB (OC-143) | **Target:** `sse-manager.ts:60-69`
5000 clients × webhook burst = 600K callback invocations/minute.

### H068: Protocol Store Dedup Bypass via Key Ordering
**Origin:** Novel | **Target:** `protocol-store.ts:54-58`
Different JSON key ordering from Anchor decode vs raw webhook bypasses string dedup.

### H069: SSE Zombie Connection via NAT/CGNAT
**Origin:** Novel | **Target:** `sse-connections.ts:88-93`
Client behind CGNAT crashes → 10 slots held for 30 min → legitimate users behind same NAT blocked.

### H070: Cluster Config Poisoning via Build Cache
**Origin:** Novel | **Target:** `protocol-config.ts:25`, Railway Nixpacks
Cached build artifact retains old NEXT_PUBLIC_CLUSTER after env var change without full rebuild.

### H071: No CORS Headers on SSE Responses
**Origin:** KB (OC-088) | **Target:** `sse/protocol/route.ts:127-135`
SSE responses lack explicit CORS denial headers. fetch()-based cross-origin might succeed.

### H072: Crank Wallet Balance Logging (H076)
**Origin:** VERIFY | **Target:** `crank-runner.ts:407-409`
Wallet balance logged every cycle. Public info but concentrated monitoring source.

### H073: Candle Close Price Ordering (H033)
**Origin:** VERIFY | **Target:** `candle-aggregator.ts:123`
Concurrent webhook deliveries race to set close price. Last writer wins, not chronologically latest.

### H074: Error Truncation Loses Anchor Logs (H089)
**Origin:** VERIFY | **Target:** `crank-runner.ts:529`
300-char truncation discards Anchor `.logs` property.

### H075: No unhandledRejection in Crank (H031)
**Origin:** VERIFY | **Target:** `crank-runner.ts`
Silent crash on unhandled promise rejection.

### H076: skipPreflight on Multi-Hop v0 TXs
**Origin:** KB (OC-108) | **Target:** `multi-hop-builder.ts:381`
Devnet workaround ships to mainnet. Failed TXs broadcast without simulation.

### H077: No Compute Budget on BC TXs (H041)
**Origin:** VERIFY | **Target:** `BuyForm.tsx:183-186`
BC TXs lack ComputeBudgetProgram instructions.

### H078: ALT Cache Never Invalidated
**Origin:** KB (OC-275) | **Target:** `multi-hop-builder.ts:261`
Module-level cachedALT set once. If ALT extended, stale cache causes TX failures.

### H079: ATA Creation TOCTOU Race (Single-Hop)
**Origin:** KB (OC-271) | **Target:** `swap-builders.ts:229-240`
Check `getAccountInfo(ata)` then create. Another TX could create between check and execution.

### H080: Number Precision in toBaseUnits
**Origin:** KB (OC-305) | **Target:** `useSwap.ts:301-307`
`parseFloat * 10^9` loses precision above ~9007 SOL.

### H081: Mixed-Denomination Fee Display
**Origin:** KB (OC-310) | **Target:** `useSwap.ts:448-449`
Sell path adds SOL-denominated and token-denominated percentages.

### H082: Additive Price Impact (H072)
**Origin:** VERIFY | **Target:** `route-engine.ts:351-354`
Multi-hop impact accumulated additively not multiplicatively. Under-reports true impact.

### H083: Patch-Mint Trust Amplifier (RECHECK H021)
**Origin:** RECHECK | **Target:** `generate-constants.ts`
sync-program-ids.ts patches source from raw keypair JSON. Modified in delta.

### H084: Sell Fee Calculation Zero for Dust
**Origin:** KB (OC-309) | **Target:** `useSwap.ts:448`, frontend hooks

### H085: Token Supply Uses INITIAL_SUPPLY Fallback
**Origin:** Novel | **Target:** `useTokenSupply.ts:19-22`
Before SSE data arrives, hardcoded 1B used for MCAP calculations.

### H086: Swap State Machine No Mutex
**Origin:** KB (OC-299) | **Target:** `useSwap.ts:690-698`
Concurrent executeSwap/executeRoute possible via double-click.

### H087: Cross-Epoch Tax Rate Sniping via SSE
**Origin:** Novel | **Target:** SSE pipeline, `useEpochState.ts`
Automated client detects favorable tax rate change via SSE before UI renders.

### H088: Protocol Store Unbounded Growth
**Origin:** KB (OC-158) | **Target:** `protocol-store.ts:37-38`
Map grows if attacker sends account changes for many pubkeys. No size cap.

### H089: SSE Initial-State No Freshness Indicator
**Origin:** Novel | **Target:** `sse/protocol/route.ts:71-76`
New SSE clients receive arbitrarily stale data with no staleness timestamp.

### H090: Slot Estimation Drift
**Origin:** Novel | **Target:** `useCurrentSlot.ts:53-61`
Linear extrapolation from SSE. Wrong by 10+ slots during congestion.

### H091: Token Balance Polling No Staleness Detection
**Origin:** KB (OC-116) | **Target:** `useTokenBalances.ts:72-112`
RPC responses from behind-nodes could show older balances. No monotonic check.

### H092: WebSocket Reconnection Loses Events
**Origin:** KB (OC-125) | **Target:** `ws-subscriber.ts:299-323`
Up to 25s slot data lost in WS-to-HTTP fallback transition.

### H093: DB TLS Only in Production
**Origin:** KB (OC-156) | **Target:** `db/connection.ts:51-52`
Preview/staging environments may connect without TLS.

### H094: bigint mode:"number" Truncation
**Origin:** KB (OC-307) | **Target:** `db/schema.ts:38-39`
Schema uses bigint mode "number" which truncates above 2^53.

### H095: Connection Pool No Idle Timeout
**Origin:** KB (OC-158) | **Target:** `db/connection.ts:56-57`
Pool max=10 with no idle_timeout. Leaked connections exhaust pool.

### H096: Anchor Decode No Bounds Check
**Origin:** Novel | **Target:** `webhooks/helius/route.ts:589-593`
Decoded account data trusted without range validation (e.g., taxBps 0-10000).

### H097: anchorToJson Shallow Conversion
**Origin:** Novel | **Target:** `bigint-json.ts:102-116`
Only iterates top-level keys. Nested BN/PublicKey pass through unconverted.

### H098: SLOT_BROADCAST_INTERVAL_MS No Bounds
**Origin:** Novel | **Target:** `ws-subscriber.ts:251-253`
Env var parsed without min/max. Setting to 0 creates event firehose.

### H099: Crank Spending Cap Uses Estimated Cost
**Origin:** Novel | **Target:** `crank-runner.ts:110,500`
recordSpend uses ESTIMATED_TX_COST_LAMPORTS not actual fee.

### H100: CSP script-src unsafe-inline
**Origin:** KB (OC-091) | **Target:** `next.config.ts:33`
XSS protection degraded. If injection vector found, inline scripts execute.

### H101: iframe sandbox allow-same-origin + allow-scripts
**Origin:** KB (OC-092) | **Target:** `DocsModal.tsx:105`, `DocsStation.tsx:66`
If docs site compromised, can escalate to parent origin access.

### H102: CI No Permissions Block
**Origin:** KB (OC-214) | **Target:** `.github/workflows/ci.yml:17-28`
Workflow has broad default permissions. Should use contents: read only.

### H103: Sentry Wildcard CSP
**Origin:** KB (OC-091) | **Target:** `next.config.ts:43`
`*.ingest.sentry.io` could be used for data exfiltration.

### H104: Log Injection via RPC Method Field
**Origin:** KB (OC-077) | **Target:** `rpc/route.ts:117`
User-controlled method name interpolated into console.warn.

### H105: Log Injection via Webhook Pool Type
**Origin:** KB (OC-077) | **Target:** `webhooks/helius/route.ts:407-409`
swap.poolType from parsed events logged without sanitization.

### H106: Debug Logging in Production LaunchPage
**Origin:** KB (OC-174) | **Target:** `app/launch/page.tsx:99-107`
Console.log of curve state in production.

### H107: Smoke Test Logs Raw CLUSTER_URL
**Origin:** KB (OC-172) | **Target:** `scripts/e2e/smoke-test.ts:36`
API key in URL logged without masking in CI environments.

### H108: Crank Health Endpoint Binds 0.0.0.0
**Origin:** KB (OC-228) | **Target:** `crank-runner.ts:185`
Internal state exposed if Railway routes traffic to port 8080.

### H109: COMMITMENT Env Var No Validation
**Origin:** KB (OC-221) | **Target:** `crank-provider.ts:37`
Cast to Commitment type without runtime validation.

### H110: Shell Injection via WALLET Env Var
**Origin:** KB (OC-055) | **Target:** `verify-authority.ts:390-395`
WALLET value interpolated into execSync shell command.

### H111: deploy logger Writes to Arbitrary Path
**Origin:** KB (OC-063) | **Target:** `scripts/deploy/lib/logger.ts:64-79`
logFilePath parameter with no bounds checking.

### H112: autoConnect with Empty Adapter List
**Origin:** KB (OC-120) | **Target:** `providers.tsx:43-47`
autoConnect reconnects to last wallet. Malicious extension replacement between sessions.

### H113: Wallet Icon Tracking Pixel
**Origin:** KB (OC-203) | **Target:** `ConnectModal.tsx:127-132`
Wallet adapter icons rendered as `<img src>`. Malicious extension could provide tracking URL.

### H114: Mobile Deep Link URL Manipulation
**Origin:** KB (OC-196) | **Target:** `mobile-wallets.ts:17-31`
window.location.href passed to deep-link constructors.

### H115: Webhook Response Leaks Processing Counts
**Origin:** KB (OC-134) | **Target:** `webhooks/helius/route.ts:490-498`
200 response includes `{ processed: { transactions, swaps, epochs, carnages } }`.

### H116: Protocol Store Accepts Arbitrary Keys
**Origin:** Novel | **Target:** `protocol-store.ts:53-65`
setAccountState accepts any string key. No write authorization beyond webhook auth.

### H117: SSE Connection Double-Release Race
**Origin:** KB (OC-271) | **Target:** `sse/protocol/route.ts:110-124`
Both abort handler and cancel() can fire, running release() twice.

### H118: Rate Limiter Memory Growth
**Origin:** KB (OC-281) | **Target:** `rate-limit.ts:97-111`
Timestamp arrays grow proportional to unique IPs. No eviction for stale keys.

### H119: Decode Failure Broadcasts Raw Data via SSE
**Origin:** Novel | **Target:** `webhooks/helius/route.ts:607-619`
Failed Anchor decodes store raw account data + error messages in protocolStore → broadcast to clients.

### H120: No Secret Rotation Mechanism
**Origin:** KB (OC-009) | **Target:** Project-wide
No dual-key acceptance for HELIUS_WEBHOOK_SECRET rotation.

### H121: Deprecated npm Packages (H056)
**Origin:** VERIFY | **Target:** `package-lock.json`
glob@7.x, inflight@1.x deprecated.

### H122: Staking Escrow Monitoring Absent (H017)
**Origin:** VERIFY | **Target:** `crank-runner.ts`
No monitoring for staking escrow rent depletion.

### H123: No Emergency Pause (H106)
**Origin:** VERIFY | **Target:** All programs
ACCEPTED_RISK — no kill switch on on-chain programs.

### H124: Graduation Irreversibility (H097)
**Origin:** VERIFY | **Target:** `scripts/graduation/graduate.ts`
ACCEPTED_RISK — graduation is irreversible.

### H125: Cross-Program Upgrade Cascade (H102)
**Origin:** VERIFY | **Target:** Build pipeline
ACCEPTED_RISK — upgrading one program may break others.

### H126: EpochState Layout Coupling (H104)
**Origin:** VERIFY | **Target:** `pool_reader.rs` (on-chain)
Off-chain reads rely on specific byte offsets.

### H127: DB Without TLS in Staging (H011)
**Origin:** VERIFY | **Target:** `db/connection.ts`
TLS only in production NODE_ENV.

### H128: Vault Top-Up Cap Still Working (H013)
**Origin:** VERIFY | **Target:** `crank-runner.ts:82,421`
Verify 0.1 SOL per-operation cap intact.

### H129: Crank Infinite Retry Fixed (H029)
**Origin:** VERIFY | **Target:** `crank-runner.ts`
Verify circuit breaker still halts after 5 errors.

### H130: VRF Wait Loop Fixed (H030)
**Origin:** VERIFY | **Target:** `vrf-flow.ts`
Verify VRF timeout recovery still works.

### H131: npm Supply Chain Guard (H003)
**Origin:** VERIFY | **Target:** `.gitignore`, `railway.toml`
Verify Cargo.lock and package-lock.json committed.

### H132: Railway Dashboard SPOA (H132)
**Origin:** VERIFY | **Target:** Railway infrastructure
ACCEPTED_RISK — Railway dashboard compromise = full access.

---

## Cross-Strategy Analysis

### Potentially Related Strategies (Combination Attacks)

| Strategy A | Strategy B | Potential Combination |
|------------|------------|----------------------|
| H004 (Helius API key) | H011 (enhanced replay) | API key to register shadow webhook + replay old data |
| H005 (webhook secret) | H020 (data injection) | Secret + crafted payload = full state manipulation |
| H003 (npm install) | H002 (crank wallet key) | Supply chain → keypair theft → fund drain |
| H016 (5% slippage) | H018 (no MEV protection) | Default slippage + public mempool = guaranteed sandwich |
| H015 (IP spoofing) | H008 (batch amplification) | Bypass rate limit + batch = unlimited credit burn |
| H019 (gapFillCandles) | H047 (no rate limit) | Amplification + unprotected endpoint = server OOM |
| H010 (RPC no timeout) | H008 (batch amplification) | Batch + hang = all workers blocked |

### Off-Chain → On-Chain Chains (SOS Cross-Reference)

| Off-Chain Strategy | On-Chain Finding | Combined Attack |
|-------------------|------------------|-----------------|
| H005 (state injection) | SOS: 50% output floor | Fake reserves via SSE → user sets slippage too high → sandwich extracts up to 50% |
| H016 (5% slippage) | SOS: minimumOutput enforcement | On-chain backstop limits extraction to 5%, not 50% |
| H006 (API key exposure) | SOS: permissionless epoch | API key abuse + spammed TXs through relay |
| H054 (non-atomic Carnage) | SOS: carnage_pending flag | MEV bot front-runs Carnage execution during recovery |

### Investigation Priority Order

**Tier 1 (Investigate First — 22 strategies):**
H001-H022: All CRITICAL potential strategies, prioritized by:
1. Fund loss risk (H001, H002, H003, H007)
2. Data manipulation (H005, H011, H013, H020, H021)
3. MEV extraction (H016, H017, H018)
4. Service denial (H008, H009, H010, H014, H015, H019)
5. Supply chain (H003, H007, H009, H022)
6. Configuration (H006)

**Tier 2 (High Priority — 42 strategies):**
H023-H064: RECHECK findings + HIGH potential novel/KB strategies

**Tier 3 (Standard — 68 strategies):**
H065-H132: MEDIUM-LOW findings, VERIFY items, defense-in-depth

---

## Statistics

| Category | Count | Tier 1 | Tier 2 | Tier 3 | KB | Novel | RECHECK |
|----------|-------|--------|--------|--------|-----|-------|---------|
| Secrets & Key Mgmt | 14 | 6 | 4 | 4 | 8 | 3 | 3 |
| MEV & Trading | 10 | 3 | 3 | 4 | 4 | 4 | 2 |
| Webhook & Data Pipeline | 16 | 5 | 5 | 6 | 4 | 8 | 4 |
| Rate Limiting & DoS | 14 | 5 | 3 | 6 | 7 | 5 | 2 |
| Supply Chain & Deps | 8 | 3 | 2 | 3 | 6 | 2 | 0 |
| Transaction Construction | 10 | 0 | 3 | 7 | 5 | 3 | 2 |
| Race Conditions | 6 | 0 | 3 | 3 | 4 | 2 | 0 |
| Error Handling | 8 | 0 | 3 | 5 | 4 | 3 | 1 |
| Business Logic | 12 | 0 | 4 | 8 | 4 | 5 | 3 |
| Infrastructure & Config | 10 | 0 | 2 | 8 | 6 | 3 | 1 |
| Web Security | 6 | 0 | 2 | 4 | 4 | 1 | 1 |
| Monitoring & Info Disclosure | 8 | 0 | 2 | 6 | 3 | 3 | 2 |
| Combination/Cross-Boundary | 10 | 0 | 6 | 4 | 0 | 4 | 6 |
| **TOTAL** | **132** | **22** | **42** | **68** | **63** | **46** | **23** |

**Origin Breakdown:**

| Origin | Count | % |
|--------|-------|---|
| KB (pattern-based) | 63 | 47.7% |
| Novel (creative) | 46 | 34.8% |
| RECHECK (stacked) | 23 | 17.4% |

**Novel strategy percentage: 34.8%** (target: ≥20%) ✅

---

## Supplemental Strategies (Generated Post-Batch-1)

Based on 4 CONFIRMED + 4 POTENTIAL findings from Batch 1 investigation.

---

### S001: Chained Repo Clone → Devnet Drain + Webhook Hijack
**Category:** Combination (H001 + H004)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** Single repo clone gives attacker both the devnet wallet key (.mcp.json) AND the Helius API key (.env.devnet). Simultaneous wallet drain + shadow webhook registration provides both immediate fund theft and persistent surveillance.
**Target Code:** `.mcp.json:8`, `.env.devnet:8`, `shared/programs.ts:24`
**Investigation Approach:** Verify both keys extractable from single clone. Assess combined impact.

---

### S002: State Injection + RPC Exhaustion (Denial + Manipulation)
**Category:** Combination (H005 + H008)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** Attacker with webhook secret injects manipulated pool reserves (H005) while simultaneously exhausting Helius credits via batch amplification (H008). Users see false prices AND cannot submit corrective transactions.
**Target Code:** `webhooks/helius/route.ts:525-633`, `rpc/route.ts:102-106`
**Investigation Approach:** Verify both attacks can run concurrently. Assess if RPC exhaustion prevents users from checking on-chain state.

---

### S003: Helius API Key → Webhook URL Discovery → Secret Brute-Force
**Category:** Combination (H004 → H005)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** Committed Helius API key → `GET /webhooks` reveals webhook delivery URL → attacker reverse-engineers or brute-forces HELIUS_WEBHOOK_SECRET from the URL endpoint → full state injection capability.
**Target Code:** `.env.devnet:8`, `scripts/webhook-manage.ts`, webhook auth
**Investigation Approach:** Check if webhook URL reveals any secret material. Assess secret entropy.

---

### S004: Dependency Confusion → Crank Wallet Exfiltration
**Category:** Combination (H007 + H002)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** Attacker claims @dr-fraudsworth npm scope → publishes malicious shared package → crank build resolves from registry (npm install, not ci) → malicious code reads process.env.WALLET_KEYPAIR → exfiltrates mainnet crank wallet key.
**Target Code:** `app/package.json:18`, `railway-crank.toml:3`, crank env vars
**Investigation Approach:** Verify crank depends on @dr-fraudsworth/shared. Check if crank process has WALLET_KEYPAIR in env.

---

### S005: Batch Amplification + IP Spoofing → Unlimited Credit Burn
**Category:** Combination (H008 + H015)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** RPC batch amplification (500 methods per request) combined with IP spoofing (fresh rate-limit bucket per spoofed IP) removes ALL rate limiting constraints. Helius credits can be burned at wire speed.
**Target Code:** `rpc/route.ts:102-106`, `rate-limit.ts:129-151`
**Investigation Approach:** Verify IP spoofing bypasses rate limit. Calculate maximum credit burn rate.

---

### S006: State Injection → Manipulated minimumOutput → MEV Extraction
**Category:** Combination (H005 → MEV)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** Webhook state injection delivers fake reserves to all SSE clients → useSwap computes minimumOutput from fake data → user signs TX with near-zero minimumOutput → MEV bot sandwiches for maximum extraction. On-chain 50% slippage floor is the only remaining guard.
**Target Code:** `webhooks/helius/route.ts:588-606`, `useSwap.ts:690-808`, `useRoutes.ts:582-592`
**Investigation Approach:** Trace data flow from webhook → protocolStore → SSE → useSwap → minimumOutput. Verify on-chain floor.

---

### S007: npm Supply Chain → Crank Key Theft → Epoch Manipulation
**Category:** Combination (H003 + H002)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** Malicious package injected via npm install (H003) → reads WALLET_KEYPAIR env var → exfiltrates to attacker → attacker signs epoch transitions with stolen crank key → manipulates tax rates, Carnage, VRF.
**Target Code:** `railway-crank.toml:3`, crank env vars, epoch program
**Investigation Approach:** Verify crank has WALLET_KEYPAIR in env and that module-body code can access it.

---

### S008: Helius API Key → Shadow Webhook + Pipeline Blinding
**Category:** Combination (H004)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** With Helius API key, attacker registers shadow webhook AND deletes production webhook simultaneously. Shadow webhook captures all protocol events while production data pipeline goes blind. SSE clients receive stale data indefinitely.
**Target Code:** `.env.devnet:8`, `scripts/webhook-manage.ts`, SSE pipeline
**Investigation Approach:** Verify both webhook CRUD operations possible with single API key.

---

### S009: NEXT_PUBLIC_RPC_URL Template → Future Regression → Credit Exhaustion
**Category:** Combination (H006 + H008)
**Origin:** Supplemental
**Estimated Priority:** Tier 3
**Hypothesis:** If NEXT_PUBLIC_RPC_URL template instruction is followed AND a future developer adds a client-side reference, the API key enters the bundle. Combined with batch amplification, attacker uses the direct Helius URL to bypass proxy rate limits entirely.
**Target Code:** `.env.mainnet:49`, `rpc/route.ts`, client components
**Investigation Approach:** Assess likelihood of regression. Check if direct Helius URL bypasses proxy limits.

---

### S010: .mcp.json Key + Devnet Program Keypairs → Full Devnet Takeover
**Category:** Combination (H001 + H012)
**Origin:** Supplemental
**Estimated Priority:** Tier 2
**Hypothesis:** Repo clone gives attacker the devnet wallet AND all 17 program keypairs. Attacker drains devnet wallet, upgrades all devnet programs to malicious versions, and uses the compromised devnet to social-engineer mainnet users.
**Target Code:** `.mcp.json:8`, `keypairs/*.json`
**Investigation Approach:** Verify program keypairs match upgrade authorities. Assess social engineering risk.

---

## Notes for Investigators

### General Guidance

- Each strategy should be investigated independently
- Reference `.bulwark/ARCHITECTURE.md` for architectural context
- Reference `.bulwark/HANDOVER.md` for Audit #1 finding details
- Write findings to `.bulwark/findings/H{XXX}.md`
- For RECHECK strategies, compare current code against Audit #1 fix description
- For cross-boundary strategies, reference `.audit/ARCHITECTURE.md` for on-chain context
- Note any discoveries that suggest NEW strategies (supplemental)

### Status Definitions

- **CONFIRMED**: Vulnerability exists and is exploitable
- **POTENTIAL**: Could be vulnerable under specific conditions
- **NOT_VULNERABLE**: Protected against this attack
- **NEEDS_MANUAL_REVIEW**: Couldn't determine, needs expert

---

**This catalog is the input for Phase 4: Parallel Investigation**
