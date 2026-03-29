/**
 * Atomic Multi-Hop Transaction Builder
 *
 * Combines all swap steps (1-hop, 2-hop, or 4-step split) into a SINGLE
 * VersionedTransaction (v0) using the protocol's Address Lookup Table.
 *
 * Benefits over the previous sequential approach:
 * 1. Single wallet signature prompt (good UX)
 * 2. Atomic execution -- all hops succeed or all revert (no partial state)
 * 3. Tighter slippage -- no inter-hop pool reserve changes from other users
 * 4. Account compression via ALT (fits multi-instruction TX within 1232 bytes)
 *
 * Instruction processing:
 * - Strips per-step ComputeBudget instructions, replaces with one combined set
 * - Converts ATA creation to idempotent (handles split routes where two steps
 *   reference the same ATA, e.g. WSOL ATA in two SOL-buy legs)
 * - Removes intermediate WSOL closeAccount instructions for sell-to-SOL splits
 *   so WSOL accumulates across legs and only unwraps at the end
 *
 * v0 TX + skipPreflight: Devnet simulation rejects v0 TX with stale blockhash.
 * useProtocolWallet centralizes the skipPreflight:true override on devnet;
 * callers express their mainnet preference (skipPreflight:false) and the
 * wallet layer handles the devnet override transparently.
 */

import {
  Connection,
  Transaction,
  VersionedTransaction,
  TransactionMessage,
  TransactionInstruction,
  AddressLookupTableAccount,
  ComputeBudgetProgram,
  type PublicKey,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

import type { Route, RouteStep } from "./route-types";
import {
  buildSolBuyTransaction,
  buildSolSellTransaction,
  buildVaultConvertTransaction,
} from "./swap-builders";
import { MINTS } from "@/lib/protocol-config";
import { parseSwapError } from "./error-map";
import { pollTransactionConfirmation } from "@/lib/confirm-transaction";
import { PROTOCOL_ALT } from "@/lib/protocol-config";
import type { ProtocolWallet } from "@/hooks/useProtocolWallet";

// =============================================================================
// Types
// =============================================================================

/**
 * Result of executing an atomic multi-hop route.
 *
 * - "confirmed": all hops succeeded atomically
 * - "failed": transaction failed or was rejected, no state change
 *
 * No "partial" status -- atomic execution is all-or-nothing.
 */
export interface MultiHopResult {
  /** Overall execution status */
  status: "confirmed" | "failed";
  /** Transaction signature (single TX) */
  signatures: string[];
  /** Human-readable error message on failure */
  error?: string;
}

/**
 * Build result: unsigned v0 transaction + confirmation metadata.
 */
export interface AtomicBuildResult {
  /** Unsigned VersionedTransaction ready for wallet signing */
  transaction: VersionedTransaction;
  /** Blockhash used in the transaction (needed for confirmation) */
  blockhash: string;
  /** Block height limit for transaction validity */
  lastValidBlockHeight: number;
}

// =============================================================================
// Internal: Step Transaction Builder
// =============================================================================

/**
 * Determine the swap type for a given route step based on pool label and
 * token direction, then call the appropriate swap-builder.
 *
 * Each swap-builder returns a complete legacy Transaction with:
 * - ComputeBudget instructions
 * - ATA creation (if needed)
 * - WSOL wrap/unwrap (for SOL pools)
 * - The actual swap instruction with hook remaining_accounts
 */
async function buildStepTransaction(
  step: RouteStep,
  connection: Connection,
  userPublicKey: PublicKey,
  minimumOutput: number,
  priorityFeeMicroLamports: number,
  isMultiHopStep: boolean = false,
): Promise<Transaction> {
  const isCrime =
    step.inputToken === "CRIME" ||
    step.outputToken === "CRIME";

  // SOL pool swaps
  if (step.pool.endsWith("/SOL")) {
    if (step.inputToken === "SOL") {
      return buildSolBuyTransaction({
        connection,
        userPublicKey,
        amountInLamports: step.inputAmount,
        minimumOutput,
        isCrime,
        taxBps: step.taxBps,
        priorityFeeMicroLamports,
      });
    } else {
      return buildSolSellTransaction({
        connection,
        userPublicKey,
        amountInBaseUnits: step.inputAmount,
        minimumOutput,
        isCrime,
        priorityFeeMicroLamports,
      });
    }
  }

  // Vault conversion steps (CRIME/FRAUD <-> PROFIT)
  // When isMultiHopStep=true: convert-all mode (amount_in=0) reads the user's
  // on-chain balance — whatever the preceding AMM step deposited in the same leg.
  // When isMultiHopStep=false: exact amount — used for 1-hop converts AND for
  // vault steps at the start of a split-route leg (prevents greedy consumption).
  if (step.pool.includes("Vault")) {
    const inputMint = step.inputToken === "PROFIT" ? MINTS.PROFIT : (isCrime ? MINTS.CRIME : MINTS.FRAUD);
    const outputMint = step.outputToken === "PROFIT" ? MINTS.PROFIT : (isCrime ? MINTS.CRIME : MINTS.FRAUD);
    const effectiveAmountIn = isMultiHopStep ? 0 : step.inputAmount;

    return buildVaultConvertTransaction({
      connection,
      userPublicKey,
      amountInBaseUnits: effectiveAmountIn,
      minimumOutput,
      inputMint,
      outputMint,
      priorityFeeMicroLamports,
    });
  }

  throw new Error(`Unknown pool type: ${step.pool}`);
}

// =============================================================================
// Internal: Instruction Processing
// =============================================================================

/** ComputeBudget instruction discriminators */
const CB_SET_CU_LIMIT = 2;
const CB_SET_CU_PRICE = 3;

/** SPL Token CloseAccount instruction discriminator */
const TOKEN_CLOSE_ACCOUNT = 9;

/**
 * Extract, deduplicate, and reorder instructions from multiple step
 * Transactions into a single flat list for an atomic v0 transaction.
 *
 * Processing:
 * 1. Strip ComputeBudget instructions → replaced with one combined set
 * 2. Make ATA creation idempotent (Create → CreateIdempotent) so duplicate
 *    ATA creates in split routes are harmless no-ops
 * 3. Remove intermediate WSOL closeAccount (unwrap) instructions, keeping
 *    only the last one. In a split sell-to-SOL route, this lets WSOL
 *    accumulate across legs and unwrap once at the end.
 *
 * @param stepTransactions - Array of legacy Transactions, one per route step
 * @returns Processed instruction list with combined compute budget prepended
 */
function processInstructionsForAtomic(
  stepTransactions: Transaction[],
): TransactionInstruction[] {
  const bodyInstructions: TransactionInstruction[] = [];
  let totalCU = 0;
  let maxPriorityFee = 0;

  for (const tx of stepTransactions) {
    for (const ix of tx.instructions) {
      // --- Strip ComputeBudget instructions, accumulate values ---
      if (ix.programId.equals(ComputeBudgetProgram.programId)) {
        if (ix.data[0] === CB_SET_CU_LIMIT && ix.data.length >= 5) {
          totalCU += ix.data.readUInt32LE(1);
        } else if (ix.data[0] === CB_SET_CU_PRICE && ix.data.length >= 9) {
          const price = Number(ix.data.readBigUInt64LE(1));
          maxPriorityFee = Math.max(maxPriorityFee, price);
        }
        continue;
      }

      // --- Make ATA creation idempotent ---
      // Associated Token Account Program: Create (empty data) → CreateIdempotent ([1])
      // This ensures the second ATA create for the same account in a split route
      // is a harmless no-op instead of a failure.
      if (
        ix.programId.equals(ASSOCIATED_TOKEN_PROGRAM_ID) &&
        ix.data.length <= 1
      ) {
        bodyInstructions.push(
          new TransactionInstruction({
            programId: ix.programId,
            keys: [...ix.keys],
            data: Buffer.from([1]), // CreateIdempotent
          }),
        );
        continue;
      }

      // Keep all other instructions as-is
      bodyInstructions.push(ix);
    }
  }

  // --- Remove intermediate WSOL closeAccount (unwrap) instructions ---
  // In split routes ending with SOL (e.g. PROFIT→CRIME→SOL + PROFIT→FRAUD→SOL),
  // each sell leg adds a closeAccount. The intermediate close would destroy the
  // WSOL ATA needed by the next sell leg. Fix: keep only the LAST closeAccount
  // so WSOL accumulates across sell legs and unwraps once at the end.
  const closeIndices: number[] = [];
  for (let i = 0; i < bodyInstructions.length; i++) {
    const ix = bodyInstructions[i];
    if (
      ix.programId.equals(TOKEN_PROGRAM_ID) &&
      ix.data.length === 1 &&
      ix.data[0] === TOKEN_CLOSE_ACCOUNT
    ) {
      closeIndices.push(i);
    }
  }
  if (closeIndices.length > 1) {
    // Remove all but the last closeAccount (iterate reverse to preserve indices)
    for (let j = closeIndices.length - 2; j >= 0; j--) {
      bodyInstructions.splice(closeIndices[j], 1);
    }
  }

  // --- Prepend combined compute budget ---
  const budgetInstructions: TransactionInstruction[] = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: totalCU }),
  ];
  if (maxPriorityFee > 0) {
    budgetInstructions.push(
      ComputeBudgetProgram.setComputeUnitPrice({ microLamports: maxPriorityFee }),
    );
  }

  return [...budgetInstructions, ...bodyInstructions];
}

// =============================================================================
// Internal: ALT Fetching
// =============================================================================

/** Module-level cache to avoid refetching the ALT on every swap */
let cachedALT: AddressLookupTableAccount | null = null;

export async function fetchProtocolALT(
  connection: Connection,
): Promise<AddressLookupTableAccount> {
  if (cachedALT) return cachedALT;

  const altAccount = await connection.getAddressLookupTable(PROTOCOL_ALT);
  if (!altAccount.value) {
    throw new Error(
      `Protocol ALT ${PROTOCOL_ALT.toBase58()} not found on-chain`,
    );
  }

  cachedALT = altAccount.value;
  return cachedALT;
}

// =============================================================================
// Public API: Build
// =============================================================================

/**
 * Build a single atomic VersionedTransaction (v0) for any route.
 *
 * Works for 1-hop (direct), 2-hop (multi-hop), and 4-step (split) routes.
 * All steps are combined into one transaction -- one signature, atomic.
 *
 * Since execution is atomic, intermediate hops use full expected outputs
 * (no slippage reduction). Slippage is only applied to steps producing
 * the route's final output token. Split routes are handled correctly:
 * each independent leg starts from its own quoted input amount.
 *
 * @param route - Complete route with 1+ steps
 * @param connection - Solana RPC connection
 * @param userPublicKey - User's wallet public key (feePayer)
 * @param priorityFeeMicroLamports - Priority fee per compute unit (0 = no priority)
 * @returns Unsigned VersionedTransaction + confirmation metadata
 */
export async function buildAtomicRoute(
  route: Route,
  connection: Connection,
  userPublicKey: PublicKey,
  priorityFeeMicroLamports: number,
): Promise<AtomicBuildResult> {
  // 1. Calculate slippage BPS from the route's minimumOutput vs outputAmount
  const slippageBps =
    route.outputAmount > 0
      ? Math.floor((1 - route.minimumOutput / route.outputAmount) * 10_000)
      : 100; // Default 1% if output is 0

  // 2. Build a legacy Transaction for each step (reuses all account resolution)
  //
  //    For multi-hop routes, pool reserves can change between QUOTE TIME
  //    (client-side) and TX EXECUTION (on-chain). Step 1's actual output may
  //    be less than quoted. To prevent step 2 from requesting more tokens than
  //    step 1 produced, we use step N's minimumOutput (slippage-adjusted) as
  //    step N+1's inputAmount. The AMM enforces minimumOutput on-chain — if
  //    step N succeeds, the user has at least that many tokens.
  //
  //    Vault steps in multi-hop routes use convert-all mode (amount_in=0),
  //    which reads the user's on-chain balance and converts everything.
  //    This eliminates intermediate token leakage from slippage differences.
  //
  //    Split routes have TWO INDEPENDENT legs (e.g., steps [0,1] and [2,3]).
  //    Each leg's first step must use its own inputAmount from the route
  //    quote, not the output from the previous leg. A leg boundary is detected
  //    when a step's inputToken matches the route's overall inputToken.
  const stepTransactions: Transaction[] = [];
  let previousMinimumOutput: number | null = null;

  for (let i = 0; i < route.steps.length; i++) {
    const step = route.steps[i];

    // Detect leg boundary in split routes: a new independent leg starts
    // when a step's inputToken matches the route's inputToken AND it's
    // not the very first step (which always starts fresh).
    const isNewLeg =
      i > 0 &&
      route.isSplit &&
      step.inputToken === route.inputToken;

    // For intermediate hops within a leg, use the previous step's guaranteed
    // minimum output as this step's input (safe: AMM enforces this on-chain).
    // For new legs or the first step, use the step's own quoted inputAmount.
    const effectiveInput: number =
      isNewLeg || previousMinimumOutput === null
        ? step.inputAmount
        : previousMinimumOutput;

    const effectiveStep: RouteStep =
      effectiveInput !== step.inputAmount
        ? { ...step, inputAmount: effectiveInput }
        : step;

    const minimumOutput = Math.floor(
      effectiveStep.outputAmount * (10_000 - slippageBps) / 10_000,
    );

    // Convert-all mode (amount_in=0) should only be used for vault steps
    // that RECEIVE tokens from a preceding AMM step in the same leg.
    // In split routes, vault steps at the START of a leg must use exact
    // amounts — otherwise leg 1's vault greedily converts the user's entire
    // balance, leaving 0 for leg 2's vault (ZeroAmount error).
    const isFirstStepInLeg = i === 0 || isNewLeg;
    const useConvertAll = route.steps.length > 1 && !isFirstStepInLeg;
    const tx = await buildStepTransaction(
      effectiveStep,
      connection,
      userPublicKey,
      minimumOutput,
      priorityFeeMicroLamports,
      useConvertAll,
    );
    stepTransactions.push(tx);

    // Vault conversions are deterministic (100:1, zero slippage). The actual
    // on-chain output is always the full outputAmount, not the slippage-reduced
    // minimumOutput. Use the exact vault output as the next step's input to
    // prevent intermediate token leakage (e.g., PROFIT→CRIME→SOL leaving
    // ~5% CRIME stranded when slippage is applied to the vault step).
    previousMinimumOutput = step.pool.includes("Vault")
      ? effectiveStep.outputAmount
      : minimumOutput;
  }

  // 3. Process instructions: strip duplicates, make ATAs idempotent
  const instructions = processInstructionsForAtomic(stepTransactions);

  // 4. Fetch protocol ALT and latest blockhash in parallel
  const [alt, { blockhash, lastValidBlockHeight }] = await Promise.all([
    fetchProtocolALT(connection),
    connection.getLatestBlockhash("confirmed"),
  ]);

  // 5. Compile to v0 message with ALT for account compression
  const messageV0 = new TransactionMessage({
    payerKey: userPublicKey,
    recentBlockhash: blockhash,
    instructions,
  }).compileToV0Message([alt]);

  return {
    transaction: new VersionedTransaction(messageV0),
    blockhash,
    lastValidBlockHeight,
  };
}

// =============================================================================
// Public API: Execute
// =============================================================================

/**
 * Execute an atomic multi-hop route.
 *
 * Signs the single v0 transaction (one wallet prompt), sends it, and
 * waits for confirmation. Passes skipPreflight:false (mainnet preference);
 * on devnet, useProtocolWallet overrides this to true centrally since v0
 * simulation returns "Blockhash not found" on devnet RPCs.
 *
 * @param build - Result from buildAtomicRoute
 * @param wallet - Protocol wallet (wallet-adapter wrapper)
 * @param connection - Solana RPC connection
 * @returns MultiHopResult with status and signature
 */
export async function executeAtomicRoute(
  build: AtomicBuildResult,
  wallet: ProtocolWallet,
  connection: Connection,
): Promise<MultiHopResult> {
  // 1. Sign and send (single wallet prompt, Blowfish-compatible)
  // Callers pass skipPreflight:false (mainnet preference). On devnet,
  // useProtocolWallet overrides to true centrally — no per-callsite logic.
  let signature: string;
  try {
    signature = await wallet.sendTransaction(
      build.transaction,
      connection,
      { skipPreflight: false, maxRetries: 3 },
    );
  } catch (err) {
    return { status: "failed", signatures: [], error: parseSwapError(err) };
  }

  // 2. Confirm (HTTP polling — more reliable than websocket)
  try {
    const confirmation = await pollTransactionConfirmation(
      connection,
      signature,
      build.lastValidBlockHeight,
    );

    // With skipPreflight, failed TXs are still "confirmed" on Solana.
    // Must check confirmation.err to detect on-chain failures.
    if (confirmation.err) {
      return {
        status: "failed",
        signatures: [signature],
        error: `Transaction failed on-chain: ${JSON.stringify(confirmation.err)}`,
      };
    }

    return {
      status: "confirmed",
      signatures: [signature],
    };
  } catch (err) {
    return {
      status: "failed",
      signatures: [],
      error: parseSwapError(err),
    };
  }
}
