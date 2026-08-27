---
title: Wire protocol
status: active
updated: 2026-08-26
covers:
  - crates/mogeung-core/src/wire.rs
  - crates/mogeung-core/src/pricing.rs
  - crates/mogeung-core/src/usage.rs
  - crates/mogeungd/src/api.rs
---

# Wire protocol

One WebSocket carries live state: commands in, events out. Bulk reads are also
available as plain REST so the daemon is curl-able without a UI.

## Commands (`ClientMsg`)

| Command | Effect |
|---|---|
| `Subscribe` | **Re-send** the full snapshot. The daemon already pushes one on connect, before the socket has said anything — so this is the explicit recovery path, not the way a client gets its first board. The window does not send it on open: doing so bought a second copy of the largest payload on the wire and nothing else (`R-J69`) |
| `SetHunkReviewed` | Mark or unmark one hunk |
| `ReviewAll` | Mark every hunk in the current diff |
| `RefreshChange` | The full diff — hunks included — answered **on the asking socket only** (`R-J59`). Served from the daemon's cache unless `force` (serde-defaulted, so older clients keep their meaning) insists on git being consulted again; selection changes and summary-driven refreshes want the current answer cheaply, the pane's *recompute from disk* button wants a recompute |
| `FetchEvents` | Replay stored transcript events from `since` |
| `ForgetSession` | Stop tracking; drop review state |
| `LaunchTerminal` | Open a real interactive agent CLI, optionally in a new worktree. `source` picks which one (`R-J51`) and is `#[serde(default)]` — a client built before the choice existed omits it and gets Claude Code, which is the answer it was already getting. All three CLIs have a recipe since `R-J72`; a source the daemon had none for was an error in words, never a different agent started quietly, and that arm remains the answer for any source added to the enum before its recipe. Codex's is deliberately **not** the analogue of the other two — `--ask-for-approval never --sandbox workspace-write`, not `--dangerously-bypass-approvals-and-sandbox`, because that flag also turns off a sandbox neither sibling CLI has to give up, and a click saying *yolo* does not get to make that trade for you. `headless` (`R-J61`, also serde-defaulted, to the terminal window older clients were already getting) starts a detached tmux session and opens no window at all — `yolomo -d` as a daemon path; without tmux it refuses in words rather than falling back, because a fallback that opened a window would be the opposite of what was asked |
| `Rescan` | Scan now instead of waiting for the next poll |
| `FetchHealth` | Ask what mogeung can and cannot currently see |
| `Snooze` | Silence a session for N minutes; 0 clears it |
| `FetchReviewDebt` | How much of a repo's agent output nobody has read |
| `FetchBlastRadius` | What else references the symbols a file's diff changed |
| `FocusTerminal` | Bring the terminal *app* a live session runs in to the front — iTerm2, Terminal.app, the tmux client; not a mogeung pane |
| `OpenFolder` | Show a session's `cwd` in the machine's file manager — Finder on macOS, `xdg-open`'s handler elsewhere (`R-J34`). A handoff, and it runs where the daemon is, because a path is not the same answer on two machines |
| `FetchWorkspace` | A session's own root, the folders added to it by hand, any that have gone missing (`R-J40`), and the folders mogeung has *noticed* it working in — `WorkspaceHint { path, source, files }`, offered and never added (`R-J39`) |
| `AddWorkspaceDir` / `RemoveWorkspaceDir` | Add or drop a folder. **Gated with the repository writes** — not because they write one, but because they widen what this daemon will read out, which is ADR-0012's rule wearing a third hat |
| `ListDir` | One directory of the session's worktree, for the explorer (`R-B24`). A path is relative to the session's own root, **or absolute** — in which case it is served only from inside a folder the workspace holds (`R-J40`) |
| `FetchFile` | One worktree file, capped and text-only — there is no write counterpart, by design. Same path rule as `ListDir` |
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
untouched and nothing is typed. Neither is `OpenFolder`: it hands a directory
to another application, which is what [pillar K](../product/roadmap.md#k-explicitly-not)
asks for — this window reads a worktree and never writes it, so anything you
want to *do* to a file belongs to a program that can. Nor is "copy as prompt" a command at all — the
client builds the text and puts it on your clipboard, and you paste it
([ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md)).

## Events (`ServerMsg`)

`Snapshot` · `SessionUpdated` · `SessionRemoved` · `Events` · `Queue` ·
`ChangeUpdated` · `ChangeSummary` · `Health` · `ReviewDebt` · `BlastRadius` · `DirListing` ·
`FileContent` · `TreeListing` · `ContentMatches` · `GitCommits` ·
`GitCommitDiff` · `GitLocalChanges` · `GitFileDiff` · `GitAnnotation` ·
`GitRefsInfo` · `GitStashList` · `GitStashDiff` · `GitSubmoduleList` ·
`GitRangeDiff` · `GitFileAtRevContent` · `GitReflogList` ·
`GitWorktreeList` · `GitConflictStages` · `Error`

**The periodic scan is change-gated; a request never is.** `Queue`, `Health`
and the diff messages from the scan loop are sent only when their content
differs from what was last broadcast — before the gates, every 1.5 s tick
re-sent the full diff of every actively-writing session, and a fresh `Health`,
to every client, unchanged or not. A client that *asks* (`Rescan`,
`RefreshChange`, the review verbs) is always answered, even with an identical
payload: the echo is its confirmation. New clients lose nothing to the gates —
the snapshot carries the queue, and a session's diff is fetched on selection.

**A gate only works if the payload can hold still.** `R-J65`. `Queue` was
gated from the start and never once stayed silent, because `AttentionItem`'s
`detail` carried a rendered duration — so any waiting row differed from its own
previous value every tick and the whole 28.5 KB list went to every window at
the poll rate. `detail` is static text now and the clock travels as `since`, an
anchor instant the window renders from. `since` is
`#[serde(default, skip_serializing_if = "Option::is_none")]`, so it is absent
on rows that have no clock and a client built before the change parses the
queue unchanged. The general rule this is the second instance of — `R-J55` was
the first, for `Health` — is that **a by-construction-volatile field inside a
gated payload silently disables the gate**; either mask it in the comparison or
move it off the wire.

**A moved diff travels as a summary; hunks travel on request.** `R-J53`. The
scan loop announces `ChangeSummary` — per-file counts, paths, review tallies,
no hunk bodies — because the full `Change` grows for the life of the session
(the base is pinned at session start) and was the largest recurring payload on
the wire. The full `ChangeUpdated` still exists on two paths: as the reply to
`RefreshChange`, and broadcast by the review verbs, whose marks must move the
hunks every window holds and which arrive at human rate.

**Answers that only the asker can use go to the asker.** `R-J59`. Each
connection has a **bounded** reply lane (256 deep — a full lane is a stalled
sink, and the client's own retry is the recovery) multiplexed into the one
event stream: `Subscribe`'s snapshot (broadcast, it made every window
re-ingest the board whenever any window reconnected), `FetchEvents`' replay
(served as the newest 5000 events, matching the client's own retention cap),
`RefreshChange`'s hunks, `FetchHealth`, `RunOutputHistory`, `RunEnvValue` —
a revealed secret, which was going to every window — the run verbs' refusals,
and the `Error` a malformed or failed command earns through the `err` path.
The git write verbs' error arms still broadcast (`R-J60` holds the sweep).
The two lanes carry **no cross-ordering promise**; every message on the wire
converges regardless. The contract is otherwise unchanged — commands still
have no replies to await; answers still arrive on the stream.

**`SessionUpdated` coasts when only counters moved.** `R-J54`. A fold that
changed nothing but token totals, tool tallies or the activity line updates
the daemon's memory and flushes — store row and broadcast together — within a
few seconds; a permission prompt opening, a failure, a prompt or a status flip
still broadcasts immediately. A clean shutdown flushes whatever is coasting.

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
The write family is complete.

**One verb reaches beyond this machine**, added 2026-08-01: `GitFetch`, and its
answer `GitFetched`. [ADR-0014](../decisions/0014-fetch-is-not-publishing.md)
supersedes ADR-0012 and moves the line from *the network* to *publishing and
merging* — `fetch` reads a remote and changes nothing there, `push` publishes,
`pull` merges under a possibly-running agent. `R-D24` keeps its number and now
means those last two, still refused by protocol.

`GitFetch` passes the same guard as the write verbs, which is not a category
error: "an open socket must not be able to make this daemon talk to someone
else's server" is the same rule wearing a different hat. It is never sent on a
timer, runs with `GIT_TERMINAL_PROMPT=0` and no stdin so a credential prompt
fails rather than parking a thread for ever, and always answers — including
when nothing moved, since a silent success cannot be told from a silent no-op.

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

## Memory and skills (`R-F14`, `R-F15`)

`FetchKit` → `Kit`: every memory and skill under `~/.claude`, **without
bodies**. `FetchKitDoc { path }` → `KitDoc { path, body, truncated }`, one file
at a time and echoing its own path, so a slow read cannot render under a file
you have since clicked away from — the rule every search answer here follows.

Two properties are load-bearing rather than incidental:

- **The list carries no bodies.** 113 entries on this machine and the number
  only grows; a list that carried every body would get slower the longer you
  used the tool that describes it.
- **`FetchKitDoc` takes a path from the network**, and the daemon binds
  loopback with no token. So the path is canonicalised — through `..` and
  symlinks — and must still sit under a published root, or it is refused.
  Without that, "show me a skill" is "read any file this user can read". The
  same rule `FetchFile` applies to a session's worktree, for the same reason.

Both are reads of `~/.claude`, which mogeung never writes
([CLAUDE.md](../../CLAUDE.md)) — and the stakes are higher here than for a
transcript, because these files change what an agent does next.

## Notes (`R-B35`)

`NoteList`, `NoteSave`, `NoteDelete`, answered by `Notes` with the **whole
set** every time. Not a page and not a delta: notes are small by nature, and
replacing the client's copy wholesale is what makes two windows on one daemon
unable to drift — the property daemon ownership was chosen for
([ADR-0015](../decisions/0015-markdown-is-the-truth.md)).

They are not in the write family and do not pass its guard. That guard is about
not letting an open socket run **git**; these change the daemon's own store,
like `SetHunkReviewed` and `SetSignalCommand` already did, and are covered by
the token layer along with everything else when the bind is not loopback.

An empty `id` on `NoteSave` mints a new note and the daemon answers with the id
it chose, so a client never invents one.

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

`Health` is pushed unsolicited — after any scan that changed what it says, and
on a slow heartbeat (`HEALTH_HEARTBEAT_SECS`) regardless, so the health
window's *"last scan"* line stays honest without every idle tick re-sending an
identical snapshot (`R-J55`; `last_scan`/`scans` are masked from the
comparison because they move by construction). The request paths republish it
unconditionally — `Rescan`'s answer is queue *and* health, and the client's
"rescanning…" spinner clears on the health message. A client still never has
to ask whether the board it is showing is complete — see
[health-and-canary.md](health-and-canary.md).

## Run and debug (`R-N4`, `R-N5`)

```
fetch_run_configs {session_id}          -> run_configs {session_id, configs, allowed}
run_start {session_id, config_id}       -> run_started {run}, then run_output {line}…
run_stop {run_id}                       -> run_ended {run}
fetch_run_output {run_id}               -> run_output_history {run_id, lines}
reveal_run_env {session_id, config_id, key} -> run_env_value {config_id, key, value}
```

**`run_start` has no `command` field, and that absence is the feature.**
[ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md) clause 1
is *named, not supplied*: a request identifies a configuration the repository
itself produced, so reaching this port lets you run the project's own test suite
rather than handing you a shell. Adding a command string here would quietly turn
an unauthenticated loopback port into a remote shell — it is the single change
to this file that would matter most, and it should never be made.

`allowed` carries clause 4 to the client, so a panel can explain that a daemon
was started without `--allow-run` instead of drawing buttons that will be
refused.

**`reveal_run_env` takes one key.** `R-N6`: values never travel in
`run_configs`, which carries variable *names* only, and unmasking is a per-value
act — a verb returning the whole block would make *"show me this one"*
indistinguishable from *"print every secret"*.

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

- **Usage** — `FetchUsage` → `UsageStats`. The window-limit figure inside
  is an estimate from observed limit hits and is labelled so on the type.
  **Since `R-J21` it also carries money**, per
  [ADR-0024](../decisions/0024-equivalent-cost-in-dollars.md), and the
  shape is arranged so a client cannot accidentally overstate it:
  `TokenSplit` separates the four input buckets, which are priced 1 : 0.1
  : 1.25 : 2 and were previously summed into one `tokens_in`; `ModelBurn`
  carries `cost_usd` as an **option**, where `null` means no published
  rate rather than free; `unpriced_models` names every model missing from
  the totals; and `rates_as_of` dates the price table so a client can say
  when it was read. `tokens_in`/`tokens_out` keep their old meaning — all
  input buckets summed — so nothing that already read them changed.
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
- **Kit (2026-08-06)** — `FetchKit` → `Kit`, `FetchKitDoc` → `KitDoc`, the
  path guarded against every root but the ones published.
- **Auth (`R-I4`)** — `router_with_token` wraps everything when the
  daemon starts with `--token`: `Authorization: Bearer …` or `?token=`
  (the WS client cannot set headers), constant-time compare, clean 401.
  No token → the historical open-on-loopback behaviour, byte for byte.

Every WS command keeps a REST twin (`/api/usage`, `/api/insight/*`,
`/api/repos/{repo}/docscan`, …) so the daemon stays curl-able.
