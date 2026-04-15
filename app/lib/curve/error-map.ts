/**
 * Bonding Curve Error Map
 *
 * Maps Anchor error codes from the Bonding Curve program to human-readable
 * messages suitable for display in the launch page UI.
 *
 * Error code range: 6000-6023 (24 variants)
 * Source: programs/bonding_curve/src/error.rs
 *
 * Follows the same pattern as app/lib/swap/error-map.ts.
 */

import { PROGRAM_IDS } from "@/lib/protocol-config";
import { isBraveBrowser } from "@/lib/brave-detect";

// =============================================================================
// Bonding Curve Errors (6000-6023)
// Source: programs/bonding_curve/src/error.rs
// =============================================================================

const CURVE_ERRORS: Record<number, string> = {
  // 6000: Overflow
  6000: "Calculation error. Please try a different amount.",
  // 6001: CurveNotActive
  6001: "This curve is no longer accepting buys.",
  // 6002: CurveNotActiveForSell
  6002: "This curve is no longer accepting sells.",
  // 6003: DeadlinePassed
  6003: "The curve deadline has passed. No more purchases accepted.",
  // 6004: BelowMinimum
  6004: "Minimum purchase is 0.05 SOL.",
  // 6005: WalletCapExceeded
  6005: "Purchase would exceed the 20M token per-wallet cap.",
  // 6006: SlippageExceeded
  6006: "Price moved beyond your slippage tolerance. Try increasing slippage.",
  // 6007: InvalidStatus
  6007: "The curve is not in the expected state for this operation.",
  // 6008: CurveNotFunded
  6008: "This curve has not been funded yet. Please wait for setup to complete.",
  // 6009: ZeroAmount
  6009: "Token amount must be greater than zero.",
  // 6010: InsufficientTokenBalance
  6010: "You don't have enough tokens to sell that amount.",
  // 6011: EscrowNotConsolidated
  6011: "Tax escrow must be consolidated before claiming refunds.",
  // 6012: NotRefundEligible
  6012: "This curve is not eligible for refunds.",
  // 6013: CurveAlreadyFilled
  6013: "This curve has already reached its target.",
  // 6014: InsufficientTokensOut
  6014: "Purchase too small -- would receive zero tokens. Try a larger amount.",
  // 6015: VaultInsolvency
  6015: "Vault solvency check failed. Please report this issue.",
  // 6016: DeadlineNotPassed
  6016: "The 48-hour deadline has not yet expired.",
  // 6017: CurveNotGraduated
  6017: "Curve has not graduated yet.",
  // 6018: NothingToBurn
  6018: "You have no tokens to burn for a refund.",
  // 6019: EscrowAlreadyConsolidated
  6019: "Tax escrow has already been consolidated.",
  // 6020: EscrowAlreadyDistributed
  6020: "Tax escrow has already been distributed.",
  // 6021: CRIMECurveNotFilled
  6021: "CRIME curve is not filled -- both curves must fill for graduation.",
  // 6022: FRAUDCurveNotFilled
  6022: "FRAUD curve is not filled -- both curves must fill for graduation.",
  // 6023: NoTokensOutstanding
  6023: "No tokens outstanding -- cannot calculate refund.",
};

// =============================================================================
// Error Parser
// =============================================================================

/**
 * Parse a bonding curve error into a human-readable message.
 *
 * Handles multiple error formats:
 * 1. Anchor-style: "Error Number: 6002"
 * 2. Solana-style: "custom program error: 0x1772"
 * 3. Common transaction errors (blockhash, insufficient funds, etc.)
 * 4. User cancellation
 *
 * @param error - Error object, string, or unknown value from a failed transaction
 * @returns Human-readable error message suitable for UI display
 */
export function parseCurveError(error: unknown): string {
  const errStr = String(error);

  // (a) Anchor "Error Number: (\d+)" pattern
  const anchorMatch = errStr.match(/Error Number:\s*(\d+)/);
  if (anchorMatch) {
    const code = parseInt(anchorMatch[1], 10);
    if (CURVE_ERRORS[code]) return CURVE_ERRORS[code];
  }

  // (b) Solana "custom program error: 0x..." pattern
  const hexMatch = errStr.match(/custom program error:\s*0x([0-9a-fA-F]+)/);
  if (hexMatch) {
    const code = parseInt(hexMatch[1], 16);
    if (CURVE_ERRORS[code]) return CURVE_ERRORS[code];
  }

  // (c) Anchor program error with program ID context
  const programErrorMatch = errStr.match(
    /Program (\w+) failed.*custom program error:\s*0x([0-9a-fA-F]+)/
  );
  if (programErrorMatch) {
    const programId = programErrorMatch[1];
    const code = parseInt(programErrorMatch[2], 16);
    if (programId === PROGRAM_IDS.BONDING_CURVE.toBase58() && CURVE_ERRORS[code]) {
      return CURVE_ERRORS[code];
    }
  }

  // (d) Blockhash / block height errors
  if (
    /Blockhash not found/i.test(errStr) ||
    /block height exceeded/i.test(errStr)
  ) {
    return "Transaction expired. Please try again.";
  }

  // (e) Insufficient funds
  if (/insufficient funds/i.test(errStr) || /Insufficient/i.test(errStr)) {
    return "Insufficient balance for this transaction.";
  }

  // (f) Transaction too large
  if (/Transaction too large/i.test(errStr)) {
    return "Transaction is too large. This is an unexpected error.";
  }

  // (g) User rejection
  if (/User rejected/i.test(errStr) || /rejected/i.test(errStr)) {
    return "Transaction was cancelled.";
  }

  // (h) Wallet extension popup closed / failed to open.
  // "Plugin Closed" is thrown by Backpack (and potentially other extension wallets)
  // when the signing popup fails to open or is immediately dismissed. Common cause:
  // Brave browser's built-in Solana wallet conflicts with extension wallets.
  if (/Plugin Closed/i.test(errStr)) {
    return "Wallet popup failed to open. If using Brave browser, go to brave://settings/wallet and set the Default Solana Wallet to \"Extensions (no fallback)\", then reload the page.";
  }

  // (i) Brave Wallet-specific errors
  if (isBraveBrowser()) {
    if (/Signature verification failed/i.test(errStr)
      || /WalletSendTransactionError/i.test(errStr)
      || /internal error/i.test(errStr)
      || /An internal error has occurred/i.test(errStr)) {
      return "This transaction failed due to a known Brave Wallet compatibility issue. "
        + "For the best experience, please use Phantom, Solflare, or Backpack instead. "
        + "Go to brave://settings/wallet and set Default Solana Wallet to "
        + "\"Extensions (no fallback)\", then reload the page.";
    }
  }

  // (j) Fallback -- include Brave hint if applicable
  if (isBraveBrowser()) {
    return "Transaction failed. If transactions keep failing, try switching from Brave Wallet "
      + "to Phantom or Solflare (brave://settings/wallet > \"Extensions (no fallback)\").";
  }
  return "Transaction failed. Please try again or adjust the amount.";
}
