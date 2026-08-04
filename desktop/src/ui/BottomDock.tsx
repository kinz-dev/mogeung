/**
 * The bottom dock: reference material, out of the way until wanted.
 *
 * The centre is for what you are *doing* — the diff, the conversation, the file,
 * the agent. Insight, Git and Debt are what you *consult*: you open one, read
 * it, and go back. Making them tabs in the same strip as the Transcript meant
 * every consultation cost you the thing you were reading.
 *
 * Info is **not** here, and the reason is worth keeping: these three answer a
 * question about the repository or about every session at once, where Info
 * answers "what is the row I just clicked". It lives under the queue instead —
 * see `InfoDock`.
 *
 * Same construction as the right rail, turned on its side, and for the same
 * reason ([ADR-0017](../../docs/decisions/0017-the-rail-is-chrome.md)): this is
 * **chrome**, not a pane. Collapsed it is a strip of names above the status bar
 * — never nothing, because a dock you can lose entirely is one you have to
 * rediscover.
 */

import { useEffect, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";
import { useStore } from "@/store";
import type { DockTool } from "@/store/prefs";
import { IconButton } from "@/ui/primitives";
import { ZoomPane } from "@/ui/ZoomPane";
import { cn } from "@/lib/cn";
import { InsightPane } from "@/panes/InsightPane";
import { GitPane } from "@/panes/GitPane";
import { DebtPane } from "@/panes/DebtPane";

export const DOCK_TOOLS: { id: DockTool; label: string; hint: string }[] = [
  { id: "git", label: "Git", hint: "commits, changes and diffs of this session's repo" },
  { id: "insight", label: "Insight", hint: "across every session — search, analytics, digest, docs" },
  { id: "debt", label: "Debt", hint: "how much of this repo's agent output nobody has read" },
];

const MIN_HEIGHT = 140;

export function BottomDock() {
  const dock = useStore((s) => s.prefs.dock);
  const stored = useStore((s) => s.prefs.dockHeight);
  const setPrefs = useStore((s) => s.setPrefs);
  const [height, setHeight] = useState(stored);
  const heightRef = useRef(height);
  heightRef.current = height;

  useEffect(() => setHeight(stored), [stored]);

  const onDrag = (e: React.MouseEvent) => {
    const startY = e.clientY;
    const startH = heightRef.current;
    const move = (ev: MouseEvent) =>
      setHeight(Math.min(window.innerHeight - 220, Math.max(MIN_HEIGHT, startH - (ev.clientY - startY))));
    const up = () => {
      // Written on pointer-up only, the rule every draggable edge here follows.
      setPrefs({ dockHeight: heightRef.current });
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const show = (tool: DockTool) => setPrefs({ dock: dock === tool ? null : tool });

  return (
    <>
      {dock && (
        <div className="flex shrink-0 flex-col border-t border-[var(--border)]" style={{ height }}>
          <div
            onMouseDown={onDrag}
            className="h-1 shrink-0 cursor-row-resize hover:bg-[var(--blue)]"
            title="drag to resize"
          />
          <div className="min-h-0 flex-1 bg-[var(--bg-panel)]">
            {/* Keyed per tool so each keeps its own zoom, and so switching
                tools does not hand one pane's scroll position to another. */}
            <ZoomPane name={`dock:${dock}`}>
              {dock === "git" && <GitPane />}
              {dock === "insight" && <InsightPane />}
              {dock === "debt" && <DebtPane />}
            </ZoomPane>
          </div>
        </div>
      )}

      {/*
        The strip. Always present, above the status bar and not in it — the
        status bar describes the selection, and a row of buttons among that
        would make both harder to read.
      */}
      <div className="flex h-6 shrink-0 items-center gap-0.5 border-t border-[var(--border)] px-1">
        {DOCK_TOOLS.map((t) => (
          <button
            key={t.id}
            type="button"
            title={t.hint}
            aria-pressed={dock === t.id}
            onClick={() => show(t.id)}
            className={cn(
              "rounded-sm px-2 py-0.5 text-2xs outline-none",
              "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
              "focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2",
              dock === t.id
                ? "bg-[var(--state-focus)] text-[var(--text-strong)]"
                : "text-[var(--dim)] hover:bg-[var(--state-hover)] hover:text-[var(--text)]",
            )}
          >
            {t.label}
          </button>
        ))}
        {dock && (
          <div className="ml-auto">
            <IconButton title="collapse the dock" onClick={() => setPrefs({ dock: null })}>
              <ChevronDown size={12} />
            </IconButton>
          </div>
        )}
      </div>
    </>
  );
}
