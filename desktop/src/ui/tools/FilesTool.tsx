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
import { ChevronDown, ChevronRight, RefreshCw } from "lucide-react";
import { FileIcon } from "@/ui/FileIcon";
import { useStore } from "@/store";
import { Dim, Empty, IconButton, Input, Loading } from "@/ui/primitives";
import { explorerFetch, join, openFile, toggleDir } from "@/lib/explorer";
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

export function FilesTool() {
  const id = useStore((s) => s.selected);
  const session = useStore((s) => (s.selected ? s.sessions[s.selected] : null));
  const st = useStore((s) => (s.selected ? s.explorer[s.selected] : undefined));
  const patchExplorer = useStore((s) => s.patchExplorer);
  const [filter, setFilter] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  // In the render, not in a click handler — the same "one door" rule the Rust
  // side follows, so a tool that is merely *visible* works without ever having
  // been switched to.
  useEffect(() => {
    if (id) explorerFetch(id);
  });

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
    if (!filter.trim()) return out;
    const q = filter.trim().toLowerCase();
    return out.filter((r) => r.path.toLowerCase().includes(q));
  }, [st, filter]);

  const activePath = st && st.active[st.focus] !== null ? st.open[st.active[st.focus]!]?.path : null;

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
        <div className="ml-auto">
          <IconButton
            title="re-list the directories that are open"
            onClick={() => patchExplorer(id, { dirs: {}, pending: [] })}
          >
            <RefreshCw size={12} />
          </IconButton>
        </div>
      </div>

      <div className="shrink-0 px-2 py-1">
        <Input value={filter} onChange={setFilter} placeholder="filter this tree" />
      </div>

      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto pb-2">
        {!st?.dirs[""] ? (
          <Loading what="listing" />
        ) : rows.length === 0 ? (
          <Empty>{filter ? "nothing matches" : "this folder is empty"}</Empty>
        ) : (
          <div className="min-w-max">
            {rows.map((r) => (
              <div
                key={r.path}
                title={r.path}
                onClick={() => {
                  if (r.isDir) toggleDir(id, r.path);
                  // Single click previews, double click pins — the tree's
                  // existing rule. The Code pane is raised by the pane itself
                  // when a file arrives, so a click is never a no-op.
                  else openFile(id, r.path);
                }}
                onDoubleClick={() => {
                  if (!r.isDir) openFile(id, r.path, { pin: true });
                }}
                className={cn(
                  "flex cursor-default items-center gap-1 whitespace-nowrap py-px pr-3 text-sm hover:bg-[var(--bg-faint)]",
                  activePath === r.path && "bg-[var(--selection-bg)] text-[var(--text-strong)]",
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

      {st?.treeTruncated && (
        <div className="shrink-0 border-t border-[var(--border)] px-2 py-1">
          <Dim className="text-2xs">the walk hit its cap — this is a prefix of the tree</Dim>
        </div>
      )}
    </>
  );
}
