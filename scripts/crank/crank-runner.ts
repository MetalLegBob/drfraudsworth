/**
 * Crank Runner — 24/7 Epoch Advancement with Atomic Carnage
 *
 * A lean, production-focused process that continuously advances protocol
 * epochs via VRF and atomically executes Carnage when triggered.
 *
 * Unlike the overnight E2E runner, this does NOT:
 * - Create test users or wrap WSOL
 * - Execute test swaps or stake tokens
 * - Request devnet airdrops
 * - Write JSONL files or markdown reports
 *
 * All output goes to stdout (Railway captures logs automatically).
 *
 * Carnage atomic bundling: reveal + consume + executeCarnageAtomic are
 * bundled in a single v0 VersionedTransaction, closing the CARN-002 MEV
 * gap. No CarnagePending event is visible on-chain before the swap.
 *
 * Run locally:
 *   set -a && source .env && set +a && npx tsx scripts/crank/crank-runner.ts
 *
 * Required env vars (Railway):
 *   CLUSTER_URL     - Solana RPC endpoint
 *   COMMITMENT      - Transaction commitment (default: confirmed)
 *   PDA_MANIFEST    - Full JSON of pda-manifest.json
 *
 * Optional env vars:
 *   WALLET_KEYPAIR          - JSON byte array of wallet keypair (for mainnet)
 *   WALLET                  - Path to wallet keypair file
 *   MIN_EPOCH_SLOTS_OVERRIDE - Override auto-detected epoch slot count
 *   CRANK_LOW_BALANCE_SOL   - SOL balance warning threshold (default: auto)
 *   HEALTH_PORT             - Port for internal /health endpoint (default: 8080)
 *   TELEGRAM_BOT_TOKEN      - Telegram bot token for crank alerts (optional)
 *   TELEGRAM_CHAT_ID        - Telegram chat ID for crank alerts (optional)
 */

import { createServer, Server } from "http";
import * as fs from "fs";
import { Keypair, PublicKey, LAMPORTS_PER_SOL, SystemProgram, Transaction, Connection } from "@solana/web3.js";
import { AnchorProvider, Program } from "@coral-xyz/anchor";
import * as sb from "@switchboard-xyz/on-demand";
import { NATIVE_MINT } from "@solana/spl-token";
import { loadCrankProvider, loadCrankPrograms, loadManifest } from "./crank-provider";
import {
  advanceEpochWithVRF,
  closeRandomnessAccount,
  VRFAccounts,
  waitForSlotAdvance,
  sleep,
} from "../vrf/lib/vrf-flow";
import { readEpochState } from "../vrf/lib/epoch-reader";
import { getOrCreateProtocolALT } from "../e2e/lib/alt-helper";
import { sendAlert } from "./lib/telegram";

// ---- Constants ----

/**
 * Slots to wait between epoch transitions.
 * On-chain SLOTS_PER_EPOCH = 750 (devnet) / 4500 (mainnet).
 * We add a small buffer to ensure the boundary has passed.
 */
const SLOT_WAIT_BUFFER = 10;

/** Base delay for cycle error retry. Exponential: 15s * 2^(errors-1), capped at 240s. */
const ERROR_BASE_DELAY_MS = 15_000;
const ERROR_MAX_DELAY_MS = 240_000;

/** Delay between RPC calls to respect rate limits (ms) */
const RPC_DELAY_MS = 200;

/**
 * Vault balance safeguard — prevents the on-chain rent-bug danger zone.
 *
 * The on-chain bounty check is `vault_balance >= TRIGGER_BOUNTY_LAMPORTS` but
 * doesn't account for rent-exempt minimum. If vault is between 1M and ~1.89M
 * lamports, the check passes but the system_program::transfer fails because
 * the remaining balance would be sub-rent-exempt.
 *
 * The crank tops up the vault before this can happen.
 */
const TRIGGER_BOUNTY_LAMPORTS = 1_000_000; // Must match on-chain constant
const RENT_EXEMPT_MINIMUM = 890_880;       // 0-data SystemAccount
const MIN_VAULT_BALANCE = TRIGGER_BOUNTY_LAMPORTS + RENT_EXEMPT_MINIMUM + 100_000; // ~2M
const VAULT_TOP_UP_LAMPORTS = 5_000_000;   // 0.005 SOL — covers ~5 bounties

/** Safety-net sweep interval. Every N cycles, re-run the startup sweep to catch any leaked accounts. */
const PERIODIC_SWEEP_INTERVAL = 50;

/**
 * H013 — Maximum SOL the crank will send to the vault in a single top-up.
 * Prevents a bug from draining the crank wallet into the vault.
 */
const MAX_TOPUP_LAMPORTS = 100_000_000;    // 0.1 SOL ceiling per top-up

// ---- Circuit Breaker (H019) ----

/**
 * Consecutive error threshold. After this many failures in a row (without
 * any successful epoch cycle in between), the crank halts and exits.
 * This prevents infinite error loops (RPC down, bad program state, etc.).
 */
const CIRCUIT_BREAKER_THRESHOLD = 5;

let consecutiveErrors = 0;
let lastSuccessTimestamp: number | null = null;

// ---- Spending Cap (H019) ----

/**
 * Rolling-hour SOL spending cap. If the crank spends more than this in any
 * 60-minute window, it halts. Normal usage is ~0.01 SOL/hour (one TX every
 * ~5 min at 10k lamports). The 0.5 SOL cap provides ~50x headroom while
 * catching runaway loops.
 */
const MAX_HOURLY_SPEND_LAMPORTS = 500_000_000; // 0.5 SOL

/**
 * Conservative per-transaction cost estimate: base fee (5000 lamports) +
 * priority fee headroom. Imprecise but safe — the cap has 50x headroom.
 */
const ESTIMATED_TX_COST_LAMPORTS = 10_000;

interface SpendEntry {
  lamports: number;
  timestamp: number;
}

const spendingLog: SpendEntry[] = [];

/** Sum spend entries within the last hour */
function getCurrentHourlySpend(): number {
  const cutoff = Date.now() - 3_600_000; // 1 hour ago
  return spendingLog
    .filter((e) => e.timestamp >= cutoff)
    .reduce((sum, e) => sum + e.lamports, 0);
}

/** Remove entries older than 1 hour */
function pruneSpendingLog(): void {
  const cutoff = Date.now() - 3_600_000;
  while (spendingLog.length > 0 && spendingLog[0].timestamp < cutoff) {
    spendingLog.shift();
  }
}

/**
 * Record a transaction's estimated cost. Returns false (halt) if the
 * hourly cap would be exceeded.
 */
function recordSpend(lamports: number): boolean {
  const currentSpend = getCurrentHourlySpend();
  if (currentSpend + lamports >= MAX_HOURLY_SPEND_LAMPORTS) {
    console.error(
      `[crank] CRITICAL: Hourly spending cap reached! ` +
      `Current: ${currentSpend} + ${lamports} >= ${MAX_HOURLY_SPEND_LAMPORTS} lamports. Halting.`
    );
    return false;
  }
  spendingLog.push({ lamports, timestamp: Date.now() });
  return true;
}

// ---- Health Endpoint (H019) ----

/**
 * Minimal HTTP health server for Railway internal health checks.
 * Binds to 0.0.0.0 so Railway's internal probes can reach it.
 * No public domain assigned — zero public attack surface.
 */
const HEALTH_PORT = parseInt(process.env.HEALTH_PORT || "8080", 10);

let healthServer: Server | null = null;

function startHealthServer(): Server {
  const server = createServer((req, res) => {
    if (req.method === "GET" && req.url === "/health") {
      const status = {
        status: consecutiveErrors >= CIRCUIT_BREAKER_THRESHOLD ? "halted" : "running",
        consecutiveErrors,
        circuitBreakerThreshold: CIRCUIT_BREAKER_THRESHOLD,
        hourlySpendLamports: getCurrentHourlySpend(),
        maxHourlySpendLamports: MAX_HOURLY_SPEND_LAMPORTS,
        uptime: process.uptime(),
        lastSuccessAt: lastSuccessTimestamp
          ? new Date(lastSuccessTimestamp).toISOString()
          : null,
      };
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(status));
    } else {
      res.writeHead(404);
      res.end();
    }
  });

  server.listen(HEALTH_PORT, "0.0.0.0", () => {
    console.log(`[health] Listening on :${HEALTH_PORT}/health`);
  });

  return server;
}

// ---- Graceful Shutdown ----

let shutdownRequested = false;

process.on("SIGINT", () => {
  shutdownRequested = true;
  console.log("\n[crank] Shutdown requested (SIGINT). Finishing current cycle...");
  if (healthServer) healthServer.close();
});

process.on("SIGTERM", () => {
  shutdownRequested = true;
  console.log("\n[crank] Shutdown requested (SIGTERM). Finishing current cycle...");
  if (healthServer) healthServer.close();
});

// ---- Carnage WSOL Loader ----

/**
 * Load the Carnage WSOL account pubkey.
 * PUBKEY-ONLY: No secret key loading in production.
 * Set CARNAGE_WSOL_PUBKEY env var in all environments (Railway, local dev).
 */
function loadCarnageWsolPubkey(): PublicKey {
  const envPubkey = process.env.CARNAGE_WSOL_PUBKEY;
  if (!envPubkey) {
    throw new Error(
      "CARNAGE_WSOL_PUBKEY env var not set. " +
      "Set it to the Carnage WSOL token account pubkey (base58)."
    );
  }
  return new PublicKey(envPubkey);
}

// ---- Persistent Randomness Keypair ----

/**
 * Load or generate the persistent randomness keypair.
 *
 * Switchboard's Randomness.create() generates a per-account ALT whose rent
 * (~0.0035 SOL) can never be reclaimed (authority is a Switchboard PDA).
 * By reusing a single randomness account across epochs, we create one ALT
 * total instead of one per epoch (~0.168 SOL/day savings at 48 epochs/day).
 *
 * Priority:
 *   1. RNG_KEYPAIR env var (JSON byte array) — for Railway (no filesystem)
 *   2. randomness-keypair.json file — for local dev
 *   3. Generate new keypair and save to file
 */
const RNG_KEYPAIR_FILE = "randomness-keypair.json";

function loadOrCreatePersistentRng(): Keypair {
  // Railway: keypair in env var (no persistent filesystem)
  const envKp = process.env.RNG_KEYPAIR;
  if (envKp) {
    try {
      const kp = Keypair.fromSecretKey(new Uint8Array(JSON.parse(envKp)));
      console.log(`  Persistent RNG (env): ${kp.publicKey.toBase58().slice(0, 12)}...`);
      return kp;
    } catch (err) {
      console.log(`  WARNING: RNG_KEYPAIR env var invalid, generating fresh: ${String(err).slice(0, 100)}`);
    }
  }

  // Local: load from file
  if (fs.existsSync(RNG_KEYPAIR_FILE)) {
    try {
      const data = JSON.parse(fs.readFileSync(RNG_KEYPAIR_FILE, "utf8"));
      const kp = Keypair.fromSecretKey(new Uint8Array(data));
      console.log(`  Persistent RNG (file): ${kp.publicKey.toBase58().slice(0, 12)}...`);
      return kp;
    } catch (err) {
      console.log(`  WARNING: ${RNG_KEYPAIR_FILE} invalid, generating fresh: ${String(err).slice(0, 100)}`);
    }
  }

  // First run: generate and save
  const kp = Keypair.generate();
  try {
    fs.writeFileSync(RNG_KEYPAIR_FILE, JSON.stringify(Array.from(kp.secretKey)));
    console.log(`  Persistent RNG (new, saved to ${RNG_KEYPAIR_FILE}): ${kp.publicKey.toBase58().slice(0, 12)}...`);
  } catch {
    // Railway: no filesystem write — keypair lives only in memory this session.
    // On restart a new keypair is generated. Set RNG_KEYPAIR env var to persist.
    console.log(`  Persistent RNG (new, in-memory only): ${kp.publicKey.toBase58().slice(0, 12)}...`);
  }
  return kp;
}

/**
 * Save a new persistent randomness keypair (after oracle recovery creates a fresh one).
 * Updates the file if writable; logs the env var value for Railway config.
 */
function savePersistentRng(kp: Keypair): void {
  const secretArr = Array.from(kp.secretKey);
  try {
    fs.writeFileSync(RNG_KEYPAIR_FILE, JSON.stringify(secretArr));
    console.log(`  [rng] Saved new persistent keypair to ${RNG_KEYPAIR_FILE}: ${kp.publicKey.toBase58().slice(0, 12)}...`);
  } catch {
    // Can't write (Railway) — log the value so operator can update env var
    console.log(`  [rng] New persistent keypair (update RNG_KEYPAIR env var): ${kp.publicKey.toBase58().slice(0, 12)}...`);
    console.log(`  [rng] RNG_KEYPAIR=${JSON.stringify(secretArr)}`);
  }
}

// ---- RPC URL Masking ----

/**
 * Mask API keys in RPC URLs for safe logging.
 * Helius keys appear as path segments (/v0/YOUR_API_KEY), other providers
 * may use query params. This masks both without hiding the host.
 */
function maskRpcUrl(url: string): string {
  try {
    const parsed = new URL(url);
    // Mask path components that look like API keys (long alphanumeric strings)
    const maskedPath = parsed.pathname.replace(
      /\/([a-zA-Z0-9_-]{20,})/g,
      '/***masked***'
    );
    // Also mask query param values
    const maskedSearch = parsed.search.replace(
      /=([a-zA-Z0-9_-]{10,})/g,
      '=***masked***'
    );
    return `${parsed.protocol}//${parsed.host}${maskedPath}${maskedSearch}`;
  } catch {
    return '***invalid-url***';
  }
}

// ---- Configurable Settings ----

/**
 * Determine MIN_EPOCH_SLOTS for epoch boundary wait calculation.
 * Priority:
 *   1. MIN_EPOCH_SLOTS_OVERRIDE env var (explicit override always wins)
 *   2. Auto-detect from CLUSTER_URL: "devnet" in URL = 750, else 4500 (mainnet)
 */
function getMinEpochSlots(): number {
  const override = process.env.MIN_EPOCH_SLOTS_OVERRIDE;
  if (override) {
    const parsed = parseInt(override, 10);
    if (isNaN(parsed) || parsed <= 0) {
      throw new Error(`Invalid MIN_EPOCH_SLOTS_OVERRIDE: ${override}`);
    }
    return parsed;
  }

  const clusterUrl = (process.env.CLUSTER_URL || "").toLowerCase();
  if (clusterUrl.includes("devnet")) {
    return 750;
  }
  // Default to mainnet timing (conservative -- better to wait too long than retry too early)
  return 4500;
}

/**
 * Low balance warning threshold in SOL.
 * Priority:
 *   1. CRANK_LOW_BALANCE_SOL env var
 *   2. Auto-detect: 0.5 SOL (devnet), 1.0 SOL (mainnet)
 */
function getLowBalanceThreshold(): number {
  const override = process.env.CRANK_LOW_BALANCE_SOL;
  if (override) {
    const parsed = parseFloat(override);
    if (isNaN(parsed) || parsed <= 0) {
      throw new Error(`Invalid CRANK_LOW_BALANCE_SOL: ${override}`);
    }
    return parsed;
  }

  const clusterUrl = (process.env.CLUSTER_URL || "").toLowerCase();
  return clusterUrl.includes("devnet") ? 0.5 : 1.0;
}

// ---- Startup Sweep: Close Stale Randomness Accounts ----

/**
 * Sweeps for Switchboard randomness accounts created by this crank wallet
 * that were never closed (e.g. from a previous session that crashed or
 * hit the circuit breaker). Reclaims rent from each.
 *
 * Uses getProgramAccounts with a memcmp filter on the authority field
 * (offset 8, after the 8-byte Anchor discriminator).
 */
async function sweepStaleRandomnessAccounts(
  provider: AnchorProvider,
  excludePubkey?: PublicKey,
): Promise<void> {
  const crankWallet = provider.wallet.publicKey;
  const connection = provider.connection;

  console.log("[crank] Sweeping for stale randomness accounts...");

  try {
    const sbProgramId = await sb.getProgramId(connection);

    const accounts = await connection.getProgramAccounts(sbProgramId, {
      filters: [
        {
          memcmp: {
            offset: 8, // authority field: after 8-byte Anchor discriminator
            bytes: crankWallet.toBase58(),
          },
        },
      ],
    });

    if (accounts.length === 0) {
      console.log("  [sweep] No stale randomness accounts found");
      return;
    }

    console.log(`  [sweep] Found ${accounts.length} stale account(s). Closing...`);
    let closedCount = 0;
    let reclaimedLamports = 0;

    for (const { pubkey, account } of accounts) {
      // Skip the persistent randomness account — it's intentionally kept open for reuse
      if (excludePubkey && pubkey.toBase58() === excludePubkey.toBase58()) {
        console.log(`  [sweep] Skipping persistent account ${pubkey.toBase58().slice(0, 12)}...`);
        continue;
      }
      const balanceBefore = account.lamports;
      const sig = await closeRandomnessAccount(provider, pubkey);
      if (sig) {
        closedCount++;
        reclaimedLamports += balanceBefore;
        console.log(
          `  [sweep] Closed ${pubkey.toBase58().slice(0, 12)}... ` +
          `(~${(balanceBefore / LAMPORTS_PER_SOL).toFixed(4)} SOL) TX: ${sig.slice(0, 16)}...`
        );
      }
      await sleep(200); // Rate limit
    }

    console.log(
      `  [sweep] Done: ${closedCount}/${accounts.length} closed, ` +
      `~${(reclaimedLamports / LAMPORTS_PER_SOL).toFixed(4)} SOL reclaimed`
    );
  } catch (err) {
    // Non-fatal — sweep is best-effort
    console.log(`  [sweep] WARNING: Sweep failed: ${String(err).slice(0, 200)}`);
  }
}

// ---- Main ----

async function main(): Promise<void> {
  console.log("=".repeat(60));
  console.log("  CRANK RUNNER — 24/7 Epoch Advancement");
  console.log(`  Started: ${new Date().toISOString()}`);
  console.log("=".repeat(60));
  console.log();

  const crankStartMs = Date.now();

  // ---- Load Configuration ----
  console.log("[crank] Loading configuration...");

  const provider = loadCrankProvider();
  const programs = loadCrankPrograms(provider);
  const manifest = loadManifest();

  const carnageWsolPubkey = loadCarnageWsolPubkey();
  console.log(`  Carnage WSOL: ${carnageWsolPubkey.toBase58().slice(0, 12)}...`);

  // Load persistent randomness keypair (saves ~0.168 SOL/day in ALT rent)
  console.log("[crank] Loading persistent randomness keypair...");
  let persistentRngKp = loadOrCreatePersistentRng();

  // Load ALT (reads committed alt-address.json; passes carnage WSOL pubkey to avoid file read on Railway)
  console.log("[crank] Loading Address Lookup Table...");
  const alt = await getOrCreateProtocolALT(provider, manifest, carnageWsolPubkey);
  console.log();

  // ---- Build VRF Accounts ----
  const crimePool = manifest.pools["CRIME/SOL"];
  const fraudPool = manifest.pools["FRAUD/SOL"];

  const vrfAccounts: VRFAccounts = {
    epochStatePda: new PublicKey(manifest.pdas.EpochState),
    treasuryPda: provider.wallet.publicKey,
    stakingAuthorityPda: new PublicKey(manifest.pdas.StakingAuthority),
    stakePoolPda: new PublicKey(manifest.pdas.StakePool),
    stakingProgramId: new PublicKey(manifest.programs.Staking),
    carnageFundPda: new PublicKey(manifest.pdas.CarnageFund),

    // Full carnage accounts for atomic bundling (CARN-002 fix)
    carnageAccounts: {
      carnageSignerPda: new PublicKey(manifest.pdas.CarnageSigner),
      carnageSolVault: new PublicKey(manifest.pdas.CarnageSolVault),
      carnageWsol: carnageWsolPubkey,
      carnageCrimeVault: new PublicKey(manifest.pdas.CarnageCrimeVault),
      carnageFraudVault: new PublicKey(manifest.pdas.CarnageFraudVault),
      crimePool: new PublicKey(crimePool.pool),
      crimePoolVaultA: new PublicKey(crimePool.vaultA),
      crimePoolVaultB: new PublicKey(crimePool.vaultB),
      fraudPool: new PublicKey(fraudPool.pool),
      fraudPoolVaultA: new PublicKey(fraudPool.vaultA),
      fraudPoolVaultB: new PublicKey(fraudPool.vaultB),
      mintA: NATIVE_MINT,
      crimeMint: new PublicKey(manifest.mints.CRIME),
      fraudMint: new PublicKey(manifest.mints.FRAUD),
      taxProgram: new PublicKey(manifest.programs.TaxProgram),
      ammProgram: new PublicKey(manifest.programs.AMM),
      swapAuthority: new PublicKey(manifest.pdas.SwapAuthority),
    },

    alt,

    persistentRngKp,
  };

  // ---- Compute Configurable Settings ----
  const MIN_EPOCH_SLOTS = getMinEpochSlots();
  const LOW_BALANCE_SOL = getLowBalanceThreshold();

  console.log("[crank] Configuration loaded. VRF accounts ready.");
  console.log(`  Epoch Program: ${manifest.programs.EpochProgram}`);
  console.log(`  Wallet: ${provider.wallet.publicKey.toBase58()}`);
  console.log(`  RPC: ${maskRpcUrl(process.env.CLUSTER_URL || "http://localhost:8899")}`);
  console.log(`  Epoch slots: ${MIN_EPOCH_SLOTS} (${process.env.MIN_EPOCH_SLOTS_OVERRIDE ? 'env override' : 'auto-detected'})`);
  console.log(`  Balance alert: < ${LOW_BALANCE_SOL} SOL`);
  console.log();

  // ---- Read Initial State ----
  let epochState = await readEpochState(
    programs.epochProgram,
    vrfAccounts.epochStatePda
  );
  console.log(`[crank] Current epoch: ${epochState.currentEpoch}`);
  console.log(`[crank] Last carnage epoch: ${epochState.lastCarnageEpoch}`);
  console.log(`[crank] VRF pending: ${epochState.vrfPending}`);
  console.log();

  // ---- Sweep Stale Randomness Accounts ----
  await sweepStaleRandomnessAccounts(provider, persistentRngKp.publicKey);
  console.log();

  // ---- Main Loop ----
  let cycleCount = 0;
  let carnageTriggerCount = 0;

  // ---- Start Health Server ----
  healthServer = startHealthServer();

  console.log("[crank] Starting epoch advancement loop...");
  console.log();

  while (!shutdownRequested) {
    cycleCount++;
    const cycleStartMs = Date.now();

    try {
      // Periodic sweep: catch randomness accounts leaked by failed inline closes
      if (cycleCount > 1 && cycleCount % PERIODIC_SWEEP_INTERVAL === 0) {
        console.log(`[crank] Periodic sweep (every ${PERIODIC_SWEEP_INTERVAL} cycles)...`);
        await sweepStaleRandomnessAccounts(provider, persistentRngKp.publicKey);
      }

      // 1. Read current state to determine wait time
      epochState = await readEpochState(
        programs.epochProgram,
        vrfAccounts.epochStatePda
      );
      await sleep(RPC_DELAY_MS);

      // 2. Check wallet balance (log warning if low)
      const balance = await provider.connection.getBalance(
        provider.wallet.publicKey
      );
      if (balance < LOW_BALANCE_SOL * LAMPORTS_PER_SOL) {
        console.log(
          `[crank] WARNING: Wallet balance low: ${(balance / LAMPORTS_PER_SOL).toFixed(3)} SOL (threshold: ${LOW_BALANCE_SOL} SOL)`
        );
      }
      await sleep(RPC_DELAY_MS);

      // 2b. Check carnage vault balance — top up if in rent-bug danger zone
      const vaultPubkey = vrfAccounts.carnageAccounts!.carnageSolVault;
      const vaultBalance = await provider.connection.getBalance(vaultPubkey);
      await sleep(RPC_DELAY_MS);

      if (vaultBalance < MIN_VAULT_BALANCE) {
        // H013 — Cap top-up amount to prevent wallet drain
        const requestedTopUp = VAULT_TOP_UP_LAMPORTS;
        const cappedTopUp = Math.min(requestedTopUp, MAX_TOPUP_LAMPORTS);
        if (requestedTopUp > MAX_TOPUP_LAMPORTS) {
          console.log(
            `[crank] WARNING: Top-up amount ${requestedTopUp} exceeds ceiling ${MAX_TOPUP_LAMPORTS}. Capping.`
          );
        }

        // Check spending cap before vault top-up
        if (!recordSpend(cappedTopUp)) {
          break; // Spending cap reached — halt
        }

        console.log(
          `[crank] Vault balance low: ${vaultBalance} lamports ` +
          `(need ${MIN_VAULT_BALANCE}). Topping up ${cappedTopUp} lamports...`
        );
        const topUpTx = new Transaction().add(
          SystemProgram.transfer({
            fromPubkey: provider.wallet.publicKey,
            toPubkey: vaultPubkey,
            lamports: cappedTopUp,
          })
        );
        const sig = await provider.sendAndConfirm(topUpTx, []);
        console.log(
          `[crank] Vault topped up (+${(cappedTopUp / LAMPORTS_PER_SOL).toFixed(4)} SOL). TX: ${sig}`
        );
      }

      // 3. Wait for next epoch boundary (skip if VRF recovery needed)
      // When vrfPending=true, the current epoch's VRF was committed but never
      // consumed (e.g. TX3 expired). advanceEpochWithVRF handles recovery
      // internally — no need to wait for the NEXT epoch boundary first.
      // Without this check, a TX3 failure causes a ~30-minute unnecessary wait
      // because trigger_epoch_transition already updated epochStartSlot.
      if (epochState.vrfPending) {
        console.log(
          `[crank] VRF pending from previous cycle — skipping epoch boundary wait, entering recovery...`
        );
      } else {
        // On-chain SLOTS_PER_EPOCH is 750 (devnet) or 4500 (mainnet).
        // We read the current slot and epoch_start_slot to calculate remaining wait.
        const currentSlot = await provider.connection.getSlot();
        await sleep(RPC_DELAY_MS);

        // Calculate slots since epoch started
        const slotsSinceEpochStart = currentSlot - Number(epochState.epochStartSlot);

        // Determine how many more slots to wait using configurable MIN_EPOCH_SLOTS.
        // The VRF flow already handles EpochBoundaryNotReached retries as a safety net.
        const slotsToWait = Math.max(
          0,
          MIN_EPOCH_SLOTS - slotsSinceEpochStart + SLOT_WAIT_BUFFER
        );

        if (slotsToWait > 0) {
          const waitMinutes = ((slotsToWait * 0.4) / 60).toFixed(1);
          console.log(
            `[crank] Waiting ${slotsToWait} slots (~${waitMinutes} min) for epoch boundary...`
          );
          await waitForSlotAdvance(provider.connection, slotsToWait);
        }
      }

      // 4. Advance epoch with VRF (atomic carnage bundling enabled)
      const epochBeforeAdvance = epochState.currentEpoch;
      console.log("[crank] Advancing epoch...");
      const result = await advanceEpochWithVRF(
        provider,
        programs.epochProgram,
        vrfAccounts
      );

      const durationSec = ((Date.now() - cycleStartMs) / 1000).toFixed(1);

      if (result.carnageTriggered) {
        carnageTriggerCount++;
      }

      // Detect epoch skips (VRF-02 crank-side warning)
      const epochDelta = result.epoch - epochBeforeAdvance;
      if (epochDelta > 1) {
        console.log(
          `[crank] WARNING: Skipped ${epochDelta - 1} epoch(s) (${epochBeforeAdvance} -> ${result.epoch})`
        );
      }

      // 5. Record spend for VRF transactions (commit + reveal + consume, possibly carnage)
      //    Conservative: 3 TXs for normal cycle, 4 if carnage triggered
      const txCount = result.carnageTriggered ? 4 : 3;
      if (!recordSpend(ESTIMATED_TX_COST_LAMPORTS * txCount)) {
        break; // Spending cap reached — halt
      }

      // 6. Handle randomness account lifecycle (persistent reuse)
      if (result.newRandomnessKp) {
        // Oracle recovery created a fresh account — adopt it as the new persistent one.
        // Close the OLD persistent account (stuck on dead oracle).
        const oldPubkey = persistentRngKp.publicKey;
        console.log(`  [rng] Oracle recovery: switching persistent keypair ${oldPubkey.toBase58().slice(0, 12)}... -> ${result.newRandomnessKp.publicKey.toBase58().slice(0, 12)}...`);

        const oldCloseSig = await closeRandomnessAccount(provider, oldPubkey);
        if (oldCloseSig) {
          console.log(`  [rng] Closed old persistent account. TX: ${oldCloseSig.slice(0, 16)}...`);
        }

        // Update persistent keypair for future cycles
        persistentRngKp = result.newRandomnessKp;
        vrfAccounts.persistentRngKp = persistentRngKp;
        savePersistentRng(persistentRngKp);
      } else if (result.randomnessPubkey) {
        // Normal cycle — DON'T close if it's the persistent account (we want to reuse it)
        const isPersistent = result.randomnessPubkey.toBase58() === persistentRngKp.publicKey.toBase58();
        if (!isPersistent) {
          // This was a non-persistent account (e.g. from a code path without persistence)
          const closeSig = await closeRandomnessAccount(provider, result.randomnessPubkey);
          if (closeSig) {
            console.log(
              `  [close] Reclaimed rent from ${result.randomnessPubkey.toBase58().slice(0, 12)}... TX: ${closeSig.slice(0, 16)}...`
            );
          }
        }
      }

      // 7. Success — reset circuit breaker, prune spending log
      consecutiveErrors = 0;
      lastSuccessTimestamp = Date.now();
      pruneSpendingLog();

      // 8. Log result as JSON line to stdout
      const logEntry = {
        ts: new Date().toISOString(),
        cycle: cycleCount,
        epoch: result.epoch,
        cheapSide: result.cheapSide,
        flipped: result.flipped,
        lowTax: result.lowTaxBps,
        highTax: result.highTaxBps,
        carnage: result.carnageTriggered,
        carnageAtomic: result.carnageExecutedAtomically,
        vrfBytes: result.vrfBytes.slice(0, 8),
        randomness: result.randomnessPubkey?.toBase58() ?? "unknown",
        durationSec: parseFloat(durationSec),
        totalCarnage: carnageTriggerCount,
        hourlySpend: getCurrentHourlySpend(),
        // VRF instrumentation (CRANK-03)
        gateway_ms: result.gatewayMs ?? 0,
        reveal_attempts: result.revealAttempts ?? 0,
        recovery_time_ms: result.recoveryTimeMs ?? 0,
        commit_to_reveal_slots: result.commitToRevealSlots ?? 0,
      };
      console.log(`[epoch] ${JSON.stringify(logEntry)}`);

    } catch (err) {
      consecutiveErrors++;
      const errStr = String(err).slice(0, 300);
      console.error(
        `[crank] ERROR cycle ${cycleCount} (${consecutiveErrors}/${CIRCUIT_BREAKER_THRESHOLD} consecutive): ${errStr}`
      );

      // Circuit breaker — halt after too many consecutive failures
      if (consecutiveErrors >= CIRCUIT_BREAKER_THRESHOLD) {
        console.error(
          `[crank] CRITICAL: Circuit breaker tripped after ${CIRCUIT_BREAKER_THRESHOLD} consecutive errors. Halting.`
        );

        // Send Telegram alert (best-effort, non-blocking relative to halt)
        const alertBalance = await provider.connection.getBalance(
          provider.wallet.publicKey
        ).catch(() => 0);

        await sendAlert({
          event: "CIRCUIT BREAKER TRIPPED",
          lastError: errStr,
          epoch: epochState?.currentEpoch ?? 0,
          walletBalanceSol: alertBalance / LAMPORTS_PER_SOL,
          consecutiveErrors,
          uptimeSeconds: (Date.now() - crankStartMs) / 1000,
        });

        break;
      }

      // Prune spending log on each cycle (success or failure)
      pruneSpendingLog();

      // Don't exit — sleep and retry next cycle with exponential backoff
      if (!shutdownRequested) {
        const errorDelay = Math.min(
          ERROR_BASE_DELAY_MS * Math.pow(2, consecutiveErrors - 1),
          ERROR_MAX_DELAY_MS
        );
        console.log(
          `[crank] Retrying in ${(errorDelay / 1000).toFixed(0)}s (attempt ${consecutiveErrors}/${CIRCUIT_BREAKER_THRESHOLD})...`
        );
        await sleep(errorDelay);
      }
    }
  }

  // ---- Shutdown ----
  console.log();
  console.log("=".repeat(60));
  console.log(`[crank] Shutdown complete.`);
  console.log(`  Cycles completed: ${cycleCount}`);
  console.log(`  Carnage triggers: ${carnageTriggerCount}`);
  console.log(`  Ended: ${new Date().toISOString()}`);
  console.log("=".repeat(60));
}

main().catch((err) => {
  console.error(`[crank] FATAL: ${String(err)}`);
  process.exit(1);
});
