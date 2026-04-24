'use client';

/**
 * SplashScreen -- Full-viewport intro gate for Dr. Fraudsworth's Finance Factory.
 *
 * Shows every page load (doubles as a loading screen while the main page hydrates).
 *
 * Visual sequence:
 * 1. Black background, centered brass gear ring (wheel.png) rotates 360° over 1s
 * 2. Gear completes 2 full rotations (2s total)
 * 3. "Push the Button" text fades in below the brass button (wheelbutton.png)
 * 4. User clicks the button → text changes to "Thanks", pauses 1s, then overlay fades out
 *
 * Replaces the Phase 59 WelcomeModal (which was first-visit-only text dialog).
 * This is a visual gate that shows every load.
 */

import { useState, useEffect, useCallback } from 'react';
import Image from 'next/image';
import { useAudio } from '@/hooks/useAudio';

/** Duration of the gear spin animation (2 full rotations at 1s each) */
const SPIN_DURATION_MS = 2000;
/** Duration of the "Thanks" pause before fading out */
const THANKS_PAUSE_MS = 1000;
/** Duration of the fade-out when the overlay disappears */
const FADE_OUT_MS = 500;

export function SplashScreen() {
  const { initAudio } = useAudio();

  // Phase tracking: 'spinning' → 'ready' → 'thanking' → 'exiting' → unmounted
  const [phase, setPhase] = useState<'spinning' | 'ready' | 'thanking' | 'exiting' | 'done'>('spinning');

  // After 2 gear rotations, enable the button
  useEffect(() => {
    const timer = setTimeout(() => {
      setPhase('ready');
    }, SPIN_DURATION_MS);
    return () => clearTimeout(timer);
  }, []);

  // Handle button click: init audio (gesture gate) → show "Thanks" → pause → fade out
  const handleEnter = useCallback(() => {
    if (phase !== 'ready') return;

    // Gesture-gated AudioContext creation + music start.
    // MUST be first: browsers only allow AudioContext creation within the
    // narrow synchronous window of a user gesture. Any async work (setTimeout,
    // setState flush) could close this window.
    initAudio();

    setPhase('thanking');

    // After the "Thanks" pause, start the fade-out
    setTimeout(() => {
      setPhase('exiting');

      // After fade-out completes, unmount entirely
      setTimeout(() => {
        setPhase('done');
        if (typeof window !== 'undefined') {
          window.dispatchEvent(new Event('drfraudsworth:splash-complete'));
        }
      }, FADE_OUT_MS);
    }, THANKS_PAUSE_MS);
  }, [phase, initAudio]);

  // Once done, render nothing
  if (phase === 'done') return null;

  const isReady = phase === 'ready';
  const isThanking = phase === 'thanking';
  const isExiting = phase === 'exiting';

  return (
    <div
      className={`splash-overlay ${isExiting ? 'splash-exiting' : ''}`}
      aria-label="Loading Dr. Fraudsworth's Finance Factory"
      role="status"
    >
      {/* Gear assembly: wheel (outer ring) + button (inner disc), both centered */}
      <div className="splash-gear-assembly">
        {/* Outer gear ring -- rotates */}
        <div className={`splash-wheel ${phase === 'spinning' ? 'splash-spinning' : ''}`}>
          <Image
            src="/splash/wheel.png"
            alt=""
            width={400}
            height={400}
            priority
            aria-hidden="true"
          />
        </div>

        {/* Inner button disc -- spins with gear, becomes clickable after */}
        <button
          type="button"
          className={`splash-button ${phase === 'spinning' ? 'splash-spinning' : ''} ${isReady || isThanking ? 'splash-button-active' : ''}`}
          onClick={handleEnter}
          disabled={!isReady && !isThanking}
          aria-label="Enter the Factory"
        >
          <Image
            src="/splash/wheelbutton.png"
            alt="Enter"
            width={400}
            height={400}
            priority
          />
        </button>
      </div>

      {/* CTA text -- fades in after gear spin, swaps to "Thanks" on click */}
      <p className={`splash-cta ${isReady || isThanking || isExiting ? 'splash-cta-visible' : ''}`}>
        {isThanking || isExiting ? 'Thanks' : 'Push the Button'}
      </p>
    </div>
  );
}
