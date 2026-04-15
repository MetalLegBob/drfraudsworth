/**
 * Staking Error Map
 *
 * Maps Anchor error codes from the Staking Program to human-readable
 * messages suitable for display in the staking UI.
 *
 * Error code range: 6000-6011 (12 variants)
 * Source: programs/staking/src/errors.rs
 *
 * Unlike the swap error map which handles two programs (Tax + AMM),
 * staking only has a single program so error lookup is straightforward.
 */

import { isBraveBrowser } from "@/lib/brave-detect";

// =============================================================================
// Staking Program Errors (6000-6011)
// Source: programs/staking/src/errors.rs
// =============================================================================

const STAKING_ERRORS: Record<number, string> = {
  // 6000: ZeroAmount
  6000: "Amount must be greater than zero.",
  // 6001: InsufficientBalance
  6001: "You don't have enough PROFIT staked to unstake this amount.",
  // 6002: InsufficientEscrowBalance
  6002: "The reward escrow doesn't have enough SOL. Please try again later.",
  // 6003: NothingToClaim
  6003: "No rewards available to claim.",
  // 6004: Unauthorized
  6004: "You don't own this stake account.",
  // 6005: Overflow
  6005: "Calculation error. Please try a smaller amount.",
  // 6006: Underflow
  6006: "Calculation error. Please try a smaller amount.",
  // 6007: DivisionByZero
  6007: "Calculation error. Please report this issue.",
  // 6008: AlreadyUpdated
  6008: "Epoch already finalized. This is an internal error.",
  // 6009: NotInitialized
  6009: "The staking pool has not been initialized. Please report this issue.",
  // 6010: AlreadyInitialized
  6010: "The staking pool is already initialized.",
  // 6011: CooldownActive
  6011: "Cooldown active. You must wait 12 hours after claiming before unstaking.",
};

// =============================================================================
// Error Parser
// =============================================================================

/**
 * Parse a staking error into a human-readable message.
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
export function parseStakingError(error: unknown): string {
  const errStr = String(error);

  // (a) Anchor "Error Number: (\d+)" pattern
  const anchorMatch = errStr.match(/Error Number:\s*(\d+)/);
  if (anchorMatch) {
    const code = parseInt(anchorMatch[1], 10);
    if (STAKING_ERRORS[code]) return STAKING_ERRORS[code];
  }

  // (b) Solana "custom program error: 0x..." pattern
  const hexMatch = errStr.match(/custom program error:\s*0x([0-9a-fA-F]+)/);
  if (hexMatch) {
    const code = parseInt(hexMatch[1], 16);
    if (STAKING_ERRORS[code]) return STAKING_ERRORS[code];
  }

  // (c) Blockhash / block height errors
  if (/Blockhash not found/i.test(errStr) || /block height exceeded/i.test(errStr)) {
    return "Transaction expired. Please try again.";
  }

  // (d) Insufficient funds (SOL for fees)
  if (/insufficient funds/i.test(errStr) || /Insufficient/i.test(errStr)) {
    return "Insufficient SOL for transaction fees.";
  }

  // (e) User rejection
  if (/User rejected/i.test(errStr) || /rejected/i.test(errStr)) {
    return "Transaction was cancelled.";
  }

  // (f) Wallet extension popup closed / failed to open.
  // "Plugin Closed" is thrown by Backpack (and potentially other extension wallets)
  // when the signing popup fails to open or is immediately dismissed. Common cause:
  // Brave browser's built-in Solana wallet conflicts with extension wallets.
  // Fix: brave://settings/wallet -> set Default Solana Wallet to "Extensions (no fallback)".
  if (/Plugin Closed/i.test(errStr)) {
    return "Wallet popup failed to open. If using Brave browser, go to brave://settings/wallet and set the Default Solana Wallet to \"Extensions (no fallback)\", then reload the page.";
  }

  // (g) Brave Wallet-specific errors
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

  // (h) Fallback -- include Brave hint if applicable
  if (isBraveBrowser()) {
    return "Staking operation failed. If transactions keep failing, try switching from Brave Wallet "
      + "to Phantom or Solflare (brave://settings/wallet > \"Extensions (no fallback)\").";
  }
  return "Staking operation failed. Please try again.";
}
