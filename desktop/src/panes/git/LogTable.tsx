/**
 * The log: one row per commit — graph, subject with its refs as chips,
 * author, date, and the two marks that are mogeung's. `R-D26`.
 *
 * Virtualised, and the next page is asked for as the end approaches, so
 * there is no *load more*. The graph is computed over every loaded row and
 * hidden whenever a text filter is in force or a client-side toggle narrows
 * the rows — lanes drawn over a subset join dots that are not adjacent, and
 * a lying graph is worse than none (`R-D13`).
 */

import { useEffect, useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useStore } from "@/store";
import { Chip, Dim, Empty } from "@/ui/primitives";
import { ContextMenu, MenuItem, MenuSeparator } from "@/ui/Menu";
import { graphCell, lanes, LANE_PX, ROW_PX } from "@/lib/gitGraph";
import { hidesGraph, patchText } from "@/lib/gitFilter";
import { askLog, copyText, diffAgainstMark, hostLink, markRange, queryOf, selectCommit } from "@/lib/gitActions";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import { rowSelected } from "@/ui/styles";
import type { CommitInfo } from "@/wire/types";
import type { Only } from "./LogToolbar";
import { useWidth } from "./Dropdown";

/** How close to the end the scroll gets before the next page is asked for. */
const AHEAD = 15;

/**
 * The columns give way from the right as the pane narrows: the date first,
 * then the author, then the marks — the subject is what a row is for. Below
 * this the graph and the subject are all that is drawn. The widths are the
 * dock at a laptop's size with the queue and the rail open, which is where
 * the first build put the subject at zero and the chips over the graph.
 */
const COLS = {
  wide: 700,
  mid: 480,
};


function refChip(r: string) {
  if (r.startsWith("HEAD")) return <Chip key={r} color="var(--blue)">{r.replace("HEAD -> ", "HEAD → ")}</Chip>;
  if (r.startsWith("tag: ")) return <Chip key={r} color="var(--amber)">⌖ {r.slice(5)}</Chip>;
  return <Chip key={r} color="var(--green)">{r}</Chip>;
}

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
  const all = commits ?? [];

  const rows = useMemo(
    () => all.filter((c) => (!only.session || c.touches_session) && (!only.read || readBySha?.[c.sha])),
    [all, only.session, only.read, readBySha],
  );
  const showGraph = !only.session && !only.read && !hidesGraph({ ...queryOf(id), grep, author, path, pickaxe });
  const graph = useMemo(() => (showGraph ? lanes(all) : null), [showGraph, all]);
  const gw = graph ? graph.width * LANE_PX + 6 : 10;

  const parentRef = useRef<HTMLDivElement>(null);
  const width = useWidth(parentRef);
  // Zero before the first measurement — jsdom, and the first paint — draws
  // every column rather than none.
  const columns =
    width === 0 || width >= COLS.wide
      ? `${gw}px minmax(120px, 1fr) 96px 84px 56px`
      : width >= COLS.mid
        ? `${gw}px minmax(120px, 1fr) 72px 40px`
        : `${gw}px minmax(0, 1fr)`;
  const showAuthor = width === 0 || width >= COLS.wide;
  const showDate = width === 0 || width >= COLS.mid;
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_PX,
    overscan: 12,
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

  return (
    <div
      ref={parentRef}
      role="listbox"
      aria-label="commits"
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="relative min-h-0 flex-1 overflow-y-auto outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
    >
      <div style={{ height: virt.getTotalSize(), position: "relative" }}>
        {items.map((v) => {
          const c = rows[v.index];
          const graphIndex = graph ? all.indexOf(c) : -1;
          return (
            <LogRow
              key={c.sha}
              id={id}
              c={c}
              top={v.start}
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
      {!done && (
        <Dim className="block py-1 text-center text-2xs">{pending ? "reading more…" : ""}</Dim>
      )}
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
            "grid cursor-default items-center overflow-hidden pr-1.5 text-sm hover:bg-[var(--state-hover)]",
            selected && rowSelected,
          )}
        >
          <span className="flex items-center pl-1" aria-hidden="true">
            {graphSvg ? (
              // Numbers and colour tokens only — the string is drawn from
              // lane indices, never from anything git or a client typed.
              <span dangerouslySetInnerHTML={{ __html: graphSvg }} />
            ) : (
              <span className="mx-auto inline-block h-1.5 w-1.5 rounded-full bg-[var(--dim)] opacity-60" />
            )}
          </span>
          <span className="flex min-w-0 items-center gap-1.5 overflow-hidden">
            <span className="min-w-0 shrink truncate" title={c.summary}>
              {c.summary}
            </span>
            {c.refs.map(refChip)}
          </span>
          {showAuthor && (
            <Dim className="truncate text-xs" title={c.author}>
              {c.author}
            </Dim>
          )}
          {showDate && (
            <Dim className="text-2xs tabular-nums" title={new Date(c.epoch * 1000).toLocaleString()}>
              {stamp(c.epoch)}
            </Dim>
          )}
          {showDate && (
          <span className="flex items-center justify-end gap-1">
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
      {hasMark ? (
        <MenuItem onSelect={() => diffAgainstMark(id, c.sha)}>Diff from the marked commit to this one</MenuItem>
      ) : (
        <MenuItem onSelect={() => markRange(id, marked ? null : c.sha)}>{marked ? "Unmark" : "Mark for a range diff"}</MenuItem>
      )}
      {hasMark && <MenuItem onSelect={() => markRange(id, null)}>Unmark</MenuItem>}
    </ContextMenu>
  );
}
