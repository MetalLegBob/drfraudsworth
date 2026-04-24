'use client';

/**
 * MobileNav -- Steampunk-themed vertical navigation for mobile viewports.
 *
 * Replaces the desktop interactive factory scene with a touch-friendly
 * station list. Each item calls openModal() with the station ID, providing
 * 100% feature parity with the desktop SceneStation buttons.
 *
 * Structure:
 *   1. Fixed header (~120px): cropped factory background with gradient fade
 *      and wallet connection status badge
 *   2. Station list: 6 touch-friendly items (56px min-height) with inline
 *      SVG icons, labels, and right chevrons
 *   3. Decorative footer: steampunk pipe accent
 *
 * The station ordering is optimized for mobile (DeFi actions first), which
 * differs from the desktop tab order (wallet first for keyboard navigation).
 */

import Image from 'next/image';
import type { ReactNode } from 'react';
import { useModal } from '@/hooks/useModal';
import { useProtocolWallet } from '@/hooks/useProtocolWallet';
import { STATIONS } from '@/components/scene/scene-data';
import type { StationMeta } from '@/components/scene/scene-data';
import type { StationId } from '@/components/modal/ModalProvider';

// ---------------------------------------------------------------------------
// Mobile station order (DeFi actions first, utilities last)
// ---------------------------------------------------------------------------

/**
 * Mobile ordering prioritizes core DeFi actions over utilities.
 * Desktop tab order puts "Connect Wallet" first because keyboard users
 * need it. Mobile users have a wallet badge in the header and are more
 * likely to tap Swap or Carnage immediately.
 */
const MOBILE_ORDER: StationId[] = [
  'wallet',
  'swap',
  'staking',
  'carnage',
  'docs',
  'settings',
];

// ---------------------------------------------------------------------------
// Inline SVG icons (24x24, stroke-based, currentColor)
// ---------------------------------------------------------------------------

/** Stroke-based icons for each station. Uses currentColor so they inherit
 *  the parent's text color (--color-factory-accent via CSS). */
const STATION_ICONS: Record<StationId, ReactNode> = {
  /* Swap: two opposing arrows forming an exchange symbol */
  swap: (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M7 10L3 6L7 2" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M3 6H17" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M17 14L21 18L17 22" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M21 18H7" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),

  /* Carnage: bubbling cauldron -- pot outline with 3 bubble circles */
  carnage: (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M5 12C5 16.418 8.582 20 12 20C15.418 20 19 16.418 19 12H5Z" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M3 12H21" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="9" cy="8" r="1" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="14" cy="6" r="1.25" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="11" cy="4" r="0.75" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  ),

  /* Staking: dripping vat -- rectangle with drip paths underneath */
  staking: (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="4" y="4" width="16" height="10" rx="2" stroke="currentColor" strokeWidth="1.5" />
      <path d="M8 14V17" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M12 14V19" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M16 14V16" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M8 17C8 18.5 8 20 8 20" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M12 19C12 19.5 12 20 12 20" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M16 16C16 17.5 16 20 16 20" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  ),

  /* Wallet: wallet outline with flap */
  wallet: (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="3" y="6" width="18" height="14" rx="2" stroke="currentColor" strokeWidth="1.5" />
      <path d="M3 10H21" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M7 4L17 4C17 4 17 6 17 6H7C7 6 7 4 7 4Z" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="17" cy="15" r="1" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  ),

  /* Docs: document/scroll with lines */
  docs: (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M14 2H6C5.448 2 5 2.448 5 3V21C5 21.552 5.448 22 6 22H18C18.552 22 19 21.552 19 21V7L14 2Z" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M14 2V7H19" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M9 13H15" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M9 17H13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  ),

  /* Settings: gear/cog with teeth */
  settings: (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth="1.5" />
      <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.32 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
};

// ---------------------------------------------------------------------------
// MobileNavItem -- Individual station button
// ---------------------------------------------------------------------------

function MobileNavItem({ station }: { station: StationMeta }) {
  const { openModal } = useModal();

  const handleTap = () => {
    openModal(station.stationId, {
      // Origin from bottom-center: mobile modals slide up from the bottom,
      // so the iris origin is set to bottom of viewport for visual coherence.
      x: typeof window !== 'undefined' ? window.innerWidth / 2 : 512,
      y: typeof window !== 'undefined' ? window.innerHeight : 800,
    });
  };

  return (
    <button
      type="button"
      className="mobile-nav-item"
      onClick={handleTap}
      aria-label={`Open ${station.label}`}
      data-tutorial-id={`mobile-station-${station.stationId}`}
    >
      <span className="mobile-nav-icon" aria-hidden="true">
        {STATION_ICONS[station.stationId]}
      </span>
      <span className="mobile-nav-label">{station.label}</span>
      <span className="mobile-nav-chevron" aria-hidden="true">
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path
            d="M6 4L10 8L6 12"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
    </button>
  );
}

// ---------------------------------------------------------------------------
// MobileNav -- Main navigation component
// ---------------------------------------------------------------------------

export function MobileNav() {
  const { connected } = useProtocolWallet();

  return (
    <div className="mobile-nav">
      {/* Fixed header: factory scene teaser + wallet status */}
      <header className="mobile-header">
        <div className="mobile-header-image">
          <Image
            src="/scene/background/factory-bg-1920.webp"
            alt="Dr. Fraudsworth's Finance Factory"
            fill
            className="object-cover object-center"
            priority
            quality={60}
          />
          {/* Bottom gradient fade to background color */}
          <div className="mobile-header-fade" />
        </div>

        {/* Title lockup over the image */}
        <div className="mobile-header-content">
          <h1 className="mobile-header-title">Dr. Fraudsworth&apos;s</h1>
          <p className="mobile-header-subtitle">Fantastical Finance Factory</p>
        </div>

        {/* Wallet status badge */}
        <div className="mobile-wallet-badge">
          <span className={`mobile-wallet-dot ${connected ? 'connected' : ''}`} />
        </div>
      </header>

      {/* Station navigation list */}
      <nav className="mobile-station-list" aria-label="Factory stations">
        {MOBILE_ORDER.map((stationId) => {
          const station = STATIONS.find((s) => s.stationId === stationId)!;
          return <MobileNavItem key={stationId} station={station} />;
        })}
      </nav>

      {/* Decorative bottom pipe */}
      <div className="mobile-nav-footer" aria-hidden="true" />
    </div>
  );
}
