/**
 * The window's own zoom. `R-B30`'s global half.
 *
 * A Tauri webview has no browser zoom: `Ctrl+=` does nothing, which is why
 * "I can't resize the screen" was a bug report rather than a preference. So the
 * window provides it, and — unlike the browser's — it is remembered.
 *
 * Per-*pane* zoom lives in `ui/ZoomPane.tsx`. The two multiply, deliberately:
 * one sets how big this machine's screen wants everything, the other says this
 * particular diff is dense today.
 */

import { useStore } from "@/store";

const MIN = 0.6;
const MAX = 2.0;
const STEP = 1.1;

/** Apply the stored factor to the document. Called on change and at startup. */
export function applyAppZoom(zoom: number): void {
  // On the root element rather than `body`: portalled content — tooltips, the
  // palette — hangs off `body`, and zooming only `body` would leave them at a
  // different scale from the window they belong to.
  (document.documentElement.style as unknown as { zoom: string }).zoom = String(zoom);
}

export function nudgeAppZoom(direction: 1 | -1 | 0): void {
  const { prefs, setPrefs } = useStore.getState();
  const next =
    direction === 0
      ? 1
      : Math.min(MAX, Math.max(MIN, prefs.appZoom * (direction > 0 ? STEP : 1 / STEP)));
  // Snap near-1 back to exactly 1, so "reset" is reachable by stepping as well
  // as by pressing the reset key.
  const snapped = Math.abs(next - 1) < 0.02 ? 1 : next;
  setPrefs({ appZoom: snapped });
  applyAppZoom(snapped);
}
