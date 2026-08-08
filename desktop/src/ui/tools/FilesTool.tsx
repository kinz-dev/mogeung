/**
 * The worktree, in the rail. `R-B41`.
 *
 * This tree used to live inside the Code pane. It is here so it can be read
 * with the Transcript, the Git pane or a terminal forward — a map of the
 * project is the wrong thing to be able to see from only one tab.
 *
 * Read-only, permanently. Pillar K, and the daemon offers no write path at all,
 * so this could not edit even if it wanted to.
 */

import { useEffect, useMemo, useRef } from "react";
import { ChevronDown, ChevronRight, Crosshair, RefreshCw } from "lucide-react";
import { FileIcon } from "@/ui/FileIcon";
import { useStore } from "@/store";
import { Dim, Empty, IconButton, Input, Loading } from "@/ui/primitives";
import { basename, explorerFetch, fileFilter, join, openFile, reveal, toggleDir } from "@/lib/explorer";
import { parseFilePaneId } from "@/lib/panes";
import { cn } from "@/lib/cn";
import { repoName } from "@/wire/types";
import { useState } from "react";

interface FlatRow {
  path: string;
  name: string;
  isDir: boolean;
  depth: number;
  open: boolean;
}

/** Rows drawn for one query. Plain divs, not a virtual list — `.*` over a
 *  monorepo must not be what discovers that. */
const HIT_CAP = 300;

export function FilesTool() {
  const id = useStore((s) => s.selected);
  const session = useStore((s) => (s.selected ? s.sessions[s.selected] : null));
  const st = useStore((s) => (s.selected ? s.explorer[s.selected] : undefined));
  const patchExplorer = useStore((s) => s.patchExplorer);
  const send = useStore((s) => s.send);
  const [filter, setFilter] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  // Following the file you are reading. `R-J25`.
  const follow = useStore((s) => s.prefs.filesFollow);
  const setPrefs = useStore((s) => s.setPrefs);
  const activePane = useStore((s) => s.activePane);
  const active = activePane ? parseFilePaneId(activePane) : null;
  // What was last revealed *by following*, so the effect below is idempotent.
  // Without it every render would re-mark the same row, and marking it is a
  // store write — which renders again.
  const followed = useRef<string | null>(null);

  /**
   * **Only when the file belongs to the tree on screen.** A file pane is bound
   * to the session that opened it (`R-B53`) and this tree shows the *selected*
   * session's worktree, so the two can differ — and revealing into the wrong
   * worktree would expand directories that do not contain the file and mark a
   * row that is not there. Following silently does nothing in that case, which
   * is the honest answer: the file is not in this tree.
   */
  useEffect(() => {
    if (!follow || !id || !active || active.session !== id) return;
    const mark = `${id}\u0000${active.path}`;
    if (followed.current === mark) return;
    followed.current = mark;
    reveal(id, active.path);
  }, [follow, id, active]);

  // In the render, not in a click handler — the same "one door" rule the Rust
  // side follows, so a tool that is merely *visible* works without ever having
  // been switched to.
  useEffect(() => {
    if (id) explorerFetch(id);
  });

  const match = useMemo(() => fileFilter(filter), [filter]);

  // A filter searches the **whole worktree**, not the part that happens to be
  // expanded. Filtering the visible rows was the obvious implementation and the
  // wrong feature: a file two collapsed folders down could not be found however
  // exactly you typed its name, which reads as "the filter is broken" rather
  // than "you have not opened that folder". Asked for on first keystroke and
  // kept until the refresh button drops it.
  useEffect(() => {
    if (!id || match.empty) return;
    if (st?.treePaths || st?.treePending) return;
    patchExplorer(id, { treePending: true });
    send({ cmd: "list_tree", session_id: id });
  }, [id, match.empty, st?.treePaths, st?.treePending, patchExplorer, send]);

  const rows = useMemo<FlatRow[]>(() => {
    if (!st) return [];
    const out: FlatRow[] = [];
    const walk = (dir: string, depth: number) => {
      const entries = st.dirs[dir];
      if (!entries) return;
      for (const e of entries) {
        const path = join(dir, e.name);
        const open = st.expanded.includes(path);
        out.push({ path, name: e.name, isDir: e.is_dir, depth, open });
        if (e.is_dir && open) walk(path, depth + 1);
      }
    };
    walk("", 0);
    return out;
  }, [st]);

  // Matches are flat and carry their directory, because a hit three levels down
  // needs to say where it is — and indenting a filtered list by a depth whose
  // parents are not on screen only looks like damage.
  const hits = useMemo<string[]>(() => {
    if (match.empty || !st?.treePaths) return [];
    return st.treePaths.filter((p) => match.test(p)).slice(0, HIT_CAP);
  }, [match, st?.treePaths]);

  // Since `R-B53` every open file is its own pane, so "the active one" is
  // dockview's answer rather than this tool's. The tree highlights **every**
  // open file instead, which is more useful anyway: it says what you have out,
  // not which tab happens to be forward.
  const openPaths = st ? st.open.map((t) => t.path) : [];

  if (!id || !session) {
    return (
      <Empty hint="a worktree belongs to one — pick a session in the queue">no session selected</Empty>
    );
  }

  const root = session.repo_root ?? session.cwd;

  return (
    <>
      <div className="flex shrink-0 items-center gap-1 border-b border-[var(--border)] px-2 py-1">
        <span className="truncate text-sm font-semibold text-[var(--text-strong)]" title={root}>
          {repoName(session)}
        </span>
        <div className="ml-auto flex items-center gap-0.5">
          {/* Sticky rather than a one-shot, because the ask was to *sync* the
              view: a button you press after every file is the thing being
              complained about. The tab's own menu has the one-shot for anyone
              who wants the tree to stay where they put it. `R-J25`. */}
          <IconButton
            title={
              follow
                ? "following the file you are reading — click to leave the tree where you put it"
                : "follow the file you are reading, and reveal it here as you switch"
            }
            active={follow}
            onClick={() => {
              const next = !follow;
              setPrefs({ filesFollow: next });
              // Reveal on the way *on*, so switching it on does something
              // rather than waiting for the next file you happen to open.
              if (next && id && active && active.session === id) {
                followed.current = `${id}\u0000${active.path}`;
                reveal(id, active.path);
              }
            }}
          >
            <Crosshair size={12} />
          </IconButton>
          <IconButton
            title="re-list the directories that are open"
            // The walk goes too: a refresh that re-listed the open folders and
            // left the filter answering from a remembered tree would be the
            // stalest possible half-refresh.
            // `treePending` is cleared as well as the paths, so this is also
            // the way out of a walk that errored: the reply that would have
            // lowered the flag never arrives, and without this the filter would
            // say "walking the worktree" until the window was restarted.
            onClick={() =>
              patchExplorer(id, { dirs: {}, pending: [], treePaths: null, treePending: false })
            }
          >
            <RefreshCw size={12} />
          </IconButton>
        </div>
      </div>

      <div className="shrink-0 px-2 py-1">
        <Input
          value={filter}
          onChange={setFilter}
          placeholder="filter — regex, over name and path"
        />
        {!match.empty && !match.regex && (
          <Dim className="mt-0.5 block text-2xs">
            not a valid pattern — matching it literally
          </Dim>
        )}
      </div>

      {!match.empty ? (
        <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto pb-2">
          {!st?.treePaths ? (
            <Loading what="walking the worktree" />
          ) : hits.length === 0 ? (
            <Empty hint="the pattern is matched against the file name and its whole path">
              nothing matches
            </Empty>
          ) : (
            <div className="min-w-max">
              {hits.map((p) => (
                <div
                  key={p}
                  title={p}
                  onClick={() => openFile(id, p)}
                  onDoubleClick={() => openFile(id, p, { pin: true })}
                  className={cn(
                    "flex cursor-default items-center gap-1 whitespace-nowrap py-px pr-3 pl-2 text-sm hover:bg-[var(--bg-faint)]",
                    openPaths.includes(p) && "bg-[var(--selection-bg)] text-[var(--text-strong)]",
                  )}
                >
                  <FileIcon name={basename(p)} className="shrink-0" />
                  <span className="text-[var(--text)]">{basename(p)}</span>
                  {/* The directory, dimmed and after the name: two files called
                      `mod.rs` are told apart by where they are, and that is the
                      part you read second. */}
                  {p.includes("/") && (
                    <Dim className="text-2xs">{p.slice(0, p.lastIndexOf("/"))}</Dim>
                  )}
                </div>
              ))}
              {hits.length === HIT_CAP && (
                <Dim className="block px-2 py-1 text-2xs">
                  first {HIT_CAP} matches — narrow the pattern to see the rest
                </Dim>
              )}
            </div>
          )}
        </div>
      ) : (
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto pb-2">
        {!st?.dirs[""] ? (
          <Loading what="listing" />
        ) : rows.length === 0 ? (
          <Empty>this folder is empty</Empty>
        ) : (
          <div className="min-w-max">
            {rows.map((r) => (
              <div
                key={r.path}
                title={r.path}
                onClick={() => {
                  if (r.isDir) toggleDir(id, r.path);
                  // Single click previews, double click pins — the tree's
                  // existing rule. `openFile` raises the Code pane, so a click
                  // from here is never a no-op, whatever tab is forward.
                  else openFile(id, r.path);
                }}
                onDoubleClick={() => {
                  if (!r.isDir) openFile(id, r.path, { pin: true });
                }}
                className={cn(
                  "flex cursor-default items-center gap-1 whitespace-nowrap py-px pr-3 text-sm hover:bg-[var(--bg-faint)]",
                  openPaths.includes(r.path) && "bg-[var(--selection-bg)] text-[var(--text-strong)]",
                )}
                style={{ paddingLeft: 6 + r.depth * 12 }}
              >
                {r.isDir ? (
                  r.open ? (
                    <ChevronDown size={11} className="shrink-0 text-[var(--dim)]" />
                  ) : (
                    <ChevronRight size={11} className="shrink-0 text-[var(--dim)]" />
                  )
                ) : (
                  // The chevron column is kept for files too, so names line up
                  // in a column rather than stepping in and out by depth.
                  <span className="w-[11px] shrink-0" />
                )}
                <FileIcon name={r.name} isDir={r.isDir} open={r.open} className="shrink-0" />
                <span className={r.isDir ? "text-[var(--text)]" : "text-[var(--text)]"}>{r.name}</span>
              </div>
            ))}
          </div>
        )}
      </div>
      )}

      {st?.treeTruncated && (
        <div className="shrink-0 border-t border-[var(--border)] px-2 py-1">
          <Dim className="text-2xs">the walk hit its cap — this is a prefix of the tree</Dim>
        </div>
      )}
    </>
  );
}
