'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { usePathname } from 'next/navigation';
import {
  DESKTOP_TUTORIAL_STEPS,
  MOBILE_TUTORIAL_STEPS,
  TUTORIAL_INTRO_SUBTITLE,
  TUTORIAL_INTRO_TITLE,
} from '@/components/onboarding/tutorial-copy';

const STORAGE_KEY = 'dr-fraudsworth-tutorial-complete-v1';
const SPLASH_DONE_EVENT = 'drfraudsworth:splash-complete';
const TOGGLE_TUTORIAL_EVENT = 'drfraudsworth:tutorial-toggle';

interface TargetRect {
  top: number;
  left: number;
  width: number;
  height: number;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(value, max));
}

function getAdjustedHighlightRect(
  stepSelector: string,
  rect: TargetRect,
): TargetRect {
  // Step 1 (wallet): reduce highlight height while keeping bottom edge fixed.
  if (stepSelector.includes('station-wallet')) {
    const reducedHeight = rect.height * 0.7;
    return {
      ...rect,
      top: rect.top + (rect.height - reducedHeight),
      height: reducedHeight,
    };
  }

  // Step 4 (rewards/staking): reduce highlight height while keeping top fixed.
  if (stepSelector.includes('station-staking')) {
    return {
      ...rect,
      height: rect.height * 0.7,
    };
  }

  return rect;
}

export function TutorialOverlay() {
  const pathname = usePathname();
  const [isVisible, setIsVisible] = useState(false);
  const [stepIndex, setStepIndex] = useState(0);
  const [targetRect, setTargetRect] = useState<TargetRect | null>(null);
  const [isDesktop, setIsDesktop] = useState(false);
  const [viewport, setViewport] = useState({ width: 1024, height: 768 });

  const steps = useMemo(
    () => (isDesktop ? DESKTOP_TUTORIAL_STEPS : MOBILE_TUTORIAL_STEPS),
    [isDesktop],
  );
  const currentStep = steps[stepIndex];

  useEffect(() => {
    if (typeof window === 'undefined') return;
    const mediaQuery = window.matchMedia('(min-width: 1024px)');
    const syncViewport = () => {
      setViewport({ width: window.innerWidth, height: window.innerHeight });
    };
    const syncViewportMode = () => setIsDesktop(mediaQuery.matches);
    syncViewportMode();
    syncViewport();
    mediaQuery.addEventListener('change', syncViewportMode);
    window.addEventListener('resize', syncViewport);
    return () => {
      mediaQuery.removeEventListener('change', syncViewportMode);
      window.removeEventListener('resize', syncViewport);
    };
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    if (pathname !== '/') {
      return;
    }

    const completed = localStorage.getItem(STORAGE_KEY) === 'true';
    if (completed) return;

    const maybeShow = () => {
      const splashStillMounted = Boolean(document.querySelector('.splash-overlay'));
      const hasTutorialTargets = Boolean(
        document.querySelector('[data-tutorial-id="station-wallet"], [data-tutorial-id="mobile-station-wallet"]'),
      );
      if (!splashStillMounted && hasTutorialTargets) {
        setIsVisible(true);
      }
    };

    const onSplashDone = () => maybeShow();
    window.addEventListener(SPLASH_DONE_EVENT, onSplashDone);

    // Fallback if splash event is unavailable (e.g. no splash in some flows).
    const timer = window.setTimeout(maybeShow, 2600);

    return () => {
      window.removeEventListener(SPLASH_DONE_EVENT, onSplashDone);
      window.clearTimeout(timer);
    };
  }, [pathname]);

  useEffect(() => {
    if (typeof window === 'undefined') return;

    const onToggleTutorial = () => {
      if (pathname !== '/') return;
      if (isVisible) {
        setIsVisible(false);
        return;
      }
      setStepIndex(0);
      setIsVisible(true);
    };

    window.addEventListener(TOGGLE_TUTORIAL_EVENT, onToggleTutorial);
    return () => window.removeEventListener(TOGGLE_TUTORIAL_EVENT, onToggleTutorial);
  }, [isVisible, pathname]);

  const markComplete = useCallback(() => {
    if (typeof window !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, 'true');
    }
    setIsVisible(false);
  }, []);

  useEffect(() => {
    if (!isVisible || !currentStep || typeof window === 'undefined') {
      return;
    }

    const syncTarget = () => {
      const element = document.querySelector<HTMLElement>(currentStep.selector);
      if (!element) {
        setTargetRect(null);
        return;
      }

      element.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'center' });
      const rect = element.getBoundingClientRect();
      setTargetRect({
        top: rect.top,
        left: rect.left,
        width: rect.width,
        height: rect.height,
      });
    };

    syncTarget();
    window.addEventListener('resize', syncTarget);
    window.addEventListener('scroll', syncTarget, true);

    return () => {
      window.removeEventListener('resize', syncTarget);
      window.removeEventListener('scroll', syncTarget, true);
    };
  }, [currentStep, isVisible]);

  if (!isVisible || pathname !== '/' || !currentStep) return null;

  const panelWidth = 340;
  const panelHeight = currentStep.warning ? 290 : 220;
  const panelGap = 16;
  const targetCenterX = targetRect ? targetRect.left + targetRect.width / 2 : viewport.width / 2;
  const panelLeft = clamp(
    targetCenterX - panelWidth / 2,
    16,
    Math.max(16, viewport.width - panelWidth - 16),
  );
  const panelTop = targetRect
    ? (() => {
        const belowTop = targetRect.top + targetRect.height + panelGap;
        const aboveTop = targetRect.top - panelHeight - panelGap;
        const belowOverflows = belowTop + panelHeight > viewport.height - 16;
        if (belowOverflows && aboveTop >= 16) {
          return aboveTop;
        }
        return clamp(belowTop, 16, Math.max(16, viewport.height - panelHeight - 16));
      })()
    : 24;

  return (
    <div className="tutorial-overlay" role="dialog" aria-modal="true" aria-label="Site tutorial">
      {stepIndex === 0 ? (
        <div className="tutorial-intro-overlay" aria-label="Tutorial introduction">
          <p className="tutorial-intro-title">{TUTORIAL_INTRO_TITLE}</p>
          <p className="tutorial-intro-subtitle">{TUTORIAL_INTRO_SUBTITLE}</p>
        </div>
      ) : null}
      {targetRect ? (() => {
        const adjustedRect = getAdjustedHighlightRect(currentStep.selector, targetRect);
        return (
          <div
            className="tutorial-highlight"
            style={{
              top: `${adjustedRect.top - 8}px`,
              left: `${adjustedRect.left - 8}px`,
              width: `${adjustedRect.width + 16}px`,
              height: `${adjustedRect.height + 16}px`,
            }}
          />
        );
      })() : null}

      <section
        className="tutorial-panel"
        style={{
          top: `${panelTop}px`,
          left: `${panelLeft}px`,
        }}
      >
        <p className="tutorial-step-count">
          Step {stepIndex + 1} of {steps.length}
        </p>
        <h2>{currentStep.title}</h2>
        <p>{currentStep.description}</p>
        {currentStep.warning ? (
          <p className="tutorial-warning" role="note" aria-label="Important warning">
            <span className="tutorial-warning-icon" aria-hidden="true">
              ⚠
            </span>
            <span>{currentStep.warning}</span>
          </p>
        ) : null}

        <div className="tutorial-actions">
          <button
            type="button"
            className="tutorial-btn tutorial-btn-secondary"
            onClick={markComplete}
          >
            Skip
          </button>

          <button
            type="button"
            className="tutorial-btn tutorial-btn-secondary"
            onClick={() => setStepIndex((prev) => Math.max(0, prev - 1))}
            disabled={stepIndex === 0}
          >
            Back
          </button>

          {stepIndex === steps.length - 1 ? (
            <button type="button" className="tutorial-btn tutorial-btn-primary" onClick={markComplete}>
              Finish
            </button>
          ) : (
            <button
              type="button"
              className="tutorial-btn tutorial-btn-primary"
              onClick={() => setStepIndex((prev) => Math.min(steps.length - 1, prev + 1))}
            >
              Next
            </button>
          )}
        </div>
      </section>
    </div>
  );
}
