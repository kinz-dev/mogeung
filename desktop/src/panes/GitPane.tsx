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

import { useEffect, useState } from "react";
import { CloudDownload, GitBranch, GitCommitVertical, RefreshCw } from "lucide-react";
import { useStore, useSelectedSession } from "@/store";
import { Chip, Dim, Empty, IconButton, Input, Mono, PaneHeader, Row, Segmented } from "@/ui/primitives";
import { DiffList } from "@/ui/DiffView";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";

type View = "log" | "local" | "refs" | "stashes";

export function GitPane() {
  const s = useSelectedSession();
  const id = s?.id ?? null;
  const git = useStore((st) => (st.selected ? st.git[st.selected] : undefined));
  const patchGit = useStore((st) => st.patchGit);
  const send = useStore((st) => st.send);
  const [view, setView] = useState<View>("log");
  const [grep, setGrep] = useState("");
  const repoRoot = s?.repo_root ?? null;

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
    patchGit(id, { selected: sha, diff: null, detail: null });
    send({ cmd: "git_show", session_id: id, sha });
  };

  return (
    <div className="flex h-full min-h-0 bg-[var(--bg-panel)]">
      <div className="flex w-[340px] shrink-0 flex-col border-r border-[var(--border)]">
        <PaneHeader title="Git">
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
            }}
            options={[
              { value: "log", label: "log", title: "commits, newest first" },
              { value: "local", label: "local", title: "uncommitted changes" },
              { value: "refs", label: "refs", title: "branches, tags and remotes" },
              { value: "stashes", label: "stashes", title: "shelved work" },
            ]}
          />
        </div>

        {view === "log" && (
          <>
            <div className="shrink-0 px-2 py-1">
              <Input
                value={grep}
                onChange={setGrep}
                placeholder="filter messages — Enter"
                onKeyDown={(e) => {
                  if (e.key !== "Enter" || !id) return;
                  patchGit(id, { commits: [], grep });
                  send({ cmd: "git_log", session_id: id, skip: 0, limit: 100, grep: grep || null });
                }}
              />
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
                  onClick={() =>
                    id && send({ cmd: "git_log", session_id: id, skip: git.commits.length, limit: 100, grep: grep || null })
                  }
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
                    onClick={() => {
                      if (!id) return;
                      patchGit(id, { selected: null, diff: null });
                      send({ cmd: "git_diff_file", session_id: id, path: e.path });
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
                    <Mono className="truncate text-xs">{e.path}</Mono>
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

        {git?.fetched && (
          <div className="max-h-24 shrink-0 overflow-y-auto border-t border-[var(--border)] px-2 py-1">
            <Dim className="text-2xs">
              {git.fetched.length === 0 ? "fetch: the remotes had nothing new" : git.fetched.join("\n")}
            </Dim>
          </div>
        )}
      </div>

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
        <div className="min-h-0 flex-1 overflow-y-auto">
          {git?.diff && id ? (
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
