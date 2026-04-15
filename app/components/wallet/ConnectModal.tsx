"use client";

/**
 * ConnectModal -- Wallet connection modal (standalone, outside ModalShell).
 *
 * Shows detected wallet-standard wallets and lets the user select one.
 * Used by WalletButton and LaunchWalletButton as the connection entry point.
 *
 * Desktop: Shows auto-detected wallet-standard extensions (Phantom, Solflare, etc.)
 * Mobile:  Shows deep-link wallet options that open our dApp URL inside the
 *          wallet's in-app browser, where wallet-standard auto-detect works normally.
 *          Detected wallets (user is already in a wallet's in-app browser) show
 *          as normal connect buttons at the top of the list.
 */

import { useEffect, useCallback, useMemo, useState } from "react";
import { useWallet } from "@solana/wallet-adapter-react";
import { isMobile } from "@/lib/isMobile";
import { MOBILE_WALLETS } from "@/lib/mobile-wallets";
import { isBraveBrowser, isBraveWalletActive, areExtensionWalletsAllowed } from "@/lib/brave-detect";

interface ConnectModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function ConnectModal({ isOpen, onClose }: ConnectModalProps) {
  const { wallets, select, connecting } = useWallet();

  const mobile = useMemo(() => isMobile(), []);

  // Brave browser detection: warn users if Brave Wallet is intercepting extension wallets.
  // Computed once when modal opens, not reactive (user must reload after changing settings).
  const [braveState] = useState(() => ({
    isBrave: isBraveBrowser(),
    walletActive: isBraveWalletActive(),
    extensionsAllowed: areExtensionWalletsAllowed(),
  }));

  // Close modal on Escape key
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    },
    [onClose],
  );

  useEffect(() => {
    if (isOpen) {
      document.addEventListener("keydown", handleKeyDown);
      return () => document.removeEventListener("keydown", handleKeyDown);
    }
  }, [isOpen, handleKeyDown]);

  if (!isOpen) return null;

  const handleSelectWallet = (walletName: string) => {
    // WalletName is a branded string type -- cast is safe here
    select(walletName as any); // eslint-disable-line @typescript-eslint/no-explicit-any
    onClose();
  };

  // Filter to installed/detected wallets
  const detectedWallets = wallets.filter(
    (w) => w.readyState === "Installed" || w.readyState === "Loadable",
  );

  // Names of detected wallets (for filtering mobile deep-link list)
  // Cast to Set<string> since WalletName is a branded string type
  const detectedNames = new Set(
    detectedWallets.map((w) => w.adapter.name as string),
  );

  // Current page URL for deep links
  const currentUrl =
    typeof window !== "undefined" ? window.location.href : "";

  return (
    // Fixed overlay with semi-transparent backdrop
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={onClose}
    >
      {/* Modal card -- stop click propagation so clicking inside doesn't close */}
      <div
        className="relative w-full max-w-md mx-4 bg-factory-surface border border-factory-border rounded-xl shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between p-6 pb-2">
          <h2 className="text-lg font-semibold text-factory-text">
            Connect to Dr. Fraudsworth
          </h2>
          <button
            onClick={onClose}
            className="text-factory-text-muted hover:text-factory-text transition-colors"
            aria-label="Close modal"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <p className="px-6 text-sm text-factory-text-secondary mb-4">
          {mobile ? "Choose your wallet app" : "Select a wallet to connect"}
        </p>

        <div className="px-6 pb-6 space-y-2">
          {/* Brave browser warning -- shown when Brave Wallet is intercepting extensions.
              Brave's default "Default Solana wallet" setting blocks Phantom/Solflare/Backpack.
              Users must change this setting for extension wallets to work. */}
          {!mobile && braveState.isBrave && !braveState.extensionsAllowed && (
            <div className="rounded-lg border border-yellow-500/40 bg-yellow-500/10 p-3 mb-2">
              <p className="text-sm font-medium text-yellow-400 mb-1">
                Brave Wallet is overriding your extension wallets
              </p>
              <p className="text-xs text-yellow-400/80 leading-relaxed">
                Brave&apos;s built-in wallet is blocking Phantom, Solflare, and other
                extension wallets. To fix this:
              </p>
              <ol className="text-xs text-yellow-400/80 mt-1 ml-4 list-decimal space-y-0.5 leading-relaxed">
                <li>
                  Open{" "}
                  <span className="font-mono text-yellow-300 select-all">
                    brave://settings/wallet
                  </span>
                </li>
                <li>
                  Set &quot;Default Solana wallet&quot; to{" "}
                  <span className="font-semibold text-yellow-300">
                    &quot;Brave Wallet (prefer extensions)&quot;
                  </span>
                </li>
                <li>Reload this page</li>
              </ol>
            </div>
          )}

          {/* Brave info banner -- shown when Brave is detected but extensions ARE allowed.
              Gentle heads-up that Brave Wallet is less reliable than dedicated wallets. */}
          {!mobile && braveState.isBrave && braveState.extensionsAllowed && braveState.walletActive && (
            <div className="rounded-lg border border-factory-border bg-factory-surface-elevated p-3 mb-2">
              <p className="text-xs text-factory-text-muted leading-relaxed">
                We recommend using Phantom, Solflare, or Backpack instead of
                Brave&apos;s built-in wallet for the most reliable experience.
              </p>
            </div>
          )}

          {mobile ? (
            /* ---- MOBILE: detected wallets first, then deep-link options ---- */
            <>
              {/* Wallets already detected (user is in a wallet's in-app browser) */}
              {detectedWallets.map((wallet) => (
                <button
                  key={wallet.adapter.name}
                  onClick={() => handleSelectWallet(wallet.adapter.name)}
                  disabled={connecting}
                  className="w-full flex items-center gap-3 rounded-lg border border-factory-border p-3 min-h-[48px] hover:brightness-110 transition-[filter] bg-factory-surface-elevated disabled:opacity-50"
                >
                  {wallet.adapter.icon && (
                    <img
                      src={wallet.adapter.icon}
                      alt={wallet.adapter.name}
                      width={28}
                      height={28}
                      className="rounded-md"
                    />
                  )}
                  <span className="text-sm font-medium text-factory-text">
                    {wallet.adapter.name}
                  </span>
                </button>
              ))}

              {/* Deep-link options for wallets NOT detected */}
              {MOBILE_WALLETS.filter((mw) => !detectedNames.has(mw.name)).map(
                (mw) => (
                  <a
                    key={mw.name}
                    href={mw.deepLink(currentUrl)}
                    rel="noopener noreferrer"
                    onClick={onClose}
                    className="w-full flex items-center gap-3 rounded-lg border border-factory-border p-3 min-h-[48px] hover:brightness-110 transition-[filter] bg-factory-surface-elevated"
                  >
                    <img
                      src={mw.icon}
                      alt={mw.name}
                      width={28}
                      height={28}
                      className="rounded-md"
                    />
                    <span className="text-sm font-medium text-factory-text flex-1">
                      {mw.name}
                    </span>
                    {/* "Install" badge -- shown because wallet is not detected,
                        meaning the user likely doesn't have it installed.
                        The deep link opens the app if installed, or the app store. */}
                    <span className="text-[10px] font-mono uppercase tracking-wider text-factory-text-muted bg-factory-surface px-2 py-0.5 rounded-full border border-factory-border">
                      Open App
                    </span>
                  </a>
                ),
              )}
            </>
          ) : (
            /* ---- DESKTOP: unchanged extension-only behavior ---- */
            <>
              {detectedWallets.length === 0 ? (
                <div className="rounded-lg border border-factory-border p-4 text-center">
                  <p className="text-sm text-factory-text-muted">
                    No wallets detected. Install Phantom, Solflare, or Backpack
                    to continue.
                  </p>
                </div>
              ) : (
                detectedWallets.map((wallet) => (
                  <button
                    key={wallet.adapter.name}
                    onClick={() => handleSelectWallet(wallet.adapter.name)}
                    disabled={connecting}
                    className="w-full flex items-center gap-3 rounded-lg border border-factory-border p-3 hover:brightness-110 transition-[filter] bg-factory-surface-elevated disabled:opacity-50"
                  >
                    {wallet.adapter.icon && (
                      <img
                        src={wallet.adapter.icon}
                        alt={wallet.adapter.name}
                        width={28}
                        height={28}
                        className="rounded-md"
                      />
                    )}
                    <span className="text-sm font-medium text-factory-text">
                      {wallet.adapter.name}
                    </span>
                  </button>
                ))
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
