# Public Repository Push Checklist

**Purpose:** Military-grade sanitization checklist for pushing ANY changes from the private repo to the public GitHub (MetalLegBob/drfraudsworth). Run this EVERY TIME, no exceptions — even for "just docs changes."

**Why this exists:** During Phase 104, 15+ sanitization passes still missed a devnet private key that the Bulwark auditor had helpfully pasted into its findings as evidence. It sat in the public repo for 10 days before being caught during the v1.4 milestone completion. The patterns we searched for (JSON keypair arrays, hex API keys) didn't catch a base58-encoded private key.

---

## Pre-Flight

- [ ] **Identify what changed**: `cd /tmp/drfraudsworth-public && git diff HEAD --stat` — know exactly what files are being pushed
- [ ] **No rush**: If you're in a hurry, STOP. Sanitization errors are permanent (git history).

---

## Phase 1: Automated Secret Scanning

Run ALL of these grep patterns against the public staging dir. Every pattern must return zero matches OR confirmed-safe results.

### 1A. Solana Keypairs (JSON array format)

```bash
# 10+ consecutive numbers in arrays = potential keypair bytes
grep -rn '[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\},[0-9]\{1,3\}' --include="*.json" --include="*.md" --include="*.ts" .
```

- [ ] Zero matches OR all matches are example patterns from skill knowledge-base docs

### 1B. Solana Keypairs (Base58 private key format)

```bash
# Base58 strings 80+ chars = potential private keys or TX signatures
grep -rPn '[A-HJ-NP-Za-km-z1-9]{80,}' --include="*.md" --include="*.ts" --include="*.json" . | grep -v 'node_modules\|package-lock\|explorer.solana.com/tx\|integrity.*sha512'
```

- [ ] Zero matches after excluding TX signatures (explorer URLs) and npm integrity hashes

### 1C. API Keys in URLs

```bash
# Helius RPC URLs with actual keys
grep -rni 'helius-rpc\.com/.*[a-f0-9]\{8,\}' --include="*.ts" --include="*.json" --include="*.sh" .
grep -rni 'api\.helius\.xyz.*[a-f0-9]\{20,\}' --include="*.ts" --include="*.json" .
```

- [ ] Zero matches (only `your-helius-api-key-here` placeholders allowed)

### 1D. Environment Variable Values (not names)

```bash
# Keys with actual values (not placeholders)
grep -rPn 'HELIUS_API_KEY=(?!\[REDACTED|your-|CHANGE_ME|\$|"?\$)' .
grep -rPn 'HELIUS_WEBHOOK_SECRET=(?!\[REDACTED|your-|CHANGE_ME|\$|"?\$)' .
grep -rPn 'SENTRY_DSN=(?!\[REDACTED|your-|CHANGE_ME|\$|"?\$)' .
grep -rPn 'DATABASE_URL=(?!postgres://user:)' --include="*.env*" .
grep -rPn 'SUPERMEMORY_CC_API_KEY=(?!your-)' --include="*.env*" .
grep -rPn 'WALLET_KEYPAIR=' --include="*.env*" .
```

- [ ] Zero matches on ALL six patterns

### 1E. Sentry DSNs

```bash
grep -rn 'https://[a-f0-9]*@.*\.ingest\..*\.sentry\.io' .
```

- [ ] Zero matches

### 1F. Database Connection Strings with Real Credentials

```bash
grep -rn 'postgres://.*:.*@.*railway\|postgres://.*:.*@.*\.com' --include="*.ts" --include="*.json" --include="*.env*" .
```

- [ ] Zero matches (only `postgres://user:pass@localhost` examples allowed)

### 1G. PEM Keys and Certificates

```bash
grep -rn '\-\-\-\-\-BEGIN.*PRIVATE KEY\-\-\-\-\-' .
```

- [ ] Zero matches

---

## Phase 2: Dangerous File Detection

### 2A. Files That Should NEVER Exist

```bash
# Check for files that must not be in public repo
ls -la .env .env.local .env.devnet .env.mainnet 2>/dev/null
ls -la .mcp.json 2>/dev/null
ls -la keypairs/ 2>/dev/null
ls -la app/certificates/ 2>/dev/null
find . -name "*.pem" -o -name "*.key" -o -name "*-keypair.json" | grep -v node_modules
```

- [ ] ALL commands return "No such file or directory" or empty results

### 2B. .env.example Has Only Placeholders

```bash
grep -v '^#\|^$\|CHANGE_ME\|your-.*-here\|NEXT_PUBLIC_\|=true\|=false\|=devnet\|=mainnet\|=launch\|=live' .env.example
```

- [ ] Zero matches (every non-comment, non-empty line is a placeholder or public config)

---

## Phase 3: Audit & Bulwark Findings Deep Scan

**THIS IS THE MOST DANGEROUS ZONE.** AI auditors quote actual secret values as evidence in their findings. The `.audit/`, `.bulwark/`, and `.bulwark-history/` directories are the #1 source of leaked secrets.

### 3A. Known Dangerous Patterns in Audit Findings

```bash
# Private keys quoted as evidence
grep -rn '2zJgKnGrwg\|8kPzhQoUP' .audit/ .bulwark/ .bulwark-history/ 2>/dev/null

# Partial webhook secrets
grep -rn '62b9474' .audit/ .bulwark/ .bulwark-history/ 2>/dev/null

# Any base58 string 44+ chars that ISN'T a known public address
grep -rPn '[A-HJ-NP-Za-km-z1-9]{44,88}' .audit/findings/ .bulwark/findings/ 2>/dev/null | \
  grep -v 'cRiME\|FraUd\|pRoFiT\|5JsS\|CiQP\|43fZ\|4Heq\|12b3\|5uaw\|DpX3\|23g7\|F84X\|4SMc\|3ihh\|REDACTED'
```

- [ ] Zero matches on all three

### 3B. Keypair Byte Arrays in Audit Context

```bash
# Partial or full keypair bytes in audit findings
grep -rn '\[1[0-9][0-9],[0-9]\{1,3\},[0-9]\{1,3\},' .audit/ .bulwark/ .bulwark-history/ 2>/dev/null | grep -v REDACTED
```

- [ ] Zero matches

### 3C. Manual Spot Check

Open and visually scan these high-risk files (if they exist):

- [ ] `.bulwark/findings/H001.md` — original private key finding
- [ ] `.bulwark/findings/S001.md` — secret management summary
- [ ] `.bulwark/findings/S004.md` — git history secrets
- [ ] `.bulwark/findings/S010.md` — remediation priority
- [ ] `.bulwark/findings/H120.md` — webhook secret finding
- [ ] `.audit/findings/S004.md` — SOS git history findings
- [ ] Any file with "credential" or "secret" in the filename

---

## Phase 4: Personal Information

### 4A. Personal Paths

```bash
# Source/config files only (docs mentions are acceptable)
grep -rn '/Users/mlbob' --include="*.ts" --include="*.tsx" --include="*.js" --include="*.json" --include="*.sh" --include="*.toml" .
```

- [ ] Zero matches in source/config files

### 4B. Personal Identifiers

```bash
# Emails beyond git commit metadata
grep -rni '@gmail\.com\|@protonmail\|@hotmail' --include="*.ts" --include="*.json" --include="*.md" . | grep -v node_modules | grep -v '.git/'
```

- [ ] Only expected addresses (metallegbob@gmail.com in README/git config is fine)

---

## Phase 5: Cross-Check Against Known Secrets

These are secrets we KNOW exist in the private repo. Verify NONE appear in the public repo.

| Secret | Pattern to Search | Must NOT Match |
|--------|------------------|----------------|
| Devnet wallet private key | `2zJgKnGrwg` | Any file |
| Mainnet crank wallet key | Any 88-char base58 near `F84X` | Source files |
| Helius API key (devnet) | 32-char hex after `helius` | Source files |
| Helius API key (mainnet) | 32-char hex after `helius` | Source files |
| Webhook secret | `62b9474` or any hex after `WEBHOOK_SECRET=` | Any file |
| Sentry DSN value | `https://[hex]@*.ingest.*.sentry.io` | Any file |
| Supermemory API key | After `SUPERMEMORY_CC_API_KEY=` (not placeholder) | Any file |
| Squads signer private keys | Keypair arrays in any signer JSON | Any file |
| Database password | After `postgres://` with real creds | Any file |

- [ ] ALL nine checked, ALL clean

---

## Phase 6: Final Gate

- [ ] `git diff --stat` reviewed — every file in the diff is expected
- [ ] No binary files in the diff (images must be committed intentionally)
- [ ] Commit message does NOT contain secret values
- [ ] If ANY doubt about ANY file — DO NOT PUSH. Ask first.

---

## Phase 7: Double Clean Sweep Gate

**Two consecutive clean sweeps are MANDATORY before presenting to the user for approval.**

- [ ] **Sweep 1**: Run ALL checks from Phases 1-6 above. Fix any issues found.
- [ ] **Sweep 2**: Re-run ALL checks from Phases 1-6 with zero fixes needed. If ANY fix is required during Sweep 2, reset the counter — you need two MORE consecutive clean sweeps.
- [ ] Both sweeps documented with timestamps

**Why:** Phase 109 found 4 additional issues on a second pass that the first pass missed (devnet mint keypairs, personal paths, partial keypair bytes). A single pass is not sufficient — fixes during one pass can introduce new issues or miss patterns that only become visible after other files are removed/modified.

---

## Push

Only after ALL checkboxes above are checked AND two consecutive clean sweeps pass:

```bash
cd /tmp/drfraudsworth-public
git push origin main
```

---

## Post-Push Verification

- [ ] Visit https://github.com/MetalLegBob/drfraudsworth and spot-check changed files
- [ ] GitHub's secret scanning alerts page shows no new alerts

---

## Emergency: Secret Found Post-Push

If a secret is discovered after pushing:

1. **Rotate the secret immediately** (new API key, new wallet, new webhook secret)
2. **Do NOT force-push** to remove — the secret is already in GitHub's CDN cache and may be scraped
3. **Push a redaction commit** to remove from HEAD
4. **Contact GitHub support** to purge from server-side cache if it's a mainnet private key
5. **Document the incident** in `.planning/debug/`

---

## Lessons Learned (Update This Section)

| Date | What Leaked | How It Got Through | Fix Applied |
|------|------------|-------------------|-------------|
| 2026-03-25 | Devnet private key (base58) | Bulwark auditor quoted key as evidence; `.bulwark/` allowlisted in gitleaks; grep patterns didn't search for base58 format | Added base58 80+ char pattern to Phase 1B; Added Phase 3 (audit findings deep scan) |
| 2026-03-25 | Partial webhook secret | Same allowlist issue | Added explicit hex check in Phase 3A |
| 2026-03-25 | Partial keypair bytes | SOS auditor showed git history evidence with real bytes | Added keypair byte array pattern in Phase 3B |

---

*Created: 2026-03-25 after v1.4 milestone completion*
*This checklist is mandatory for EVERY public push. No exceptions. No shortcuts.*
