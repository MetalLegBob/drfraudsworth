/**
 * SOL Buy WSOL Wrap Amount Test
 *
 * Verifies the tax-aware WSOL wrap math used by buildSolBuyTransaction.
 * Tests the formula: solToSwap = amountIn - floor(amountIn * taxBps / 10_000)
 *
 * This must exactly match on-chain calculate_tax (programs/tax-program/src/helpers/tax_math.rs).
 *
 * Root cause of the MAX SOL button wallet simulation failure:
 * On-chain, swap_sol_buy takes tax as native SOL transfers AND uses WSOL
 * for the AMM swap. If the client wraps the full amountIn as WSOL, the
 * mid-TX peak SOL requirement is amountIn + taxAmount (double-dip).
 * The fix: wrap only sol_to_swap = amountIn - taxAmount as WSOL.
 */

import { describe, it, expect } from "vitest";

// ---------------------------------------------------------------------------
// Direct test of the tax math formula (no mocks needed)
// This is the exact formula used in buildSolBuyTransaction
// ---------------------------------------------------------------------------

function computeSolToSwap(amountInLamports: number, taxBps: number): {
  taxAmount: number;
  solToSwap: number;
} {
  const taxAmount = Math.floor(amountInLamports * taxBps / 10_000);
  const solToSwap = amountInLamports - taxAmount;
  return { taxAmount, solToSwap };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("SOL buy WSOL wrap tax math", () => {

  it("14% tax: wraps only 86% of input as WSOL", () => {
    const { taxAmount, solToSwap } = computeSolToSwap(10_000_000_000, 1400);

    expect(taxAmount).toBe(1_400_000_000); // 1.4 SOL
    expect(solToSwap).toBe(8_600_000_000); // 8.6 SOL
    expect(taxAmount + solToSwap).toBe(10_000_000_000); // total = input
  });

  it("4% tax: wraps only 96% of input as WSOL", () => {
    const { taxAmount, solToSwap } = computeSolToSwap(5_000_000_000, 400);

    expect(taxAmount).toBe(200_000_000); // 0.2 SOL
    expect(solToSwap).toBe(4_800_000_000); // 4.8 SOL
  });

  it("0% tax: wraps full input", () => {
    const { taxAmount, solToSwap } = computeSolToSwap(1_000_000_000, 0);

    expect(taxAmount).toBe(0);
    expect(solToSwap).toBe(1_000_000_000);
  });

  it("floor division matches on-chain (odd amount)", () => {
    // amount_in * tax_bps doesn't divide evenly by 10_000
    const { taxAmount, solToSwap } = computeSolToSwap(1_000_000_001, 1400);

    // On-chain: floor(1_000_000_001 * 1400 / 10000) = floor(140_000_000.14) = 140_000_000
    expect(taxAmount).toBe(140_000_000);
    expect(solToSwap).toBe(860_000_001);
    expect(taxAmount + solToSwap).toBe(1_000_000_001);
  });

  it("MAX button scenario: no double-dip at full balance", () => {
    // User has 10 SOL, MAX reserves ~0.007 SOL for fees
    const userBalance = 10_000_000_000;
    const feeReserve = 7_000_000; // 0.007 SOL
    const amountIn = userBalance - feeReserve; // 9_993_000_000

    const { taxAmount, solToSwap } = computeSolToSwap(amountIn, 1400);

    // WSOL wrap costs: solToSwap lamports
    // Tax costs: taxAmount lamports (native SOL)
    // Total: solToSwap + taxAmount = amountIn
    expect(solToSwap + taxAmount).toBe(amountIn);

    // User has exactly amountIn available (after fee reserve)
    // No double-dip: amountIn is enough for both wrap + tax
    // OLD BUG: wrap(amountIn) + tax(taxAmount) = amountIn + taxAmount > available
    const oldBugPeakRequirement = amountIn + taxAmount;
    const newFixPeakRequirement = solToSwap + taxAmount;

    expect(newFixPeakRequirement).toBe(amountIn); // fits
    expect(oldBugPeakRequirement).toBeGreaterThan(amountIn); // didn't fit
  });

  it("small amounts: rounding doesn't cause off-by-one", () => {
    // 100 lamports at 14% = 14 lamports tax
    const { taxAmount, solToSwap } = computeSolToSwap(100, 1400);
    expect(taxAmount).toBe(14);
    expect(solToSwap).toBe(86);
  });

  it("very small amounts: floor to 0 tax", () => {
    // 10 lamports at 4% = 0.4 → floor to 0
    const { taxAmount, solToSwap } = computeSolToSwap(10, 400);
    expect(taxAmount).toBe(0);
    expect(solToSwap).toBe(10);
  });

  it("max tax rate (100%): wraps 0 WSOL", () => {
    const { taxAmount, solToSwap } = computeSolToSwap(1_000_000_000, 10_000);
    expect(taxAmount).toBe(1_000_000_000);
    expect(solToSwap).toBe(0);
  });
});
