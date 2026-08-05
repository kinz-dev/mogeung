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
import { isTauri, setWebviewZoom } from "@/lib/tauri";

const MIN = 0.6;
const MAX = 2.0;
const STEP = 1.1;

/** The CSS fallback: zoom the document, with everything that costs. */
function applyCssZoom(zoom: number): void {
  // On the root element rather than `body`: portalled content — tooltips, the
  // palette — hangs off `body`, and zooming only `body` would leave them at a
  // different scale from the window they belong to.
  (document.documentElement.style as unknown as { zoom: string }).zoom = String(zoom);
}

/**
 * Apply the stored factor. Called on change and at startup.
 *
 * **The webview's own zoom where there is one, not CSS.** This shipped as a
 * `zoom` on the document root, and that is what made *"I can't select text in
 * the Agent pane correctly"* a defect rather than a preference: a CSS `zoom`
 * scales layout but leaves `MouseEvent.clientX` and `getBoundingClientRect()`
 * reported in two different spaces, so anything mapping a pointer to a cell
 * drifts — further the further you drag. xterm measures a character cell in
 * device pixels and is exactly that.
 *
 * It is the third fix for this and the first at the right layer. `R-B30` gave
 * the terminal its own font-size zoom so a *pane* factor would not scale it;
 * the Ctrl+wheel fix on 2026-08-05 let it declare `data-owns-zoom` so the pane
 * wrapper would keep its hands off. Neither could help, because this factor is
 * applied above every pane, to the document itself — and a pane cannot opt out
 * of an ancestor it does not have.
 *
 * The webview's zoom is applied by the compositor, so hit-testing scales with
 * the pixels and the two spaces stay one. The CSS path remains for the browser
 * dev tab, which has no webview to ask — and no pty either, so no terminal is
 * rendered there to be skewed by it.
 */
export function applyAppZoom(zoom: number): void {
  if (!isTauri()) {
    applyCssZoom(zoom);
    return;
  }
  void setWebviewZoom(zoom).catch((e) => {
    // Degrade rather than refuse to zoom: a window stuck at one size is worse
    // than a terminal whose selection drifts, and `Terminal.tsx` undoes a CSS
    // zoom it can see anyway. Loud in the console because it means the
    // capability is missing, which is a build problem rather than a user one.
    console.error("webview zoom unavailable, falling back to CSS zoom:", e);
    applyCssZoom(zoom);
  });
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
