"use client";

import { ConnectionProvider, WalletProvider } from "@solana/wallet-adapter-react";
import { ModalProvider } from "@/components/modal/ModalProvider";
import { ClusterConfigProvider } from "@/providers/ClusterConfigProvider";
import { SettingsProvider } from "@/providers/SettingsProvider";
import { AudioProvider } from "@/providers/AudioProvider";
import { ProtocolStateProvider } from "@/providers/ProtocolStateProvider";
import { ModalRoot } from "@/components/modal/ModalShell";
import { ToastProvider, ToastContainer, useToast } from "@/components/toast/ToastProvider";
import { SplashScreen } from "@/components/onboarding/SplashScreen";
import { TutorialOverlay } from "@/components/onboarding/TutorialOverlay";
import { QuickMuteButton } from "@/components/audio/QuickMuteButton";
import { useWallet } from "@solana/wallet-adapter-react";
import { useEffect, useRef, useMemo } from "react";

/** Fires "Wallet connected" toast on fresh wallet connection.
 *  Watches useWallet().connected -- when it transitions false->true, fires toast.
 *  useRef tracks previous state to detect the transition. */
function WalletConnectionToast() {
  const { showToast } = useToast();
  const { connected } = useWallet();
  const wasConnected = useRef(false);

  useEffect(() => {
    if (connected && !wasConnected.current) {
      showToast("success", "Wallet connected");
    }
    wasConnected.current = connected;
  }, [connected, showToast]);

  return null;
}

export default function Providers({ children }: { children: React.ReactNode }) {
  // RPC endpoint: /api/rpc proxy keeps the Helius API key server-side.
  // The proxy forwards allowed JSON-RPC methods to the real Helius endpoint.
  // Must use full URL — @solana/web3.js Connection rejects relative paths.
  const endpoint = typeof window !== "undefined"
    ? `${window.location.origin}/api/rpc`
    : "http://localhost:3000/api/rpc";
  // Empty array: all target wallets (Phantom, Solflare, Backpack) implement
  // wallet-standard and auto-register via WalletProvider's built-in detection.
  // No explicit adapter constructors needed.
  const wallets = useMemo(() => [], []);

  return (
    <ConnectionProvider endpoint={endpoint}>
      <WalletProvider wallets={wallets} autoConnect>
        {/* Provider tree: Connection > Wallet > Settings > Audio > Modal > ProtocolState > Toast
            AudioProvider after SettingsProvider so it can read muted/volume.
            AudioProvider before ModalProvider so modal content can useAudio().

            ModalProvider inside wallet providers so modal content can
            access wallet hooks like useProtocolWallet. ModalRoot renders the
            singleton <dialog> element -- always present in the DOM regardless
            of which page is active.

            ToastProvider wraps children so any component can call useToast().
            ToastContainer is a sibling of ModalRoot -- it renders via portal
            at document.body level so toasts appear above the dialog and
            persist after the modal closes.

            ProtocolStateProvider inside ModalProvider because it uses
            useVisibility() which calls useModal(). Creates a single SSE
            connection per tab -- all useProtocolState() consumers share it.

            QuickMuteButton is a sibling of SplashScreen -- renders the
            floating mute toggle after audio is initialized. */}
        <ClusterConfigProvider>
          <SettingsProvider>
            <AudioProvider>
              <ModalProvider>
                <ProtocolStateProvider>
                  <ToastProvider>
                    {children}
                    <ModalRoot />
                    <SplashScreen />
                    <TutorialOverlay />
                    <QuickMuteButton />
                    <ToastContainer />
                    <WalletConnectionToast />
                  </ToastProvider>
                </ProtocolStateProvider>
              </ModalProvider>
            </AudioProvider>
          </SettingsProvider>
        </ClusterConfigProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}
