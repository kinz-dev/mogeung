/**
 * The log: one row per commit — graph, subject with its refs as chips,
 * author, date, and the two marks that are mogeung's. `R-D26`.
 *
 * Virtualised, and the next page is asked for as the end approaches, so
 * there is no *load more*. The graph is computed over every loaded row and
 * hidden whenever a text filter is in force or a client-side toggle narrows
 * the rows — lanes drawn over a subset join dots that are not adjacent, and
 * a lying graph is worse than none (`R-D13`).
 *
 * **Columns have a header, dividers and widths of their own** since the
 * first day of use (asked 2026-09-11: *"make the grid line clearer so that I
 * can resize the column, and make all column resizable"*). The header row
 * sticks above the rows; each divider is dragged the way every other edge in
 * this window is — local state during the drag, the preferences on release.
 *
 * **The four text columns are shares of the width the graph leaves**, drawn
 * as `fr` tracks, so the whole row scales with its pane (asked the same
 * afternoon: *"auto resize all the column when the panel is resize"* — the
 * first cut kept author, date and marks in pixels, gave every resize to the
 * subject alone, and dropped columns below two breakpoints, which is what
 * *strange* meant). A divider moves the boundary between its two neighbours
 * and nothing else: the left column takes what the right one gives up, the
 * row's width is unchanged, and the line follows the pointer. The graph
 * column stays in pixels — its width is a count of lanes, not a share of
 * anything — sized from the lanes until a hand sets it, and then clipped.
 * Floors keep a column from being dragged, or squeezed, out of existence.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useStore } from "@/store";
import { Chip, Dim, Empty } from "@/ui/primitives";
import { ContextMenu, MenuItem, MenuSeparator } from "@/ui/Menu";
import { graphCell, lanes, LANE_PX, ROW_PX } from "@/lib/gitGraph";
import { hidesGraph, patchText } from "@/lib/gitFilter";
import { askLog, copyText, diffAgainstMark, hostLink, markRange, queryOf, selectCommit } from "@/lib/gitActions";
import { showDiffPane } from "@/lib/panes";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import { rowSelected } from "@/ui/styles";
import type { CommitInfo } from "@/wire/types";
import type { LogColumns } from "@/store/prefs";
import type { Only } from "./LogToolbar";

/** How close to the end the scroll gets before the next page is asked for. */
const AHEAD = 15;
/** The header row, which the virtualiser is told about as a scroll margin. */
const HEADER_PX = 22;

/**
 * A column's floor in pixels: neither a divider nor a narrow pane takes it
 * lower. The subject's is the largest — it is what a row is for.
 */
const MIN = { graph: 18, subject: 120, author: 40, date: 64, marks: 24 };

/** The shared columns, left to right; a divider sits between each pair. */
const SHARED = ["subject", "author", "date", "marks"] as const;

/** The column to the right of each divider, keyed by the one on its left. */
const NEXT = { subject: "author", author: "date", date: "marks" } as const;
type Edge = keyof typeof NEXT;

/**
 * A share is a percentage, to two decimals — a pixel in a ten-thousand-pixel
 * pane. Percentages rather than fractions of one because of a grid rule:
 * flex factors that sum to less than 1 fill only that fraction of the free
 * space, and once a column is pinned at its floor the rest sum below 1 —
 * so `0.66fr 0.14fr …` left a 312-px log with 54 px of nothing on the right,
 * where `66fr 14fr …` fills it.
 */
const share = (x: number) => Math.round(x * 100) / 100;

function refChip(r: string) {
  if (r.startsWith("HEAD")) return <Chip key={r} color="var(--blue)">{r.replace("HEAD -> ", "HEAD → ")}</Chip>;
  if (r.startsWith("tag: ")) return <Chip key={r} color="var(--amber)">⌖ {r.slice(5)}</Chip>;
  return <Chip key={r} color="var(--green)">{r}</Chip>;
}

const cell = "flex h-full min-w-0 items-center overflow-hidden border-r border-[var(--border)]";

export function LogTable({ id, only, onEnter }: { id: string; only: Only; onEnter: () => void }) {
  const commits = useStore((s) => s.git[id]?.commits);
  const selected = useStore((s) => s.git[id]?.selected ?? null);
  const done = useStore((s) => s.git[id]?.done ?? false);
  const pending = useStore((s) => s.git[id]?.logPending ?? false);
  const readBySha = useStore((s) => s.git[id]?.readBySha);
  const rangeMark = useStore((s) => s.git[id]?.rangeMark ?? null);
  const diff = useStore((s) => s.git[id]?.diff ?? null);
  const grep = useStore((s) => s.git[id]?.grep ?? "");
  const author = useStore((s) => s.git[id]?.author ?? "");
  const path = useStore((s) => s.git[id]?.path ?? "");
  const pickaxe = useStore((s) => s.git[id]?.pickaxe ?? "");
  const saved = useStore((s) => s.prefs.gitLogColumns);
  const setPrefs = useStore((s) => s.setPrefs);
  const all = commits ?? [];

  const rows = useMemo(
    () => all.filter((c) => (!only.session || c.touches_session) && (!only.read || readBySha?.[c.sha])),
    [all, only.session, only.read, readBySha],
  );
  const showGraph = !only.session && !only.read && !hidesGraph({ ...queryOf(id), grep, author, path, pickaxe });
  const graph = useMemo(() => (showGraph ? lanes(all) : null), [showGraph, all]);

  // The widths on screen: the saved ones, overridden while a divider is
  // being dragged, and written back once on release.
  const [dragging, setDragging] = useState<LogColumns | null>(null);
  const cols = dragging ?? saved;
  const gw = cols.graph ?? (graph ? graph.width * LANE_PX + 6 : 10);

  const parentRef = useRef<HTMLDivElement>(null);
  // The graph in pixels, then each shared column as its percentage of the
  // rest, over its floor — the grid scales the four together as the pane
  // moves, with nothing measured and nothing to do on resize.
  const columns = `${gw}px ${SHARED.map((k) => `minmax(${MIN[k]}px, ${cols[k]}fr)`).join(" ")}`;

  /**
   * A divider is dragged. `at` is the column on its left: the graph's sits on
   * its own right edge and sets its pixels; every other one sits between two
   * shared columns and moves the boundary, the left taking what the right
   * gives up. The width the shares divide is read once, at the grab — a
   * pointer move must not touch the DOM, and a pane does not resize mid-drag.
   */
  const startDrag = (e: React.MouseEvent, at: "graph" | Edge) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const from: LogColumns = { ...saved, graph: gw };
    const span = (parentRef.current?.clientWidth ?? 0) - gw;
    let latest = from;
    const move = (ev: MouseEvent) => {
      const dx = ev.clientX - startX;
      if (at === "graph") {
        latest = { ...from, graph: Math.max(MIN.graph, gw + dx) };
      } else {
        // Unmeasured — nothing to convert pixels against, so nothing moves.
        if (span <= 0) return;
        const right = NEXT[at];
        const pair = from[at] + from[right];
        const pct = (px: number) => (100 * px) / span;
        const l = Math.min(pair - pct(MIN[right]), Math.max(pct(MIN[at]), from[at] + pct(dx)));
        latest = { ...from, [at]: share(l), [right]: share(pair - l) };
      }
      setDragging(latest);
    };
    const up = () => {
      setPrefs({ gitLogColumns: latest });
      setDragging(null);
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_PX,
    overscan: 12,
    scrollMargin: HEADER_PX,
  });
  const items = virt.getVirtualItems();
  const lastIndex = items.length ? items[items.length - 1].index : -1;

  // The end approaching asks for the next page — once, because `logPending`
  // holds until the answer lands, and only while nothing narrows the rows
  // client-side, since then the end on screen is not the end of the log.
  useEffect(() => {
    if (done || pending || all.length === 0) return;
    if (only.session || only.read) return;
    if (lastIndex >= rows.length - AHEAD) askLog(id, all.length);
  }, [lastIndex, rows.length, all.length, done, pending, id, only.session, only.read]);

  // Keep the selected row in view when the keyboard moves it.
  const selectedIndex = selected ? rows.findIndex((c) => c.sha === selected) : -1;
  useEffect(() => {
    if (selectedIndex >= 0) virt.scrollToIndex(selectedIndex, { align: "auto" });
    // The virtualiser instance is stable; listing it would only re-run this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedIndex]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (rows.length === 0) return;
    // Ctrl+D: the selected commit, every file, as a pane in the centre.
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "d") {
      if (selected) {
        e.preventDefault();
        showDiffPane(id, selected, "*");
      }
      return;
    }
    const i = selectedIndex;
    let n: number | null = null;
    if (e.key === "ArrowDown" || e.key === "j") n = Math.min(rows.length - 1, i + 1);
    else if (e.key === "ArrowUp" || e.key === "k") n = Math.max(0, i - 1);
    else if (e.key === "PageDown") n = Math.min(rows.length - 1, i + 10);
    else if (e.key === "PageUp") n = Math.max(0, i - 10);
    else if (e.key === "Home") n = 0;
    else if (e.key === "End") n = rows.length - 1;
    else if (e.key === "Enter" && i >= 0) {
      e.preventDefault();
      onEnter();
      return;
    } else return;
    e.preventDefault();
    if (n !== i) selectCommit(id, rows[n].sha);
  };

  if (all.length === 0) {
    return pending || !done ? (
      <Empty>reading the log…</Empty>
    ) : (
      <Empty hint="Enter runs the query — clear the filters to see the whole log">no commit matches these filters</Empty>
    );
  }
  if (rows.length === 0) {
    return (
      <Empty hint={only.session ? "attribution is a hint: a commit inside this session's lifetime that touches files it edited" : "a commit is read once every hunk of its diff is"}>
        {only.session ? "no commit here is attributed to this session" : "no commit here has been read in full"}
      </Empty>
    );
  }

  const divider = (at: "graph" | Edge, title: string) => (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={title}
      title={title}
      onMouseDown={(e) => startDrag(e, at)}
      className="absolute top-0 -right-0.5 z-10 h-full w-1 cursor-col-resize hover:bg-[var(--blue)]"
    />
  );

  return (
    <div
      ref={parentRef}
      role="listbox"
      aria-label="commits"
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="relative min-h-0 flex-1 overflow-x-hidden overflow-y-auto outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
    >
      <div
        role="row"
        aria-label="columns"
        style={{ gridTemplateColumns: columns, height: HEADER_PX }}
        className="sticky top-0 z-10 grid select-none border-b border-[var(--border)] bg-[var(--bg-raised)] text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase"
      >
        <div className={cn(cell, "relative pl-1")}>{divider("graph", "drag to resize the graph column")}</div>
        <div className={cn(cell, "relative pl-1")}>
          subject
          {divider("subject", "drag to resize the subject and author columns")}
        </div>
        <div className={cn(cell, "relative pl-1")}>
          author
          {divider("author", "drag to resize the author and date columns")}
        </div>
        <div className={cn(cell, "relative pl-1")}>
          date
          {divider("date", "drag to resize the date and marks columns")}
        </div>
        <div className="flex h-full items-center justify-end pr-1.5">marks</div>
      </div>
      <div style={{ height: virt.getTotalSize(), position: "relative" }}>
        {items.map((v) => {
          const c = rows[v.index];
          const graphIndex = graph ? all.indexOf(c) : -1;
          return (
            <LogRow
              key={c.sha}
              id={id}
              c={c}
              top={v.start - HEADER_PX}
              columns={columns}
              graphSvg={graph && graphIndex >= 0 ? graphCell(graph.rows[graphIndex], graph.width) : null}
              selected={c.sha === selected}
              read={!!readBySha?.[c.sha]}
              marked={rangeMark === c.sha}
              hasMark={!!rangeMark && rangeMark !== c.sha}
              canPatch={c.sha === selected && !!diff && diff.length > 0}
              onPatch={() => diff && copyText(patchText(diff))}
            />
          );
        })}
      </div>
      {!done && <Dim className="block py-1 text-center text-2xs">{pending ? "reading more…" : ""}</Dim>}
    </div>
  );
}

function LogRow({
  id,
  c,
  top,
  columns,
  graphSvg,
  selected,
  read,
  marked,
  hasMark,
  canPatch,
  onPatch,
}: {
  id: string;
  c: CommitInfo;
  top: number;
  columns: string;
  graphSvg: string | null;
  selected: boolean;
  read: boolean;
  marked: boolean;
  hasMark: boolean;
  canPatch: boolean;
  onPatch: () => void;
}) {
  const link = hostLink(id, c.sha);
  return (
    <ContextMenu
      trigger={
        <div
          role="option"
          aria-selected={selected}
          data-sha={c.sha}
          onClick={() => selectCommit(id, c.sha)}
          onDoubleClick={() => showDiffPane(id, c.sha, "*")}
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            width: "100%",
            height: ROW_PX,
            transform: `translateY(${top}px)`,
            gridTemplateColumns: columns,
          }}
          className={cn(
            "grid cursor-default items-center overflow-hidden border-b border-[var(--border)] text-sm hover:bg-[var(--state-hover)]",
            selected && rowSelected,
          )}
        >
          <span className={cn(cell, "overflow-hidden pl-1")} aria-hidden="true">
            {graphSvg ? (
              // Numbers and colour tokens only — the string is drawn from
              // lane indices, never from anything git or a client typed.
              <span className="shrink-0" dangerouslySetInnerHTML={{ __html: graphSvg }} />
            ) : (
              <span className="mx-auto inline-block h-1.5 w-1.5 rounded-full bg-[var(--dim)] opacity-60" />
            )}
          </span>
          <span className={cn(cell, "gap-1.5 overflow-hidden pl-1")}>
            <span className="min-w-0 shrink truncate" title={c.summary}>
              {c.summary}
            </span>
            {c.refs.map(refChip)}
          </span>
          <Dim className={cn(cell, "truncate pl-1 text-xs")} title={c.author}>
            {c.author}
          </Dim>
          <Dim className={cn(cell, "pl-1 text-2xs whitespace-nowrap tabular-nums")} title={new Date(c.epoch * 1000).toLocaleString()}>
            {stamp(c.epoch)}
          </Dim>
          <span className="flex h-full items-center justify-end gap-1 pr-1.5">
              {marked && <Chip color="var(--amber)" title="marked — the next commit diffed against it makes a range">from</Chip>}
              {c.touches_session && (
                <span
                  className="inline-block h-1.5 w-1.5 rounded-full bg-[var(--blue)]"
                  title="lands in this session's lifetime and touches files it edited — a hint, not an author column"
                />
              )}
              {read && <Chip color="var(--dim)" title="every hunk of this commit has been read (R-D17)">read</Chip>}
          </span>
        </div>
      }
    >
      <MenuItem onSelect={() => copyText(c.sha)}>Copy sha</MenuItem>
      <MenuItem onSelect={() => copyText(c.summary)}>Copy subject</MenuItem>
      <MenuItem disabled={!canPatch} onSelect={onPatch}>
        {canPatch ? "Copy as patch" : "Copy as patch — select the commit first"}
      </MenuItem>
      {link && <MenuItem onSelect={() => copyText(link)}>Copy link on host</MenuItem>}
      <MenuSeparator />
      <MenuItem onSelect={() => showDiffPane(id, c.sha, "*")}>Open the diff in the centre  (Ctrl+D)</MenuItem>
      <MenuSeparator />
      {hasMark ? (
        <MenuItem onSelect={() => diffAgainstMark(id, c.sha)}>Diff from the marked commit to this one</MenuItem>
      ) : (
        <MenuItem onSelect={() => markRange(id, marked ? null : c.sha)}>{marked ? "Unmark" : "Mark for a range diff"}</MenuItem>
      )}
      {hasMark && <MenuItem onSelect={() => markRange(id, null)}>Unmark</MenuItem>}
    </ContextMenu>
  );
}
