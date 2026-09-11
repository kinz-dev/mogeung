/**
 * The working tree: what is uncommitted, grouped the way git holds it —
 * Conflicted, Staged, Unstaged, Untracked — and the selected file's diff
 * against HEAD, or its three stages when it is conflicted. `R-D26`.
 *
 * **Read-only.** The wire carries `git_stage`, `git_commit`, `git_discard`
 * and `git_resolve`, and this client sends none of them; the checkboxes and
 * the commit box are `R-D28`, [A26](../../../../docs/product/assumptions.md)'s
 * test, and until then this tab is the list it was — regrouped, with the
 * *this session* filter [feature 0010] asked for and the port left out.
 */

import { useMemo, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useStore } from "@/store";
import { Chip, Dim, Empty, Mono } from "@/ui/primitives";
import { DiffList } from "@/ui/DiffView";
import { selectPath } from "@/lib/gitActions";
import { cn } from "@/lib/cn";
import { interactive, row as rowCls, rowSelected } from "@/ui/styles";
import type { StatusEntry } from "@/wire/types";
import { charsFor, FilePath } from "./FilePath";
import { Splitter, useDragWidth } from "./Dropdown";

type GroupKey = "conflicted" | "staged" | "unstaged" | "untracked";
const GROUPS: { key: GroupKey; label: string; hint: string }[] = [
  { key: "conflicted", label: "Conflicted", hint: "unresolved merges — the one uncommitted state that is never routine" },
  { key: "staged", label: "Staged", hint: "in the index — what a commit would take" },
  { key: "unstaged", label: "Unstaged", hint: "changed on disk and not yet in the index" },
  { key: "untracked", label: "Untracked", hint: "files git has never seen" },
];

function groupOf(e: StatusEntry): GroupKey {
  if (e.conflicted) return "conflicted";
  if (e.state === "??") return "untracked";
  if (e.staged) return "staged";
  return "unstaged";
}

export function LocalChanges({ id, repoRoot }: { id: string; repoRoot: string }) {
  const status = useStore((s) => s.git[id]?.status ?? null);
  const selectedPath = useStore((s) => s.git[id]?.selectedPath ?? null);
  const diff = useStore((s) => s.git[id]?.diff ?? null);
  const conflict = useStore((s) => s.git[id]?.conflict ?? null);
  const touched = useStore((s) => s.sessions[id]?.touched_files);
  const saved = useStore((s) => s.prefs.gitSidebarWidth);
  const setPrefs = useStore((s) => s.setPrefs);
  const [width, onDrag] = useDragWidth(saved, 220, 900, (px) => setPrefs({ gitSidebarWidth: px }));
  const [onlySession, setOnlySession] = useState(false);
  const [collapsed, setCollapsed] = useState<ReadonlySet<GroupKey>>(() => new Set());
  const chars = charsFor(width);

  const touchedSet = useMemo(() => new Set((touched ?? []).map((p) => p.replace(/\/+$/, ""))), [touched]);
  const entries = useMemo(() => {
    const listed = (status ?? []).filter((e) => e.state !== "!!");
    // The session's files are absolute; status paths are repo-relative. The
    // same join the attribution heuristic uses, and the same limit — two
    // sessions on one file both match.
    return onlySession ? listed.filter((e) => touchedSet.has(`${repoRoot}/${e.path}`)) : listed;
  }, [status, onlySession, touchedSet, repoRoot]);
  const grouped = useMemo(() => {
    const g: Record<GroupKey, StatusEntry[]> = { conflicted: [], staged: [], unstaged: [], untracked: [] };
    for (const e of entries) g[groupOf(e)].push(e);
    return g;
  }, [entries]);

  const toggle = (k: GroupKey) =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });

  return (
    <div className="flex h-full min-h-0">
      <div className="flex shrink-0 flex-col" style={{ width }}>
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
          <span className="text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">Changes</span>
          {status && <Dim className="text-2xs tabular-nums">{entries.length}</Dim>}
          <button
            type="button"
            aria-pressed={onlySession}
            title="only files this session touched — the Changes tab's set, applied to the whole repository's status"
            onClick={() => setOnlySession(!onlySession)}
            className={cn(
              "ml-auto inline-flex h-5 items-center gap-1 rounded-sm px-1.5 text-2xs",
              interactive,
              onlySession ? "bg-[var(--state-focus)] text-[var(--text-strong)]" : "text-[var(--dim)] hover:text-[var(--text)]",
            )}
          >
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-[var(--blue)]" /> this session
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto py-0.5">
          {!status ? (
            <Empty>reading status…</Empty>
          ) : entries.length === 0 ? (
            <Empty hint={onlySession ? "nothing this session touched is uncommitted" : "nothing to stage, nothing to commit"}>
              {onlySession ? "clean, for this session" : "the working tree is clean"}
            </Empty>
          ) : (
            GROUPS.map((g) => {
              const rows = grouped[g.key];
              if (rows.length === 0) return null;
              const open = !collapsed.has(g.key);
              return (
                <div key={g.key}>
                  <div
                    role="button"
                    tabIndex={0}
                    onClick={() => toggle(g.key)}
                    onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && toggle(g.key)}
                    title={g.hint}
                    className={cn(rowCls, "flex h-5 items-center gap-1 px-1.5")}
                  >
                    {open ? <ChevronDown size={11} className="text-[var(--dim)]" /> : <ChevronRight size={11} className="text-[var(--dim)]" />}
                    <span className={cn("text-2xs font-semibold tracking-wider uppercase", g.key === "conflicted" ? "text-[var(--red)]" : "text-[var(--dim)]")}>
                      {g.label}
                    </span>
                    <Dim className="ml-auto text-2xs tabular-nums">{rows.length}</Dim>
                  </div>
                  {open &&
                    rows.map((e) => (
                      <div
                        key={e.path}
                        role="option"
                        aria-selected={selectedPath === e.path}
                        tabIndex={-1}
                        onClick={() => selectPath(id, e.path, e.conflicted)}
                        className={cn(rowCls, "flex h-5 items-center gap-2 pr-1.5 pl-5 text-sm", selectedPath === e.path && rowSelected)}
                      >
                        <Mono
                          className={cn(
                            "w-5 shrink-0 text-2xs",
                            e.conflicted ? "text-[var(--red)]" : e.staged ? "text-[var(--add-fg)]" : "text-[var(--amber)]",
                          )}
                          title={`porcelain ${JSON.stringify(e.state)}`}
                        >
                          {e.state.trim() || "·"}
                        </Mono>
                        <FilePath path={e.path} chars={chars} />
                        {e.staged && e.unstaged && !e.conflicted && (
                          <Chip color="var(--amber)" title="staged, and changed again since">+ unstaged</Chip>
                        )}
                      </div>
                    ))}
                </div>
              );
            })
          )}
        </div>
        <div className="flex h-5 shrink-0 items-center border-t border-[var(--border)] px-2">
          <Dim className="text-2xs">staging and committing from here is R-D28 — until then, the terminal</Dim>
        </div>
      </div>
      <Splitter onMouseDown={onDrag} title="drag to resize — a path is longer than a list is wide" />
      <div className="flex min-w-0 flex-1 flex-col">
        {conflict ? (
          <ConflictView path={conflict.path} base={conflict.base} ours={conflict.ours} theirs={conflict.theirs} truncated={conflict.truncated} />
        ) : selectedPath && !diff ? (
          <Empty>reading {selectedPath}…</Empty>
        ) : diff && diff.length === 0 ? (
          <Empty hint="a binary file, or a change git shows no text for">nothing to show for {selectedPath ?? "that"}</Empty>
        ) : diff && selectedPath ? (
          <>
            <div className="flex h-6 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2 text-2xs">
              <Mono className="truncate text-[var(--text)]">{selectedPath}</Mono>
              <Dim className="shrink-0">the working tree against HEAD</Dim>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              <DiffList files={diff} sessionId={id} />
            </div>
          </>
        ) : (
          <Empty hint="its diff against HEAD shows here; a conflicted file shows its three sides">pick a changed file</Empty>
        )}
      </div>
    </div>
  );
}

/**
 * `R-D16`: ours, base and theirs, side by side and read-only. The markers in
 * the worktree file are the one view that shows neither original; this is
 * the other one. Resolving is `R-D28`.
 */
export function ConflictView({ path, base, ours, theirs, truncated }: { path: string; base: string; ours: string; theirs: string; truncated: boolean }) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-6 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
        <Mono className="truncate text-xs text-[var(--text-strong)]">{path}</Mono>
        <Dim className="shrink-0 text-2xs">three stages, read-only — resolving from here is R-D28</Dim>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-3">
        {(
          [
            ["ours", ours, "var(--add-fg)"],
            ["base", base, "var(--dim)"],
            ["theirs", theirs, "var(--del-fg)"],
          ] as const
        ).map(([name, body, colour]) => (
          <div key={name} className="flex min-h-0 flex-col border-r border-[var(--border)]">
            <div className="shrink-0 px-2 py-0.5 text-2xs" style={{ color: colour }}>
              {name}
            </div>
            <pre className="m-0 min-h-0 flex-1 overflow-auto px-2 font-mono text-2xs whitespace-pre">{body || "(empty on this side)"}</pre>
          </div>
        ))}
      </div>
      {truncated && <Dim className="shrink-0 px-2 py-1 text-2xs">one of the sides went past the size cap — this is its head</Dim>}
    </div>
  );
}
