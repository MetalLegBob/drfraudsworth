'use client';

/**
 * QuickMuteButton -- Floating toolbar: mute toggle + social links.
 *
 * A row of 36px brass-themed circular buttons fixed in the top-left corner.
 * Contains the mute toggle, X (Twitter) link, and Telegram link.
 * Hidden until the audio system is initialized (splash screen dismissed).
 *
 * Why SettingsProvider and not AudioProvider for mute state?
 *   SettingsProvider owns persistence (localStorage) and is the canonical
 *   source. AudioProvider reads muted/volume FROM SettingsProvider and
 *   pushes to AudioManager. Writing through setMuted() keeps everything
 *   in sync: QuickMuteButton <-> SettingsStation Toggle <-> AudioManager.
 *
 * z-index: var(--z-index-overlays) = 10. Above page content, below
 * modals (50), splash screen (9999), and Privy dialogs (999999).
 */

import { useSettings } from '@/hooks/useSettings';
import { useAudio } from '@/hooks/useAudio';

const TOGGLE_TUTORIAL_EVENT = 'drfraudsworth:tutorial-toggle';

export function QuickMuteButton() {
  const { settings, setMuted } = useSettings();
  const { isInitialized } = useAudio();

  // Don't render until audio system is initialized (splash screen dismissed).
  // This prevents floating buttons from appearing during the splash overlay.
  if (!isInitialized) return null;

  const isMuted = settings.muted;

  return (
    <div className="quick-toolbar">
      {/* Mute toggle */}
      <button
        type="button"
        aria-label="Mute"
        aria-pressed={isMuted}
        className="quick-toolbar-btn"
        onClick={() => setMuted(!isMuted)}
      >
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
          {isMuted ? (
            <>
              <line x1="23" y1="9" x2="17" y2="15" />
              <line x1="17" y1="9" x2="23" y2="15" />
            </>
          ) : (
            <>
              <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
              <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
            </>
          )}
        </svg>
      </button>

      {/* X (Twitter) */}
      <a
        href="https://x.com/fraudsworth"
        target="_blank"
        rel="noopener noreferrer"
        aria-label="Follow on X"
        className="quick-toolbar-btn"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="currentColor"
          aria-hidden="true"
        >
          <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
        </svg>
      </a>

      {/* Telegram */}
      <a
        href="https://t.me/fraudsworth"
        target="_blank"
        rel="noopener noreferrer"
        aria-label="Join Telegram"
        className="quick-toolbar-btn"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="currentColor"
          aria-hidden="true"
        >
          <path d="M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12.056 0h-.112zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.479.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z" />
        </svg>
      </a>

      {/* Tutorial toggle */}
      <button
        type="button"
        aria-label="Toggle tutorial"
        className="quick-toolbar-btn"
        onClick={() => {
          window.dispatchEvent(new Event(TOGGLE_TUTORIAL_EVENT));
        }}
      >
        <span aria-hidden="true" style={{ fontWeight: 700, fontSize: '16px', lineHeight: 1 }}>
          ?
        </span>
      </button>
    </div>
  );
}
