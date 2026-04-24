# Tutorial Overlay Instructions

This folder contains the first-visit tutorial overlay for the factory scene.

## What is wired

- `TutorialOverlay.tsx`
  - Renders the guided overlay and spotlight highlight.
  - Shows once per browser via localStorage key: `dr-fraudsworth-tutorial-complete-v1`.
  - Can be toggled on/off at runtime from the top-left `?` toolbar button.
- `tutorial-copy.ts`
  - Single source of truth for tutorial copy.
  - Update text here instead of editing component logic.

## Where targets come from

- Desktop targets are tagged in `SceneStation.tsx` with:
  - `data-tutorial-id="station-<stationId>"`
- Mobile targets are tagged in `MobileNav.tsx` with:
  - `data-tutorial-id="mobile-station-<stationId>"`

The selectors in `tutorial-copy.ts` must match those attributes.

## Common edits

### Update tutorial text

Edit:
- `TUTORIAL_INTRO_TITLE`
- `TUTORIAL_INTRO_SUBTITLE`
- `STEP_COPY`

in `tutorial-copy.ts`.

### Reorder steps

Adjust the order of:
- `DESKTOP_TUTORIAL_STEPS`
- `MOBILE_TUTORIAL_STEPS`

in `tutorial-copy.ts`.

### Tune spotlight shape

`TutorialOverlay.tsx` includes per-step highlight adjustments:
- wallet step: reduced height, bottom-aligned
- rewards step: reduced height, top-aligned

Edit `getAdjustedHighlightRect()` to tweak this behavior.

## QA checklist

1. Open `/` and click the `?` button in the top-left toolbar.
2. Verify all 6 steps render and spotlight the correct section.
3. Verify Step 2 warning block shows icon + highlighted style.
4. Verify intro title/subtitle style and placement on Step 1.
5. Verify tooltip flips above when below would overflow.
