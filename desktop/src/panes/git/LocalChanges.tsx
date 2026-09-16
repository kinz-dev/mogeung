/**
 * The working tree: what is uncommitted, grouped the way git holds it —
 * Conflicted, Staged, Unstaged, Untracked — the selected file's diff against
 * HEAD or its three stages, and the commit box. `R-D26` drew it; `R-D28`
 * made it act.
 *
 * **The checkbox is the index.** IntelliJ's checkbox means "in the commit",
 * and in git that is staging, so ticking a file is `git_stage` and unticking
 * it is `git_unstage`. Every verb is answered by the daemon re-broadcasting
 * the status, so the list shows what git says one round trip after the
 * click — never what this client assumed. **Discard is the one verb with no
 * undo**, and the only one that asks first, naming every file.
 *
 * This is [A26](../../../../docs/product/assumptions.md)'s test, run for
 * the first time: [feature 0025](../../../../docs/features/0025-git-write-local.md)'s
 * removal condition stands — unused through a week and the checkboxes and
 * the box come out, and the list stays.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useStore } from "@/store";
import { Button, Checkbox, Chip, Dim, Empty, Mono } from "@/ui/primitives";
import { DiffList } from "@/ui/DiffView";
import { Dialog } from "@/ui/Dialog";
import { ContextMenu, MenuItem, MenuSeparator } from "@/ui/Menu";
import { commit, copyText, discard, resolve, selectPath, stage, unstage } from "@/lib/gitActions";
import { cn } from "@/lib/cn";
import { interactive, row as rowCls, rowSelected } from "@/ui/styles";
import type { StatusEntry } from "@/wire/types";
import { charsFor, FilePath } from "./FilePath";
import { Splitter, useDragWidth, useWidth } from "./Dropdown";

type GroupKey = "conflicted" | "staged" | "unstaged" | "untracked";
const GROUPS: { key: GroupKey; label: string; hint: string }[] = [
  { key: "conflicted", label: "Conflicted", hint: "unresolved merges — the one uncommitted state that is never routine" },
  { key: "staged", label: "Staged", hint: "in the index — what a commit would take" },
  { key: "unstaged", label: "Unstaged", hint: "changed on disk and not yet in the index" },
  { key: "untracked", label: "Untracked", hint: "files git has never seen — discarding one deletes it" },
];

function groupOf(e: StatusEntry): GroupKey {
  if (e.conflicted) return "conflicted";
  if (e.state === "??") return "untracked";
  if (e.staged) return "staged";
  return "unstaged";
}

const NONE: never[] = [];

export function LocalChanges({ id, repoRoot }: { id: string; repoRoot: string }) {
  const status = useStore((s) => s.git[id]?.status ?? null);
  const selectedPath = useStore((s) => s.git[id]?.selectedPath ?? null);
  const diff = useStore((s) => s.git[id]?.diff ?? null);
  const conflict = useStore((s) => s.git[id]?.conflict ?? null);
  const touched = useStore((s) => s.sessions[id]?.touched_files);
  const consoleRows = useStore((s) => s.git[id]?.console ?? NONE);
  const commits = useStore((s) => s.git[id]?.commits);
  const headSha = useStore((s) => s.git[id]?.refs?.head_sha ?? null);
  const saved = useStore((s) => s.prefs.gitSidebarWidth);
  const savedCommit = useStore((s) => s.prefs.gitInspectorWidth);
  const setPrefs = useStore((s) => s.setPrefs);
  const [width, onDrag] = useDragWidth(saved, 220, 900, (px) => setPrefs({ gitSidebarWidth: px }));
  const [commitWidth, onDragCommit] = useDragWidth(savedCommit, 240, 900, (px) => setPrefs({ gitInspectorWidth: px }), -1);
  const rootRef = useRef<HTMLDivElement>(null);
  const total = useWidth(rootRef);
  const lw = total > 0 ? Math.min(width, Math.floor(total * 0.34)) : width;
  const cw = total > 0 ? Math.min(commitWidth, Math.floor(total * 0.34)) : commitWidth;
  const [onlySession, setOnlySession] = useState(false);
  const [collapsed, setCollapsed] = useState<ReadonlySet<GroupKey>>(() => new Set());
  const [discarding, setDiscarding] = useState<string[] | null>(null);
  const chars = charsFor(lw);

  const touchedSet = useMemo(() => new Set((touched ?? []).map((p) => p.replace(/\/+$/, ""))), [touched]);
  const listed = useMemo(() => (status ?? []).filter((e) => e.state !== "!!"), [status]);
  const entries = useMemo(
    // The session's files are absolute; status paths are repo-relative. The
    // same join the attribution heuristic uses, and the same limit — two
    // sessions on one file both match.
    () => (onlySession ? listed.filter((e) => touchedSet.has(`${repoRoot}/${e.path}`)) : listed),
    [listed, onlySession, touchedSet, repoRoot],
  );
  const grouped = useMemo(() => {
    const g: Record<GroupKey, StatusEntry[]> = { conflicted: [], staged: [], unstaged: [], untracked: [] };
    for (const e of entries) g[groupOf(e)].push(e);
    return g;
  }, [entries]);
  const stagedCount = listed.filter((e) => e.staged && !e.conflicted).length;

  const toggle = (k: GroupKey) =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });

  return (
    <div ref={rootRef} className="flex h-full min-h-0">
      <div className="flex shrink-0 flex-col" style={{ width: lw }}>
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
              const paths = rows.map((e) => e.path);
              return (
                <div key={g.key}>
                  <div
                    role="button"
                    tabIndex={0}
                    onClick={() => toggle(g.key)}
                    onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && toggle(g.key)}
                    title={g.hint}
                    className={cn(rowCls, "group flex h-5 items-center gap-1 px-1.5")}
                  >
                    {open ? <ChevronDown size={11} className="text-[var(--dim)]" /> : <ChevronRight size={11} className="text-[var(--dim)]" />}
                    <span className={cn("text-2xs font-semibold tracking-wider uppercase", g.key === "conflicted" ? "text-[var(--red)]" : "text-[var(--dim)]")}>
                      {g.label}
                    </span>
                    <Dim className="text-2xs tabular-nums">{rows.length}</Dim>
                    {/* Whole-group verbs, on the header where the group is
                        named; a click on them must not fold the group. */}
                    <span className="ml-auto flex items-center gap-1">
                      {(g.key === "unstaged" || g.key === "untracked") && (
                        <GroupVerb label="stage all" onClick={() => stage(id, paths)} />
                      )}
                      {g.key === "staged" && <GroupVerb label="unstage all" onClick={() => unstage(id, paths)} />}
                      {(g.key === "unstaged" || g.key === "untracked") && (
                        <GroupVerb label="discard all…" danger onClick={() => setDiscarding(paths)} />
                      )}
                    </span>
                  </div>
                  {open &&
                    rows.map((e) => (
                      <ContextMenu
                        key={e.path}
                        trigger={
                          <div
                            role="option"
                            aria-selected={selectedPath === e.path}
                            tabIndex={-1}
                            onClick={() => selectPath(id, e.path, e.conflicted)}
                            className={cn(rowCls, "flex h-5 items-center gap-1.5 pr-1.5 pl-4 text-sm", selectedPath === e.path && rowSelected)}
                          >
                            {e.conflicted ? (
                              <span className="w-3 shrink-0" />
                            ) : (
                              <input
                                type="checkbox"
                                className="h-3 w-3 shrink-0 accent-[var(--blue)] outline-none"
                                checked={e.staged}
                                aria-label={`${e.staged ? "unstage" : "stage"} ${e.path}`}
                                title={e.staged ? "staged — untick to take it out of the next commit" : "tick to stage it for the next commit"}
                                onClick={(ev) => ev.stopPropagation()}
                                onChange={() => (e.staged ? unstage(id, [e.path]) : stage(id, [e.path]))}
                              />
                            )}
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
                              <Chip color="var(--amber)" title="staged, and changed again since — tick to stage the rest">+ unstaged</Chip>
                            )}
                          </div>
                        }
                      >
                        {!e.conflicted && (e.staged ? <MenuItem onSelect={() => unstage(id, [e.path])}>Unstage</MenuItem> : <MenuItem onSelect={() => stage(id, [e.path])}>Stage</MenuItem>)}
                        <MenuItem onSelect={() => copyText(e.path)}>Copy path</MenuItem>
                        <MenuSeparator />
                        <MenuItem danger onSelect={() => setDiscarding([e.path])}>
                          {e.state === "??" ? "Delete this untracked file…" : "Discard changes…"}
                        </MenuItem>
                      </ContextMenu>
                    ))}
                </div>
              );
            })
          )}
        </div>
      </div>
      <Splitter onMouseDown={onDrag} title="drag to resize — a path is longer than a list is wide" />
      <div className="flex min-w-0 flex-1 flex-col">
        {conflict ? (
          <ConflictView id={id} path={conflict.path} base={conflict.base} ours={conflict.ours} theirs={conflict.theirs} truncated={conflict.truncated} />
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
      <Splitter onMouseDown={onDragCommit} />
      <div className="flex shrink-0 flex-col" style={{ width: cw }}>
        <CommitBox
          id={id}
          stagedCount={stagedCount}
          tipSubject={headSha && commits ? (commits.find((c) => c.sha.startsWith(headSha))?.summary ?? null) : null}
          lastError={[...consoleRows].reverse().find((r) => r.cmd === "git_commit" && r.error)?.error ?? null}
          status={status}
        />
      </div>
      {discarding && (
        <Dialog
          title={discarding.length === 1 ? "Discard this change?" : `Discard ${discarding.length} changes?`}
          subtitle="git keeps no copy — it cannot bring these back"
          onClose={() => setDiscarding(null)}
        >
          <div className="flex flex-col gap-3 px-3 py-3">
            <Dim className="text-xs">
              A tracked file goes back to HEAD. An untracked file is <b>deleted</b>. Nothing here is recoverable from git afterwards.
            </Dim>
            <ul className="m-0 max-h-60 list-none overflow-y-auto p-0">
              {discarding.map((p) => (
                <li key={p} className="py-0.5">
                  <Mono className="text-xs">{p}</Mono>
                </li>
              ))}
            </ul>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                className="border-[var(--red)] text-[var(--red)]"
                onClick={() => {
                  discard(id, discarding);
                  setDiscarding(null);
                }}
              >
                Discard {discarding.length === 1 ? "it" : `${discarding.length} files`}
              </Button>
              <Button variant="outline" onClick={() => setDiscarding(null)}>
                Keep {discarding.length === 1 ? "it" : "them"}
              </Button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
}

function GroupVerb({ label, onClick, danger }: { label: string; onClick: () => void; danger?: boolean }) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={cn(
        interactive,
        "rounded-sm px-1 text-2xs opacity-0 group-hover:opacity-100 focus-visible:opacity-100",
        danger ? "text-[var(--red)]" : "text-[var(--dim)] hover:text-[var(--text)]",
      )}
    >
      {label}
    </button>
  );
}

/**
 * The commit box. Commits only what is staged, never `-a`; the daemon runs
 * the hooks with `stdin` on `/dev/null`, so a hook that prompts fails loudly
 * instead of hanging a thread. The session trailer is on by default because
 * it is the reason committing here is worth anything — `R-F2` reads it back.
 */
function CommitBox({
  id,
  stagedCount,
  tipSubject,
  lastError,
  status,
}: {
  id: string;
  stagedCount: number;
  tipSubject: string | null;
  lastError: string | null;
  status: StatusEntry[] | null;
}) {
  const [message, setMessage] = useState("");
  const [amend, setAmend] = useState(false);
  const [trailer, setTrailer] = useState(true);
  const box = useRef<HTMLTextAreaElement>(null);
  /**
   * *Commit…* in the operations popup means **type the message now**.
   * `R-D31`.
   *
   * A counter rather than a boolean, and `focusRail`'s device: the tab also
   * mounts when the dock is simply opened, so an `autoFocus` would take the
   * keyboard every time you glanced at a diff — and two commits in a row have
   * to focus twice, which a boolean already true cannot do.
   */
  const wanted = useStore((s) => s.commitFocus);
  useEffect(() => {
    if (wanted > 0) box.current?.focus();
  }, [wanted]);
  // A commit was sent and not yet answered. The answer is the status
  // re-broadcast: when it arrives with nothing staged, the box empties.
  const sent = useRef(false);
  useEffect(() => {
    if (sent.current && status && stagedCount === 0) {
      sent.current = false;
      setMessage("");
      setAmend(false);
    }
  }, [status, stagedCount]);
  const blank = !message.trim();
  const nothing = stagedCount === 0 && !amend;
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
        <span className="text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">Commit</span>
        <Dim className="ml-auto text-2xs tabular-nums">{stagedCount} staged</Dim>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-1.5 p-2">
        <textarea
          ref={box}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          aria-label="commit message"
          placeholder="commit message — the first line is the subject"
          spellCheck={false}
          className={cn(
            "min-h-16 flex-1 resize-none rounded-sm border border-[var(--border)] bg-[var(--bg)] p-1.5 font-mono text-xs leading-4 text-[var(--text)]",
            "outline-none placeholder:text-[var(--dim)] hover:border-[var(--border-hover)] focus:border-[var(--ring)]",
          )}
        />
        <Checkbox
          checked={amend}
          label="amend the tip commit"
          title="replace the tip commit instead of adding one — the message box fills with its subject"
          onChange={(v) => {
            setAmend(v);
            if (v && blank && tipSubject) setMessage(tipSubject);
          }}
        />
        <Checkbox
          checked={trailer}
          label="name this session in a trailer"
          title="a Key: value trailer naming the session whose work this was, for prompt-blame (R-F2)"
          onChange={setTrailer}
        />
        <div className="flex items-center gap-2">
          <Button
            variant="solid"
            disabled={blank}
            title={blank ? "a message first — a blank subject is refused before git sees it" : nothing ? "nothing is staged: git will refuse, in its own words" : "commit what is staged"}
            onClick={() => {
              if (commit(id, message, amend, trailer)) sent.current = true;
            }}
          >
            Commit
          </Button>
          <Dim className="text-2xs">only what is staged · hooks run</Dim>
        </div>
        {lastError && (
          <Mono className="text-2xs whitespace-pre-wrap text-[var(--red)]" title="git's own words, from the Console">
            {lastError}
          </Mono>
        )}
      </div>
    </div>
  );
}

/**
 * `R-D16`: ours, base and theirs, side by side. `R-D28` adds the three ways
 * out — take ours, take theirs, or mark what is on disk resolved after
 * editing elsewhere — every one of which ends in `git add`, because in git a
 * conflict is resolved by staging the result. Whole-file only; anything
 * finer is editing. The content is deliberately not inspected.
 */
export function ConflictView({
  id,
  path,
  base,
  ours,
  theirs,
  truncated,
}: {
  id: string;
  path: string;
  base: string;
  ours: string;
  theirs: string;
  truncated: boolean;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
        <Mono className="truncate text-xs text-[var(--text-strong)]">{path}</Mono>
        <Dim className="shrink-0 text-2xs">three stages</Dim>
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" variant="outline" title="keep this branch's version, and stage it" onClick={() => resolve(id, path, "ours")}>
            take ours
          </Button>
          <Button size="sm" variant="outline" title="keep the incoming version, and stage it" onClick={() => resolve(id, path, "theirs")}>
            take theirs
          </Button>
          <Button size="sm" variant="outline" title="what is on disk is already resolved — you edited it elsewhere; stage it as it is" onClick={() => resolve(id, path, "mine")}>
            mark resolved
          </Button>
        </div>
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
