/**
 * The right tool-window rail. `R-B40`.
 *
 * Chrome, not a pane — [ADR-0017](../../docs/decisions/0017-the-rail-is-chrome.md).
 * The strip is always present: a panel you can lose entirely is one you have to
 * rediscover, which is the rule the Attention strip already follows on the
 * other edge.
 *
 * The strip keeps the outermost edge and the open tool sits inboard of it. The
 * other way round, opening a tool slides the strip sideways and moves the very
 * button you are about to press to close it.
 */

import { useEffect, useRef, useState } from "react";
import { Bookmark, ChevronRight, Folder, NotebookPen, Search } from "lucide-react";
import { useStore } from "@/store";
import { useChord } from "@/lib/keymap";
import type { RailTool } from "@/store/prefs";
import { IconButton, Tooltip } from "@/ui/primitives";
import { FilesTool } from "@/ui/tools/FilesTool";
import { SearchTool } from "@/ui/tools/SearchTool";
import { NotesTool } from "@/ui/tools/NotesTool";
import { BookmarksTool } from "@/ui/tools/BookmarksTool";
import { ZoomPane } from "@/ui/ZoomPane";

/**
 * The chord comes from the keymap rather than from a string here. `R-J19`.
 *
 * These four said `Alt+4`–`Alt+7`, which had not been the rail's chords since
 * `R-B47` moved the digits to the dock strip — the tooltips had been naming
 * the *dock's* keys for two days, and a tooltip that gives you the wrong key
 * is worse than one that gives you none. Reading `ACTIONS` also means they
 * follow a rebind, and say `⌘F` rather than `Alt+F` on a Mac.
 */
const TOOLS: { id: RailTool; label: string; icon: typeof Folder }[] = [
  { id: "files", label: "Files", icon: Folder },
  { id: "search", label: "Search", icon: Search },
  { id: "notes", label: "Notes", icon: NotebookPen },
  { id: "bookmarks", label: "Bookmarks", icon: Bookmark },
];

export function Rail() {
  // One call per tool rather than a hook in the loop below — same four, in
  // fixed order, and lint will not have to take that on trust.
  const chords: Record<RailTool, string> = {
    files: useChord("rail.files"),
    search: useChord("rail.search"),
    notes: useChord("rail.notes"),
    bookmarks: useChord("rail.bookmarks"),
  };
  const rail = useStore((s) => s.prefs.rail);
  const railWidth = useStore((s) => s.prefs.railWidth);
  const setPrefs = useStore((s) => s.setPrefs);
  const [width, setWidth] = useState(railWidth);
  const widthRef = useRef(width);
  widthRef.current = width;

  useEffect(() => setWidth(railWidth), [railWidth]);

  const onDrag = (e: React.MouseEvent) => {
    const startX = e.clientX;
    const startW = widthRef.current;
    const move = (ev: MouseEvent) => setWidth(Math.min(760, Math.max(220, startW - (ev.clientX - startX))));
    const up = () => {
      // Written on pointer-up only, so a drag does not save on every frame.
      setPrefs({ railWidth: widthRef.current });
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const show = (tool: RailTool) => setPrefs({ rail: rail === tool ? null : tool });

  return (
    <div className="flex shrink-0">
      {rail && (
        <>
          <div onMouseDown={onDrag} className="w-1 cursor-col-resize hover:bg-[var(--blue)]" title="drag to resize" />
          <div className="flex flex-col border-l border-[var(--border)]" style={{ width }}>
            <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
              <span className="text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
                {TOOLS.find((t) => t.id === rail)?.label}
              </span>
              <div className="ml-auto">
                <IconButton title="collapse to the strip  (])" onClick={() => setPrefs({ rail: null })}>
                  <ChevronRight size={14} />
                </IconButton>
              </div>
            </div>
            <ZoomPane name={`rail:${rail}`}>
              <div className="flex h-full min-h-0 flex-col">
                {rail === "files" && <FilesTool />}
                {rail === "search" && <SearchTool />}
                {rail === "notes" && <NotesTool />}
                {rail === "bookmarks" && <BookmarksTool />}
              </div>
            </ZoomPane>
          </div>
        </>
      )}

      <div className="flex w-[30px] shrink-0 flex-col items-center gap-2 border-l border-[var(--border)] py-2">
        {TOOLS.map((t) => {
          const Icon = t.icon;
          const hint = `${t.label}${chords[t.id] ? `  (${chords[t.id]})` : ""}`;
          return (
            <Tooltip key={t.id} content={hint}>
              <IconButton title={hint} active={rail === t.id} onClick={() => show(t.id)}>
                <Icon size={14} />
              </IconButton>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}
