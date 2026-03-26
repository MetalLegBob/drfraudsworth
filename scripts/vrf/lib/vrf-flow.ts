/**
 * VRF 3-TX Flow Helper
 *
 * Implements the complete Switchboard On-Demand VRF commit-reveal cycle for
 * epoch transitions. Encapsulates the three-transaction flow:
 *   TX 1: Create randomness account (wait for finalization)
 *   TX 2: Commit + trigger_epoch_transition
 *   TX 3: Reveal + consume_randomness + executeCarnageAtomic (bundled v0)
 *
 * When carnageAccounts are provided, TX3 bundles executeCarnageAtomic as a
 * third instruction. The on-chain no-op guard returns Ok(()) when Carnage
 * doesn't trigger, making it always safe. This eliminates the CARN-002 MEV
 * window entirely -- no CarnagePending event is visible before the swap.
 *
 * Why three transactions?
 * The Switchboard SDK's commitIx() reads the randomness account client-side
 * before constructing the commit instruction. The account MUST exist and be
 * finalized before commitIx() can be called. Combining TX 1 + TX 2 always fails.
 *
 * Anti-patterns avoided:
 * - NEVER combine create + commit (SDK reads account client-side)
 * - NEVER use "confirmed" for TX 1 -- always "finalized"
 * - NEVER hardcode Switchboard addresses -- use getProgramId() + getDefaultQueue()
 * - Add 200ms delays between RPC calls for Helius rate limiting
 *
 * Source: tests/devnet-vrf.ts (existing), VRF_Implementation_Reference.md Section 4
 */

import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import {
  AddressLookupTableAccount,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  ComputeBudgetProgram,
  SystemProgram,
  Connection,
} from "@solana/web3.js";
import * as sb from "@switchboard-xyz/on-demand";

// ─── Interfaces ────────────────────────────────────────────────────────────

/**
 * Result from a single VRF-driven epoch transition.
 * Contains all data needed for validation reporting.
 */
export interface EpochTransitionResult {
  /** New epoch number after transition */
  epoch: number;
  /** Which token is "cheap" this epoch: "CRIME" or "FRAUD" */
  cheapSide: string;
  /** Low tax band (100-400 bps) */
  lowTaxBps: number;
  /** High tax band (1100-1400 bps) */
  highTaxBps: number;
  /** CRIME buy tax in basis points */
  crimeBuyTaxBps: number;
  /** CRIME sell tax in basis points */
  crimeSellTaxBps: number;
  /** FRAUD buy tax in basis points */
  fraudBuyTaxBps: number;
  /** FRAUD sell tax in basis points */
  fraudSellTaxBps: number;
  /** Whether cheap_side changed from the previous epoch */
  flipped: boolean;
  /** First 6 VRF bytes (for logging/debugging) */
  vrfBytes: number[];
  /** TX 1 signature (create randomness account) */
  createSig: string;
  /** TX 2 signature (commit + trigger) */
  commitSig: string;
  /** TX 3 signature (reveal + consume) */
  consumeSig: string;
  /** Total duration of this transition in milliseconds */
  durationMs: number;
  /** Whether Carnage was triggered (VRF byte 5 < 11) */
  carnageTriggered: boolean;
  /** Whether Carnage was executed atomically within the lock window (TX4) */
  carnageExecutedAtomically: boolean;
  /** Pubkey of the randomness account used this cycle (null if consumed by another crank) */
  randomnessPubkey: PublicKey | null;

  /** VRF instrumentation (CRANK-03) */
  /** Wall-clock time for successful revealIx() call in ms */
  gatewayMs?: number;
  /** Total reveal retry count (cumulative across stale + fresh) */
  revealAttempts?: number;
  /** Total recovery path wall-clock time in ms (0 = happy path) */
  recoveryTimeMs?: number;
  /** Slot delta between commit TX confirmation and successful reveal */
  commitToRevealSlots?: number;
}

/**
 * Account addresses required for the VRF epoch transition flow.
 * All addresses come from scripts/deploy/pda-manifest.json.
 */
export interface VRFAccounts {
  /** EpochState PDA: seeds=["epoch_state"] on epoch_program */
  epochStatePda: PublicKey;
  /** Treasury PDA (bounty transfer target -- wallet for now) */
  treasuryPda: PublicKey;
  /** StakingAuthority PDA: seeds=["staking_authority"] on epoch_program */
  stakingAuthorityPda: PublicKey;
  /** StakePool PDA from Staking Program */
  stakePoolPda: PublicKey;
  /** Staking Program ID */
  stakingProgramId: PublicKey;
  /** CarnageFund PDA: seeds=["carnage_fund"] on epoch_program */
  carnageFundPda: PublicKey;

  /**
   * Optional: Carnage execution accounts for atomic bundling (CARN-002 fix).
   * When provided, after consume_randomness detects a Carnage trigger,
   * vrf-flow immediately sends executeCarnageAtomic as TX4 within the
   * 50-slot lock window, closing the MEV gap.
   */
  carnageAccounts?: {
    carnageSignerPda: PublicKey;
    carnageSolVault: PublicKey;
    carnageWsol: PublicKey;
    carnageCrimeVault: PublicKey;
    carnageFraudVault: PublicKey;
    crimePool: PublicKey;
    crimePoolVaultA: PublicKey;
    crimePoolVaultB: PublicKey;
    fraudPool: PublicKey;
    fraudPoolVaultA: PublicKey;
    fraudPoolVaultB: PublicKey;
    mintA: PublicKey;
    crimeMint: PublicKey;
    fraudMint: PublicKey;
    taxProgram: PublicKey;
    ammProgram: PublicKey;
    swapAuthority: PublicKey;
  };

  /**
   * Optional: AddressLookupTableAccount for v0 VersionedTransaction compression.
   * Required for Sell path Carnage (23 named + 8 remaining accounts > 1232 bytes).
   */
  alt?: AddressLookupTableAccount;
}

// ─── Utilities ─────────────────────────────────────────────────────────────

/**
 * Sleep for a specified duration.
 * Used for rate limiting between RPC calls and oracle wait loops.
 */
export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Wait for slot advancement on the cluster.
 *
 * For short waits (<=30 slots, e.g. oracle processing after commit):
 *   Polls getSlot() every 500ms. ~6 calls for a 3-slot wait.
 *
 * For long waits (>30 slots, e.g. epoch boundary ~750 slots):
 *   Sleeps for the estimated wall-clock time minus a 15-second buffer,
 *   then polls every 10 seconds. This reduces RPC calls from ~600 per
 *   epoch wait to ~5-10 (95%+ reduction in Helius credits).
 *
 * Wall-clock timeout (H030): If slot production halts (network outage)
 * or RPC returns stale slots, the function throws after maxWaitMs.
 * Default: 3x expected time at 400ms/slot, minimum 30 seconds.
 *
 * @param connection Solana connection
 * @param targetSlots Number of slots to wait for
 * @param maxWaitMs Optional wall-clock timeout in milliseconds
 */
export async function waitForSlotAdvance(
  connection: Connection,
  targetSlots: number,
  maxWaitMs?: number
): Promise<void> {
  // Wall-clock timeout: 3x expected time (generous), floor at 30s for RPC latency
  const effectiveTimeout = maxWaitMs ?? Math.max(30_000, targetSlots * 400 * 3);
  const startTime = Date.now();

  const startSlot = await connection.getSlot();
  const targetSlot = startSlot + targetSlots;
  console.log(`  [slot] Current: ${startSlot}, waiting for +${targetSlots} (target: ${targetSlot}, timeout: ${Math.round(effectiveTimeout / 1000)}s)...`);

  // For long waits, sleep most of the estimated time first to save RPC credits.
  // Slot time is ~400ms, so 750 slots ≈ 300 seconds.
  // Cap the sleep so we wake up at least 5s before the timeout.
  if (targetSlots > 30) {
    const estimatedMs = targetSlots * 400;
    const sleepMs = Math.min(estimatedMs - 15_000, effectiveTimeout - 5_000); // Wake up before timeout
    if (sleepMs > 0) {
      console.log(`  [slot] Sleeping ${(sleepMs / 1000).toFixed(0)}s before polling...`);
      await sleep(sleepMs);
    }
  }

  // Poll interval: 500ms for short waits (oracle processing needs speed),
  // 10s for long waits (epoch boundary doesn't need sub-second precision).
  const pollMs = targetSlots > 30 ? 10_000 : 500;

  while (true) {
    // Wall-clock timeout check BEFORE sleeping -- don't sleep unnecessarily
    if (Date.now() - startTime > effectiveTimeout) {
      const currentSlot = await connection.getSlot().catch(() => -1);
      throw new Error(
        `waitForSlotAdvance timed out after ${Math.round((Date.now() - startTime) / 1000)}s. ` +
        `Target: ${targetSlot}, current: ${currentSlot}, started at: ${startSlot}`
      );
    }

    await sleep(pollMs);
    try {
      const currentSlot = await connection.getSlot();
      if (currentSlot >= targetSlot) {
        console.log(`  [slot] Advanced to ${currentSlot}`);
        return;
      }
    } catch (e) {
      // Transient RPC error -- retry after brief delay
      await sleep(2000);
    }
  }
}

// ─── Enum Helpers ───────────────────────────────────────────────────────────

/**
 * Convert Anchor-serialized CheapSide enum to string.
 * Handles both numeric (0/1) and object ({ crime: {} }) representations.
 */
function cheapSideToStr(val: any): string {
  if (typeof val === "number") return val === 0 ? "CRIME" : "FRAUD";
  if (val && val.crime !== undefined) return "CRIME";
  if (val && val.fraud !== undefined) return "FRAUD";
  return String(val) === "0" ? "CRIME" : "FRAUD";
}

// ─── Reveal + Consume Helpers ───────────────────────────────────────────────

/**
 * Result from a tryReveal() call, including timing instrumentation.
 */
interface RevealResult {
  revealIx: any;
  attempts: number;
  durationMs: number;
}

/**
 * Try to get a reveal instruction from the oracle's default gateway.
 *
 * Why no gateway rotation? Each randomness account is assigned to a specific
 * oracle during commit. The reveal instruction verifies the oracle's signature
 * on-chain. Alternative gateways serve different oracles, so their signatures
 * fail verification (error 0x1780). Only the assigned oracle's gateway can
 * produce a valid reveal.
 *
 * If the default gateway is down, we return null and let the caller fall
 * through to VRF timeout recovery (which creates fresh randomness that may
 * get assigned to a different, working oracle).
 *
 * @param randomness Switchboard Randomness instance
 * @param maxAttempts Max retry attempts on the oracle's default gateway
 * @returns RevealResult with instruction and timing data, or null if gateway is unresponsive
 */
async function tryReveal(
  randomness: any,
  maxAttempts: number
): Promise<RevealResult | null> {
  console.log("  [tx3] Getting reveal instruction...");
  const startMs = Date.now();
  let attempts = 0;

  const REVEAL_BASE_DELAY_MS = 1000;
  const REVEAL_MAX_DELAY_MS = 16_000;

  for (let i = 0; i < maxAttempts; i++) {
    attempts++;
    try {
      const revealIx = await randomness.revealIx();
      console.log(`  [tx3] Got reveal instruction (attempt ${attempts}/${maxAttempts})`);
      return { revealIx, attempts, durationMs: Date.now() - startMs };
    } catch (e) {
      const errMsg = String(e).slice(0, 100);
      console.log(
        `  [tx3] Reveal failed (attempt ${attempts}/${maxAttempts}): ${errMsg}`
      );
      if (i < maxAttempts - 1) {
        // Exponential backoff: 1s, 2s, 4s, 8s, 16s (capped) — oracle may need time to process
        const delay = Math.min(REVEAL_BASE_DELAY_MS * Math.pow(2, i), REVEAL_MAX_DELAY_MS);
        await sleep(delay);
      }
    }
  }

  console.log(`  [tx3] Oracle gateway unresponsive after ${maxAttempts} attempts`);
  return null;
}

/**
 * Send the reveal + consume transaction.
 * Extracted to avoid duplication between happy path and recovery path.
 *
 * When carnageAccounts AND alt are provided, bundles reveal + consume +
 * executeCarnageAtomic in ONE VersionedTransaction v0. The on-chain no-op
 * guard ensures executeCarnageAtomic returns Ok(()) when Carnage doesn't
 * trigger, so this is always safe. When Carnage DOES trigger, the swap
 * executes atomically in the same transaction -- zero MEV window.
 *
 * When carnageAccounts are NOT provided, builds a legacy Transaction with
 * just reveal + consume (backward compatible).
 */
async function sendRevealAndConsume(
  provider: AnchorProvider,
  epochProgram: Program,
  accounts: VRFAccounts,
  revealIx: any,
  randomnessPubkey: PublicKey,
  wallet: Wallet
): Promise<string> {
  const connection = provider.connection;

  const consumeIx = await epochProgram.methods
    .consumeRandomness()
    .accounts({
      caller: wallet.publicKey,
      epochState: accounts.epochStatePda,
      randomnessAccount: randomnessPubkey,
      stakingAuthority: accounts.stakingAuthorityPda,
      stakePool: accounts.stakePoolPda,
      stakingProgram: accounts.stakingProgramId,
      carnageState: accounts.carnageFundPda,
    })
    .instruction();

  // When carnageAccounts and ALT are available, bundle all three instructions
  // in a single v0 VersionedTransaction. The no-op guard on executeCarnageAtomic
  // makes this safe: if Carnage doesn't trigger, it returns Ok(()) immediately.
  // If Carnage triggers, the swap executes atomically -- no CarnagePending event
  // is visible on-chain before the swap completes. CARN-002 MEV gap: closed.
  if (accounts.carnageAccounts && accounts.alt) {
    // Dynamic import to avoid circular dependency: carnage-flow.ts imports from vrf-flow.ts
    const { buildExecuteCarnageAtomicIx } = await import(
      "../../e2e/lib/carnage-flow"
    );

    const carnageIx: TransactionInstruction = await buildExecuteCarnageAtomicIx(
      epochProgram,
      accounts,
      wallet.publicKey,
      connection
    );

    // Bundle all three: reveal + consume + executeCarnageAtomic in ONE v0 TX
    // 600,000 CU covers reveal (~50k) + consume (~100k) + executeCarnageAtomic (~300k swap)
    const { sendV0Transaction } = await import("../../e2e/lib/alt-helper");
    const instructions = [
      ComputeBudgetProgram.setComputeUnitLimit({ units: 600_000 }),
      revealIx,
      consumeIx,
      carnageIx,
    ];

    const sig = await sendV0Transaction(
      connection,
      wallet.publicKey,
      instructions,
      [wallet.payer],
      accounts.alt
    );
    console.log(`  [tx3] Reveal+Consume+CarnageAtomic (bundled v0): ${sig.slice(0, 16)}...`);
    return sig;
  }

  // Legacy path: reveal + consume only (no carnageAccounts provided)
  const consumeTx = new Transaction().add(
    ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
    revealIx,
    consumeIx
  );

  const consumeSig = await provider.sendAndConfirm(consumeTx, [wallet.payer]);
  console.log(`  [tx3] Reveal+Consume: ${consumeSig.slice(0, 16)}...`);
  return consumeSig;
}

// ─── Main VRF Flow ─────────────────────────────────────────────────────────

/**
 * Execute the complete 3-TX VRF epoch transition flow.
 *
 * This is the core function: it creates a randomness account, commits it
 * alongside trigger_epoch_transition, waits for the oracle, then reveals
 * and consumes the randomness to update tax rates.
 *
 * @param provider Anchor provider with wallet and connection
 * @param epochProgram Epoch Program instance (typed from IDL)
 * @param accounts PDA addresses for all required accounts
 * @returns EpochTransitionResult with full details
 */
export async function advanceEpochWithVRF(
  provider: AnchorProvider,
  epochProgram: Program,
  accounts: VRFAccounts
): Promise<EpochTransitionResult> {
  const startMs = Date.now();
  const connection = provider.connection;
  const wallet = provider.wallet as Wallet;

  // Read state before transition to detect flip
  const stateBefore = await (epochProgram.account as any).epochState.fetch(
    accounts.epochStatePda
  );
  const previousCheapSide = stateBefore.cheapSide;

  await sleep(200); // Rate limit

  // ─── Switchboard Setup ────────────────────────────────────────────────
  // Dynamic resolution: no hardcoded addresses
  console.log("  [sb] Setting up Switchboard...");
  const sbProgramId = await sb.getProgramId(connection);
  await sleep(200);

  const sbIdl = await Program.fetchIdl(sbProgramId, provider);
  if (!sbIdl) throw new Error("Failed to fetch Switchboard IDL from chain");
  const sbProgram = new Program(sbIdl, provider);
  await sleep(200);

  const queueAccount = await sb.getDefaultQueue(connection.rpcEndpoint);
  console.log(`  [sb] Queue: ${queueAccount.pubkey.toBase58()}`);

  // ─── Recovery: Clear stale VRF if pending ──────────────────────────────
  // If a previous call failed mid-flow (oracle timeout, network error, etc.),
  // EpochState may still have vrf_pending=true with a committed randomness
  // account that was never consumed. Without recovery, triggerEpochTransition
  // rejects with VrfAlreadyPending (0x1774) and no further transitions are
  // possible until the stale VRF is cleared.
  //
  // Recovery strategy:
  //   1. Try to reveal+consume the stale randomness (oracle may have responded)
  //   2. If reveal fails, wait for VRF_TIMEOUT_SLOTS (300), then retry_epoch_vrf
  //      with fresh randomness to replace the stale request
  //   3. Return the completed transition result (caller wanted a transition)
  if (stateBefore.vrfPending) {
    console.log("  [recovery] Stale VRF detected (vrf_pending=true). Recovering...");

    const stalePubkey: PublicKey = stateBefore.pendingRandomnessAccount;
    console.log(`  [recovery] Pending randomness: ${stalePubkey.toBase58()}`);

    // Attempt 1: Try to reveal the stale randomness (oracle may have responded late)
    const recoveryStartMs = Date.now();
    const staleRandomness = new sb.Randomness(sbProgram as any, stalePubkey);
    const staleRevealResult = await tryReveal(staleRandomness, 5);
    let totalRevealAttempts = staleRevealResult?.attempts ?? 5; // Track cumulative attempts

    let recoveryConsumeSig: string;
    let recoveryRandomnessPubkey: PublicKey = stalePubkey; // Updated if timeout retry uses fresh account

    let staleRevealSucceeded = false;
    if (staleRevealResult) {
      // Oracle responded — try to complete the stale transition directly.
      // If the on-chain reveal fails (e.g., 0x1780 stale/expired randomness),
      // fall through to the timeout retry path instead of throwing.
      console.log("  [recovery] Oracle revealed stale VRF. Completing transition...");
      try {
        recoveryConsumeSig = await sendRevealAndConsume(
          provider, epochProgram, accounts, staleRevealResult.revealIx, stalePubkey, wallet
        );
        staleRevealSucceeded = true;
      } catch (revealErr) {
        const errMsg = String(revealErr).slice(0, 300);

        // VRF-04: Check if the error indicates the randomness was already consumed
        // (TOCTOU race -- another crank instance consumed it between our read and TX)
        // Detectable signals:
        // - "already" (runtime: "already been processed")
        // - "VrfNotPending" (epoch program: vrf_pending was already cleared)
        // - 0x07DC / 2012 (Anchor ConstraintRaw: pending_randomness_account mismatch)
        const alreadyConsumed = errMsg.includes("already") ||
          errMsg.includes("VrfNotPending") ||
          (errMsg.includes("0x") && errMsg.includes("07DC"));

        if (alreadyConsumed) {
          console.log(`  [recovery] VRF already consumed by another process (TOCTOU). Re-reading state...`);
          // State already advanced -- skip to final state read
          await sleep(200);
          const stateAfter = await (epochProgram.account as any).epochState.fetch(
            accounts.epochStatePda
          );

          const newCheapSide = cheapSideToStr(stateAfter.cheapSide);
          const prevCheapSide = cheapSideToStr(previousCheapSide);

          console.log(`  [recovery] State already advanced. Epoch: ${stateAfter.currentEpoch}`);

          return {
            epoch: stateAfter.currentEpoch,
            cheapSide: newCheapSide,
            lowTaxBps: stateAfter.lowTaxBps,
            highTaxBps: stateAfter.highTaxBps,
            crimeBuyTaxBps: stateAfter.crimeBuyTaxBps,
            crimeSellTaxBps: stateAfter.crimeSellTaxBps,
            fraudBuyTaxBps: stateAfter.fraudBuyTaxBps,
            fraudSellTaxBps: stateAfter.fraudSellTaxBps,
            flipped: newCheapSide !== prevCheapSide,
            vrfBytes: [0, 0, 0, 0, 0, 0], // Unknown -- consumed by other process
            createSig: "already-consumed",
            commitSig: "already-consumed",
            consumeSig: "already-consumed",
            durationMs: Date.now() - startMs,
            carnageTriggered: stateAfter.lastCarnageEpoch === stateAfter.currentEpoch,
            carnageExecutedAtomically: false,
            randomnessPubkey: null, // Another crank consumed — we don't own the close
            // VRF instrumentation (CRANK-03): TOCTOU — no meaningful measurements
            gatewayMs: 0,
            revealAttempts: 0,
            recoveryTimeMs: Date.now() - recoveryStartMs,
            commitToRevealSlots: 0,
          };
        }

        console.log(`  [recovery] Stale reveal failed on-chain: ${errMsg}`);
        console.log("  [recovery] Falling through to VRF timeout retry...");
      }
    }

    if (!staleRevealSucceeded) {
      // Oracle didn't respond — wait for VRF timeout and use retry_epoch_vrf
      const vrfRequestSlot = Number(stateBefore.vrfRequestSlot);
      const currentSlot = await connection.getSlot();
      const VRF_TIMEOUT_SLOTS = 300; // Must match on-chain constant
      const slotsToWait = Math.max(0, vrfRequestSlot + VRF_TIMEOUT_SLOTS - currentSlot + 5);

      if (slotsToWait > 0) {
        console.log(`  [recovery] Waiting for VRF timeout (${slotsToWait} slots)...`);
        await waitForSlotAdvance(connection, slotsToWait);
      } else {
        console.log("  [recovery] VRF timeout already elapsed.");
      }

      // Create fresh randomness for the retry
      console.log("  [recovery] Creating fresh randomness for retry...");
      const retryRngKp = Keypair.generate();
      const [retryRandomness, retryCreateIx] = await sb.Randomness.create(
        sbProgram as any, retryRngKp, queueAccount.pubkey
      );

      const retryCreateTx = new Transaction().add(retryCreateIx);
      retryCreateTx.feePayer = wallet.publicKey;
      retryCreateTx.recentBlockhash = (
        await connection.getLatestBlockhash()
      ).blockhash;
      retryCreateTx.sign(wallet.payer, retryRngKp);

      const retryCreateSig = await connection.sendRawTransaction(
        retryCreateTx.serialize(), { skipPreflight: true, maxRetries: 3 }
      );
      console.log("  [recovery] Create TX sent, waiting for finalization...");
      await connection.confirmTransaction(retryCreateSig, "finalized");
      await sleep(200);

      // Commit + retry_epoch_vrf to replace the stale VRF request
      console.log("  [recovery] Building retry commit + retry_epoch_vrf...");
      const retryCommitIx = await retryRandomness.commitIx(queueAccount.pubkey);
      const retryVrfIx = await epochProgram.methods
        .retryEpochVrf()
        .accounts({
          payer: wallet.publicKey,
          epochState: accounts.epochStatePda,
          randomnessAccount: retryRngKp.publicKey,
        })
        .instruction();

      const retryCommitTx = new Transaction().add(
        ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
        retryCommitIx,
        retryVrfIx
      );
      await provider.sendAndConfirm(retryCommitTx, [wallet.payer]);
      console.log("  [recovery] Retry commit sent");

      // Wait for oracle on the fresh randomness
      await waitForSlotAdvance(connection, 3);

      const retryRevealResult = await tryReveal(retryRandomness, 10);
      if (!retryRevealResult) {
        throw new Error("VRF recovery failed: oracle not responding after retry_epoch_vrf");
      }
      totalRevealAttempts += retryRevealResult.attempts;

      recoveryRandomnessPubkey = retryRngKp.publicKey;
      try {
        recoveryConsumeSig = await sendRevealAndConsume(
          provider, epochProgram, accounts, retryRevealResult.revealIx,
          retryRngKp.publicKey, wallet
        );
      } catch (retryConsumeErr) {
        const retryErrMsg = String(retryConsumeErr).slice(0, 300);

        // VRF-04: Same TOCTOU check for the timeout retry path.
        // Another crank may have consumed the fresh randomness between our
        // retry_epoch_vrf and our reveal+consume TX.
        const alreadyConsumed = retryErrMsg.includes("already") ||
          retryErrMsg.includes("VrfNotPending") ||
          (retryErrMsg.includes("0x") && retryErrMsg.includes("07DC"));

        if (alreadyConsumed) {
          console.log(`  [recovery] VRF already consumed by another process during timeout retry (TOCTOU). Re-reading state...`);

          // CRANK-01: Close both leaked accounts before returning.
          // stalePubkey (original stale) and retryRngKp (fresh retry) both leak
          // since another crank consumed the VRF and won't close our accounts.
          try {
            console.log(`  [recovery] Closing stale randomness ${stalePubkey.toBase58().slice(0, 12)}...`);
            const staleCloseSig = await closeRandomnessAccount(provider, stalePubkey);
            if (staleCloseSig) {
              console.log(`  [recovery] Stale account closed. TX: ${staleCloseSig.slice(0, 16)}...`);
            } else {
              console.log(`  [recovery] WARNING: Stale account close failed (startup sweep will catch it)`);
            }
          } catch {
            console.log(`  [recovery] WARNING: Stale account close failed (startup sweep will catch it)`);
          }
          try {
            console.log(`  [recovery] Closing retry randomness ${retryRngKp.publicKey.toBase58().slice(0, 12)}...`);
            const retryCloseSig = await closeRandomnessAccount(provider, retryRngKp.publicKey);
            if (retryCloseSig) {
              console.log(`  [recovery] Retry account closed. TX: ${retryCloseSig.slice(0, 16)}...`);
            } else {
              console.log(`  [recovery] WARNING: Retry account close failed (startup sweep will catch it)`);
            }
          } catch {
            console.log(`  [recovery] WARNING: Retry account close failed (startup sweep will catch it)`);
          }

          await sleep(200);
          const stateAfter = await (epochProgram.account as any).epochState.fetch(
            accounts.epochStatePda
          );

          const newCheapSide = cheapSideToStr(stateAfter.cheapSide);
          const prevCheapSide = cheapSideToStr(previousCheapSide);

          console.log(`  [recovery] State already advanced. Epoch: ${stateAfter.currentEpoch}`);

          return {
            epoch: stateAfter.currentEpoch,
            cheapSide: newCheapSide,
            lowTaxBps: stateAfter.lowTaxBps,
            highTaxBps: stateAfter.highTaxBps,
            crimeBuyTaxBps: stateAfter.crimeBuyTaxBps,
            crimeSellTaxBps: stateAfter.crimeSellTaxBps,
            fraudBuyTaxBps: stateAfter.fraudBuyTaxBps,
            fraudSellTaxBps: stateAfter.fraudSellTaxBps,
            flipped: newCheapSide !== prevCheapSide,
            vrfBytes: [0, 0, 0, 0, 0, 0],
            createSig: "already-consumed",
            commitSig: "already-consumed",
            consumeSig: "already-consumed",
            durationMs: Date.now() - startMs,
            carnageTriggered: stateAfter.lastCarnageEpoch === stateAfter.currentEpoch,
            carnageExecutedAtomically: false,
            randomnessPubkey: null, // Another crank consumed — we don't own the close
            // VRF instrumentation (CRANK-03): TOCTOU — no meaningful gateway measurement
            gatewayMs: 0,
            revealAttempts: totalRevealAttempts,
            recoveryTimeMs: Date.now() - recoveryStartMs,
            commitToRevealSlots: 0,
          };
        }

        // Not a TOCTOU error -- rethrow
        throw retryConsumeErr;
      }
    }

    await sleep(200);

    // CRANK-01: Close the stale randomness account if a fresh one was used.
    // recoveryRandomnessPubkey is updated to retryRngKp.publicKey when timeout retry
    // creates fresh randomness. The stale account (original pending) leaks otherwise.
    if (recoveryRandomnessPubkey.toBase58() !== stalePubkey.toBase58()) {
      try {
        console.log(`  [recovery] Closing stale randomness ${stalePubkey.toBase58().slice(0, 12)}...`);
        const staleCloseSig = await closeRandomnessAccount(provider, stalePubkey);
        if (staleCloseSig) {
          console.log(`  [recovery] Stale account closed. TX: ${staleCloseSig.slice(0, 16)}...`);
        } else {
          console.log(`  [recovery] WARNING: Stale account close failed (startup sweep will catch it)`);
        }
      } catch {
        console.log(`  [recovery] WARNING: Stale account close failed (startup sweep will catch it)`);
      }
    }

    // Read final state and return the completed transition
    const stateAfter = await (epochProgram.account as any).epochState.fetch(
      accounts.epochStatePda
    );
    const newCheapSide = cheapSideToStr(stateAfter.cheapSide);
    const prevCheapSide = cheapSideToStr(previousCheapSide);
    const lowByte = (stateAfter.lowTaxBps - 100) / 100;
    const highByte = (stateAfter.highTaxBps - 1100) / 100;

    console.log(`  [recovery] Stale VRF recovered. Epoch: ${stateAfter.currentEpoch}`);

    // Determine gateway timing from the last successful reveal
    const recoveryGatewayMs = staleRevealSucceeded && staleRevealResult
      ? staleRevealResult.durationMs
      : 0; // Timeout retry path — stale reveal didn't produce the final instruction

    return {
      epoch: stateAfter.currentEpoch,
      cheapSide: newCheapSide,
      lowTaxBps: stateAfter.lowTaxBps,
      highTaxBps: stateAfter.highTaxBps,
      crimeBuyTaxBps: stateAfter.crimeBuyTaxBps,
      crimeSellTaxBps: stateAfter.crimeSellTaxBps,
      fraudBuyTaxBps: stateAfter.fraudBuyTaxBps,
      fraudSellTaxBps: stateAfter.fraudSellTaxBps,
      flipped: newCheapSide !== prevCheapSide,
      vrfBytes: [
        newCheapSide !== prevCheapSide ? 0 : 192,
        lowByte, highByte,
        stateAfter.carnagePending ? 5 : 50,
        0, 0,
      ],
      createSig: "recovery", // No TX1 in recovery path
      commitSig: "recovery", // Commit was from previous (failed) call or retry
      consumeSig: recoveryConsumeSig,
      durationMs: Date.now() - startMs,
      carnageTriggered: stateAfter.carnagePending === true ||
        stateAfter.lastCarnageEpoch === stateAfter.currentEpoch,
      carnageExecutedAtomically: false, // Recovery path does not attempt atomic Carnage
      randomnessPubkey: recoveryRandomnessPubkey,
      // VRF instrumentation (CRANK-03)
      gatewayMs: recoveryGatewayMs,
      revealAttempts: totalRevealAttempts,
      recoveryTimeMs: Date.now() - recoveryStartMs,
      commitToRevealSlots: 0, // Recovery — commit was in a prior cycle
    };
  }

  // ─── TX 1: Create Randomness Account ──────────────────────────────────
  // MUST be separate from TX 2 -- SDK reads account client-side after create
  console.log("  [tx1] Creating randomness account...");
  const rngKp = Keypair.generate();

  const [randomness, createIx] = await sb.Randomness.create(
    sbProgram as any,
    rngKp,
    queueAccount.pubkey
  );
  console.log(`  [tx1] Randomness: ${randomness.pubkey.toBase58()}`);

  const createTx = new Transaction().add(createIx);
  createTx.feePayer = wallet.publicKey;
  createTx.recentBlockhash = (
    await connection.getLatestBlockhash()
  ).blockhash;
  createTx.sign(wallet.payer, rngKp);

  // skipPreflight: true because the SDK's LUT creation uses a finalized slot
  // that can be slightly stale. The actual on-chain execution will succeed
  // even if preflight simulation rejects it due to slot staleness.
  const createSig = await connection.sendRawTransaction(createTx.serialize(), {
    skipPreflight: true,
    maxRetries: 3,
  });
  console.log(`  [tx1] Sent: ${createSig.slice(0, 16)}...`);

  // CRITICAL: Must wait for FINALIZATION, not just confirmation.
  // commitIx() reads the account client-side and will fail if not finalized.
  console.log("  [tx1] Waiting for finalization...");
  await connection.confirmTransaction(createSig, "finalized");
  console.log("  [tx1] Finalized!");

  await sleep(200); // Rate limit

  // ─── TX 2: Commit + Trigger ───────────────────────────────────────────
  console.log("  [tx2] Building commit + trigger...");
  const commitIx = await randomness.commitIx(queueAccount.pubkey);
  await sleep(200);

  // Treasury is the wallet pubkey (bounty transfer placeholder, same as devnet-vrf.ts)
  const triggerIx = await epochProgram.methods
    .triggerEpochTransition()
    .accounts({
      payer: wallet.publicKey,
      epochState: accounts.epochStatePda,
      treasury: accounts.treasuryPda,
      randomnessAccount: rngKp.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .instruction();

  const commitTx = new Transaction().add(
    ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
    commitIx,
    triggerIx
  );

  const commitSig = await provider.sendAndConfirm(commitTx, [wallet.payer]);
  console.log(`  [tx2] Commit+Trigger: ${commitSig.slice(0, 16)}...`);

  await sleep(200); // Rate limit

  // ─── Wait for Oracle ──────────────────────────────────────────────────
  // Oracle needs ~3 slots to process the commitment
  console.log("  [wait] Waiting for oracle (~3 slots)...");
  await waitForSlotAdvance(connection, 3);

  // ─── TX 3: Reveal + Consume ───────────────────────────────────────────
  // Try to get reveal instruction. If oracle is down (404), fall back to
  // VRF timeout recovery: wait 300 slots, retry with fresh randomness.
  let consumeSig: string;
  let activeRngKp = rngKp;
  let activeRandomness = randomness;

  // Capture commit slot for instrumentation (CRANK-03)
  const commitSlot = await connection.getSlot();

  const happyRevealResult = await tryReveal(randomness, 5);
  let happyPathRevealAttempts = happyRevealResult?.attempts ?? 5;
  let happyPathGatewayMs = happyRevealResult?.durationMs ?? 0;
  let happyPathRecoveryTimeMs = 0; // 0 = no recovery needed
  let happyPathCommitToRevealSlots = 0;

  if (happyRevealResult) {
    // Happy path: oracle responded, build reveal + consume
    consumeSig = await sendRevealAndConsume(
      provider,
      epochProgram,
      accounts,
      happyRevealResult.revealIx,
      activeRngKp.publicKey,
      wallet
    );

    // Capture reveal slot for slot delta instrumentation
    const revealSlot = await connection.getSlot();
    happyPathCommitToRevealSlots = revealSlot - commitSlot;
  } else {
    // Oracle failed after 5 retries (~31 seconds) -- fall back to VRF timeout recovery
    const recoveryStartMs = Date.now();
    console.log("  [recovery] Oracle failed. Starting VRF timeout recovery...");

    // VRF-05: Calculate remaining wait from the ACTUAL VRF request slot, not from now.
    // The on-chain request_slot was set during trigger_epoch_transition (TX2).
    // If we've already spent time retrying reveals, some of the 300 slots may have elapsed.
    const stateForTimeout = await (epochProgram.account as any).epochState.fetch(
      accounts.epochStatePda
    );
    const vrfRequestSlot = Number(stateForTimeout.vrfRequestSlot);
    const currentSlotForTimeout = await connection.getSlot();
    const VRF_TIMEOUT_SLOTS = 300; // Must match on-chain constant
    const slotsToWait = Math.max(0, vrfRequestSlot + VRF_TIMEOUT_SLOTS - currentSlotForTimeout + 5);

    console.log(
      `[crank] VRF timeout recovery: waited ${slotsToWait} slots from request_slot ${vrfRequestSlot}, creating fresh randomness`
    );
    console.log(
      `  [recovery] VRF timeout recovery: request_slot=${vrfRequestSlot}, ` +
      `current=${currentSlotForTimeout}, waiting ${slotsToWait} slots`
    );

    if (slotsToWait > 0) {
      await waitForSlotAdvance(connection, slotsToWait);
    } else {
      console.log("  [recovery] VRF timeout already elapsed.");
    }

    await sleep(200);

    // Create fresh randomness account for retry
    console.log("  [recovery] Creating fresh randomness account...");
    const retryRngKp = Keypair.generate();
    const [retryRandomness, retryCreateIx] = await sb.Randomness.create(
      sbProgram as any,
      retryRngKp,
      queueAccount.pubkey
    );

    const retryCreateTx = new Transaction().add(retryCreateIx);
    retryCreateTx.feePayer = wallet.publicKey;
    retryCreateTx.recentBlockhash = (
      await connection.getLatestBlockhash()
    ).blockhash;
    retryCreateTx.sign(wallet.payer, retryRngKp);

    const retryCreateSig = await connection.sendRawTransaction(
      retryCreateTx.serialize(),
      { skipPreflight: true, maxRetries: 3 }
    );
    console.log(`  [recovery] Create TX sent, waiting for finalization...`);
    await connection.confirmTransaction(retryCreateSig, "finalized");

    await sleep(200);

    // Retry commit using retry_epoch_vrf
    console.log("  [recovery] Building retry commit + retry_epoch_vrf...");
    const retryCommitIx = await retryRandomness.commitIx(queueAccount.pubkey);
    const retryIx = await epochProgram.methods
      .retryEpochVrf()
      .accounts({
        payer: wallet.publicKey,
        epochState: accounts.epochStatePda,
        randomnessAccount: retryRngKp.publicKey,
      })
      .instruction();

    const retryCommitTx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      retryCommitIx,
      retryIx
    );
    await provider.sendAndConfirm(retryCommitTx, [wallet.payer]);
    console.log("  [recovery] Retry commit sent");

    // Wait for oracle on retry
    await waitForSlotAdvance(connection, 3);

    // Try reveal again with the new randomness account (10 attempts, ~93s with exponential backoff)
    const retryRevealResult = await tryReveal(retryRandomness, 10);
    if (!retryRevealResult) {
      throw new Error("VRF recovery failed: oracle still not responding after retry");
    }

    // Accumulate attempts from both the failed happy-path and retry
    happyPathRevealAttempts += retryRevealResult.attempts;
    happyPathGatewayMs = retryRevealResult.durationMs;
    happyPathRecoveryTimeMs = Date.now() - recoveryStartMs;

    activeRngKp = retryRngKp;
    activeRandomness = retryRandomness;

    // CRANK-01: Close the original rngKp account that was orphaned by timeout recovery.
    // activeRngKp now points to retryRngKp, so crank-runner will close that one.
    // The original rngKp.publicKey (created in TX1) leaks without this inline close.
    try {
      console.log(`  [recovery] Closing original randomness ${rngKp.publicKey.toBase58().slice(0, 12)}...`);
      const origCloseSig = await closeRandomnessAccount(provider, rngKp.publicKey);
      if (origCloseSig) {
        console.log(`  [recovery] Original account closed. TX: ${origCloseSig.slice(0, 16)}...`);
      } else {
        console.log(`  [recovery] WARNING: Original account close failed (startup sweep will catch it)`);
      }
    } catch {
      console.log(`  [recovery] WARNING: Original account close failed (startup sweep will catch it)`);
    }

    consumeSig = await sendRevealAndConsume(
      provider,
      epochProgram,
      accounts,
      retryRevealResult.revealIx,
      activeRngKp.publicKey,
      wallet
    );
  }

  await sleep(200); // Rate limit

  // Carnage execution (if triggered) was already bundled atomically in TX3.
  // No separate TX4 needed -- the no-op guard on executeCarnageAtomic
  // ensures the TX succeeds even when Carnage doesn't trigger.
  // We detect whether Carnage executed by reading post-TX3 state.

  // ─── Read Final State ─────────────────────────────────────────────────
  // Wait for RPC propagation after v0 TX (consistent with codebase pattern)
  if (accounts.carnageAccounts && accounts.alt) {
    await sleep(2000);
  }
  const stateAfter = await (epochProgram.account as any).epochState.fetch(
    accounts.epochStatePda
  );

  // Detect whether Carnage was executed atomically within TX3.
  // When carnageAccounts were provided AND carnage_pending is now false AND
  // lastCarnageEpoch matches current epoch, Carnage executed in the bundled TX.
  // The no-op guard means carnage_pending=false could also mean it didn't trigger,
  // so we check lastCarnageEpoch to distinguish.
  const carnageExecutedAtomically = !!(
    accounts.carnageAccounts &&
    !stateAfter.carnagePending &&
    stateAfter.lastCarnageEpoch === stateAfter.currentEpoch
  );

  if (carnageExecutedAtomically) {
    console.log("  [tx3] Carnage executed atomically in bundled TX (CARN-002 MEV gap: closed)");
  } else if (accounts.carnageAccounts) {
    console.log("  [tx3] No Carnage triggered this epoch (no-op path taken)");
  }

  // Determine if cheap_side flipped
  const newCheapSideStr = cheapSideToStr(stateAfter.cheapSide);
  const prevCheapSideStr = cheapSideToStr(previousCheapSide);
  const flipped = newCheapSideStr !== prevCheapSideStr;

  // VRF bytes: We can't read them directly from the randomness account after
  // consume (the program already processed them). We infer from the state.
  // For logging, we can derive the bytes that produced these values.
  // byte 0: coinFlip (tax high/low)
  // byte 1: crimeLow (CRIME low tax)
  // byte 2: crimeHigh (CRIME high tax)
  // byte 3: fraudLow (FRAUD low tax)
  // byte 4: fraudHigh (FRAUD high tax)
  // byte 5: carnageTrigger (< 11 = Carnage ~4.3%)
  // byte 6: carnageAction (< 5 = sell)
  // byte 7: carnageCoin (< 128 = CRIME, >= 128 = FRAUD)
  // We can reverse-engineer bytes 1 and 2 from the rates:
  const lowByte = (stateAfter.lowTaxBps - 100) / 100; // 0-3
  const highByte = (stateAfter.highTaxBps - 1100) / 100; // 0-3
  const vrfBytes = [
    flipped ? 0 : 192, // approximate (we know if it flipped but not exact byte)
    lowByte,
    highByte,
    // If Carnage executed atomically, carnage_pending is cleared but we know it triggered.
    carnageExecutedAtomically ? 5 : 50,
    0, // unknown
    0, // unknown
  ];

  // Check for carnage trigger. Carnage was triggered if:
  // - lastCarnageEpoch matches current (Carnage was executed this epoch)
  // - carnage_pending is still true (shouldn't happen with bundling, but safety net)
  const carnageTriggered = stateAfter.carnagePending === true ||
    stateAfter.lastCarnageEpoch === stateAfter.currentEpoch;

  const durationMs = Date.now() - startMs;

  return {
    epoch: stateAfter.currentEpoch,
    cheapSide: newCheapSideStr,
    lowTaxBps: stateAfter.lowTaxBps,
    highTaxBps: stateAfter.highTaxBps,
    crimeBuyTaxBps: stateAfter.crimeBuyTaxBps,
    crimeSellTaxBps: stateAfter.crimeSellTaxBps,
    fraudBuyTaxBps: stateAfter.fraudBuyTaxBps,
    fraudSellTaxBps: stateAfter.fraudSellTaxBps,
    flipped,
    vrfBytes,
    createSig,
    commitSig,
    consumeSig,
    durationMs,
    carnageTriggered,
    carnageExecutedAtomically,
    randomnessPubkey: activeRngKp.publicKey,
    // VRF instrumentation (CRANK-03)
    gatewayMs: happyPathGatewayMs,
    revealAttempts: happyPathRevealAttempts,
    recoveryTimeMs: happyPathRecoveryTimeMs,
    commitToRevealSlots: happyPathCommitToRevealSlots,
  };
}

// ─── Close Randomness Account ───────────────────────────────────────────
/**
 * Close a Switchboard randomness account to reclaim rent (~0.008 SOL).
 * Best-effort: returns null on failure (never throws). Safe to call on
 * already-closed or nonexistent accounts.
 *
 * @param provider Anchor provider with crank wallet
 * @param randomnessPubkey The randomness account to close
 * @returns TX signature on success, null on failure
 */
export async function closeRandomnessAccount(
  provider: AnchorProvider,
  randomnessPubkey: PublicKey,
): Promise<string | null> {
  try {
    const connection = provider.connection;

    // Check if account still exists before attempting close
    const acctInfo = await connection.getAccountInfo(randomnessPubkey);
    if (!acctInfo) {
      console.log(`  [close] Account ${randomnessPubkey.toBase58().slice(0, 12)}... already closed or does not exist`);
      return null;
    }

    // Resolve Switchboard program (same dynamic pattern as advanceEpochWithVRF)
    const sbProgramId = await sb.getProgramId(connection);
    const sbIdl = await Program.fetchIdl(sbProgramId, provider);
    if (!sbIdl) {
      console.log("  [close] WARNING: Could not fetch Switchboard IDL, skipping close");
      return null;
    }
    const sbProgram = new Program(sbIdl, provider);

    const randomness = new sb.Randomness(sbProgram as any, randomnessPubkey);
    const closeIx = await randomness.closeIx();

    const tx = new Transaction().add(closeIx);
    const sig = await provider.sendAndConfirm(tx, []);
    return sig;
  } catch (err) {
    console.log(`  [close] WARNING: Failed to close randomness account: ${String(err).slice(0, 200)}`);
    return null;
  }
}
