/**
 * The three lists IntelliJ's tool window has no tab for and the wire has
 * carried since `R-D13`/`R-D15`: the reflog, the worktrees, the submodules.
 * Kept under a fifth tab rather than dropped — a reflog is how work a reset
 * moved off a branch is found, and the worktrees are the ones mogeung
 * itself creates. `R-D26`.
 */

import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { Chip, Dim, Empty, Mono, Segmented } from "@/ui/primitives";
import { selectCommit } from "@/lib/gitActions";
import { cn } from "@/lib/cn";
import { row as rowCls, rowSelected } from "@/ui/styles";
import { CommitInspector } from "./CommitInspector";
import { charsFor, FilePath } from "./FilePath";
import { Splitter, useDragWidth } from "./Dropdown";

type List = "reflog" | "worktrees" | "submodules";
const CMD = { reflog: "git_reflog", worktrees: "git_worktrees", submodules: "git_submodules" } as const;

export function MoreTab({ id }: { id: string }) {
  const [list, setList] = useState<List>("reflog");
  const reflog = useStore((s) => s.git[id]?.reflog ?? null);
  const worktrees = useStore((s) => s.git[id]?.worktrees ?? null);
  const submodules = useStore((s) => s.git[id]?.submodules ?? null);
  const selected = useStore((s) => s.git[id]?.selected ?? null);
  const send = useStore((s) => s.send);
  const saved = useStore((s) => s.prefs.gitSidebarWidth);
  const setPrefs = useStore((s) => s.setPrefs);
  const [width, onDrag] = useDragWidth(saved, 220, 900, (px) => setPrefs({ gitSidebarWidth: px }));
  const chars = charsFor(width);

  const have = { reflog, worktrees, submodules }[list];
  useEffect(() => {
    if (have === null) send({ cmd: CMD[list], session_id: id });
  }, [have, list, id, send]);

  return (
    <div className="flex h-full min-h-0">
      <div className="flex shrink-0 flex-col" style={{ width }}>
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
          <Segmented
            value={list}
            onChange={setList}
            options={[
              { value: "reflog", label: "reflog", title: "where HEAD has been — including what a reset moved off" },
              { value: "worktrees", label: "worktrees", title: "every checkout of this repository, including the ones mogeung made" },
              { value: "submodules", label: "submodules", title: "nested repositories and their state" },
            ]}
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto py-0.5">
          {list === "reflog" &&
            (!reflog ? (
              <Empty>reading the reflog…</Empty>
            ) : reflog.length === 0 ? (
              <Empty>nothing in the reflog</Empty>
            ) : (
              reflog.map((e, i) => (
                <div
                  key={`${e.sha}:${i}`}
                  role="option"
                  aria-selected={selected === e.sha}
                  tabIndex={-1}
                  onClick={() => selectCommit(id, e.sha)}
                  title="show this commit — the reflog is how you find work a reset moved off a branch"
                  className={cn(rowCls, "px-2 py-0.5", selected === e.sha && rowSelected)}
                >
                  <div className="flex items-center gap-1.5">
                    <Mono className="shrink-0 text-2xs text-[var(--amber)]">{e.sha.slice(0, 8)}</Mono>
                    <Mono className="shrink-0 text-2xs text-[var(--dim)]">{e.selector}</Mono>
                  </div>
                  <div className="truncate text-xs">{e.summary}</div>
                </div>
              ))
            ))}
          {list === "worktrees" &&
            (!worktrees ? (
              <Empty>reading worktrees…</Empty>
            ) : (
              worktrees.map((w) => (
                <div key={w.path} className="border-b border-[var(--border)] px-2 py-1">
                  <FilePath path={w.path} chars={chars} />
                  <div className="flex items-center gap-2">
                    <Dim className="text-2xs">{w.branch ?? "(detached)"}</Dim>
                    <Mono className="text-2xs text-[var(--dim)]">{w.sha.slice(0, 8)}</Mono>
                  </div>
                </div>
              ))
            ))}
          {list === "submodules" &&
            (!submodules ? (
              <Empty>reading submodules…</Empty>
            ) : submodules.length === 0 ? (
              <Empty>this repository has no submodules</Empty>
            ) : (
              submodules.map((m) => (
                <div key={m.path} className="border-b border-[var(--border)] px-2 py-1">
                  <FilePath path={m.path} chars={chars} />
                  <div className="flex items-center gap-2">
                    <Mono className="text-2xs text-[var(--dim)]">{m.sha.slice(0, 8)}</Mono>
                    {m.note && <Dim className="truncate text-2xs">{m.note}</Dim>}
                    {/* The state character is git's own — `-` uninitialised,
                        `+` moved, `U` conflicted — shown rather than translated
                        into a word that would be a guess. */}
                    {m.state.trim() && <Chip color="var(--amber)">{m.state}</Chip>}
                  </div>
                </div>
              ))
            ))}
        </div>
      </div>
      <Splitter onMouseDown={onDrag} />
      <div className="flex min-w-0 flex-1 flex-col">
        <CommitInspector id={id} onBack={() => {}} />
      </div>
    </div>
  );
}
