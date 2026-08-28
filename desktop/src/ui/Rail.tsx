/**
 * The right tool-window rail. `R-B40`, stacked since `R-J33`.
 *
 * Chrome, not a pane — [ADR-0017](../../docs/decisions/0017-the-rail-is-chrome.md).
 * The strip is always present: a panel you can lose entirely is one you have to
 * rediscover, which is the rule the Attention strip already follows on the
 * other edge.
 *
 * The strip keeps the outermost edge and the open tools sit inboard of it. The
 * other way round, opening a tool slides the strip sideways and moves the very
 * button you are about to press to close it.
 *
 * **More than one tool at a time, since 2026-08-19.** ADR-0017 wrote down that
 * the rail showed one and named this as the thing to revisit if two were
 * genuinely wanted at once; they were, and
 * [ADR-0027](../../docs/decisions/0027-the-rail-stacks.md) is the answer. They
 * stack down one column rather than splitting the rail in two, because the rail
 * is 300px wide and half of that is not a file tree. Order is the strip's
 * order, not the order you opened them in — see `RAIL_TOOLS`.
 */

import * as React from "react";
import { Fragment, useEffect, useRef, useState } from "react";
import { Bookmark, Folder, MessageSquare, NotebookPen, Search, X } from "lucide-react";
import { useStore } from "@/store";
import { useChord } from "@/lib/keymap";
import { RAIL_TOOLS, type RailTool } from "@/store/prefs";
import { closeRail, toggleRail } from "@/lib/rail";
import { IconButton, Tooltip } from "@/ui/primitives";
import { FilesTool } from "@/ui/tools/FilesTool";
import { SearchTool } from "@/ui/tools/SearchTool";
import { NotesTool } from "@/ui/tools/NotesTool";
import { BookmarksTool } from "@/ui/tools/BookmarksTool";
import { ChatTool } from "@/ui/tools/ChatTool";
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
const TOOLS: Record<RailTool, { label: string; icon: typeof Folder }> = {
  files: { label: "Files", icon: Folder },
  search: { label: "Search", icon: Search },
  notes: { label: "Notes", icon: NotebookPen },
  bookmarks: { label: "Bookmarks", icon: Bookmark },
  // *Ask Mogeung* rather than *Chat*, asked for 2026-08-28. "Chat" names the
  // widget; this names what it is for, and in a rail whose other four headers
  // are nouns for things you already have, the one that answers questions is
  // worth spelling as an invitation.
  chat: { label: "Ask Mogeung", icon: MessageSquare },
};

const BODIES: Record<RailTool, React.FunctionComponent> = {
  files: FilesTool,
  search: SearchTool,
  notes: NotesTool,
  bookmarks: BookmarksTool,
  chat: ChatTool,
};

/**
 * The shortest a stacked tool may be dragged, in pixels: its header and enough
 * body to read a row. Squeezing one to nothing looks exactly like closing it,
 * except that the strip icon is still lit — which is the state where you stop
 * trusting the panel.
 */
const MIN_SECTION = 96;

export function Rail() {
  // One call per tool rather than a hook in the loop below — same four, in
  // fixed order, and lint will not have to take that on trust.
  const chords: Record<RailTool, string> = {
    files: useChord("rail.files"),
    search: useChord("rail.search"),
    notes: useChord("rail.notes"),
    bookmarks: useChord("rail.bookmarks"),
    chat: useChord("rail.chat"),
  };
  const open = useStore((s) => s.prefs.rail);
  const railWidth = useStore((s) => s.prefs.railWidth);
  const railSizes = useStore((s) => s.prefs.railSizes);
  const setPrefs = useStore((s) => s.setPrefs);
  const [width, setWidth] = useState(railWidth);
  const widthRef = useRef(width);
  widthRef.current = width;
  // The stack's weights, mirrored while a divider is under the pointer. Same
  // rule as the width below: the store is written on pointer-up, not on every
  // frame of the drag.
  const [sizes, setSizes] = useState(railSizes);
  // Measured at the moment a divider is grabbed, so the pixels-to-weight
  // conversion does not have to re-read the DOM sixty times a second.
  const sections = useRef<Partial<Record<RailTool, HTMLDivElement | null>>>({});

  useEffect(() => setWidth(railWidth), [railWidth]);
  useEffect(() => setSizes(railSizes), [railSizes]);

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

  const weight = (tool: RailTool) => sizes[tool] ?? 1;

  /**
   * Drag the line between two stacked tools.
   *
   * Only those two move. The alternative — pushing the change down the stack —
   * means the divider you are holding is not the only thing that answers to
   * you, and with three tools open the bottom one shrinks for a drag it had
   * nothing to do with.
   */
  const onSplit = (above: RailTool, below: RailTool) => (e: React.MouseEvent) => {
    e.preventDefault();
    const top = sections.current[above];
    const bottom = sections.current[below];
    if (!top || !bottom) return;
    const startY = e.clientY;
    const topPx = top.getBoundingClientRect().height;
    const px = topPx + bottom.getBoundingClientRect().height;
    const share = weight(above) + weight(below);
    // A pair with no room for two minimums cannot be dragged at all, and the
    // clamp below would invert without this.
    if (px < MIN_SECTION * 2) return;
    // Carried in the closure rather than read back off a ref at the end. The
    // ref is only as fresh as the last render, and a drag that begins and ends
    // inside one frame — a flick, or a click on the divider that moves a pixel
    // — would then save the weights it started with.
    let latest = sizes;
    const move = (ev: MouseEvent) => {
      const wantPx = Math.min(px - MIN_SECTION, Math.max(MIN_SECTION, topPx + (ev.clientY - startY)));
      const w = (wantPx / px) * share;
      latest = { ...latest, [above]: w, [below]: share - w };
      setSizes(latest);
    };
    const up = () => {
      setPrefs({ railSizes: latest });
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const show = (tool: RailTool) => setPrefs({ rail: toggleRail(open, tool) });

  return (
    <div className="flex shrink-0">
      {open.length > 0 && (
        <>
          <div onMouseDown={onDrag} className="w-1 cursor-col-resize hover:bg-[var(--blue)]" title="drag to resize" />
          <div className="flex min-h-0 flex-col border-l border-[var(--border)]" style={{ width }}>
            {open.map((tool, i) => {
              const Body = BODIES[tool];
              const hint = chords[tool] ? `  (${chords[tool]})` : "";
              return (
                <Fragment key={tool}>
                  {i > 0 && (
                    <div
                      onMouseDown={onSplit(open[i - 1], tool)}
                      className="h-1 shrink-0 cursor-row-resize bg-[var(--border)] hover:bg-[var(--blue)]"
                      title="drag to resize"
                      data-testid={`rail-split-${tool}`}
                    />
                  )}
                  <div
                    ref={(el) => {
                      sections.current[tool] = el;
                    }}
                    className="flex min-h-0 flex-col"
                    // `flex-basis: 0` and not `auto`: with `auto` a tool whose
                    // content is tall claims that height first and the weights
                    // only divide what is left, so two equal weights are not
                    // two equal halves.
                    style={{ flexGrow: weight(tool), flexBasis: 0 }}
                  >
                    <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
                      {/* A heading rather than a styled span: the rail holds
                          several of these now, and the only thing telling you
                          where one tool ends and the next begins is this line. */}
                      <h2 className="text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
                        {TOOLS[tool].label}
                      </h2>
                      <div className="ml-auto">
                        <IconButton
                          title={`close ${TOOLS[tool].label}${hint}`}
                          onClick={() => setPrefs({ rail: closeRail(open, tool) })}
                        >
                          <X size={14} />
                        </IconButton>
                      </div>
                    </div>
                    <ZoomPane name={`rail:${tool}`}>
                      <div className="flex h-full min-h-0 flex-col">
                        <Body />
                      </div>
                    </ZoomPane>
                  </div>
                </Fragment>
              );
            })}
          </div>
        </>
      )}

      <div className="flex w-[30px] shrink-0 flex-col items-center gap-2 border-l border-[var(--border)] py-2">
        {RAIL_TOOLS.map((t) => {
          const Icon = TOOLS[t].icon;
          const hint = `${TOOLS[t].label}${chords[t] ? `  (${chords[t]})` : ""}`;
          return (
            <Tooltip key={t} content={hint}>
              <IconButton title={hint} active={open.includes(t)} onClick={() => show(t)}>
                <Icon size={14} />
              </IconButton>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}
