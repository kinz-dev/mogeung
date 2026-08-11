/**
 * The bottom dock: reference material, out of the way until wanted.
 *
 * It started as the three you *consult* — Insight, Git and Debt — against a
 * centre holding what you were *doing*: the diff, the conversation, the file,
 * the agent. Making them tabs in one strip meant every consultation cost you
 * the thing you were reading.
 *
 * **Changes and Transcript moved here on 2026-08-06**, at an explicit ask, and
 * the line the dock was built on moved with them: the centre is now the file
 * and the agent, and everything you read *about* a session is down here. The
 * price is that the dock shows one tool at a time, so the diff and the
 * conversation can no longer sit side by side — the tile tree allowed that and
 * this does not.
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
import { useChord } from "@/lib/keymap";
import type { DockTool } from "@/store/prefs";
import { IconButton } from "@/ui/primitives";
import { ZoomPane } from "@/ui/ZoomPane";
import { cn } from "@/lib/cn";
import { InsightPane } from "@/panes/InsightPane";
import { GitPane } from "@/panes/GitPane";
import { DebtPane } from "@/panes/DebtPane";
import { ChangesPane } from "@/panes/ChangesPane";
import { TranscriptPane } from "@/panes/TranscriptPane";
import { RunPane } from "@/panes/RunPane";

/**
 * The order of the strip, and of the chords: `Alt+2` to `Alt+8` run left to
 * right, and Git sits at the far right on `Alt+9`. Changes and Transcript lead
 * because they are what you open the dock *for*; the rest is what you consult
 * afterwards, and Git is furthest from the ones reached most.
 *
 * **`Run` took `Alt+4` on 2026-08-11 and Insight moved to `Alt+8`**, asked for
 * directly. It is the right way round: a run is checked against the session in
 * front of you, where Insight is the across-everything view you go to
 * deliberately — so the cheaper chord belongs to the thing reached mid-flow.
 * Insight moved **in the strip** as well, and had to: a test asserts the drawn
 * order and the chord order are the same list, because a strip whose numbers
 * jump about is one you read instead of reach for.
 *
 * **The chord is not written here.** It was, in each `hint`, and a Mac reads
 * `⌘2` where this said `Alt+2` (`R-J19`) — as would anyone who had rebound
 * one. `useChord` appends the live binding instead, so the order this comment
 * describes is asserted by a test against the keymap rather than by two
 * strings agreeing.
 */
export const DOCK_TOOLS: { id: DockTool; label: string; hint: string }[] = [
  { id: "changes", label: "Changes", hint: "what this session changed, risk-ordered, with read marks" },
  { id: "transcript", label: "Transcript", hint: "the conversation, turn by turn" },
  // `R-N5`, moved here 2026-08-11 from the centre. A run is something you
  // *consult against* the code you are reading — the same argument `R-B45`
  // made when it moved Changes and Transcript down — and in the centre it was
  // competing for room with the agent it exists to check.
  { id: "run", label: "Run", hint: "run this repository's own tests and builds, and watch" },
  { id: "debt", label: "Debt", hint: "how much of this repo's agent output nobody has read" },
  { id: "insight", label: "Insight", hint: "across every session — search, analytics, digest, docs" },
  { id: "git", label: "Git", hint: "commits, changes and diffs of this session's repo" },
];

const MIN_HEIGHT = 140;

export function BottomDock() {
  // One per tool, in the strip's own order — a hook cannot go in the map.
  const chords: Record<DockTool, string> = {
    changes: useChord("dock.changes"),
    transcript: useChord("dock.transcript"),
    run: useChord("dock.run"),
    debt: useChord("dock.debt"),
    insight: useChord("dock.insight"),
    git: useChord("dock.git"),
  };
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
              {dock === "changes" && <ChangesPane />}
              {dock === "transcript" && <TranscriptPane />}
              {dock === "git" && <GitPane />}
              {dock === "run" && <RunPane />}
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
            title={`${t.hint}${chords[t.id] ? `  (${chords[t.id]})` : ""}`}
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
