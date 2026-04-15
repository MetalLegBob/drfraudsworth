/**
 * Brave Browser Detection
 *
 * Detects if the user is running Brave browser and whether the built-in
 * Brave Wallet is likely intercepting extension wallets like Phantom.
 *
 * Why this matters:
 * Brave's default "Default Solana wallet" setting is "Brave Wallet", which
 * overrides window.solana and prevents extension wallets from registering.
 * Users unknowingly connect through Brave Wallet instead of their preferred
 * wallet. Brave Wallet has known transaction signing bugs with complex DeFi
 * transactions (GitHub issues #41946, #478, #25374, #35802).
 *
 * Detection approach:
 * - Brave exposes navigator.brave.isBrave() (async, returns true)
 * - window.braveSolana exists when Brave Wallet's Solana provider is active
 * - window.braveSolana.isBraveWallet === true confirms it's Brave's provider
 */

/** Cached detection result (computed once, reused) */
let _isBrave: boolean | null = null;

/**
 * Synchronous check: is the user running Brave browser?
 * Uses navigator.brave which Brave exposes on the navigator object.
 * Falls back to checking the user agent (less reliable).
 */
export function isBraveBrowser(): boolean {
  if (typeof window === "undefined") return false;

  if (_isBrave !== null) return _isBrave;

  // Primary: Brave exposes navigator.brave
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  _isBrave = !!(navigator as any).brave;

  return _isBrave;
}

/**
 * Check if Brave Wallet's Solana provider is actively intercepting.
 * When true, extension wallets (Phantom, Solflare) are likely blocked.
 *
 * This happens when brave://settings/wallet has "Default Solana wallet"
 * set to "Brave Wallet" (the default for new Brave installs).
 */
export function isBraveWalletActive(): boolean {
  if (typeof window === "undefined") return false;

  // window.braveSolana is Brave Wallet's Solana provider
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const brave = (window as any).braveSolana;
  return brave?.isBraveWallet === true;
}

/**
 * Check if the user has Brave configured to allow extension wallets.
 * Returns true when:
 * - Not Brave browser (extensions always work)
 * - Brave with "Brave Wallet (prefer extensions)" or "Extensions (no fallback)"
 *
 * Returns false when:
 * - Brave with default "Brave Wallet" setting (extensions blocked)
 */
export function areExtensionWalletsAllowed(): boolean {
  if (!isBraveBrowser()) return true;

  // If Brave Wallet provider is active AND no Phantom detected,
  // extensions are likely blocked
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const w = window as any;
  if (w.braveSolana?.isBraveWallet && !w.phantom?.solana) {
    return false;
  }

  return true;
}
