/**
 * Priority Fee Helper — Dynamic fee estimation for mainnet transaction landing.
 *
 * On congested mainnet, transactions without priority fees are deprioritized by
 * validators and expire (TransactionExpiredBlockheightExceededError). This module
 * provides dynamic fee estimation using Helius's getPriorityFeeEstimate RPC method,
 * with a static fallback for non-Helius RPCs.
 *
 * Fee strategy:
 *   - Uses Helius getPriorityFeeEstimate (priority level "high") when available
 *   - Falls back to a conservative static fee (50,000 microlamports/CU)
 *   - Caps fees at MAX_PRIORITY_FEE to prevent overspending
 *   - Caches estimates for CACHE_TTL_MS to reduce RPC calls
 *
 * Cost impact at 50,000 microlamports/CU:
 *   - 400,000 CU TX: 20,000 lamports = 0.00002 SOL
 *   - 600,000 CU TX: 30,000 lamports = 0.00003 SOL
 *   Per epoch cycle (~3 TXs, ~1.2M CU): ~0.00006 SOL
 *   At 48 epochs/day: ~0.003 SOL/day. Very affordable.
 *   The dynamic Helius estimate usually returns lower values than the fallback.
 */

import { Connection, ComputeBudgetProgram, TransactionInstruction } from "@solana/web3.js";

// ---- Constants ----

/** Fallback priority fee when Helius API is unavailable (microlamports per CU) */
const FALLBACK_PRIORITY_FEE = 50_000;

/** Minimum priority fee floor (microlamports per CU) — never go below this */
const MIN_PRIORITY_FEE = 1_000;

/** Maximum priority fee cap (microlamports per CU) — prevents overspending */
const MAX_PRIORITY_FEE = 500_000;

/** Cache TTL for fee estimates (ms) — re-query every 30s */
const CACHE_TTL_MS = 30_000;

// ---- Cache ----

let cachedFee: number | null = null;
let cachedAt = 0;

/**
 * Get the recommended priority fee in microlamports per compute unit.
 *
 * Uses Helius's getPriorityFeeEstimate RPC extension when available.
 * This method returns percentile-based fee estimates across recent transactions.
 * We use "high" priority level to ensure reliable landing.
 *
 * Falls back to FALLBACK_PRIORITY_FEE if the RPC doesn't support the method
 * or if the call fails.
 *
 * @param connection Solana connection (must be Helius for dynamic estimation)
 * @returns Priority fee in microlamports per CU, clamped to [MIN, MAX]
 */
export async function getRecommendedPriorityFee(
  connection: Connection
): Promise<number> {
  // Return cached value if fresh
  if (cachedFee !== null && Date.now() - cachedAt < CACHE_TTL_MS) {
    return cachedFee;
  }

  try {
    // Helius-specific RPC method: getPriorityFeeEstimate
    // See: https://docs.helius.dev/solana-rpc-nodes/alpha-priority-fee-api
    const response = await (connection as any)._rpcRequest(
      "getPriorityFeeEstimate",
      [
        {
          options: {
            priorityLevel: "high",
          },
        },
      ]
    );

    if (response?.result?.priorityFeeEstimate != null) {
      const estimate = Math.round(response.result.priorityFeeEstimate);
      const clamped = Math.max(MIN_PRIORITY_FEE, Math.min(MAX_PRIORITY_FEE, estimate));
      cachedFee = clamped;
      cachedAt = Date.now();
      console.log(
        `  [fee] Priority fee estimate: ${estimate} -> ${clamped} microlamports/CU (Helius)`
      );
      return clamped;
    }
  } catch (err) {
    // Non-Helius RPC or API error — use fallback
    console.log(
      `  [fee] Helius fee estimate unavailable: ${String(err).slice(0, 80)}. Using fallback.`
    );
  }

  // Fallback: use static fee
  cachedFee = FALLBACK_PRIORITY_FEE;
  cachedAt = Date.now();
  console.log(`  [fee] Using fallback priority fee: ${FALLBACK_PRIORITY_FEE} microlamports/CU`);
  return FALLBACK_PRIORITY_FEE;
}

/**
 * Build a ComputeBudgetProgram.setComputeUnitPrice instruction.
 *
 * Convenience wrapper that fetches the recommended fee and returns
 * the instruction ready to prepend to any transaction.
 *
 * @param connection Solana connection
 * @returns TransactionInstruction for setComputeUnitPrice
 */
export async function buildPriorityFeeIx(
  connection: Connection
): Promise<TransactionInstruction> {
  const fee = await getRecommendedPriorityFee(connection);
  return ComputeBudgetProgram.setComputeUnitPrice({ microLamports: fee });
}
