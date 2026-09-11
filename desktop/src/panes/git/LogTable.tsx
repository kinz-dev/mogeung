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
 * this window is — local state during the drag, the preferences on release
 * — and resizes the fixed column beside it, since the subject is the one
 * that flexes. The graph column sizes itself from the lanes until a hand
 * sets it, and then clips.
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
import { useWidth } from "./Dropdown";

/** How close to the end the scroll gets before the next page is asked for. */
const AHEAD = 15;
/** The header row, which the virtualiser is told about as a scroll margin. */
const HEADER_PX = 22;

/**
 * The columns give way from the right as the pane narrows: the author first,
 * then the date and the marks — the subject is what a row is for. Below the
 * last step the graph and the subject are all that is drawn.
 */
const COLS = { wide: 700, mid: 480 };

/** A column's floor: a divider cannot drag it out of existence. */
const MIN = { graph: 18, author: 40, date: 48, marks: 24 };

function refChip(r: string) {
  if (r.startsWith("HEAD")) return <Chip key={r} color="var(--blue)">{r.replace("HEAD -> ", "HEAD → ")}</Chip>;
  if (r.startsWith("tag: ")) return <Chip key={r} color="var(--amber)">⌖ {r.slice(5)}</Chip>;
  return <Chip key={r} color="var(--green)">{r}</Chip>;
}

const cell = "flex h-full min-w-0 items-center border-r border-[var(--border)]";

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
  const width = useWidth(parentRef);
  // Zero before the first measurement — jsdom, and the first paint — draws
  // every column rather than none.
  const showAuthor = width === 0 || width >= COLS.wide;
  const showDate = width === 0 || width >= COLS.mid;
  const columns = showAuthor
    ? `${gw}px minmax(120px, 1fr) ${cols.author}px ${cols.date}px ${cols.marks}px`
    : showDate
      ? `${gw}px minmax(120px, 1fr) ${cols.date}px ${cols.marks}px`
      : `${gw}px minmax(0, 1fr)`;

  /**
   * A divider is dragged: `key` is the column it resizes and `sign` which way
   * a rightward drag goes — `+1` for the graph, whose divider is on its own
   * right edge, `-1` for the columns to the right of the subject, whose
   * divider is on their left.
   */
  const startDrag = (e: React.MouseEvent, key: keyof LogColumns, sign: 1 | -1) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const from: LogColumns = { ...saved, graph: gw };
    let latest = from;
    const move = (ev: MouseEvent) => {
      const w = Math.max(MIN[key], (from[key] ?? gw) + sign * (ev.clientX - startX));
      latest = { ...from, [key]: w };
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

  const divider = (key: keyof LogColumns, sign: 1 | -1, title: string) => (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={title}
      title={title}
      onMouseDown={(e) => startDrag(e, key, sign)}
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
      className="relative min-h-0 flex-1 overflow-y-auto outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
    >
      <div
        role="row"
        aria-label="columns"
        style={{ gridTemplateColumns: columns, height: HEADER_PX }}
        className="sticky top-0 z-10 grid select-none border-b border-[var(--border)] bg-[var(--bg-raised)] text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase"
      >
        <div className={cn(cell, "relative pl-1")}>{divider("graph", 1, "drag to resize the graph column")}</div>
        <div className={cn(cell, "relative pl-1")}>
          subject
          {showAuthor && divider("author", -1, "drag to resize the author column")}
          {!showAuthor && showDate && divider("date", -1, "drag to resize the date column")}
        </div>
        {showAuthor && (
          <div className={cn(cell, "relative pl-1")}>
            author
            {divider("date", -1, "drag to resize the date column")}
          </div>
        )}
        {showDate && (
          <div className={cn(cell, "relative pl-1")}>
            date
            {divider("marks", -1, "drag to resize the marks column")}
          </div>
        )}
        {showDate && <div className="flex h-full items-center justify-end pr-1.5">marks</div>}
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
              showAuthor={showAuthor}
              showDate={showDate}
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
  showAuthor,
  showDate,
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
  showAuthor: boolean;
  showDate: boolean;
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
          {showAuthor && (
            <Dim className={cn(cell, "truncate pl-1 text-xs")} title={c.author}>
              {c.author}
            </Dim>
          )}
          {showDate && (
            <Dim className={cn(cell, "pl-1 text-2xs tabular-nums")} title={new Date(c.epoch * 1000).toLocaleString()}>
              {stamp(c.epoch)}
            </Dim>
          )}
          {showDate && (
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
          )}
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
