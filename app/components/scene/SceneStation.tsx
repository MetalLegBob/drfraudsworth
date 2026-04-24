'use client';

/**
 * SceneStation -- Interactive button wrapper for factory station overlays.
 *
 * This is the core interactive component that turns static overlay images into
 * clickable, glowing, modal-opening factory machines. Each station renders as
 * a native <button> element for full accessibility.
 *
 * Architecture (two-layer approach):
 * - Outer <div> provides absolute positioning from SCENE_DATA.
 * - Inner <button> fills the div and holds all interactive CSS states
 *   (glow, press, focus indicators).
 *
 * Click coordinates: Uses e.clientX/clientY for mouse clicks (iris opens from
 * exact click point) and falls back to button center for keyboard activation
 * (Enter/Space have no mouse coordinates).
 */

import { useCallback } from 'react';
import Image from 'next/image';
import { SCENE_DATA } from '@/lib/image-data';
import { useModal } from '@/hooks/useModal';
import { useProtocolWallet } from '@/hooks/useProtocolWallet';
import type { StationMeta } from './scene-data';

interface SceneStationProps {
  /** Station metadata from STATIONS array */
  station: StationMeta;
}

export function SceneStation({ station }: SceneStationProps) {
  const { openModal } = useModal();
  const { connected } = useProtocolWallet();

  // Wallet station: swap to "wallet-connected" image when connected
  const isWalletConnected =
    station.overlayId === 'connect-wallet' && connected;
  const overlayId = isWalletConnected ? 'wallet-connected' : station.overlayId;
  const overlay = SCENE_DATA.overlays[overlayId];

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      // Use actual click coordinates for the iris animation origin.
      // clientX/clientY are 0 for keyboard activation (Enter/Space),
      // in which case we fall back to the button's center point.
      let x = e.clientX;
      let y = e.clientY;

      if (x === 0 && y === 0) {
        // Keyboard activation -- compute button center from bounding rect.
        const rect = e.currentTarget.getBoundingClientRect();
        x = rect.left + rect.width / 2;
        y = rect.top + rect.height / 2;
      }

      openModal(station.stationId, { x, y });
    },
    [openModal, station.stationId],
  );

  // Guard: unknown overlay -- don't render if SCENE_DATA doesn't have it.
  if (!overlay) {
    if (process.env.NODE_ENV === 'development') {
      console.warn(`SceneStation: unknown overlayId "${station.overlayId}"`);
    }
    return null;
  }

  // When wallet is connected, render as static decoration (no button, no glow)
  if (isWalletConnected) {
    return (
      <div
        className="absolute z-overlays"
        data-tutorial-id={`station-${station.stationId}`}
        style={{
          left: `${overlay.left}%`,
          top: `${overlay.top}%`,
          width: `${overlay.widthPct}%`,
          height: `${overlay.heightPct}%`,
        }}
      >
        <Image
          src={overlay.src}
          alt="Wallet Connected"
          fill
          quality={82}
          placeholder="blur"
          blurDataURL={overlay.blurDataURL}
          loading="lazy"
          className="object-contain"
        />
      </div>
    );
  }

  return (
    <div
      className="absolute z-overlays"
      data-tutorial-id={`station-${station.stationId}`}
      style={{
        left: `${overlay.left}%`,
        top: `${overlay.top}%`,
        width: `${overlay.widthPct}%`,
        height: `${overlay.heightPct}%`,
      }}
    >
      <button
        type="button"
        onClick={handleClick}
        aria-label={station.label}
        className="
          w-full h-full cursor-pointer
          transition-[filter,transform] duration-150 ease-out
          hover:drop-shadow-[0_0_28px_rgba(212,160,74,0.9)] hover:brightness-140
          focus-visible:drop-shadow-[0_0_28px_rgba(212,160,74,0.9)] focus-visible:brightness-140
          focus-visible:outline-none
          active:scale-95 active:duration-100
        "
      >
        <Image
          src={overlay.src}
          alt=""
          fill
          quality={82}
          placeholder="blur"
          blurDataURL={overlay.blurDataURL}
          loading="lazy"
          className="object-contain pointer-events-none"
        />
      </button>
    </div>
  );
}
