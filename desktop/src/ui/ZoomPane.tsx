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
 *
 * One exception, and it is the Code pane: Monaco already scales everything it
 * draws from its own `fontSize`, and it measures in device pixels to map a
 * click to a character. So it takes the factor as a font size and this wrapper
 * keeps its hands off the CSS — `scale={false}`.
 */

import { useEffect, useRef, type ReactNode } from "react";
import { useStore } from "@/store";

export function ZoomPane({
  name,
  scale = true,
  children,
}: {
  name: string;
  /**
   * Whether this wrapper applies the factor itself.
   *
   * `false` for a pane that scales its own content — the Code pane hands the
   * factor to Monaco as a font size. CSS `zoom` over Monaco is not merely
   * redundant there, it is wrong twice: the factor would apply on top of the
   * font size for a compounded scale, and Monaco measures in device pixels to
   * map a click to a character, which a scaling context throws off.
   */
  scale?: boolean;
  children: ReactNode;
}) {
  const zoom = useStore((s) => s.prefs.zoom[name] ?? 1);
  const zoomPane = useStore((s) => s.zoomPane);
  const hostRef = useRef<HTMLDivElement>(null);

  /**
   * A **native** listener, registered `{ passive: false }` and in the
   * **capture** phase.
   *
   * Two separate reasons, both learned from a Ctrl+wheel that did nothing:
   *
   * React attaches `onWheel` at the root as a passive listener, so
   * `preventDefault()` inside a synthetic wheel handler is silently ignored —
   * the page keeps scrolling and the zoom never takes.
   *
   * And bubbling is not enough over a pane that handles its own scrolling.
   * Monaco's scrollable element consumes the wheel and calls `stopPropagation`,
   * so an event over the editor never reached this host at all — which is why
   * the Code pane, alone among the panes, could not be zoomed. Capture runs
   * root-first, so the modifier case is claimed before the editor sees it.
   */
  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      // Only with a modifier: a bare wheel scrolls, and a pane that zoomed on
      // an ordinary scroll would be unusable. Untouched otherwise — capture
      // means every scroll in the pane passes through here first, and a stray
      // `preventDefault` would break scrolling everywhere at once.
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      // Claimed, so the content below does not also act on it. Monaco reads a
      // Ctrl+wheel as a scroll (or, with `mouseWheelZoom`, as a zoom of its own
      // that nothing here would remember).
      e.stopPropagation();
      // A fixed ratio rather than one derived from `deltaY`: trackpads and
      // wheels report wildly different magnitudes, and scaling by the raw value
      // sends a trackpad past the clamp in a single gesture.
      zoomPane(name, e.deltaY < 0 ? 1.1 : 1 / 1.1);
    };
    el.addEventListener("wheel", onWheel, { passive: false, capture: true });
    return () => el.removeEventListener("wheel", onWheel, { capture: true });
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
      style={zoom === 1 || !scale ? undefined : ({ zoom } as React.CSSProperties)}
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
