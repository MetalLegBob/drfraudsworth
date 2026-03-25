---
task_id: db-phase1-SEC-02
provides: [secret-credential-findings, secret-credential-invariants]
focus_area: secret-credential-management
files_analyzed: [shared/constants.ts, shared/programs.ts, app/app/api/webhooks/helius/route.ts, app/db/connection.ts, app/lib/sentry.ts, app/lib/connection.ts, app/providers/providers.tsx, app/next.config.ts, app/instrumentation-client.ts, scripts/crank/crank-provider.ts, scripts/crank/crank-runner.ts, scripts/webhook-manage.ts, scripts/backfill-candles.ts, scripts/deploy/lib/connection.ts, scripts/deploy/initialize.ts, .mcp.json, .gitignore, keypairs/devnet-wallet.json]
finding_count: 9
severity_breakdown: {critical: 1, high: 2, medium: 4, low: 2}
---
<!-- CONDENSED_SUMMARY_START -->
# Secret & Credential Management — Condensed Summary

## Key Findings (Top 5-10)
1. **Solana private key committed in `.mcp.json`**: Base58-encoded private key (`SOLANA_PRIVATE_KEY`) is tracked by git in `.mcp.json:8`. This is a real secret key committed to version control history. — `.mcp.json:8`
2. **Devnet wallet keypair committed to git**: `keypairs/devnet-wallet.json` (64-byte secret key array) is tracked by git. While devnet-only, if this wallet is reused on mainnet or holds non-trivial SOL, it is compromised. — `keypairs/devnet-wallet.json`
3. **Helius webhook auth is optional (skip if unset)**: When `HELIUS_WEBHOOK_SECRET` env var is missing, the webhook endpoint accepts unauthenticated POST requests from anyone. This is the production path on Railway. — `app/app/api/webhooks/helius/route.ts:135-141`
4. **Helius API key hardcoded in source (4 locations)**: The same UUID `[REDACTED-DEVNET-HELIUS-KEY]` is hardcoded in `shared/constants.ts:474`, `shared/programs.ts:22`, `scripts/webhook-manage.ts:28`, `scripts/backfill-candles.ts:47`. Comments say "not a secret" (free tier), but it enables webhook management API calls (create/delete webhooks) and RPC access.
5. **WALLET_KEYPAIR env var stored as plaintext JSON array**: The crank runner on Railway receives the full 64-byte secret key as an environment variable in plaintext. No encryption at rest. — `scripts/crank/crank-provider.ts:41-57`
6. **Program deploy keypairs committed to git**: 10 keypair files in `keypairs/` are git-tracked (amm, bonding-curve, epoch, staking, tax, transfer-hook, vault, etc.). These are program authority keys. — `keypairs/*.json`
7. **No secret rotation mechanism exists**: No code supports rotating any secret (Helius API key, webhook secret, wallet keypair, DATABASE_URL credentials). Single static values everywhere.
8. **Sentry DSN exposed via NEXT_PUBLIC_ prefix**: `NEXT_PUBLIC_SENTRY_DSN` is used in client-side code, exposing the DSN (including the ingest key) in the browser bundle. While DSNs are semi-public by design, it enables spam error submission. — `app/lib/sentry.ts:30`, `app/instrumentation-client.ts:37`
9. **Helius API key in `.claude/settings.local.json`**: The API key appears in CLI permission rules in the settings file, which is git-tracked. — `.claude/settings.local.json:193,360,374-375`

## Critical Mechanisms
- **Crank wallet loading**: Loads signing keypair from `WALLET_KEYPAIR` env var (JSON byte array) or disk file. Logs truncated public key. No HSM/KMS. — `scripts/crank/crank-provider.ts:34-87`
- **Webhook authentication**: Optional `Authorization` header check against `HELIUS_WEBHOOK_SECRET`. When unset, any HTTP client can inject fake transaction events into the database. — `app/app/api/webhooks/helius/route.ts:131-141`
- **Database connection**: `DATABASE_URL` loaded from env var with fail-fast validation. No hardcoded fallback. Properly lazy-initialized. — `app/db/connection.ts:37-60`
- **RPC URL resolution**: Falls back from `NEXT_PUBLIC_RPC_URL` env var to hardcoded `DEVNET_RPC_URL` (which embeds the Helius API key). The fallback URL ships in the client bundle via `shared/programs.ts`. — `app/lib/connection.ts:32-33`, `app/providers/providers.tsx:35`

## Invariants & Assumptions
- INVARIANT: DATABASE_URL must be set at runtime or the app throws — enforced at `app/db/connection.ts:41-46`
- INVARIANT: Crank requires either WALLET_KEYPAIR env var or a keypair file to exist — enforced at `scripts/crank/crank-provider.ts:44-70`
- INVARIANT: .env files are gitignored — enforced at `.gitignore:1`
- ASSUMPTION: Helius API key is "not a secret" because it's free tier — UNVALIDATED (free-tier keys can still be abused for webhook management, rate-limit exhaustion)
- ASSUMPTION: Program deploy keypairs in `keypairs/` are devnet-only and safe to commit — UNVALIDATED (no enforcement prevents mainnet reuse)
- ASSUMPTION: HELIUS_WEBHOOK_SECRET is set in Railway production — NOT ENFORCED (code explicitly allows skipping auth when unset)

## Risk Observations (Prioritized)
1. **[CRITICAL] Private key in version-controlled `.mcp.json`**: `SOLANA_PRIVATE_KEY` with a real base58-encoded key is committed to git. Even if devnet-only, the key is permanently in git history. Any contributor or attacker with repo access can extract it. — `.mcp.json:8`
2. **[HIGH] Unauthenticated webhook endpoint in production**: If `HELIUS_WEBHOOK_SECRET` is not set on Railway, any attacker can POST fake transaction data, injecting false swap events, epoch transitions, and carnage events into the database. This poisons price charts (OHLCV candles) and SSE broadcasts to connected clients. — `app/app/api/webhooks/helius/route.ts:135-141`
3. **[HIGH] 10 program keypair files committed to git**: All Anchor program deploy keypairs are tracked. These contain the program upgrade authority private keys. If these programs are deployed on mainnet with the same keypairs, the authority is compromised to anyone with repo access. — `keypairs/*.json`
4. **[MEDIUM] Helius API key hardcoded in 4+ source files**: Enables webhook CRUD operations via `scripts/webhook-manage.ts`. An attacker could delete or redirect webhooks to intercept production transaction data. — `shared/constants.ts:474`
5. **[MEDIUM] No webhook auth enforcement**: The code pattern `if (webhookSecret) { check } else { skip }` is fail-open. Should be fail-closed (reject if no secret configured). — `app/app/api/webhooks/helius/route.ts:136`
6. **[MEDIUM] Crank wallet keypair as plaintext env var on Railway**: Railway env vars are visible to all project members and stored in Railway's infrastructure. No envelope encryption or secrets manager. — `scripts/crank/crank-provider.ts:41`
7. **[MEDIUM] RPC URL with embedded API key in client bundle**: `DEVNET_RPC_URL` from `shared/programs.ts` includes the Helius API key and is used as fallback in the Next.js client bundle via `app/providers/providers.tsx:35`. — `shared/programs.ts:22`
8. **[LOW] No secret rotation support**: Zero infrastructure for rotating any credential. All secrets are static strings with no versioning, no grace period, no dual-key support.
9. **[LOW] Sentry DSN in NEXT_PUBLIC_ variable**: Enables error spam to the Sentry project from any browser. Low impact but noise-generating.

## Novel Attack Surface
- **Webhook poisoning -> price manipulation**: An attacker who discovers the unauthenticated webhook endpoint can inject fake TaxedSwap events with arbitrary prices. These flow into OHLCV candle aggregation and SSE broadcast. Frontend users would see manipulated charts, potentially influencing trading decisions. The webhook URL is discoverable from the codebase (`/api/webhooks/helius`) and the Railway deployment URL is in the git history.
- **API key -> webhook redirect**: The hardcoded Helius API key enables webhook management. An attacker could call the Helius API to redirect the webhook to their own server, intercepting all real-time transaction data and preventing the legitimate app from receiving it.

## Cross-Focus Handoffs
- -> **API-04 (Webhooks)**: The Helius webhook endpoint has optional authentication — investigate the full attack surface of fake event injection (data integrity impact, SSE broadcast to clients).
- -> **BOT-01 (Keeper/Crank)**: The crank runner loads wallet keypairs from env vars — investigate whether the crank signing key has excessive permissions beyond what the crank needs (principle of least privilege for the wallet).
- -> **INFRA-03 (Cloud/Railway)**: Railway env var storage for WALLET_KEYPAIR — investigate Railway's security model for environment variable encryption at rest and access control.
- -> **DATA-01 (Database)**: DATABASE_URL handling is sound (env-only, no fallback), but verify Railway's Postgres connection string rotation and access logging.

## Trust Boundaries
The codebase has a clear separation between server-side secrets (DATABASE_URL, WALLET_KEYPAIR, HELIUS_WEBHOOK_SECRET) which are properly env-var-only, and what it considers "non-secrets" (Helius API key, program IDs, RPC URLs) which are hardcoded. The critical gap is that the "non-secret" classification of the Helius API key is incorrect — it grants webhook management permissions. The webhook authentication boundary is fail-open: missing configuration silently downgrades to no authentication rather than failing closed. The keypair files in `keypairs/` represent a trust boundary violation where deployment credentials are shared with all repo contributors. The `.mcp.json` private key is a clear trust boundary breach — a signing credential in version control.
<!-- CONDENSED_SUMMARY_END -->

---

# Secret & Credential Management — Full Analysis

## Executive Summary

The Dr. Fraudsworth project has a mixed security posture for secret management. Server-side secrets like `DATABASE_URL` follow proper patterns (env-var-only with fail-fast validation). However, several critical gaps exist: a Solana private key committed in `.mcp.json`, program deploy keypairs tracked by git, a hardcoded Helius API key that grants webhook management permissions, and an authentication-optional webhook endpoint that enables data injection attacks.

## Scope

All off-chain TypeScript/JavaScript files, configuration files, and shell scripts. On-chain Anchor programs in `programs/` are excluded.

**Files analyzed in depth:**
- `shared/constants.ts` — Hardcoded Helius API key, program IDs, mint addresses
- `shared/programs.ts` — RPC URL with embedded API key
- `app/app/api/webhooks/helius/route.ts` — Webhook handler with optional auth
- `app/db/connection.ts` — Database connection management
- `app/lib/sentry.ts` — Sentry DSN handling
- `app/lib/connection.ts` — RPC connection factory
- `app/providers/providers.tsx` — Client-side RPC endpoint configuration
- `app/next.config.ts` — CSP and security headers
- `app/instrumentation-client.ts` — Client-side Sentry initialization
- `scripts/crank/crank-provider.ts` — Wallet keypair loading
- `scripts/crank/crank-runner.ts` — Crank bot entry point
- `scripts/webhook-manage.ts` — Helius webhook CRUD
- `scripts/backfill-candles.ts` — Historical data backfill
- `scripts/deploy/lib/connection.ts` — Deploy script provider
- `scripts/deploy/initialize.ts` — Protocol initialization
- `.mcp.json` — MCP server configuration (contains private key)
- `.gitignore` — Git exclusion rules
- `keypairs/` — All tracked keypair files

## Key Mechanisms

### 1. Wallet/Keypair Loading

**Crank provider (`scripts/crank/crank-provider.ts:34-87`):**
Three-tier priority:
1. `WALLET_KEYPAIR` env var — JSON byte array string, parsed at runtime
2. `WALLET` env var — file path to keypair JSON
3. `keypairs/devnet-wallet.json` — committed default

The crank provider logs the first 12 characters of the public key on load (line 51, 78). This is safe (public key, truncated). Error messages truncate to 100 chars (line 55) — good practice preventing full error detail leakage.

**Deploy scripts (`scripts/deploy/lib/connection.ts:64-87`):**
Similar pattern but only supports file-based loading (env var or default path). No WALLET_KEYPAIR env var support — deploy scripts are run locally, not on Railway.

**Risk:** The `WALLET_KEYPAIR` env var contains the full 64-byte secret key as a JSON array in plaintext. Railway stores env vars encrypted at rest but they are visible to all project members in the dashboard. No KMS/HSM integration exists.

### 2. Helius API Key

The key `[REDACTED-DEVNET-HELIUS-KEY]` appears in:
- `shared/constants.ts:474` — exported as `HELIUS_API_KEY`
- `shared/programs.ts:22` — embedded in `DEVNET_RPC_URL`
- `scripts/webhook-manage.ts:28` — used for webhook management API
- `scripts/backfill-candles.ts:47` — used for RPC connections
- `scripts/deploy/pda-manifest.json:3` — stored in deployment manifest
- `.claude/settings.local.json` — in CLI permission rules

The code comments explicitly state "not a secret" and "free-tier API key." However, this key is used in `scripts/webhook-manage.ts` to call the Helius webhook management API (`https://api.helius.xyz/v0/webhooks?api-key=...`). This API allows:
- Listing all webhooks (GET)
- Creating new webhooks (POST)
- Updating webhook URLs (PUT)
- Deleting webhooks (DELETE)

An attacker with this key could redirect the webhook to their own server, intercept all transaction events, or delete the webhook to create a data gap.

### 3. Webhook Authentication

**`app/app/api/webhooks/helius/route.ts:131-141`:**
```typescript
const webhookSecret = process.env.HELIUS_WEBHOOK_SECRET;
if (webhookSecret) {
  const authHeader = req.headers.get("authorization");
  if (authHeader !== webhookSecret) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }
}
```

This is a **fail-open** pattern. If `HELIUS_WEBHOOK_SECRET` is not set in the Railway environment, authentication is silently skipped. The comment says "allows local testing" but the same code runs in production. There is no warning log when auth is skipped.

The webhook endpoint URL is discoverable: `https://dr-fraudsworth-production.up.railway.app/api/webhooks/helius` (hardcoded in `scripts/webhook-manage.ts:43`).

**Authentication comparison:** The comparison `authHeader !== webhookSecret` uses JavaScript's `!==` operator, which is NOT timing-safe. An attacker could theoretically perform timing attacks to guess the secret character by character, though the practical difficulty is high for a webhook auth header over HTTP.

### 4. Database Credentials

**`app/db/connection.ts:37-60`:**
`DATABASE_URL` is loaded exclusively from `process.env.DATABASE_URL` with immediate throw if missing. No hardcoded fallback. This is the correct pattern (SP-001 from secure-patterns guide).

The lazy Proxy pattern ensures the connection string is only accessed at request time, not during Next.js build. This is security-positive — it prevents build-time credential requirements.

**`app/drizzle.config.ts:22`:**
Uses `process.env.DATABASE_URL!` (non-null assertion). This is a config file for migration tooling, acceptable pattern.

### 5. Sentry DSN

**`app/lib/sentry.ts:26-33`:**
DSN resolution: `dsn` parameter > `NEXT_PUBLIC_SENTRY_DSN` > `SENTRY_DSN`. The `NEXT_PUBLIC_` prefix exposes the DSN in the client bundle. DSNs are designed to be semi-public (they are embedded in all Sentry client SDKs by design), but they enable error submission to the project.

**`app/instrumentation-client.ts:37`:**
Client-side error listeners registered only if `NEXT_PUBLIC_SENTRY_DSN` is set. The DSN itself is not logged. The `captureException` function is fire-and-forget with full error swallowing — good practice for error reporting infrastructure.

### 6. Private Key in `.mcp.json`

**`.mcp.json:8`:**
```json
"SOLANA_PRIVATE_KEY": "[REDACTED-DEVNET-PRIVATE-KEY]"
```

This is a base58-encoded Solana private key committed to git since at least commit `53ca01b` ("feat: add 24/7 crank runner for Railway + SVK security tooling"). The key appears in the `solana-mcp` MCP server configuration. Even if this is a devnet-only key, it is permanently in git history and exposed to anyone with repo access (current or future).

`.gitignore` does NOT include `.mcp.json`. The file is actively tracked by git.

## Trust Model

### Secrets Classification

| Secret | Storage | Classification | Actual Risk |
|--------|---------|----------------|-------------|
| `DATABASE_URL` | Env var only | Secret | Correct |
| `HELIUS_WEBHOOK_SECRET` | Env var only | Secret | Correct, but optional enforcement |
| `WALLET_KEYPAIR` | Env var (Railway) | Secret | Correct, but plaintext in Railway |
| `HELIUS_API_KEY` | Hardcoded in source | "Not a secret" | **Incorrect** — enables webhook CRUD |
| `SOLANA_PRIVATE_KEY` | `.mcp.json` (git) | Should be secret | **Critical** — committed to VCS |
| Keypair files | `keypairs/` (git) | "Devnet only" | **Risky** — program upgrade authority |
| `SENTRY_DSN` | `NEXT_PUBLIC_` env var | Semi-public | Acceptable (by design) |
| Program IDs | Hardcoded | Public | Correct |

### Trust Boundaries

1. **Git repository boundary**: Anyone with repo read access can extract: the `.mcp.json` private key, all program deploy keypairs, the Helius API key. This includes all current and former contributors, CI systems, and any service with repo access.

2. **Client-side boundary**: The Next.js client bundle contains: the Helius RPC URL with API key (via `DEVNET_RPC_URL` fallback), any `NEXT_PUBLIC_*` env vars, the Sentry DSN. These are visible to any user inspecting browser DevTools.

3. **Railway boundary**: Railway environment stores: `WALLET_KEYPAIR` (full signing keypair), `DATABASE_URL`, `HELIUS_WEBHOOK_SECRET`, `CLUSTER_URL`. These are visible to all Railway project members.

4. **Webhook boundary**: The webhook endpoint is the bridge between Helius (external) and the database (internal). Without authentication, this boundary is open.

## State Analysis

### Secrets in Environment
- `DATABASE_URL` — Postgres connection string (Railway managed)
- `WALLET_KEYPAIR` — 64-byte signing key as JSON array
- `HELIUS_WEBHOOK_SECRET` — Webhook auth token (optional)
- `CLUSTER_URL` — Solana RPC endpoint
- `COMMITMENT` — Transaction commitment level
- `PDA_MANIFEST` — Full PDA manifest JSON
- `CARNAGE_WSOL_PUBKEY` — Public key (not a secret)
- `NEXT_PUBLIC_RPC_URL` — RPC URL override (client-exposed)
- `NEXT_PUBLIC_SENTRY_DSN` — Sentry DSN (client-exposed)
- `NEXT_PUBLIC_DEMO_MODE` — Feature flag (client-exposed)
- `NEXT_PUBLIC_CURVE_PHASE` — Feature flag (client-exposed)

### Secrets in Files (Git-Tracked)
- `.mcp.json` — Contains `SOLANA_PRIVATE_KEY` (base58)
- `keypairs/devnet-wallet.json` — Devnet wallet private key (byte array)
- `keypairs/amm-keypair.json` — AMM program deploy keypair
- `keypairs/bonding-curve-keypair.json` — Bonding curve program deploy keypair
- `keypairs/epoch-program.json` — Epoch program deploy keypair
- `keypairs/staking-keypair.json` — Staking program deploy keypair
- `keypairs/tax-program-keypair.json` — Tax program deploy keypair
- `keypairs/transfer-hook-keypair.json` — Transfer hook program deploy keypair
- `keypairs/vault-keypair.json` — Vault program deploy keypair
- `keypairs/fake-tax-keypair.json` — Test keypair
- `keypairs/mock-tax-keypair.json` — Test keypair
- `keypairs/StUbofRk12S7JrEUoQJFjMe6FmACNoRpbNMyjn311ZU.json` — Named keypair

### Secrets in Files (Gitignored)
- `.env` — Root env file (gitignored)
- `app/.env.local` — App env file (gitignored)
- `scripts/deploy/mint-keypairs/` — Mint authority keypairs (gitignored, good)
- `scripts/deploy/pda-manifest.json` — Contains RPC URL with API key (gitignored, but API key is also hardcoded in source)

## Dependencies

- **Helius**: RPC provider and webhook delivery service. API key used for both RPC and webhook management.
- **Railway**: Hosting platform. Stores env vars including signing keypair.
- **Sentry**: Error reporting. DSN exposed in client bundle.
- **Postgres (via Railway)**: Database. Connection string in env var.

## Focus-Specific Analysis

### Secret Exposure Vectors

**Vector 1: Git history**
The `.mcp.json` private key and all `keypairs/*.json` files are in git history. Even if removed from the working tree now, `git log -p` or any history browser reveals them. Remediation requires git history rewriting (BFG Repo-Cleaner or `git filter-repo`) and key rotation.

**Vector 2: Client-side bundle**
`DEVNET_RPC_URL` (containing Helius API key) is imported by `app/providers/providers.tsx` and `app/lib/connection.ts`. In the production Next.js bundle, this string is inlined as a fallback. Visible in browser DevTools Network tab or source inspection.

**Vector 3: Logging**
The crank provider logs wallet public keys (truncated — safe). The `backfill-candles.ts:252` script redacts the API key in logs with `RPC_URL.replace(HELIUS_API_KEY, "***")` — good practice. The VRF reporter also masks API keys in URLs (`scripts/vrf/lib/reporter.ts:284-285`). No secret values are logged anywhere in the codebase.

**Vector 4: Error messages**
`crank-provider.ts:55` truncates error messages to 100 chars when WALLET_KEYPAIR parsing fails — prevents full error context leakage that might include partial key material.

### .gitignore Assessment

The `.gitignore` correctly excludes:
- `.env` — root env file
- `scripts/deploy/mint-keypairs/` — mint authority keys
- `scripts/deploy/pda-manifest.json` — deployment manifest

Missing from `.gitignore`:
- `.mcp.json` — contains private key
- `keypairs/` — contains program deploy keypairs and wallet keypair
- `.claude/settings.local.json` — contains API key in permission rules

### CSP and Security Headers

`app/next.config.ts` implements a strong CSP:
- `connect-src` explicitly whitelists Helius RPC endpoints (both HTTP and WSS)
- `script-src 'self' 'unsafe-inline'` — `unsafe-inline` is needed for Next.js but reduces XSS protection
- `frame-ancestors 'none'` — prevents clickjacking
- `upgrade-insecure-requests` — forces HTTPS

The CSP does NOT restrict `connect-src` enough to prevent the Helius API key from being used by injected scripts — any script that can execute in the page context can make RPC calls using the hardcoded URL.

## Cross-Focus Intersections

- **API-04 (Webhooks)**: The optional webhook authentication directly impacts data integrity. The `storeTaxedSwap`, `storeEpochEvents`, `storeCarnageEvent` functions write attacker-controlled data to the database when auth is bypassed.
- **BOT-01 (Crank)**: The crank wallet keypair loading mechanism is the most security-critical secret handling in the system. If compromised, the attacker can sign arbitrary transactions as the crank operator.
- **CHAIN-01 (TX Construction)**: The hardcoded RPC URL in the client bundle means users' transactions route through a known Helius endpoint. An attacker who compromises the Helius API key could potentially set up RPC interception.
- **DATA-01 (Database)**: DATABASE_URL handling is sound, but the optional webhook auth creates a data injection vector into the database.

## Cross-Reference Handoffs

1. -> **API-04**: Investigate full impact of unauthenticated webhook POST — what damage can an attacker do by injecting arbitrary swap/epoch/carnage events?
2. -> **BOT-01**: Verify the crank wallet has only the minimum permissions needed (can it drain protocol vaults, or is it limited to epoch transitions?).
3. -> **INFRA-03**: Assess Railway's env var encryption model — are env vars encrypted at rest, and who has dashboard access?
4. -> **DATA-01**: Verify that `onConflictDoNothing` idempotency protects against duplicate injection, but does it protect against FIRST injection of fake data?

## Risk Observations

### R1: [CRITICAL] Private key in `.mcp.json` (committed to git)
- **File:** `.mcp.json:8`
- **What:** Base58 Solana private key in MCP configuration, tracked by git
- **Impact:** Full signing authority for whoever holds this key. Permanent exposure in git history.
- **Remediation:** Immediately rotate the key. Add `.mcp.json` to `.gitignore`. Use BFG Repo-Cleaner to purge from history. Move to env-var-based MCP config.

### R2: [HIGH] Optional webhook authentication
- **File:** `app/app/api/webhooks/helius/route.ts:135-141`
- **What:** Fail-open auth pattern — no secret = no auth
- **Impact:** Database poisoning with fake events, manipulated price charts, false SSE broadcasts
- **Remediation:** Make auth mandatory (fail-closed). If `HELIUS_WEBHOOK_SECRET` is unset, reject all requests with 500. Add `X-Forwarded-For` logging for audit trail.

### R3: [HIGH] Program deploy keypairs in git
- **File:** `keypairs/*.json` (10 files)
- **What:** All Anchor program deploy keypairs are git-tracked
- **Impact:** Anyone with repo access holds the program upgrade authority for all 7 programs. If reused on mainnet, programs can be maliciously upgraded.
- **Remediation:** Generate fresh keypairs for mainnet. Add `keypairs/` to `.gitignore`. Consider multisig upgrade authority.

### R4: [MEDIUM] Helius API key hardcoded
- **File:** `shared/constants.ts:474`, `shared/programs.ts:22`, `scripts/webhook-manage.ts:28`, `scripts/backfill-candles.ts:47`
- **What:** API key with webhook management permissions in source code
- **Impact:** Webhook deletion/redirection, RPC rate limit abuse
- **Remediation:** Move to env var. Use separate keys for RPC (client-safe, read-only) vs webhook management (server-only, never exposed).

### R5: [MEDIUM] Webhook auth comparison not timing-safe
- **File:** `app/app/api/webhooks/helius/route.ts:138`
- **What:** `authHeader !== webhookSecret` uses standard string comparison
- **Impact:** Theoretical timing side-channel for secret guessing (low practical risk over HTTP)
- **Remediation:** Use `crypto.timingSafeEqual(Buffer.from(authHeader), Buffer.from(webhookSecret))` per SP-005.

### R6: [MEDIUM] RPC URL with API key in client bundle
- **File:** `shared/programs.ts:22`, `app/providers/providers.tsx:35`
- **What:** `DEVNET_RPC_URL` with embedded API key used as fallback in browser
- **Impact:** API key exposed to all frontend users
- **Remediation:** For mainnet, use an RPC proxy API route (server-side key) or a separate read-only API key for client-side use.

### R7: [MEDIUM] No enforcement that crank wallet is least-privilege
- **File:** `scripts/crank/crank-provider.ts:41`
- **What:** The crank wallet private key is loaded; no verification that it only has permissions for crank operations
- **Impact:** If the crank wallet is also the protocol admin, compromise enables protocol takeover
- **Remediation:** Verify crank wallet is a dedicated account with minimal authority.

### R8: [LOW] No secret rotation infrastructure
- **What:** No dual-key support, no rotation scripts, no grace periods
- **Impact:** If any secret is compromised, there's no zero-downtime rotation path
- **Remediation:** For mainnet, implement at minimum: dual Helius API key support, webhook secret rotation procedure, database credential rotation via Railway.

### R9: [LOW] Sentry DSN in NEXT_PUBLIC_ variable
- **File:** `app/instrumentation-client.ts:37`, `app/lib/sentry.ts:30`
- **What:** DSN exposed in client bundle
- **Impact:** Error spam, quota exhaustion. Low severity — this is standard Sentry practice.
- **Remediation:** Configure Sentry ingest rate limiting on the Sentry dashboard.

## Novel Attack Surface Observations

### Webhook Poisoning + Price Manipulation Chain
An attacker who discovers the unauthenticated webhook endpoint (URL is in the git repo) can:
1. POST fake `TaxedSwap` events with manipulated prices
2. These are stored in `swap_events` via `onConflictDoNothing` (first write wins)
3. Candle aggregation runs on the fake data (`upsertCandlesForSwap`)
4. SSE broadcast pushes fake prices to all connected frontend clients
5. Users see manipulated charts and may make trading decisions based on false data

The attack is amplified because the webhook processes events in isolation per transaction (line 258-267 error handling), so a batch of 100 fake events would all be processed even if one fails.

### Helius API Key -> Webhook Hijack
With the hardcoded API key, an attacker can:
1. Call `GET /v0/webhooks?api-key=...` to list existing webhooks
2. Call `PUT /v0/webhooks/{id}?api-key=...` to change the webhook URL to their server
3. All real-time transaction events now flow to the attacker
4. The legitimate app stops receiving events (data gap)
5. Attacker can replay events to the real endpoint later (with modifications)

## Questions for Other Focus Areas

1. **BOT-01**: What operations can the crank wallet perform? Is it the same wallet as the protocol admin/authority?
2. **INFRA-03**: How are Railway env vars protected? Is there an audit log of who accessed `WALLET_KEYPAIR`?
3. **API-04**: Is the webhook URL protected by any Railway-level firewall or IP allowlist?
4. **CHAIN-06**: Are the program IDs in `shared/constants.ts` verified against on-chain deployed programs, or could they be substituted?

## Raw Notes

- `.gitignore` only has `.env` (no `.env*` wildcard). However, `app/.env.local` exists and appears to be untracked (Next.js convention). No `.env.example` file exists documenting required variables.
- The `backfill-candles.ts` script (line 252) correctly redacts the API key in console output — shows awareness of the issue.
- The `crank-provider.ts` error message truncation (line 55) is a good defensive pattern against error-message-based secret leakage.
- No `console.log` of secret values found anywhere in the codebase — clean on this front.
- The `next.config.ts` CSP is comprehensive but `unsafe-inline` for scripts weakens XSS protection.
- `app/db/connection.ts` Proxy pattern is clever and security-positive — prevents build-time credential requirements.
- The comment "This is a free-tier API key, not a secret" at `shared/constants.ts:471` is a dangerous assumption that should be validated against the actual Helius API permissions granted to this key.
