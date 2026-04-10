/**
 * Swap Error Map
 *
 * Maps Anchor error codes from the Tax Program and AMM to human-readable
 * messages suitable for display in the swap UI.
 *
 * Error code ranges:
 * - Tax Program (programs/tax-program/src/errors.rs): 6000-6021 (22 variants)
 * - AMM Program (programs/amm/src/errors.rs): 6000-6017 (18 variants)
 *
 * Note: AMM errors arrive via CPI, so they appear with the AMM program ID
 * in transaction logs. The error code numbers overlap between programs
 * (both start at 6000), but the program ID in the error context distinguishes them.
 * For the UI, we map both sets since the user doesn't care which program errored.
 *
 * Source: Tax_Pool_Logic_Spec.md, AMM_Implementation.md
 */

import { PROGRAM_IDS } from "@/lib/protocol-config";
import { isBraveBrowser } from "@/lib/brave-detect";

// =============================================================================
// Tax Program Errors (6000-6018)
// Source: programs/tax-program/src/errors.rs
// =============================================================================

const TAX_ERRORS: Record<number, string> = {
  // 6000: InvalidPoolType
  6000: "Invalid pool type for this swap operation.",
  // 6001: TaxOverflow
  6001: "Tax calculation error. Please try a smaller amount.",
  // 6002: SlippageExceeded
  6002: "Price moved beyond your slippage tolerance. Try increasing slippage or reducing the swap size.",
  // 6003: InvalidEpochState
  6003: "The protocol's epoch state is currently unavailable. Please try again in a moment.",
  // 6004: InsufficientInput
  6004: "The swap amount is too small to produce any output after fees.",
  // 6005: OutputBelowMinimum
  6005: "Output amount is below the minimum after tax. Try increasing the swap size.",
  // 6006: InvalidSwapAuthority
  6006: "Internal swap authority error. Please report this issue.",
  // 6007: WsolProgramMismatch
  6007: "Token program mismatch for SOL wrapping. Please report this issue.",
  // 6008: Token2022ProgramMismatch
  6008: "Token program mismatch for token transfer. Please report this issue.",
  // 6009: InvalidTokenOwner
  6009: "Token account ownership error. Make sure you have the correct token account.",
  // 6010: UnauthorizedCarnageCall
  6010: "This instruction is restricted to the Carnage system.",
  // 6011: InvalidStakingEscrow
  6011: "Staking escrow verification failed. Please report this issue.",
  // 6012: InvalidCarnageVault
  6012: "Carnage vault verification failed. Please report this issue.",
  // 6013: InvalidTreasury
  6013: "Treasury address verification failed. Please report this issue.",
  // 6014: InvalidAmmProgram
  6014: "AMM program address verification failed. Please report this issue.",
  // 6015: InvalidStakingProgram
  6015: "Staking program address verification failed. Please report this issue.",
  // 6016: InsufficientOutput
  6016: "Tax exceeds the swap output. Try a larger amount.",
  // 6017: MinimumOutputFloorViolation
  6017: "Your slippage setting is below the protocol's minimum floor (50% of expected output). Increase your minimum output.",
  // 6018: InvalidPoolOwner
  6018: "Pool account verification failed. Please report this issue.",
  // 6019: TaxIdentityMismatch
  6019: "Swap rejected: pool/mint identity check failed. Please refresh and try again.",
  // 6020: PoolMintMismatch
  6020: "Swap rejected: pool/mint identity check failed. Please refresh and try again.",
  // 6021: UnknownTaxedMint
  6021: "Swap rejected: unrecognized token mint.",
};

// =============================================================================
// AMM Errors (6000-6017)
// Source: programs/amm/src/errors.rs
// =============================================================================

const AMM_ERRORS: Record<number, string> = {
  // 6000: Overflow
  6000: "Swap calculation overflow. Try a smaller amount.",
  // 6001: KInvariantViolation
  6001: "Internal AMM invariant error. Please report this issue.",
  // 6002: PoolAlreadyInitialized
  6002: "This pool has already been initialized.",
  // 6003: MintsNotCanonicallyOrdered
  6003: "Internal mint ordering error. Please report this issue.",
  // 6004: Unauthorized
  6004: "Unauthorized operation.",
  // 6005: InvalidTokenProgram
  6005: "Token program mismatch. Please report this issue.",
  // 6006: ZeroSeedAmount
  6006: "Pool seed amount must be greater than zero.",
  // 6007: DuplicateMints
  6007: "Cannot create a pool with two identical tokens.",
  // 6008: ZeroAmount
  6008: "Transfer amount must be greater than zero.",
  // 6009: SlippageExceeded
  6009: "Price moved beyond your slippage tolerance. Try increasing slippage or reducing the swap size.",
  // 6010: PoolNotInitialized
  6010: "This pool has not been initialized yet.",
  // 6011: PoolLocked
  6011: "Pool is temporarily locked. Please try again in a moment.",
  // 6012: VaultMismatch
  6012: "Pool vault verification failed. Please report this issue.",
  // 6013: InvalidMint
  6013: "Token mint verification failed. Please report this issue.",
  // 6014: ZeroEffectiveInput
  6014: "The swap amount is too small -- fees would consume the entire input.",
  // 6015: ZeroSwapOutput
  6015: "The swap amount is too small to produce any output tokens.",
  // 6016: InvalidSwapAuthority
  6016: "Swaps must go through the Tax Program. Direct AMM calls are not allowed.",
  // 6017: LpFeeExceedsMax
  6017: "LP fee exceeds the maximum allowed (5%).",
};

// =============================================================================
// Vault Errors (6000-6007)
// Source: programs/conversion-vault/src/error.rs
// =============================================================================

const VAULT_ERRORS: Record<number, string> = {
  // 6000: ZeroAmount
  6000: "Input amount must be greater than zero.",
  // 6001: OutputTooSmall
  6001: "Input amount too small for vault conversion.",
  // 6002: InvalidMintPair
  6002: "Invalid token pair for vault conversion.",
  // 6003: SameMint
  6003: "Cannot convert a token to itself.",
  // 6004: InvalidTokenProgram
  6004: "Invalid token program. Please report this issue.",
  // 6005: MathOverflow
  6005: "Vault conversion overflow. Try a smaller amount.",
  // 6006: SlippageExceeded
  6006: "Vault output below your minimum. This is unexpected for fixed-rate conversions -- please retry.",
  // 6007: InvalidOwner
  6007: "Token account ownership verification failed. Please report this issue.",
};

// =============================================================================
// Combined Error Map
// =============================================================================

/**
 * Combined error map for all swap-relevant error codes.
 *
 * Keyed by program ID (base58) -> error code -> message.
 * Falls back to Tax Program errors when program ID is unknown,
 * since most user-initiated swaps go through Tax Program first.
 */
export const SWAP_ERROR_MAP: {
  tax: Record<number, string>;
  amm: Record<number, string>;
  vault: Record<number, string>;
} = {
  tax: TAX_ERRORS,
  amm: AMM_ERRORS,
  vault: VAULT_ERRORS,
};

// =============================================================================
// Error Parser
// =============================================================================

/**
 * Parse a swap error into a human-readable message.
 *
 * Handles multiple error formats:
 * 1. Anchor-style: "Error Number: 6002"
 * 2. Solana-style: "custom program error: 0x1772"
 * 3. Common transaction errors (blockhash, insufficient funds, etc.)
 * 4. User cancellation
 *
 * For Anchor errors, attempts to determine which program (Tax vs AMM)
 * produced the error by checking for the AMM program ID in the error string.
 * Falls back to Tax Program errors since that's the entry point.
 *
 * @param error - Error object, string, or unknown value from a failed transaction
 * @returns Human-readable error message suitable for UI display
 */
export function parseSwapError(error: unknown): string {
  const errStr = String(error);

  // (a) Anchor "Error Number: (\d+)" pattern
  const anchorMatch = errStr.match(/Error Number:\s*(\d+)/);
  if (anchorMatch) {
    const code = parseInt(anchorMatch[1], 10);
    // Check program IDs in the error string to determine source
    const isVault = errStr.includes(PROGRAM_IDS.VAULT.toBase58());
    const isAmm = errStr.includes(PROGRAM_IDS.AMM.toBase58());
    if (isVault && VAULT_ERRORS[code]) return VAULT_ERRORS[code];
    const errorMap = isAmm ? AMM_ERRORS : TAX_ERRORS;
    if (errorMap[code]) return errorMap[code];
    // Try the other map as fallback
    const fallbackMap = isAmm ? TAX_ERRORS : AMM_ERRORS;
    if (fallbackMap[code]) return fallbackMap[code];
  }

  // (b) Solana "custom program error: 0x..." pattern
  const hexMatch = errStr.match(/custom program error:\s*0x([0-9a-fA-F]+)/);
  if (hexMatch) {
    const code = parseInt(hexMatch[1], 16);
    // Check program IDs to determine source
    const isVault = errStr.includes(PROGRAM_IDS.VAULT.toBase58());
    const isAmm = errStr.includes(PROGRAM_IDS.AMM.toBase58());
    if (isVault && VAULT_ERRORS[code]) return VAULT_ERRORS[code];
    const errorMap = isAmm ? AMM_ERRORS : TAX_ERRORS;
    if (errorMap[code]) return errorMap[code];
    const fallbackMap = isAmm ? TAX_ERRORS : AMM_ERRORS;
    if (fallbackMap[code]) return fallbackMap[code];
  }

  // (c) Blockhash / block height errors
  if (/Blockhash not found/i.test(errStr) || /block height exceeded/i.test(errStr)) {
    return "Transaction expired. Please try again.";
  }

  // (d) Insufficient funds
  if (/insufficient funds/i.test(errStr) || /Insufficient/i.test(errStr)) {
    return "Insufficient balance for this swap.";
  }

  // (e) Transaction too large
  if (/Transaction too large/i.test(errStr)) {
    return "Transaction is too large. This is an unexpected error.";
  }

  // (f) User rejection
  if (/User rejected/i.test(errStr) || /rejected/i.test(errStr)) {
    return "Transaction was cancelled.";
  }

  // (g) Wallet extension popup closed / failed to open.
  // "Plugin Closed" is thrown by Backpack (and potentially other extension wallets)
  // when the signing popup fails to open or is immediately dismissed. Common cause:
  // Brave browser's built-in Solana wallet conflicts with extension wallets.
  if (/Plugin Closed/i.test(errStr)) {
    return "Wallet popup failed to open. If using Brave browser, go to brave://settings/wallet and set the Default Solana Wallet to \"Extensions (no fallback)\", then reload the page.";
  }

  // (h) Brave Wallet-specific errors.
  // Brave Wallet has known bugs with complex DeFi transactions (versioned TXs,
  // transfer hooks, multi-instruction). Detect common Brave Wallet error patterns
  // and guide users to switch to a dedicated wallet.
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

  // (i) Fallback -- include Brave hint if applicable
  if (isBraveBrowser()) {
    return "Swap failed. If transactions keep failing, try switching from Brave Wallet "
      + "to Phantom or Solflare (brave://settings/wallet > \"Extensions (no fallback)\").";
  }
  return "Swap failed. Please try again or reduce the swap amount.";
}
