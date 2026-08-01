---
title: Wire protocol
status: active
updated: 2026-08-01
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

The `Git…` family was **read-only by protocol** until 2026-07-31: no staging,
commit, checkout, stash-pop, fetch or any other verb that mutates a
repository, and none to be added without an ADR — the observer rule, one
layer down. `GitCommits` echoes the ref scope and `GitAnnotation`
the revision it blamed, so a client that has since moved on can drop the
stray — the stray-session rule, applied to superseded scopes.

That ADR now exists.
[ADR-0012](../decisions/0012-write-locally-never-publish.md) admits a write
family — stage, unstage, discard, commit, branch, stash, resolve — and holds
the line at the **network**, so `fetch`, `pull` and `push` remain absent by
protocol.

**Four landed 2026-07-31.** `GitStage`, `GitUnstage` and `GitDiscard` (`R-D19`)
carry a `session_id` and a list of worktree paths; `GitCommit` (`R-D20`) carries
a message, an `amend` flag and a `session_trailer` flag. They are grouped in the
enum rather than filed beside their read siblings, so the guard that refuses
them can name a contiguous list and a fifth is visibly joining a family with a
rule. `R-D21` added five more the same day — `GitBranchCreate`, `GitSwitch`,
`GitStashPush`, `GitStashPop`, `GitStashDrop` — and `R-D22` added the last one, `GitResolve`.
The write family is complete: `R-D23` is a rendering change, and `R-D24`
(`fetch`, `pull`, `push`) stays refused by ADR-0012.

Branch names go through `valid_ref_name`, the *same* rule the read side uses to
scope a log: narrower than git's own, refusing a leading `-`, `..` and `@{`.
Sharing it matters more on this side, since reading a nonsense ref shows
nothing and writing one moves the worktree. A stash is addressed by index and
the `stash@{n}` string is built by the daemon, so no ref from outside reaches
that argument at all.

`GitResolve` takes a whole file — ours, theirs, or "what is on disk is already
right". Whole-file because that is what `R-D16`'s three-way view shows, and a
resolution mixing both sides is editing, which stays out permanently. Every
path ends in `git add`, because in git a conflict is resolved by *staging* the
result: a verb that wrote the file and left the index unmerged would show a
conflict that looks fixed and is not. The content is deliberately **not**
inspected — markers left in a file are committable once the index says
resolved, and a validator refusing them would refuse legitimate content too.

`GitSwitch` clears the pinned diff base of **every** session in that worktree
([A9](../product/assumptions.md)): a base is the last commit before a session
started, resolved once, and a switch can put it on another line of history
where it is no longer an ancestor of anything checked out. Clearing rather than
recomputing, because the scan loop already resolves a missing base — one place
that knows how to compute a base beats two.

`GitCommit` commits **only what is staged** — never `-a`. The staging list is
the instruction, and a commit verb that could sweep in a file deliberately left
unstaged would make the checkboxes a suggestion. The trailer is composed by the
daemon, not the client: the id recorded has to be the one the daemon knows the
session by, or `R-F2` could never look anything up with it.

Two properties hold for every one of them:

- **A dispatch-level guard**, not a per-verb check, refuses any write unless
  the bind is loopback or a token was presented (`A24`). The write family is
  enumerated in one function, so omitting a new verb from it is a visible gap
  in a list of three rather than a missing line in a 45-arm match. It is
  deliberately redundant with `server::admit`, which refuses to *start* a
  daemon that could reach it: `admit` guards the binary, this guards the
  router, and the router is what a test or an embedding constructs.
- **Every write answers by re-broadcasting `GitLocalChanges`**, never by
  reporting what it did. The client therefore holds no model of repository
  state that could drift from git's, and the pane shows what git says one
  round trip after the click — including when git did something other than
  what was asked.

Write failures carry **git's own words verbatim** — stderr when there is any,
stdout otherwise, because git splits refusals across both streams and the
commonest of all, `commit`'s "nothing to commit, working tree clean", arrives
on stdout with a non-zero exit. A paraphrase would throw away
the list of files and the hint that make git's own refusals actionable. See
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
                                       # R-D10/R-D11/R-D12, all read-only.
                                       # The write verbs are WebSocket-only:
                                       # there is no REST route that writes.
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

## Who is answering (`R-I5`)

`Snapshot` carries an optional `DaemonIdentity`, and `/api/health` returns the
same object under `daemon`:

| Field | For |
|---|---|
| `machine_id` | **the comparison.** `~/.mogeung/machine-id`, 16 random bytes written once |
| `host` | display only — "watching devbox" beats "watching 10.0.0.4:7717" |
| `claude_home` | which world this daemon watches; two homes on one box are two worlds |
| `pid`, `version` | so a client can name what to blame |
| `ssh_target` | how to reach this machine, when configured. Declared for `R-I6` |

This exists because the client used to answer *"am I looking at another
machine?"* with a substring test on the address it had dialled — a question
about routing standing in for a question about whose filesystem this is. An
`ssh -L 7717:localhost:7717` tunnel makes a remote daemon answer on
`127.0.0.1`, so the guess flipped to "local" and re-enabled every action that
opens an editor or a terminal. That tunnel is the *recommended* way to reach a
remote daemon, so the guess was wrong exactly where it mattered most.

**Hostnames are not the comparison, and deliberately so** — two VMs off one
image share a name, and a collision would silently re-enable the actions this
gates. **Unknown on either side means "not this machine"**: refusing prints a
sentence, acting on the wrong filesystem opens an editor on a path that is not
there.

A client older than this field ignores it; a client newer than a daemon that
does not send it falls back to the address guess rather than refusing
everything, because daemon and window sit on different machines and upgrade at
different times. Both directions are pinned by
`a_snapshot_survives_a_version_gap_in_both_directions`.

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

- **The daemon serves no TLS of its own.** It speaks plain HTTP and `ws://`, so
  a token sent straight to it travels in clear text
  ([A24](../product/assumptions.md) is the bet: trusted network, no TLS until
  the bet fails).

  What changed on 2026-07-31 is the *client* half: both clients are now built
  with `rustls` and can dial `wss://`, so putting a TLS-terminating reverse
  proxy in front of the daemon works. That is the recommended way to get
  encryption without the daemon growing certificate handling, key rotation and
  a renewal story it has no business owning.

  **The trap this walked into is worth remembering.** Enabling the
  `tokio-tungstenite` TLS feature alone left rustls with no crypto provider
  selected — which does not fail the build. It panics on the first TLS
  connection, inside the network thread. So both binaries name the provider
  explicitly (`ring`) at start-up, and
  `the_window_speaks_wss_not_just_ws` asserts a real TLS ClientHello reaches the
  wire, because the alternative failure is a panic nobody sees until they try
  the thing this was added for.
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
