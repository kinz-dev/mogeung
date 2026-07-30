---
title: Wire protocol
status: active
updated: 2026-07-30
covers:
  - crates/mogeung-core/src/wire.rs
  - crates/mogeungd/src/api.rs
---

# Wire protocol

One WebSocket carries live state: commands in, events out. Bulk reads are also
available as plain REST so the daemon is curl-able without a UI.

## Commands (`ClientMsg`)

| Command | Effect |
|---|---|
| `Subscribe` | Re-send the full snapshot |
| `SetHunkReviewed` | Mark or unmark one hunk |
| `ReviewAll` | Mark every hunk in the current diff |
| `RefreshChange` | Recompute a session's diff |
| `FetchEvents` | Replay stored transcript events from `since` |
| `ForgetSession` | Stop tracking; drop review state |
| `LaunchTerminal` | Open a real interactive `claude`, optionally in a new worktree |
| `Rescan` | Scan now instead of waiting for the next poll |
| `FetchHealth` | Ask what mogeung can and cannot currently see |
| `Snooze` | Silence a session for N minutes; 0 clears it |
| `FetchReviewDebt` | How much of a repo's agent output nobody has read |
| `FetchBlastRadius` | What else references the symbols a file's diff changed |
| `FocusTerminal` | Bring the terminal *app* a live session runs in to the front — iTerm2, Terminal.app, the tmux client; not a mogeung pane |
| `ListDir` | One directory of the session's worktree, for the explorer (`R-B24`) |
| `FetchFile` | One worktree file, capped and text-only — there is no write counterpart, by design |
| `ListTree` | Every worktree file path in one answer, for go-to-file (`R-B25`); gitignore-aware, capped at 20k with a `truncated` flag |
| `SearchContent` | Lines matching a literal query across the worktree (`R-B25`); smart-cased, capped at 500 matches |
| `GitLog` | A page of the session repo's commit log (`R-D10`), optionally scoped to a ref (`R-D11`) and narrowed by literal `grep`/`author`/`path`/`pickaxe` filters — a set path switches `--follow` on (file history, `R-D12`), `pickaxe` is `-S` ("when did this string appear", `R-D13`); each commit carries refs, parents and a session-attribution hint |
| `GitShow` | One commit's diff, in `Change`'s file/hunk shapes, plus its header — full message, committer, dates, parents (`R-D12`); the sha is validated as hex before git sees it |
| `GitStatus` | The repo's uncommitted state, staged and unstaged distinguished; conflicts marked, ignored paths included as `!!` dimming data |
| `GitDiffFile` | One uncommitted file against `HEAD` (`/dev/null` when untracked) |
| `GitBlame` | Per-line authorship, capped at 20k lines — of the worktree, or of the file at a revision (`rev`), which is what re-blame rides (`R-D11`) |
| `GitRefs` | Branches with tracking state, tags (annotated ones dereferenced to commits), remotes, HEAD, and `FETCH_HEAD`'s mtime — display only; the daemon never fetches |
| `GitStashes` | The stash list; `GitStashShow` one stash's diff by index — the `stash@{N}` spec is built daemon-side from a number |
| `GitSubmodules` | Submodule paths and their status prefix |
| `GitDiffRange` | The diff between two commits, both shas validated. `GitShow`, `GitDiffFile`, `GitStashShow` and this all take `context` (clamped ≤400) and `ignore_ws`, echoed back so a superseded cut is dropped (`R-D14`) |
| `GitCompare` | What merging a branch would bring: merge base resolved daemon-side, answered as a `GitRangeDiff` with real shas (`R-D15`) |
| `GitReflog` | Where HEAD has been — 100 entries, read-only recovery sight |
| `GitWorktrees` | `git worktree list`, including the ones mogeung itself created |
| `GitConflictFile` | A conflicted file's three stages (`:1:`/`:2:`/`:3:`), empty when a side has no version (`R-D16`) |
| `GitFileAtRev` | One file's content at a revision (`sha`, optionally `sha^`), for the Editor's revision tabs |

**Note what is absent:** nothing starts, steers or stops an agent
([ADR-0003](../decisions/0003-observe-do-not-spawn.md)).

`FocusTerminal` is not an exception. It moves *your* window; the agent is
untouched and nothing is typed. Nor is "copy as prompt" a command at all — the
client builds the text and puts it on your clipboard, and you paste it
([ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md)).

## Events (`ServerMsg`)

`Snapshot` · `SessionUpdated` · `SessionRemoved` · `Events` · `Queue` ·
`ChangeUpdated` · `Health` · `ReviewDebt` · `BlastRadius` · `DirListing` ·
`FileContent` · `TreeListing` · `ContentMatches` · `GitCommits` ·
`GitCommitDiff` · `GitLocalChanges` · `GitFileDiff` · `GitAnnotation` ·
`GitRefsInfo` · `GitStashList` · `GitStashDiff` · `GitSubmoduleList` ·
`GitRangeDiff` · `GitFileAtRevContent` · `GitReflogList` ·
`GitWorktreeList` · `GitConflictStages` · `Error`

`GitCommitDiff` hunks carry R-D8's read marks (`R-D17`): the daemon feeds
`parse_unified` the union of the repo's reviewed anchors, so a hunk a
human read in the Changes tab arrives already marked when seen through a
commit — anchors are content hashes, which is what makes the two views
agree. Its optional `CommitDetail` also names the branches containing
the commit (`R-D18`, serde-defaulted so either side may be older).

The `Git…` family (`R-D10`, `R-D11`) is **read-only by protocol**: there is
no staging, commit, checkout, stash-pop, fetch or any other verb that
mutates a repository, and none may be added without an ADR — the observer
rule, one layer down. `GitCommits` echoes the ref scope and `GitAnnotation`
the revision it blamed, so a client that has since moved on can drop the
stray — the stray-session rule, applied to superseded scopes.

That ADR now exists.
[ADR-0012](../decisions/0012-write-locally-never-publish.md) admits a write
family — stage, unstage, discard, commit, branch, stash, resolve — and holds
the line at the **network**, so `fetch`, `pull` and `push` remain absent by
protocol. **None of it is built**; every shipped `Git…` message is still a
read, and this paragraph describes the code as it stands. When the write
family lands it arrives with a dispatch-level guard refusing any write verb
unless the bind is loopback or a token was presented (`A24`) — the
"unauthenticated socket must not be able to spell a flag" rule below,
extended from arguments to verbs. See
[feature 0025](../features/0025-git-write-local.md).

Client-supplied git arguments are shape-checked before git sees them: shas
must be hex (one trailing `^` allowed — "the parent of"), ref names are
restricted to `[A-Za-z0-9/._-]` with no leading `-`, no `..` and no `@{`,
and stash indices are numbers. Log filters are literal text (`-i
--fixed-strings`, never regex), attached with `=` so they cannot open a
new argument, length-capped, control-characters refused; filter paths get
the explorer's lexical containment. An unauthenticated socket must not be
able to spell a flag.

`ListDir` and `FetchFile` paths are relative to the session root (repo root
when known, else cwd); the daemon canonicalises and refuses anything that
escapes it, symlinks included. `FileContent` is capped at 256 KiB with a
`truncated` flag, and binary files are refused — the explorer is a viewer, and
the daemon offers nothing that writes.

`ListTree` and `SearchContent` walk with the same containment, `.git` always
excluded and gitignore honoured when the root is a repo. Both run on the
blocking pool so a monorepo walk cannot wedge the event loop. Search skips
binary and oversized files *silently* — a search that errors on the one
unreadable file answers nothing. `ContentMatches` echoes the query so a client
can drop the answer to a search the user has since replaced — the stray-session
rule, applied to superseded queries.

`Health` is pushed after **every** scan, unsolicited. A client should never have
to ask whether the board it is showing is complete — see
[health-and-canary.md](health-and-canary.md).

## Design rules

**Commands are fire-and-forget.** Their effect returns on the event stream like
any other change, so clients stay pure projections with no correlation layer.

**Snapshot is unsolicited on connect**, so a client is useful before it sends
anything and reconnects self-heal.

**A slow client is dropped, not tolerated.** On broadcast lag the client is told
to reconnect rather than wedging the channel for everyone else.

**Malformed commands produce an error, not a disconnect.** Pinned by a test.

## REST

```
GET  /api/health          # liveness *and* whether it is still seeing everything
GET  /api/queue
GET  /api/repos
GET  /api/repos/{repo}/debt
GET  /api/sessions/{id}/blast?path=...
GET  /api/sessions/{id}/ls?path=...    # explorer (R-B24); path optional
GET  /api/sessions/{id}/file?path=...
GET  /api/sessions/{id}/tree           # every file path (R-B25)
GET  /api/sessions/{id}/search?q=...   # literal content search (R-B25)
GET  /api/sessions/{id}/git/log?skip=N&limit=N&rev=...&grep=...&author=...&path=...
                                       # R-D10/R-D11/R-D12, all read-only
GET  /api/sessions/{id}/git/show?sha=...
GET  /api/sessions/{id}/git/status
GET  /api/sessions/{id}/git/diff?path=...
GET  /api/sessions/{id}/git/blame?path=...&rev=...
GET  /api/sessions/{id}/git/refs
GET  /api/sessions/{id}/git/stashes
GET  /api/sessions/{id}/git/stash?index=N
GET  /api/sessions/{id}/git/submodules
GET  /api/sessions/{id}/git/range?from=...&to=...&context=N&ignore_ws=...
GET  /api/sessions/{id}/git/compare?branch=...
GET  /api/sessions/{id}/git/reflog
GET  /api/sessions/{id}/git/worktrees
GET  /api/sessions/{id}/git/conflict?path=...
GET  /api/sessions/{id}/git/file_at?sha=...&path=...
GET  /api/sessions
GET  /api/sessions/{id}
GET  /api/sessions/{id}/events?since=N
GET  /api/sessions/{id}/change
POST /api/sessions/{id}/review_all
POST /api/sessions/{id}/review     {"anchor": "...", "reviewed": true}
POST /api/rescan
```

`/api/health` returns a `headline`, `blind_ratio`, plain-language `alerts`, and
the full `detail` object. It is deliberately curl-able: "is the board empty
because nothing is happening, or because mogeung went blind?" should not require
a window.

## There is no bundled second client (`R-C3`, removed)

`GET /` used to serve one self-contained HTML file speaking this same protocol —
a phone client for triage. It shipped, was never opened, and was removed on
2026-07-30. The point it proved survives it: a client is a projection with no
local authority ([ADR-0001](../decisions/0001-rust-core-with-egui-ui.md)), so a
second one costs no daemon change. The REST surface below is the standing offer;
nothing needs to be added here to take it up.

## Security

**Loopback by default.** `127.0.0.1:7717` unless `--listen` says otherwise, and
the daemon logs a warning at startup when the bind is not a loopback address —
anyone who can reach a non-loopback port can read every transcript on the
machine and open terminals on it.

**A shared token, mandatory beyond loopback** (`R-I4`, tightened by `R-I10`).
`--token` gates every HTTP and WS request; clients send
`Authorization: Bearer …` or `?token=…`, the query form existing because a
browser socket cannot set headers. Comparison is constant time, leaking length
only. A wrong token is a clean 401.

`server::admit` decides this **before the daemon serves anything** — before the
database is opened, before the first scan — and a non-loopback bind with no
token is an error that stops start-up rather than a warning that scrolls past.
There is no `--insecure`: an override becomes the documented workaround, and
binding loopback behind an ssh tunnel is both available and strictly better
than a token in clear text. The window applies the same rule to the daemon it
hosts, checked on the main thread so the refusal is visible rather than dying
in a background one.

Two things this still does *not* do, tracked under `R-I10`:

- **No TLS.** The token and everything after it travel in clear text
  ([A24](../product/assumptions.md) is the bet: trusted network, no TLS until
  the bet fails). A reverse proxy is the obvious answer and does not work yet —
  the window is built with no TLS feature in `tokio-tungstenite`, so it cannot
  dial `wss://` at all.
- **Argument hygiene is not authentication.** The shape-checks above stop a
  client spelling a flag; they say nothing about *which* client.

Until then: loopback, or an ssh tunnel. See
[guide/remote.md](../guide/remote.md).

## The 2026-07-29 families

The one-go pass (features 0015–0022) grew the contract in five places,
all in the established shapes — fire-and-forget commands, answers that
echo their question, `#[serde(default)]` on everything new:

- **Usage** — `FetchUsage` → `UsageStats`. Tokens only (ADR-0005); the
  window-limit figure inside is an estimate from observed limit hits and
  is labelled so on the type.
- **Signals** — `SetSignalCommand` / `RunSignal` / `FetchSignal` →
  `SignalStatus`. The single place a client can make the daemon execute
  anything, and it is a human-configured check run on an explicit click;
  there is deliberately no timer that can reach it.
- **Insight** — `InsightSearch`, `FetchDigest`, `FetchRecurring`,
  `FetchPromptLibrary`, `FetchAnalytics`, `FetchSubagents`,
  `FetchDecisions`, `FetchFileSessions` → their `*Report` twins, each
  echoing query/day/path so stale answers drop.
- **Docs** — `FetchDocScan` → `DocReport`, per watched repo only — the
  daemon refuses paths no session lives in.
- **Auth (`R-I4`)** — `router_with_token` wraps everything when the
  daemon starts with `--token`: `Authorization: Bearer …` or `?token=`
  (the WS client cannot set headers), constant-time compare, clean 401.
  No token → the historical open-on-loopback behaviour, byte for byte.

Every WS command keeps a REST twin (`/api/usage`, `/api/insight/*`,
`/api/repos/{repo}/docscan`, …) so the daemon stays curl-able.
