/**
 * The session repo's git state. `R-D10`, deepened by `R-D11`–`R-D15`.
 *
 * **Read-only from end to end in this client.** The wire does carry a write
 * family (`R-D19`/`R-D20`), and the daemon will honour it — but nothing here
 * sends one. Staging and committing are exactly the pressure
 * [ADR-0012](../../docs/decisions/0012-write-locally-never-publish.md)
 * anticipated, and adding them is a decision rather than a port.
 *
 * `git fetch` is the single outbound network call in the whole product, on an
 * explicit keystroke, admitted by
 * [ADR-0014](../../docs/decisions/0014-fetch-is-not-publishing.md). It never
 * runs on a timer and it never pushes or merges.
 */

import { useEffect, useRef, useState } from "react";
import { CloudDownload, Columns2, Filter, GitBranch, GitCommitVertical, RefreshCw } from "lucide-react";
import { useStore, useSelectedSession } from "@/store";
import { Chip, Dim, Empty, IconButton, Input, Mono, PaneHeader, Row, Segmented } from "@/ui/primitives";
import { DiffList } from "@/ui/DiffView";
import { dirTail, stamp } from "@/lib/format";
import { cn } from "@/lib/cn";

type View = "log" | "local" | "refs" | "stashes" | "more";

/** The lists that are one round-trip each and are asked for on demand. */
const ON_DEMAND = {
  reflog: "git_reflog",
  worktrees: "git_worktrees",
  submodules: "git_submodules",
} as const;

/**
 * A path in a column narrower than it is.
 *
 * Plain `truncate` spends the width it has on the leading directories and then
 * cuts off the file name — the one part you were reading, and the reason a
 * column of `desktop/src/store/prefs.…` says nothing about which seven files
 * changed. [`dirTail`] is the house answer and is used here rather than a
 * second one: whole segments come off the front, never mid-name, because a
 * half-cut directory reads as a directory that exists.
 *
 * The budget is in characters and the column is in pixels, so it is estimated
 * from the pane's width — deliberately roughly. It only decides *which end*
 * gives way; CSS truncation is still the backstop, and the untouched path is on
 * the title for when two directories end the same way.
 */
function FilePath({ path, chars }: { path: string; chars: number }) {
  const shown = dirTail(path, chars);
  const cut = shown.lastIndexOf("/");
  const dir = cut < 0 ? "" : shown.slice(0, cut + 1);
  const name = shown.slice(cut + 1);
  return (
    <span className="flex min-w-0 items-baseline" title={path}>
      {dir && <Mono className="truncate text-xs text-[var(--dim)]">{dir}</Mono>}
      <Mono className="shrink-0 text-xs">{name}</Mono>
    </span>
  );
}

export function GitPane() {
  const s = useSelectedSession();
  const id = s?.id ?? null;
  const git = useStore((st) => (st.selected ? st.git[st.selected] : undefined));
  const patchGit = useStore((st) => st.patchGit);
  const send = useStore((st) => st.send);
  // Field by field, for the reason ChangesPane states: this header sits above
  // every line of the diff.
  const sideBySide = useStore((st) => st.prefs.sideBySide);
  const setPrefs = useStore((st) => st.setPrefs);
  const [view, setView] = useState<View>("log");
  const [grep, setGrep] = useState("");
  const [author, setAuthor] = useState("");
  const [path, setPath] = useState("");
  const [pickaxe, setPickaxe] = useState("");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [more, setMore] = useState<keyof typeof ON_DEMAND>("reflog");
  const [compareTo, setCompareTo] = useState("");
  const [rangeFrom, setRangeFrom] = useState("");
  const [rangeTo, setRangeTo] = useState("");
  const repoRoot = s?.repo_root ?? null;

  // The list is a column of paths, and 340px was not enough for most of them.
  // Same idiom as the queue's edge: local state during the drag, the
  // preferences file written once on release.
  const savedWidth = useStore((st) => st.prefs.gitSidebarWidth);
  const [width, setWidth] = useState(savedWidth);
  const latest = useRef(width);
  const onDrag = (e: React.MouseEvent) => {
    const startX = e.clientX;
    const startW = latest.current;
    const move = (ev: MouseEvent) => {
      // Through a ref as well as through state: the `up` below is registered
      // once and closes over this render's `width` for ever, so reading the
      // state there saves the width the drag *started* at.
      latest.current = Math.min(900, Math.max(220, startW + ev.clientX - startX));
      setWidth(latest.current);
    };
    const up = () => {
      setPrefs({ gitSidebarWidth: latest.current });
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  // How many `text-xs` monospace characters this width holds, less the state
  // column and the padding. An estimate, and only ever an estimate — see
  // [`FilePath`].
  const chars = Math.max(12, Math.floor((width - 56) / 7));

  // One door for fetching, in the render, so a docked pane works unswitched.
  useEffect(() => {
    if (!id) return;
    // **Only for a repository.** The pane already refuses to render for a
    // session that has no `repo_root`, but the fetch ran regardless — so
    // selecting a non-repo session fired three git questions the daemon could
    // only answer with an error, and the window turned that into a red banner
    // you had to dismiss by hand. Not asking is the fix; making the answer
    // quieter is a separate one.
    if (!repoRoot) return;
    if (!git?.commits.length) send({ cmd: "git_log", session_id: id, skip: 0, limit: 100 });
    if (!git?.status) send({ cmd: "git_status", session_id: id });
    if (!git?.refs) send({ cmd: "git_refs", session_id: id });
  }, [id, repoRoot, git?.commits.length, git?.status, git?.refs, send]);

  if (!s) return <Empty>select a session</Empty>;
  if (!s.repo_root) return <Empty hint="git needs a repository">this session is not in a git repo</Empty>;

  const selectCommit = (sha: string) => {
    if (!id) return;
    patchGit(id, {
      selected: sha,
      selectedPath: null,
      diff: null,
      detail: null,
      conflict: null,
      diffLabel: null,
    });
    send({ cmd: "git_show", session_id: id, sha });
  };

  /**
   * One place that asks for a log, because there are now five things that
   * narrow it — message, author, path, pickaxe and the branch scope — and a
   * second call site would inevitably drop one of them and look like the
   * filter had been ignored.
   */
  const askLog = (skip: number) => {
    if (!id) return;
    if (skip === 0) patchGit(id, { commits: [], grep, author, path, pickaxe });
    send({
      cmd: "git_log",
      session_id: id,
      skip,
      limit: 100,
      rev: git?.rev ?? null,
      grep: grep || null,
      author: author || null,
      path: path || null,
      pickaxe: pickaxe || null,
    });
  };

  return (
    <div className="flex h-full min-h-0 bg-[var(--bg-panel)]">
      <div className="flex shrink-0 flex-col border-r border-[var(--border)]" style={{ width }}>
        <PaneHeader title="Git">
          {/* The pref is shared with Changes, and until now only Changes could
              reach it — so the diff on this side of the window was whatever you
              had last set on the other, with nothing here saying so. */}
          <IconButton
            title="side by side — the file as it was left, as it is right  (R-D6)"
            active={sideBySide}
            onClick={() => setPrefs({ sideBySide: !sideBySide })}
          >
            <Columns2 size={13} />
          </IconButton>
          <IconButton
            title="update remote-tracking refs — the only outbound call there is (Ctrl+T)"
            onClick={() => {
              if (!id) return;
              patchGit(id, { fetching: true });
              send({ cmd: "git_fetch", session_id: id });
            }}
          >
            <CloudDownload size={13} />
          </IconButton>
          <IconButton
            title="reload"
            onClick={() => {
              if (!id) return;
              patchGit(id, { commits: [], status: null, refs: null });
            }}
          >
            <RefreshCw size={13} />
          </IconButton>
        </PaneHeader>

        <div className="flex shrink-0 items-center border-b border-[var(--border)] px-2 py-1">
          <Segmented
            value={view}
            onChange={(v) => {
              setView(v);
              if (v === "stashes" && id) send({ cmd: "git_stashes", session_id: id });
              if (v === "more" && id) send({ cmd: ON_DEMAND[more], session_id: id });
            }}
            options={[
              { value: "log", label: "log", title: "commits, newest first" },
              { value: "local", label: "local", title: "uncommitted changes" },
              { value: "refs", label: "refs", title: "branches, tags and remotes" },
              { value: "stashes", label: "stashes", title: "shelved work" },
              { value: "more", label: "more", title: "reflog, worktrees, submodules, compare" },
            ]}
          />
        </div>

        {view === "log" && (
          <>
            <div className="shrink-0 px-2 py-1">
              <div className="flex items-center gap-1">
                <Input
                  value={grep}
                  onChange={setGrep}
                  placeholder="filter messages — Enter"
                  onKeyDown={(e) => e.key === "Enter" && askLog(0)}
                />
                <IconButton
                  title="by author, by path, and pickaxe  (R-D12, R-D13)"
                  active={filtersOpen || !!(author || path || pickaxe)}
                  onClick={() => setFiltersOpen(!filtersOpen)}
                >
                  <Filter size={12} />
                </IconButton>
              </div>
              {filtersOpen && (
                <div className="mt-1 space-y-1">
                  <Input value={author} onChange={setAuthor} placeholder="author" onKeyDown={(e) => e.key === "Enter" && askLog(0)} />
                  <Input value={path} onChange={setPath} placeholder="path — the file's history" mono onKeyDown={(e) => e.key === "Enter" && askLog(0)} />
                  <Input
                    value={pickaxe}
                    mono
                    onChange={setPickaxe}
                    placeholder="pickaxe: when did this string appear or vanish"
                    onKeyDown={(e) => e.key === "Enter" && askLog(0)}
                  />
                  <Dim className="block text-2xs">
                    Enter runs the query. `pickaxe` is git's `-S`: commits where the number of
                    occurrences of that text changed, which is how you find where a thing was
                    introduced or deleted rather than merely mentioned.
                  </Dim>
                </div>
              )}
              {git?.rev && (
                <div className="mt-1 flex items-center gap-1">
                  <Chip color="var(--amber)">scoped to {git.rev}</Chip>
                  <button
                    type="button"
                    onClick={() => {
                      if (!id) return;
                      patchGit(id, { rev: null, commits: [] });
                      send({ cmd: "git_log", session_id: id, skip: 0, limit: 100, grep: grep || null });
                    }}
                    className="rounded-sm text-2xs text-[var(--dim)] outline-none hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-[var(--ring)]"
                  >
                    clear
                  </button>
                </div>
              )}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {!git?.commits.length ? (
                <Empty>reading the log…</Empty>
              ) : (
                git.commits.map((c) => (
                  <Row
                    key={c.sha}
                    selected={git.selected === c.sha}
                    onClick={() => selectCommit(c.sha)}
                    className="border-b border-[var(--border)] py-1"
                  >
                    <div className="flex items-center gap-1.5">
                      <Mono className="shrink-0 text-2xs text-[var(--amber)]">{c.short}</Mono>
                      <span className="truncate text-sm">{c.summary}</span>
                      {c.touches_session && (
                        <Chip color="var(--blue)" title="lands in this session's lifetime and touches files it edited — a hint, not an author column">
                          session
                        </Chip>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      <Dim className="truncate text-2xs">{c.author}</Dim>
                      <Dim className="ml-auto shrink-0 text-2xs">{stamp(c.epoch)}</Dim>
                    </div>
                    {c.refs.length > 0 && (
                      <div className="mt-0.5 flex flex-wrap gap-1">
                        {c.refs.map((r) => (
                          <Chip key={r} color="var(--green)">
                            {r}
                          </Chip>
                        ))}
                      </div>
                    )}
                  </Row>
                ))
              )}
              {git && !git.done && git.commits.length > 0 && (
                <button
                  type="button"
                  onClick={() => askLog(git.commits.length)}
                  className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] w-full py-1 text-2xs text-[var(--dim)] hover:bg-[var(--bg-faint)]"
                >
                  load more
                </button>
              )}
            </div>
          </>
        )}

        {view === "local" && (
          <div className="min-h-0 flex-1 overflow-y-auto">
            {!git?.status ? (
              <Empty>reading status…</Empty>
            ) : git.status.length === 0 ? (
              <Empty>the working tree is clean</Empty>
            ) : (
              git.status
                .filter((e) => e.state !== "!!")
                .map((e) => (
                  <Row
                    key={e.path}
                    selected={git.selectedPath === e.path}
                    onClick={() => {
                      if (!id) return;
                      patchGit(id, {
                        selected: null,
                        selectedPath: e.path,
                        diff: null,
                        detail: null,
                        conflict: null,
                        diffLabel: null,
                      });
                      // A conflicted file has no ordinary diff worth reading —
                      // it has three sides. `R-D16`.
                      if (e.conflicted) send({ cmd: "git_conflict_file", session_id: id, path: e.path });
                      else send({ cmd: "git_diff_file", session_id: id, path: e.path });
                    }}
                    className="flex items-center gap-2 border-b border-[var(--border)] py-0.5"
                  >
                    <Mono
                      className={cn(
                        "w-6 shrink-0 text-2xs",
                        e.conflicted ? "text-[var(--red)]" : e.staged ? "text-[var(--add-fg)]" : "text-[var(--amber)]",
                      )}
                    >
                      {e.state}
                    </Mono>
                    <FilePath path={e.path} chars={chars} />
                    {e.conflicted && <Chip color="var(--red)">conflict</Chip>}
                  </Row>
                ))
            )}
          </div>
        )}

        {view === "refs" && (
          <div className="min-h-0 flex-1 overflow-y-auto">
            {!git?.refs ? (
              <Empty>reading refs…</Empty>
            ) : (
              <>
                <div className="px-2 py-1">
                  <Dim className="text-2xs">
                    HEAD {git.refs.head ?? "(detached)"} · {git.refs.head_sha.slice(0, 8)}
                  </Dim>
                  {git.refs.fetch_epoch && (
                    <Dim className="block text-2xs">last fetch {stamp(git.refs.fetch_epoch)}</Dim>
                  )}
                </div>
                {git.refs.branches.map((b) => (
                  <Row
                    key={b.name}
                    onClick={() => {
                      if (!id) return;
                      patchGit(id, { commits: [], rev: b.name });
                      send({ cmd: "git_log", session_id: id, skip: 0, limit: 100, rev: b.name });
                      setView("log");
                    }}
                    className="flex items-center gap-2 border-b border-[var(--border)] py-0.5"
                    title="scope the log to this branch — nothing is checked out"
                  >
                    <GitBranch size={11} className="shrink-0 text-[var(--dim)]" />
                    <span className={cn("truncate text-sm", b.current && "text-[var(--green)]")}>{b.name}</span>
                    {(b.ahead > 0 || b.behind > 0) && (
                      <Dim className="ml-auto shrink-0 text-2xs">
                        ↑{b.ahead} ↓{b.behind}
                      </Dim>
                    )}
                  </Row>
                ))}
              </>
            )}
          </div>
        )}

        {view === "stashes" && (
          <div className="min-h-0 flex-1 overflow-y-auto">
            {!git?.stashes ? (
              <Empty>reading stashes…</Empty>
            ) : git.stashes.length === 0 ? (
              <Empty>nothing stashed</Empty>
            ) : (
              git.stashes.map((st) => (
                <Row
                  key={st.index}
                  onClick={() => id && send({ cmd: "git_stash_show", session_id: id, index: st.index })}
                  className="border-b border-[var(--border)] py-0.5"
                >
                  <Mono className="text-2xs text-[var(--amber)]">stash@{`{${st.index}}`}</Mono>{" "}
                  <span className="text-xs">{st.message}</span>
                </Row>
              ))
            )}
          </div>
        )}

        {view === "more" && (
          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="px-2 py-1">
              <Segmented
                value={more}
                onChange={(v) => {
                  setMore(v);
                  if (id) send({ cmd: ON_DEMAND[v], session_id: id });
                }}
                options={[
                  { value: "reflog", label: "reflog", title: "where HEAD has been — including what a reset moved off" },
                  { value: "worktrees", label: "worktrees", title: "every checkout of this repository" },
                  { value: "submodules", label: "submodules", title: "nested repositories and their state" },
                ]}
              />
            </div>

            {more === "reflog" &&
              (!git?.reflog ? (
                <Empty>reading the reflog…</Empty>
              ) : git.reflog.length === 0 ? (
                <Empty>nothing in the reflog</Empty>
              ) : (
                git.reflog.map((e, i) => (
                  <Row
                    key={`${e.sha}:${i}`}
                    onClick={() => selectCommit(e.sha)}
                    className="border-b border-[var(--border)] py-0.5"
                    title="show this commit — the reflog is how you find work a reset moved off a branch"
                  >
                    <div className="flex items-center gap-1.5">
                      <Mono className="shrink-0 text-2xs text-[var(--amber)]">{e.sha.slice(0, 8)}</Mono>
                      <Mono className="shrink-0 text-2xs text-[var(--dim)]">{e.selector}</Mono>
                    </div>
                    <div className="truncate text-xs">{e.summary}</div>
                  </Row>
                ))
              ))}

            {more === "worktrees" &&
              (!git?.worktrees ? (
                <Empty>reading worktrees…</Empty>
              ) : (
                git.worktrees.map((w) => (
                  <div key={w.path} className="border-b border-[var(--border)] px-2 py-1">
                    <FilePath path={w.path} chars={chars} />
                    <div className="flex items-center gap-2">
                      <Dim className="text-2xs">{w.branch ?? "(detached)"}</Dim>
                      <Mono className="text-2xs text-[var(--dim)]">{w.sha.slice(0, 8)}</Mono>
                    </div>
                  </div>
                ))
              ))}

            {more === "submodules" &&
              (!git?.submodules ? (
                <Empty>reading submodules…</Empty>
              ) : git.submodules.length === 0 ? (
                <Empty>this repository has no submodules</Empty>
              ) : (
                git.submodules.map((m) => (
                  <div key={m.path} className="border-b border-[var(--border)] px-2 py-1">
                    <FilePath path={m.path} chars={chars} />
                    <div className="flex items-center gap-2">
                      <Mono className="text-2xs text-[var(--dim)]">{m.sha.slice(0, 8)}</Mono>
                      {m.note && <Dim className="truncate text-2xs">{m.note}</Dim>}
                      {/* The state character is git's own — `-` uninitialised,
                          `+` moved, `U` conflicted — so it is shown rather than
                          translated into a word that would be a guess. */}
                      {m.state.trim() && <Chip color="var(--amber)">{m.state}</Chip>}
                    </div>
                  </div>
                ))
              ))}

            {/* Two ends, two shapes of question. Compare asks "what is on that
                branch that HEAD has not got" from the merge base, which is the
                three-dot comparison people mean and rarely type; the range is
                the literal two-ended one for when you know both. `R-D15`. */}
            <div className="border-t border-[var(--border)] px-2 py-1">
              <Dim className="mb-1 block text-2xs">compare with a branch — from the merge base</Dim>
              <div className="flex items-center gap-1">
                <Input
                  value={compareTo}
                  mono
                  onChange={setCompareTo}
                  placeholder="origin/main"
                  onKeyDown={(e) => {
                    if (e.key !== "Enter" || !id || !compareTo.trim()) return;
                    send({ cmd: "git_compare", session_id: id, branch: compareTo.trim() });
                  }}
                />
              </div>
              <Dim className="mt-2 mb-1 block text-2xs">or a literal range</Dim>
              <div className="flex items-center gap-1">
                <Input value={rangeFrom} mono onChange={setRangeFrom} placeholder="from" />
                <Input
                  value={rangeTo}
                  mono
                  onChange={setRangeTo}
                  placeholder="to — Enter"
                  onKeyDown={(e) => {
                    if (e.key !== "Enter" || !id || !rangeFrom.trim() || !rangeTo.trim()) return;
                    send({
                      cmd: "git_diff_range",
                      session_id: id,
                      from: rangeFrom.trim(),
                      to: rangeTo.trim(),
                    });
                  }}
                />
              </div>
            </div>
          </div>
        )}

        {git?.fetched && (
          <div className="max-h-24 shrink-0 overflow-y-auto border-t border-[var(--border)] px-2 py-1">
            <Dim className="text-2xs">
              {git.fetched.length === 0 ? "fetch: the remotes had nothing new" : git.fetched.join("\n")}
            </Dim>
          </div>
        )}
      </div>

      <div
        onMouseDown={onDrag}
        className="w-1 shrink-0 cursor-col-resize hover:bg-[var(--blue)]"
        title="drag to resize — a path is longer than a list is wide"
      />

      <div className="flex min-w-0 flex-1 flex-col">
        {git?.detail && (
          <div className="shrink-0 border-b border-[var(--border)] px-3 py-2">
            <div className="text-sm whitespace-pre-wrap text-[var(--text-strong)]">{git.detail.message}</div>
            <Dim className="mt-1 block text-2xs">
              {git.detail.author} · {stamp(git.detail.epoch)}
              {git.detail.branches.length > 0 && ` · in ${git.detail.branches.join(", ")}`}
            </Dim>
          </div>
        )}
        {git?.diffLabel && (
          <div className="shrink-0 border-b border-[var(--border)] px-3 py-1">
            <Dim className="text-2xs">comparing {git.diffLabel}</Dim>
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-y-auto">
          {git?.conflict ? (
            /* `R-D16`: ours, base and theirs, side by side and read-only.
               mogeung does not resolve anything — the wire has `git_resolve`
               and this client does not send it. What a conflict needs first is
               to be *read*, and the markers in the worktree file are the one
               view that shows neither original. */
            <div className="flex h-full min-h-0 flex-col">
              <div className="shrink-0 border-b border-[var(--border)] px-3 py-1">
                <Mono className="text-xs text-[var(--text-strong)]">{git.conflict.path}</Mono>
                <Dim className="ml-2 text-2xs">
                  three stages, read-only — resolving is git's job, in your terminal
                </Dim>
              </div>
              <div className="grid min-h-0 flex-1 grid-cols-3">
                {(
                  [
                    ["ours", git.conflict.ours, "var(--add-fg)"],
                    ["base", git.conflict.base, "var(--dim)"],
                    ["theirs", git.conflict.theirs, "var(--del-fg)"],
                  ] as const
                ).map(([name, body, colour]) => (
                  <div key={name} className="flex min-h-0 flex-col border-r border-[var(--border)]">
                    <div className="shrink-0 px-2 py-0.5 text-2xs" style={{ color: colour }}>
                      {name}
                    </div>
                    <pre className="min-h-0 flex-1 overflow-auto px-2 font-mono text-2xs whitespace-pre">
                      {body || "(empty on this side)"}
                    </pre>
                  </div>
                ))}
              </div>
              {git.conflict.truncated && (
                <Dim className="shrink-0 px-3 py-1 text-2xs">
                  one of the sides went past the size cap — this is its head
                </Dim>
              )}
            </div>
          ) : git?.diff?.length === 0 ? (
            /* An answer with no files at all is not the same as no answer, and
               rendering an empty list for it looks exactly like the pane
               having ignored the click. A binary file and a mode-only change
               both arrive this way. */
            <Empty hint="a binary file, or a change git shows no text for">
              nothing to show for {git.selectedPath ?? "that"}
            </Empty>
          ) : git?.diff && id ? (
            <DiffList files={git.diff} sessionId={id} />
          ) : (
            <Empty hint="every diff here is read-only, permanently">
              <span className="inline-flex items-center gap-1">
                <GitCommitVertical size={12} /> pick a commit or a changed file
              </span>
            </Empty>
          )}
        </div>
      </div>
    </div>
  );
}
