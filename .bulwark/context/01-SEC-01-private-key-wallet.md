---
task_id: db-phase1-SEC-01
provides: [SEC-01-findings, SEC-01-invariants]
focus_area: SEC-01
files_analyzed: [
  "app/app/api/webhooks/helius/route.ts",
  "app/app/api/rpc/route.ts",
  "app/app/api/health/route.ts",
  "app/hooks/useSwap.ts",
  "app/hooks/useStaking.ts",
  "app/hooks/useProtocolWallet.ts",
  "app/lib/connection.ts",
  "app/lib/protocol-config.ts",
  "app/lib/ws-subscriber.ts",
  "app/lib/sse-connections.ts",
  "app/instrumentation.ts",
  "shared/constants.ts",
  "scripts/crank/crank-provider.ts",
  "scripts/crank/crank-runner.ts",
  "scripts/deploy/fix-carnage-wsol.ts",
  "scripts/deploy/upload-metadata.ts",
  ".mcp.json",
  ".env.devnet",
  ".gitignore"
]
finding_count: 9
severity_breakdown: {critical: 1, high: 2, medium: 3, low: 3}
---
<!-- CONDENSED_SUMMARY_START -->
# SEC-01: Private Key & Wallet Security — Condensed Summary

## Key Findings (Top 9)

1. **CRITICAL — Solana private key committed in `.mcp.json`**: A base58-encoded Solana private key (`[REDACTED-DEVNET-PRIVKEY-PREFIX]...`) is hardcoded in `.mcp.json` line 8, which is tracked by git (confirmed via `git ls-files`). This key has been in git history since at least commit `53ca01b`. Even if removed from HEAD, it persists in git history. — `.mcp.json:8`

2. **HIGH — 17 devnet keypair files committed to git**: The `keypairs/` directory has 17+ keypair JSON files tracked in git (devnet-wallet, squads-signer-1/2/3, program deploy keypairs, etc.). These are Solana keypair files containing secret keys. While devnet-only, git history preserves them permanently. H005 from Audit #1 flagged this as PARTIALLY_FIXED — no remediation observed in this delta. — `keypairs/*.json` (git-tracked)

3. **HIGH — `.env.devnet` committed with Helius API key and SuperMemory API key**: The file `.env.devnet` is tracked by git and contains `HELIUS_API_KEY=[REDACTED-DEVNET-KEY]...`. File comment says "devnet credentials are non-sensitive" but the Helius API key provides webhook management access (create/delete/list webhooks), not just RPC access. — `.env.devnet:5,8`

4. **MEDIUM — Crank wallet private key loaded from env var without zeroization**: `scripts/crank/crank-provider.ts:46` parses `WALLET_KEYPAIR` env var into a `secretKey` array, constructs a `Keypair`, but never zeroizes the intermediate `secretKey` array. The raw key material persists in V8 heap until GC. In a long-running crank process, a heap dump would expose the key. — `scripts/crank/crank-provider.ts:46-48`

5. **MEDIUM — Upload-metadata script converts keypair to base58 and passes to Irys SDK**: `scripts/deploy/upload-metadata.ts:126` reads keypair bytes, base58-encodes the full private key, and passes it as a string to the Irys SDK. The base58 string persists as a V8 string on the heap (immutable, not zeroizable). — `scripts/deploy/upload-metadata.ts:125-128`

6. **MEDIUM — Health endpoint exposes internal infrastructure state**: `/api/health` returns WebSocket subscriber status, RPC credit usage stats, and postgres connectivity status. While H028 (Audit #1, LOW) flagged this as NOT_FIXED, the health endpoint has expanded to include `wsSubscriber` diagnostics and `credits` counters. No authentication required. — `app/app/api/health/route.ts:66-72`

7. **LOW — `.mcp.json` not in `.gitignore`**: The `.gitignore` file does not have an entry for `.mcp.json`. Even if the private key is removed from the file, future MCP configuration changes could re-introduce secrets. — `.gitignore`

8. **LOW — `NEXT_PUBLIC_RPC_URL` still in server-side fallback chain**: `app/lib/connection.ts:41` and `app/app/api/rpc/route.ts:131` both include `NEXT_PUBLIC_RPC_URL` as a fallback endpoint. If this env var contains the Helius API key (as it did in earlier phases), the key is embedded in the client bundle at build time. H002/H009 from Audit #1 noted this was FIXED, but the fallback path remains. — `app/lib/connection.ts:41`, `app/app/api/rpc/route.ts:131`

9. **LOW — Devnet keypair file permissions inconsistent**: Some keypair files in `keypairs/` have `0600` permissions (devnet-wallet, mainnet-crime-mint) while others have `0644` (amm-keypair, bonding-curve-keypair, squads-signer-*). The deploy script `fix-carnage-wsol.ts:105` correctly sets `mode: 0o600` on new keypair files, but existing files were not retroactively fixed. — `keypairs/` directory

## Critical Mechanisms

- **Wallet signing flow (browser)**: User transactions are signed via `useProtocolWallet.ts` which uses `signTransaction()` + `sendRawTransaction()` (sign-then-send pattern). The browser NEVER has access to a private key — signing is delegated to the wallet adapter (Phantom/Solflare/Backpack). This is structurally secure. — `app/hooks/useProtocolWallet.ts:87-121`

- **RPC proxy (API key protection)**: Browser RPC calls route through `/api/rpc` which proxies to Helius with the API key kept server-side only (`HELIUS_RPC_URL` env var, no `NEXT_PUBLIC_` prefix). Method allowlist prevents abuse. This is the correct pattern (H002/H009 fixes). — `app/app/api/rpc/route.ts:1-188`, `app/lib/connection.ts:35-36`

- **Webhook authentication (HELIUS_WEBHOOK_SECRET)**: Production fail-closed: if `HELIUS_WEBHOOK_SECRET` is unset in production, ALL webhook requests are rejected (500). When set, uses `timingSafeEqual` with length-safe comparison. Properly handles unequal-length buffers by comparing against self. — `app/app/api/webhooks/helius/route.ts:270-301`

- **Crank wallet loading**: Priority: `WALLET_KEYPAIR` env var > `WALLET` file path > `keypairs/devnet-wallet.json` default. On Railway (production), uses env var (no keypair files deployed). Locally, falls back to committed devnet keypair. — `scripts/crank/crank-provider.ts:34-80`

## Invariants & Assumptions

- INVARIANT: Browser code NEVER has access to private keys — signing is always delegated to wallet adapter. — enforced at `app/hooks/useProtocolWallet.ts:94-102` (signTransaction requires wallet support)
- INVARIANT: Helius API key is server-side only in production — browser uses `/api/rpc` proxy. — enforced at `app/lib/connection.ts:35-36` (browser path returns `window.location.origin/api/rpc`)
- INVARIANT: Webhook authentication is fail-closed in production. — enforced at `app/app/api/webhooks/helius/route.ts:273-284`
- INVARIANT: `HELIUS_WEBHOOK_SECRET` comparison uses constant-time equality. — enforced at `app/app/api/webhooks/helius/route.ts:293-299`
- INVARIANT: Mainnet keypair files are gitignored. — enforced at `.gitignore:17` (`keypairs/mainnet-*`)
- ASSUMPTION: `NEXT_PUBLIC_RPC_URL` will NOT contain API keys in production. — UNVALIDATED (env var still accepted as fallback in `connection.ts:41` and `rpc/route.ts:131`)
- ASSUMPTION: Railway production environment has `HELIUS_RPC_URL` set (not `NEXT_PUBLIC_RPC_URL`). — UNVALIDATED (no runtime check distinguishing between them)
- ASSUMPTION: Git history will be purged for committed secrets before mainnet launch. — NOT ENFORCED (H005 from Audit #1 remains PARTIALLY_FIXED)

## Risk Observations (Prioritized)

1. **Private key in `.mcp.json` in git history**: This is a devnet key but it IS a real Solana private key committed in a tracked file. If this wallet ever received mainnet SOL (even by accident), those funds are compromised. Git history purging (`git filter-branch` or BFG) is required. — `.mcp.json:8`

2. **17 devnet keypairs in git history**: Program deploy keypairs (amm, bonding-curve, epoch, staking, tax, transfer-hook, vault) are committed. While these are devnet program IDs (not mainnet), the keypairs control program upgrade authority on devnet. An attacker with git access could upgrade devnet programs maliciously. — `keypairs/*.json`

3. **Helius API key in `.env.devnet` (committed)**: The Helius API key in `.env.devnet` has broader scope than just RPC — it enables webhook management (create, delete, list webhooks). An attacker could register their own webhook endpoint to intercept protocol events, or delete the existing webhook to disrupt data pipeline. — `.env.devnet:8`

4. **No key zeroization in crank process**: The crank runner is a long-lived Node.js process on Railway. Key material from `WALLET_KEYPAIR` parsing persists in V8 heap as both the parsed `secretKey` array and the `Keypair` object's internal `_keypair` buffer. A heap dump via debug port or memory corruption could expose it. — `scripts/crank/crank-provider.ts:46`

5. **Health endpoint information disclosure**: An unauthenticated attacker can learn: whether postgres is up, whether RPC is up, WebSocket subscriber state (connected, slot count, staleness), and RPC credit consumption. This aids reconnaissance for targeted attacks (e.g., attacking during degraded state). — `app/app/api/health/route.ts:32-73`

## Novel Attack Surface

- **MCP configuration as secret exfiltration vector**: `.mcp.json` is a tool configuration file that Claude Code and other AI assistants read. If an AI assistant is compromised or logs this file's contents to a third-party service, the private key within is leaked through a non-obvious channel. This is a novel supply chain risk specific to AI-assisted development environments.

- **Helius API key in `.env.devnet` enables webhook hijack**: Unlike pure RPC keys, the Helius API key enables webhook CRUD operations. An attacker with this key could register a webhook pointing to their server for the same program IDs, receiving copies of all protocol events (swap amounts, user wallets, epoch transitions) — a passive surveillance vector.

## Cross-Focus Handoffs

- → **SEC-02 (Secret & Credential Management)**: `.env.devnet` committed with `HELIUS_API_KEY` and `SUPERMEMORY_CC_API_KEY`. Investigate full scope of both API keys. Also check if any `.env.mainnet` values are in git history.
- → **CHAIN-02 (RPC Node Trust)**: `NEXT_PUBLIC_RPC_URL` fallback path in `connection.ts:41` and `rpc/route.ts:131`. If this env var is set at build time with API key, it leaks to client bundle. Verify Railway build-time env var configuration.
- → **INFRA-03 (Cloud/Env Config)**: Railway env var configuration — verify `WALLET_KEYPAIR` is set as a secret (not visible in logs), verify `HELIUS_RPC_URL` is set (not `NEXT_PUBLIC_RPC_URL`), verify health endpoint is not publicly accessible.
- → **DATA-04 (Logging Disclosure)**: Crank runner logs wallet address prefix (`scripts/crank/crank-provider.ts:51,78`). While only 12 chars, check if any other log path emits full key material.

## Trust Boundaries

The protocol has a clean trust boundary for private keys in the browser: the wallet adapter delegates all signing to external wallets (Phantom/Solflare/Backpack), and the browser code NEVER touches private key material. The RPC proxy correctly shields the Helius API key from client-side exposure. However, the server-side trust boundary is weaker: devnet keypairs and API keys are committed to git (accessible to anyone with repo access), the crank process holds key material in-memory without zeroization, and the health endpoint leaks infrastructure state to unauthenticated callers. The most urgent concern is the private key in `.mcp.json` in git history — this is a real key that must be rotated and the git history purged.
<!-- CONDENSED_SUMMARY_END -->

---

# SEC-01: Private Key & Wallet Security — Full Analysis

## Executive Summary

The Dr. Fraudsworth protocol demonstrates strong security practices for browser-side private key handling: the wallet adapter pattern ensures the frontend never touches private keys, and the RPC proxy correctly shields API credentials from client bundles. However, the server-side and repository-level key management has significant gaps. A Solana private key is committed in `.mcp.json`, 17 devnet keypair files are tracked in git, and the `.env.devnet` file exposes API keys with broader-than-RPC scope. These findings are consistent with the project's evolution from a rapid prototyping phase to a production-ready state, but they represent real risks that must be addressed before mainnet launch.

## Scope

**Analyzed:** All off-chain code touching private keys, secret keys, wallet operations, signing flows, key loading patterns, key storage, and credential exposure vectors. Includes: React hooks (useSwap, useStaking, useProtocolWallet), API routes (webhooks, RPC proxy, health), server infrastructure (connection, ws-subscriber, instrumentation), deployment scripts (crank-provider, upload-metadata, fix-carnage-wsol), shared constants, and configuration files (.mcp.json, .env.devnet, .gitignore).

**Out of scope:** Anchor/Rust on-chain programs (programs/ directory). On-chain authority model referenced from `.audit/ARCHITECTURE.md` for cross-boundary context only.

## Key Mechanisms

### 1. Browser Wallet Signing Flow

**File:** `app/hooks/useProtocolWallet.ts`

The protocol uses the sign-then-send pattern:
1. `signTransaction(tx)` — delegates to wallet adapter (Phantom/Solflare/Backpack)
2. `signed.serialize()` — serializes the signed transaction
3. `connection.sendRawTransaction(serialized)` — sends via our Helius RPC

**Why this is secure:** The browser NEVER has access to a private key. The wallet adapter calls into the browser extension's secure context for signing. The serialized signed transaction is sent through our controlled RPC endpoint (not Phantom's).

**Why sign-then-send:** `useProtocolWallet.ts:17-24` documents that Phantom's `signAndSendTransaction` uses Phantom's own RPC endpoint, which silently drops devnet transactions. This is a known Solana devnet issue. The sign-then-send pattern gives full control over which RPC node receives the transaction.

**Concern:** `sendRawTransaction` at line 111 passes `skipPreflight` from caller options. The callers (`useSwap.ts:764`, `useStaking.ts:580`) both set `skipPreflight: false`, which is correct. But the multi-hop builder (`multi-hop-builder.ts:378`) may use different options — needs verification by CHAIN-05 auditor.

### 2. RPC Proxy (API Key Protection)

**Files:** `app/lib/connection.ts`, `app/app/api/rpc/route.ts`

The RPC architecture has three layers:
1. **Browser:** `getConnection()` returns `window.location.origin/api/rpc` — a relative URL that routes through Next.js
2. **API route:** `/api/rpc` reads `HELIUS_RPC_URL` (server-only env var) and proxies requests
3. **Server:** `getConnection()` reads `HELIUS_RPC_URL` directly for server-side operations

**Method allowlist** (`rpc/route.ts:31-59`): Only 15 specific RPC methods are allowed. This prevents abuse of the API key through the proxy (e.g., `getSnapshotSlot`, `getBlocks`, etc. are blocked).

**Residual risk:** `NEXT_PUBLIC_RPC_URL` appears as a fallback in two places:
- `connection.ts:41`: `process.env.HELIUS_RPC_URL ?? process.env.NEXT_PUBLIC_RPC_URL`
- `rpc/route.ts:131`: Third endpoint in failover list

If `NEXT_PUBLIC_RPC_URL` is set at Railway build time with the Helius API key, it would be embedded in the client-side JavaScript bundle. The comment at `connection.ts:39-40` says "Both are set per-cluster on Railway" — this suggests they may contain the same URL. This is the remnant of H002/H009 from Audit #1.

### 3. Webhook Authentication

**File:** `app/app/api/webhooks/helius/route.ts`

Three-layer defense:
1. **Rate limiting** (line 258): Per-IP rate limit checked before any processing
2. **Fail-closed** (line 273): In production, missing `HELIUS_WEBHOOK_SECRET` rejects ALL requests with 500
3. **Timing-safe comparison** (line 293-299): Uses `timingSafeEqual` with proper length-mismatch handling

The length-mismatch handling deserves special attention:
```typescript
const lengthMatch = secretBuf.length === headerBuf.length;
const compareBuf = lengthMatch ? headerBuf : secretBuf;
if (!lengthMatch || !timingSafeEqual(secretBuf, compareBuf)) {
```

When lengths differ, it compares `secretBuf` against itself (always true), but then `!lengthMatch` causes rejection. This prevents leaking timing information about the secret's length. This is a well-implemented pattern (SP-005 from secure-patterns guide).

### 4. Crank Wallet Loading

**File:** `scripts/crank/crank-provider.ts`

Priority chain:
1. `WALLET_KEYPAIR` env var (JSON array string) — used on Railway/mainnet
2. `WALLET` env var (file path) — used locally with custom wallet
3. `keypairs/devnet-wallet.json` (committed) — devnet default

**Observations:**
- Line 51 logs a truncated wallet address (`slice(0, 12)`) — acceptable, public info
- Line 55 logs error message truncated to 100 chars — prevents accidental key material in error
- Line 78 logs the file path used — no secret material
- No zeroization of the `secretKey` array after constructing the `Keypair` object
- The `Keypair` object itself holds a reference to the full private key internally

### 5. Deploy Script Key Handling

**`scripts/deploy/fix-carnage-wsol.ts`:**
- Line 71: Reads keypair from file, constructs `Keypair` — no zeroization
- Line 104: Writes new keypair with `mode: 0o600` — correct file permissions
- Properly creates backup before overwriting

**`scripts/deploy/upload-metadata.ts`:**
- Line 125-126: Reads keypair bytes, base58-encodes the FULL private key
- Line 128: Passes base58 private key string to Irys SDK
- The base58 string is a V8 immutable string — cannot be zeroized
- This is a short-lived script (not a daemon), so heap exposure risk is lower

## Trust Model

### Trust Tier: Browser (Untrusted)
- Browser code NEVER handles private keys
- All signing delegated to wallet adapter extensions
- RPC calls proxied through `/api/rpc` — API key server-side only
- User input (amounts, slippage) validated before transaction building

### Trust Tier: Server (Trusted)
- `HELIUS_RPC_URL` and `HELIUS_WEBHOOK_SECRET` in env vars
- Webhook route authenticates incoming Helius payloads
- `ws-subscriber` connects to Helius WebSocket with server-side URL
- Health endpoint exposes infrastructure diagnostics without auth

### Trust Tier: Deploy Scripts (Admin)
- Run locally or on CI with admin keypair access
- Load keypairs from `keypairs/` directory (file-based)
- Some keypair files have incorrect permissions (0644 instead of 0600)

### Trust Tier: Crank (Operational)
- Long-running Railway process with `WALLET_KEYPAIR` env var
- Key material persists in V8 heap for process lifetime
- No external alerting if key is compromised (H004 from Audit #1)

### Trust Tier: Repository (Shared)
- `.mcp.json` contains private key — TRACKED
- `.env.devnet` contains API keys — TRACKED
- `keypairs/` contains 17 devnet keypairs — TRACKED
- `.gitignore` correctly blocks mainnet keypairs and `.env`/`.env.mainnet`

## State Analysis

### In-Memory Key Material

| Component | Key Material | Lifecycle | Zeroization |
|-----------|-------------|-----------|-------------|
| Browser (useProtocolWallet) | None — delegated to wallet adapter | N/A | N/A |
| ws-subscriber | Helius RPC URL (contains API key) | Process lifetime | No |
| Crank runner | Full private key (Keypair + parsed array) | Process lifetime | No |
| Deploy scripts | Full private key (Keypair + parsed array) | Script runtime | No |
| Upload-metadata | Full private key (base58 string) | Script runtime | No (V8 immutable string) |

### Persistent Key Material

| Location | Content | Git Status | Risk |
|----------|---------|------------|------|
| `.mcp.json` | Solana private key (base58) | TRACKED | CRITICAL |
| `.env.devnet` | Helius API key, SuperMemory API key | TRACKED | HIGH |
| `keypairs/*.json` (17 files) | Solana keypair JSON | TRACKED | HIGH |
| `keypairs/mainnet-*.json` | Mainnet keypairs | GITIGNORED | Correct |
| `.env`, `.env.mainnet` | Production secrets | GITIGNORED | Correct |

## Dependencies

- **@solana/web3.js** (Keypair, Connection): Core signing and RPC library. Keypair object stores private key internally without zeroization support — this is a known limitation of the library.
- **@solana/wallet-adapter-react**: Delegates signing to browser extension wallets. No private key exposure in the app.
- **@coral-xyz/anchor** (anchor.Wallet): Wraps Keypair for Anchor provider. Same zeroization limitation.
- **@irys/upload-solana**: Requires base58 private key string. Forces key material into V8 immutable string heap.
- **node:crypto** (timingSafeEqual): Used correctly for webhook secret comparison.

## Focus-Specific Analysis

### Key Lifecycle: Generation → Storage → Usage → Disposal

**Generation:**
- `fix-carnage-wsol.ts:101`: `Keypair.generate()` — uses `nacl.box.keyPair()` internally, which uses `crypto.randomBytes(32)`. Secure CSPRNG.
- Test files: `Keypair.generate()` — correct for test contexts.

**Storage:**
- Keypair JSON files in `keypairs/` — correct format but incorrect git tracking for devnet keys
- `WALLET_KEYPAIR` env var on Railway — correct pattern (not in filesystem)
- `.mcp.json` hardcoded private key — INCORRECT, should never be in source

**Usage:**
- Crank: `Keypair.fromSecretKey()` → `anchor.Wallet` → `AnchorProvider` — correct chain
- Deploy scripts: `Keypair.fromSecretKey()` → direct signing — correct
- Browser: Wallet adapter delegation — correct (no key access)

**Disposal:**
- No explicit zeroization anywhere in the codebase
- Node.js / V8 does not guarantee timely garbage collection of key material
- Long-lived processes (crank) hold key material for entire process lifetime

### Wallet Adapter Security

**`useProtocolWallet.ts` analysis:**

The hook correctly:
- Checks for `publicKey` before signing (line 93)
- Checks for `signTransaction` capability (line 94-98)
- Serializes signed TX before sending (line 107)
- Passes caller options through (skipPreflight, maxRetries, etc.)

The hook does NOT:
- Validate transaction content before signing (this is the wallet's responsibility — Blowfish/Phantom simulation)
- Implement timeout on `signTransaction` (wallet popup can hang indefinitely)
- Handle the case where `signTransaction` returns but the user actually cancelled (some wallets throw, others return null)

These are acceptable limitations — the wallet adapter library handles edge cases internally.

### Connection Singleton and API Key Protection

**`connection.ts` analysis:**

The singleton pattern is correct:
- Browser path: Returns proxy URL, no API key exposure
- Server path: Reads `HELIUS_RPC_URL` env var, throws if missing
- Singleton invalidated if URL changes (unlikely in production)

**Residual concern:** Line 41 fallback:
```typescript
const serverUrl = process.env.HELIUS_RPC_URL ?? process.env.NEXT_PUBLIC_RPC_URL;
```
If `HELIUS_RPC_URL` is unset and `NEXT_PUBLIC_RPC_URL` contains the API key, this works correctly on the server but the `NEXT_PUBLIC_` var is also embedded in the client bundle. The code comment says "Both are set per-cluster on Railway, so no cross-cluster risk" — but cross-cluster is not the concern here. The concern is API key in client bundle.

## Cross-Focus Intersections

### SEC-01 ↔ SEC-02 (Credential Management)
The `.env.devnet` file is a shared concern. SEC-01 focuses on the private key in `.mcp.json`; SEC-02 should investigate the full scope of the Helius API key and SuperMemory API key permissions.

### SEC-01 ↔ CHAIN-02 (RPC Node Trust)
The `NEXT_PUBLIC_RPC_URL` fallback path is a shared concern. If the browser uses a different RPC endpoint than the server for the same transaction, there's a consistency risk. Currently mitigated by the proxy pattern, but the fallback path could reintroduce it.

### SEC-01 ↔ INFRA-03 (Cloud Config)
Railway environment variable configuration determines whether secrets are properly isolated. The crank wallet (`WALLET_KEYPAIR`) must be a Railway secret (not visible in build logs). The health endpoint information disclosure aids attackers in timing attacks against infrastructure.

### SEC-01 ↔ ERR-03 (Error Handling)
Error messages must not leak key material. The crank provider truncates error messages to 100 chars (`crank-provider.ts:55`). The webhook route returns generic error messages ("Unauthorized", "Internal server error"). These are correct patterns.

## Cross-Reference Handoffs

| Target Auditor | Item | File:Line |
|---------------|------|-----------|
| SEC-02 | Helius API key scope (webhook CRUD vs RPC-only) | `.env.devnet:8` |
| SEC-02 | SuperMemory API key scope and permissions | `.env.devnet:5` |
| CHAIN-02 | `NEXT_PUBLIC_RPC_URL` fallback with possible API key | `app/lib/connection.ts:41` |
| CHAIN-05 | Multi-hop builder skipPreflight settings | `app/lib/swap/multi-hop-builder.ts:378` |
| INFRA-03 | Railway env var audit (WALLET_KEYPAIR as secret, HELIUS_RPC_URL vs NEXT_PUBLIC_RPC_URL) | N/A |
| INFRA-03 | Health endpoint unauthenticated access | `app/app/api/health/route.ts:32-73` |
| DATA-04 | Crank provider wallet address logging | `scripts/crank/crank-provider.ts:51,78` |

## Risk Observations

### 1. Private Key in `.mcp.json` — CRITICAL
**What:** Base58-encoded Solana private key at `.mcp.json:8`: `"SOLANA_PRIVATE_KEY": "[REDACTED-DEVNET-PRIVKEY-PREFIX]..."`. File is tracked by git since commit `53ca01b`.
**Why risky:** Anyone with repo access (current or historical) has this private key. If this wallet ever receives mainnet SOL (even accidentally via airdrop or transfer), those funds are immediately extractable.
**Impact:** Immediate fund theft if wallet has any value. Reputational damage if discovered.
**Remediation:** (1) Rotate the key immediately (generate new keypair). (2) Run BFG Repo-Cleaner to purge from git history. (3) Add `.mcp.json` to `.gitignore`. (4) Force push cleaned history.

### 2. Devnet Keypairs in Git — HIGH
**What:** 17 keypair JSON files in `keypairs/` tracked by git.
**Why risky:** These keypairs control devnet program upgrade authority, devnet wallet funds, and Squads multisig signer keys. An attacker with repo access can upgrade devnet programs to steal user funds on devnet or perform social engineering (e.g., showing "hacked" program behavior to discredit the project).
**Impact:** Devnet program compromise, devnet fund theft, reputational damage.
**Remediation:** (1) Stop tracking devnet keypairs in git. (2) Purge from git history. (3) Regenerate all devnet keypairs. (4) Update `.gitignore` to block all `keypairs/*.json`.

### 3. Helius API Key Committed — HIGH
**What:** `HELIUS_API_KEY=[REDACTED-DEVNET-HELIUS-KEY]` in `.env.devnet`, tracked by git.
**Why risky:** Helius API keys enable webhook management (create/delete/list). An attacker can register their own webhook to passively surveil all protocol events, or delete the production webhook to disrupt the data pipeline.
**Impact:** Passive surveillance of protocol activity, data pipeline disruption.
**Remediation:** (1) Rotate the Helius API key. (2) Remove from `.env.devnet` or replace with a separate RPC-only key. (3) Purge from git history.

### 4. No Key Zeroization — MEDIUM
**What:** Key material from `WALLET_KEYPAIR` env var persists in V8 heap after initial parsing.
**Why risky:** V8's garbage collector does not guarantee immediate memory reclamation. In the crank process (long-lived, potentially hours/days), a heap dump or memory corruption could expose the raw private key.
**Impact:** Crank wallet compromise via heap dump.
**Remediation:** While JavaScript lacks reliable memory zeroization, setting the parsed `secretKey` array to `null` after `Keypair.fromSecretKey()` would allow GC to collect it sooner.

### 5. Health Endpoint Information Disclosure — MEDIUM
**What:** `/api/health` returns infrastructure state without authentication.
**Why risky:** Reveals postgres connectivity, RPC status, WebSocket subscriber state (whether connected, latest slot, staleness timer), and RPC credit consumption rates. This information helps an attacker time DoS attacks or identify when the system is degraded.
**Impact:** Enhanced reconnaissance capability for targeted attacks.
**Remediation:** (1) Add basic auth or IP allowlist to health endpoint, OR (2) Split into a minimal liveness probe (200 OK) and a detailed diagnostics endpoint behind auth.

## Novel Attack Surface Observations

### 1. AI Tool Configuration as Secret Exfiltration Vector
The `.mcp.json` file is read by Claude Code and potentially other AI assistants. If any AI tool sends file contents to a logging service, telemetry endpoint, or third-party API, the private key within is silently exfiltrated. This is a novel supply chain vector that traditional secret scanners would miss because `.mcp.json` is not a standard secret file type.

### 2. Helius Webhook CRUD via Committed API Key
Unlike pure RPC keys, the Helius API key in `.env.devnet` provides webhook management capabilities. An attacker could:
- Register a shadow webhook (same program IDs, their URL) to receive copies of all protocol events
- Delete the legitimate webhook to disrupt chart data and SSE broadcasts
- Modify webhook filters to reduce event coverage

This is a unique attack vector because the key provides control-plane access (not just data-plane).

### 3. Keypair File as Integrity Anchor
Program deploy keypairs in `keypairs/` contain the private keys that correspond to program IDs. If an attacker obtains these and the program upgrade authority hasn't been burned or transferred to Squads, they can deploy malicious program versions. The fact that these are in git creates a persistent threat that survives key rotation (git history preserves the original keys).

## Questions for Other Focus Areas

1. **For CHAIN-02:** When the RPC proxy at `/api/rpc` receives a `sendTransaction` call, does it forward the raw signed transaction to Helius? Could a malicious client send a crafted transaction through the proxy?
2. **For INFRA-03:** Is `WALLET_KEYPAIR` configured as a "secret" variable in Railway (masked in logs/UI), or is it a regular env var?
3. **For SEC-02:** What is the exact scope of the Helius API key `[REDACTED-DEVNET-KEY]...`? Is it a free-tier key with limited permissions, or does it have full account access?
4. **For DATA-04:** Are there any other log paths that could emit key material? Specifically, does the Anchor error path ever serialize account data that includes keypair bytes?
5. **For INFRA-03:** Is the `/api/health` endpoint accessible from the public internet on Railway, or is it behind an internal network?

## Raw Notes

### File-by-File Analysis Notes

**`.mcp.json`:**
- Contains `SOLANA_PRIVATE_KEY` as a full base58-encoded key
- Used by `solana-mcp` tool (Solana MCP server for Claude Code)
- Points to devnet RPC (`api.devnet.solana.com`)
- Tracked in git since commit `53ca01b` ("feat: add 24/7 crank runner for Railway + SVK security tooling")
- NOT in `.gitignore`

**`.env.devnet`:**
- Tracked in git (confirmed via `git ls-files .env* app/.env*`)
- Contains: `HELIUS_API_KEY`, `CLUSTER_URL` (with key in query string), `SUPERMEMORY_CC_API_KEY`
- Comment says "devnet credentials are non-sensitive" — this underestimates Helius key scope

**`.gitignore` analysis:**
- `.env` and `.env.mainnet` are gitignored (correct)
- `.env.devnet` is NOT gitignored (intentional, but risky)
- `keypairs/mainnet-*` is gitignored (correct)
- `keypairs/*.json` (devnet) is NOT gitignored — only specific files tracked
- `.mcp.json` is NOT gitignored (oversight)

**`shared/constants.ts`:**
- Auto-generated from `deployments/devnet.json`
- Contains ONLY public keys (program IDs, mints, PDAs)
- No private keys or API keys
- H002 from Audit #1 (Helius API key in bundle) appears to be resolved — no RPC URL in this file

**`app/lib/connection.ts`:**
- Clean implementation: browser uses proxy, server uses env var
- `NEXT_PUBLIC_RPC_URL` still present as fallback — carry-forward from H002/H009
- Throws on missing server-side URL — fail-fast is correct
- WebSocket endpoint correctly mirrors HTTP for server-side (wss:// from https://)

**`app/hooks/useSwap.ts`:**
- Never touches private keys
- Uses `wallet.sendTransaction()` which delegates to `useProtocolWallet`
- `skipPreflight: false` at line 764 — correct for regular swaps
- Error handling wraps and rethrows without key material

**`app/hooks/useStaking.ts`:**
- Same pattern as useSwap: delegates signing to useProtocolWallet
- `skipPreflight: false` at line 580
- No private key material anywhere

**`scripts/crank/crank-provider.ts`:**
- Properly prioritizes env var over file for Railway deployment
- Logs truncated wallet address (12 chars) — acceptable
- Error messages truncated to 100 chars — good practice
- No key zeroization after Keypair construction

**`scripts/deploy/fix-carnage-wsol.ts`:**
- Sets file permissions to 0o600 on new keypair — correct
- Creates backup before overwriting — good practice
- No key zeroization

**`scripts/deploy/upload-metadata.ts`:**
- Converts keypair bytes to base58 for Irys SDK — unavoidable (SDK requirement)
- The base58 string is a V8 immutable string — cannot be zeroized
- Script is short-lived, so heap exposure window is smaller

**`app/app/api/webhooks/helius/route.ts`:**
- Webhook secret loaded from env var (line 270) — correct
- Fail-closed in production (lines 273-284) — correct
- timingSafeEqual with length-safe comparison (lines 293-299) — correct
- Rate limiting before auth check (line 258) — correct order (prevents timing attacks on rate-limited requests)

**`app/lib/sse-connections.ts`:**
- Tagged SEC-01 in index due to connection tracking
- No private key material — this is infrastructure protection
- Correct globalThis singleton pattern
- MAX_PER_IP=10, MAX_GLOBAL=5000 — reasonable limits

**`app/app/api/health/route.ts`:**
- Returns postgres status, RPC status, ws-subscriber diagnostics, credit stats
- No authentication required
- H028/H085 from Audit #1 — NOT_FIXED / ACCEPTED_RISK
- Expanded since Audit #1 to include wsSubscriber and credits
