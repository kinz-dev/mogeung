/**
 * Info, under the queue.
 *
 * It belongs here rather than in the bottom dock because of what it is *about*:
 * every other reference view answers a question about the repository or about
 * every session at once, and this one answers "what is the row I just clicked".
 * Putting it directly under the list you clicked in means selecting a session
 * and reading about it are the same glance, with no pane coming forward and
 * nothing else going away.
 *
 * Collapsed it is a header, never nothing — the rule the queue strip, the rail
 * strip and the bottom dock all follow.
 */

import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useStore } from "@/store";
import { useChord } from "@/lib/keymap";
import { ZoomPane } from "@/ui/ZoomPane";
import { InfoPane } from "@/panes/InfoPane";

const MIN_HEIGHT = 100;

export function InfoDock() {
  const chord = useChord("info.toggle");
  const open = useStore((s) => s.prefs.infoOpen);
  const stored = useStore((s) => s.prefs.infoHeight);
  const setPrefs = useStore((s) => s.setPrefs);
  const [height, setHeight] = useState(stored);
  const heightRef = useRef(height);
  heightRef.current = height;

  useEffect(() => setHeight(stored), [stored]);

  const onDrag = (e: React.MouseEvent) => {
    const startY = e.clientY;
    const startH = heightRef.current;
    const move = (ev: MouseEvent) =>
      // Bounded against the *window*, not the column, so dragging it up can
      // never leave the queue with no rows — the queue is the reason this
      // application exists and must not be squeezed to nothing by furniture.
      setHeight(Math.min(window.innerHeight - 260, Math.max(MIN_HEIGHT, startH - (ev.clientY - startY))));
    const up = () => {
      setPrefs({ infoHeight: heightRef.current });
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  return (
    <div className="flex shrink-0 flex-col border-t border-[var(--border)]">
      {open && (
        <div
          onMouseDown={onDrag}
          className="h-1 shrink-0 cursor-row-resize hover:bg-[var(--blue)]"
          title="drag to resize"
        />
      )}

      <button
        type="button"
        onClick={() => setPrefs({ infoOpen: !open })}
        aria-expanded={open}
        title={`${open ? "collapse Info" : "show Info about the selected session"}${chord ? `  (${chord})` : ""}`}
        className="flex h-6 shrink-0 items-center gap-2 px-2 text-left outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:bg-[var(--state-hover)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
      >
        <span className="text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">Info</span>
        <span className="ml-auto text-[var(--dim)]">
          {open ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
        </span>
      </button>

      {open && (
        <div className="min-h-0 bg-[var(--bg-panel)]" style={{ height }}>
          <ZoomPane name="info">
            <InfoPane />
          </ZoomPane>
        </div>
      )}
    </div>
  );
}
