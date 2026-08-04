/**
 * Per-pane zoom. `R-B30`.
 *
 * Ctrl+wheel over a pane scales **that pane**, and the level is remembered per
 * pane in the preferences. The reason it is per pane rather than per window:
 * a diff and a transcript are read at different sizes by the same person in the
 * same minute, and one factor over everything means making code bigger also
 * fattens every row of the queue.
 *
 * A Tauri webview has no browser zoom of its own — `Ctrl+=` does nothing there,
 * which is what made this a bug report rather than a preference. So the window
 * has to provide both: this, and the global factor in `lib/zoom.ts`.
 *
 * Implemented with CSS `zoom` rather than a font-size cascade, because the type
 * scale is in pixels: scaling the text alone would leave every padding, border
 * and control height at its original size and the pane would come apart. `zoom`
 * scales the layout, which is what "make this bigger" actually means.
 */

import { useEffect, useRef, type ReactNode } from "react";
import { useStore } from "@/store";

export function ZoomPane({ name, children }: { name: string; children: ReactNode }) {
  const zoom = useStore((s) => s.prefs.zoom[name] ?? 1);
  const zoomPane = useStore((s) => s.zoomPane);
  const hostRef = useRef<HTMLDivElement>(null);

  /**
   * A **native** listener, registered `{ passive: false }`.
   *
   * React attaches `onWheel` at the root as a passive listener, so
   * `preventDefault()` inside a synthetic wheel handler is silently ignored —
   * the page keeps scrolling and the zoom never takes. That is why Ctrl+wheel
   * appeared to do nothing: the handler was running perfectly and the browser
   * was ignoring the one thing it asked for.
   */
  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      // Only with a modifier: a bare wheel scrolls, and a pane that zoomed on
      // an ordinary scroll would be unusable.
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      // A fixed ratio rather than one derived from `deltaY`: trackpads and
      // wheels report wildly different magnitudes, and scaling by the raw value
      // sends a trackpad past the clamp in a single gesture.
      zoomPane(name, e.deltaY < 0 ? 1.1 : 1 / 1.1);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [name, zoomPane]);

  return (
    <div
      ref={hostRef}
      // `flex-1 w-full min-w-0` is load-bearing, not decoration. The queue is a
      // fixed-width flex child whose *content* used to be this wrapper — and a
      // plain `div` in a flex row sizes to its content, so dragging the panel
      // wider grew the container and left the rows at their old width with a
      // band of empty panel beside them.
      className="relative h-full min-h-0 w-full min-w-0 flex-1"
      // Only when it is actually scaled. `zoom: 1` is not quite a no-op — it
      // still establishes a scaling context, and anything that measures itself
      // in device pixels (xterm sizing a character cell) can be thrown by it.
      // Absent is cheaper and safer than "present but neutral".
      style={zoom === 1 ? undefined : ({ zoom } as React.CSSProperties)}
    >
      {children}
      {Math.abs(zoom - 1) > 0.01 && (
        <button
          type="button"
          onClick={() => zoomPane(name, 1 / zoom)}
          title="reset this pane's zoom"
          className="absolute right-1 bottom-1 z-10 rounded-sm border border-[var(--border)] bg-[var(--bg-raised)] px-1 text-2xs text-[var(--dim)] outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
        >
          {Math.round(zoom * 100)}%
        </button>
      )}
    </div>
  );
}
