---
title: Wire protocol
status: active
updated: 2026-10-22
covers:
  - crates/mogeung-core/src/wire.rs
  - crates/mogeung-core/src/federation.rs
  - crates/mogeung-core/src/pricing.rs
  - crates/mogeung-core/src/usage.rs
  - crates/mogeung-core/src/config.rs
  - crates/mogeungd/src/api.rs
  - crates/mogeungd/src/server.rs
  - crates/mogeungd/src/scratch.rs
  - crates/mogeungd/src/notes.rs
  - crates/mogeung-core/src/team.rs
  - crates/mogeungd/src/team.rs
  - crates/mogeungd/src/seat.rs
  - crates/mogeungd/src/hooks.rs
---

# Wire protocol

One WebSocket carries live state: commands in (`ClientMsg`, 99 variants as of
2026-10-08), events out (`ServerMsg`, 67). Bulk reads are also plain REST so
the daemon is curl-able without a UI. `crates/mogeung-core/src/wire.rs` is the
contract; this document is the same thing by family, with every message in
its table and the rules that family carries beneath it.

Each command row names its **hop** class — what a daemon does with it when it
names a session mirrored from another machine (`here`: this daemon's, never
forwarded; `read` / `judge`: forwarded; `write`: forwarded only with the
follow's consent; `desktop`: never, it acts on a screen) — and each event row
its **relay** class (`board`: the watch socket carries it; `pass`: returned to
the asker with ids rewritten; `theirs`: dropped, it describes that daemon).
See [Identity and federation](#identity-and-federation).

## The contract

- **Commands are fire-and-forget.** Their effect returns on the event stream
  like any other change, so a client stays a pure projection with no
  correlation layer.
- **A snapshot is unsolicited on connect**, before the socket has said
  anything, so a client is useful before it sends and reconnects self-heal.
  The window does not send `Subscribe` on open; that bought a second copy of
  the largest payload on the wire (`R-J69`).
- **The scan's broadcasts are change-gated; a request never is.** `Queue`,
  `Health` and the diff messages from the scan loop go out only when they
  differ from what was last sent. A client that asks (`Rescan`,
  `RefreshChange`, the review verbs) is always answered, even with an
  identical payload — the echo is its confirmation. A gate only works if the
  payload can hold still: a by-construction-volatile field inside a gated
  payload silently disables the gate, which is why `AttentionItem.detail` is
  static text and the clock travels as `since` (`R-J65`), and why `Health`
  masks `last_scan` and `scans` from its comparison (`R-J55`).
- **A moved diff travels as a summary; hunks travel on request** (`R-J53`).
  The scan loop announces `ChangeSummary`; the full `ChangeUpdated` is the
  reply to `RefreshChange` and the broadcast after a review verb.
- **Answers only the asker can use go to the asker** (`R-J59`). Each
  connection has a bounded reply lane (256 deep; a full lane is a stalled
  sink and the client's own retry is the recovery) multiplexed into the one
  event stream: the snapshot a `Subscribe` re-asks for, `FetchEvents`' replay
  (the newest 5000), `RefreshChange`'s hunks, `FetchHealth`, the run output
  and a revealed secret, the scratch contents, the model's chunks and reply,
  the similar list, a loaded conversation, `SentToSession`, `Peers`, and the
  `Error` a malformed or failed command earns. The two lanes carry no
  cross-ordering promise.
- **`SessionUpdated` coasts when only counters moved** (`R-J54`): token
  totals, tool tallies and the activity line flush within a few seconds;
  a permission prompt opening, a failure, a prompt or a status flip
  broadcast at once. A clean shutdown flushes whatever is coasting.
- **Answers echo their question**, so a client can drop a stray: the search
  query, the day, the path, the ref scope, the diff options, the repository.
- **Every field added since 2026-07 is `#[serde(default)]`**, so a daemon and
  a window on different machines may be a build apart in either direction; a
  client must describe an alert kind or a message it does not know rather
  than draw it as nothing.
- **A slow client is dropped, not tolerated**: on broadcast lag it is told to
  reconnect rather than wedging the channel for everyone else. **A malformed
  command produces an error, not a disconnect.** Both pinned by tests.

## Security

**Loopback by default**: `127.0.0.1:7717` unless `--listen` says otherwise,
and a warning at start-up when the bind is not loopback, because anyone who
can reach the port can read every transcript on the machine and open
terminals on it.

**A shared token, mandatory beyond loopback** (`R-I4`, `R-I10`). `--token`
gates every HTTP and WS request; clients send `Authorization: Bearer …` or
`?token=…`, the query form existing because a browser socket cannot set
headers. The query token is percent-decoded before the constant-time compare,
and raw is accepted too (`R-I19`). A wrong token is a clean 401.
`server::admit` decides before the daemon serves anything — a non-loopback
bind with no token stops start-up — and there is no `--insecure`; the window
applies the same rule to the daemon it hosts.

**No TLS of its own.** The daemon speaks `http://` and `ws://`; the tray is
built with `rustls` and the window's webview speaks TLS natively, so both
dial `wss://` through a TLS-terminating reverse proxy, which is the
recommended way to get encryption without the daemon owning certificates
([A24](../product/assumptions.md) is the bet).

**Three gates on top of the token**, decided per connection since 2026-09-23
(`R-J103`, `R-J104`) — a client on this machine is admitted whatever the bind,
everyone else is judged by the bind and the flag:

| Gate | Covers | Beyond loopback |
|---|---|---|
| the write guard (`is_write`) | the ten git write verbs, `GitFetch`, `AddWorkspaceDir`, `RemoveWorkspaceDir`, and since `R-P9` `FileWrite` | needs the token |
| `runs_allowed` | `RunStart`, `RunStop`, `RevealRunEnv`, and since `R-P3` `TeamSave`, `TeamDelete`, `TeamBoot`, `TeamStop`, `SeatWorktreeRemove`, since `R-P6` `TeamPause`, since `R-P9` `FileWrite` (which passes the write guard too) | needs `--allow-run` ([ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md), and its 2026-10-09 amendment for the team verbs) |
| `chat_allowed` | `ModelChat`, `ModelStop`, the chat history | refused, no flag ([ADR-0031](../decisions/0031-consent-to-a-named-host.md) clause 4) |
| `send_allowed` | `SendToSession`, and since `R-P6` `SendRelease` and `SendDrop` — releasing a held send types into a seat | refused, no flag, no token ([ADR-0003](../decisions/0003-observe-do-not-spawn.md), amended) |
| `config_editable` | `ConfigSave` | refused, read from the bind |
| a seat's own token (`hooks.rs`) | `POST /api/seat/hook`, `POST /api/seat/status`, since `R-P6` `POST /api/seat/send`, and since `R-P15` `POST /api/seat/note` — the one surface an agent's **own process** talks to (`R-P5`) | refused, loopback only, and the shared token is not accepted there: it is a window's key, not a process's ([ADR-0043](../decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md) clause 3) |

The write guard is one function listing its family, so omitting a new verb is
a visible gap in a list of thirteen rather than a missing line in a
hundred-arm match; it is deliberately redundant with `admit`, which guards the
binary where this guards the router. **Argument hygiene is not
authentication**: shas must be hex (one trailing `^` allowed), ref names
`[A-Za-z0-9/._-]` with no leading `-`, `..` or `@{` (`valid_ref_name`, shared
by reads and writes), stash indices numbers, log filters literal text attached
with `=` and length-capped, paths lexically contained. An unauthenticated
socket must not be able to spell a flag; none of that says *which* client.

## Sessions, the queue and review

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `Subscribe` | — | `Snapshot`, to the asker | here |
| `Rescan` | — | a scan now; `Queue` and `Health` republished unconditionally | here |
| `FetchHealth` | — | `Health`, to the asker | here |
| `FetchEvents` | `session_id`, `since` | `Events`, to the asker, newest 5000 | read |
| `RefreshChange` | `session_id`, `force` | `ChangeUpdated`, to the asker; from the cache unless `force` | read |
| `SetHunkReviewed` | `session_id`, `anchor`, `reviewed` | `ChangeUpdated`, broadcast | judge |
| `ReviewAll` | `session_id` | `ChangeUpdated`, broadcast | judge |
| `Snooze` | `session_id`, `minutes` (≤ 0 clears) | `SessionUpdated`, `Queue` | judge |
| `ForgetSession` | `session_id` | `SessionRemoved` | write |
| `FetchReviewDebt` | `repo` | `ReviewDebt` | here |
| `FetchBlastRadius` | `session_id`, `path` | `BlastRadius` | read |

| Event | Carries | Relay |
|---|---|---|
| `Snapshot` | every session, the ranked queue, `daemon` identity, the runs; on connect and in answer to `Subscribe` | board |
| `SessionUpdated` | one session, whole | board |
| `SessionRemoved` | `session_id` | board |
| `Queue` | the ranked list, when it differs; `since` is the clock anchor | board |
| `Events` | transcript events, replayed or live | pass |
| `ChangeUpdated` | one session's full diff, hunks included | pass |
| `ChangeSummary` | per-file counts, paths, review tallies, no hunk bodies | pass |
| `Health` | what the daemon can and cannot see; when it changed and on a 30 s heartbeat | theirs |
| `ReviewDebt` | unread hunks per repo | theirs |
| `BlastRadius` | what else references the symbols a file's diff changed | pass |
| `Error` | `message`, to the asker | pass |

A `Session` may carry `provisional: true` (`R-J75`): a tmux pane running an
agent that has written nothing, id `pane:%N`, never persisted, counts zero
because they are ([ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md)).
`Health.pricing` and the alert kinds `unpriced_model` and `prices_differ`
joined with `R-J124`; a `ConfigSave` wakes the pricing task.

## Files and the workspace

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `FetchWorkspace` | `session_id` | `Workspace` | read |
| `AddWorkspaceDir` | `session_id`, `path` | `Workspace`; passes the write guard | write |
| `RemoveWorkspaceDir` | `session_id`, `path` | `Workspace`; passes the write guard | write |
| `ListDir` | `session_id`, `path` (relative, or absolute inside the workspace) | `DirListing` | read |
| `FetchFile` | `session_id`, `path` | `FileContent` | read |
| `FileWrite` | `session_id` (a seat's row), `path`, `content`, `base_hash` (the `hash` the file came with; empty for a new file), `force` — **the one write into a worktree** (`R-P9`, [ADR-0019](../decisions/0019-a-viewer-not-an-editor.md)'s amendment): a worktree the daemon made for that seat, and nothing else | `FileWritten` to the asker and the new `FileContent` broadcast; `FileWriteRefused` to the asker; passes the write guard and the runs gate | here |
| `ListTree` | `session_id` | `TreeListing`, capped at 20k | read |
| `SearchContent` | `session_id`, `query` (literal, smart-cased) | `ContentMatches`, capped at 500 | read |

| Event | Carries | Relay |
|---|---|---|
| `Workspace` | the session's root, `dirs` added, `missing`, `hints` offered, `repos` that are checkouts (the session's own first) | pass |
| `DirListing` | one directory, dirs then files, `.git` never | pass |
| `FileContent` | one file, `truncated` past 256 KiB; binary refused; since `R-P9` `hash`, sha256 of the whole file as served, which a save carries back | pass |
| `FileWritten` | `session_id`, `path`, the new `hash` — a save landed (`R-P9`) | theirs |
| `FileWriteRefused` | `why` in a sentence; `current` and `hash` when the file moved under you, so the window shows the distance and reloads; `busy` naming the seat mid-turn, when a second save with `force` is the *unless you say so* (`R-P9`, fence 3) | theirs |
| `TreeListing` | every path, gitignore respected, `truncated` | pass |
| `ContentMatches` | the matches, echoing `query` | pass |

The rules — which roots, how containment is proved, why adding a folder is
gated with the writes — are [read-boundary.md](read-boundary.md).

## Git reads

Every git command takes an optional `repo` since `R-D34`: absent means the
session's own root; present, it is resolved against `session_roots`, so
naming a repository authorises nothing that browsing it did not. `GitBlame`
has none on purpose. All read-only, all on the blocking pool, every argument
shape-checked before git sees it.

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `GitLog` | `skip`, `limit`, `rev`, `all` (`--all`), `grep`, `author`, `path` (switches `--follow` on), `pickaxe` (`-S`), `since`/`until` (unix seconds, turned into one ISO form daemon-side) | `GitCommits` | read |
| `GitShow` | `sha`, `context` (≤ 400), `ignore_ws` | `GitCommitDiff` | read |
| `GitStatus` | — | `GitLocalChanges` | read |
| `GitDiffFile` | `path`, `context`, `ignore_ws` | `GitFileDiff` | read |
| `GitBlame` | `path`, `rev` (`None` blames the worktree) | `GitAnnotation`, capped at 20k lines | read |
| `GitRefs` | — | `GitRefsInfo` | read |
| `GitStashes` | — | `GitStashList` | read |
| `GitStashShow` | `index`, `context`, `ignore_ws` | `GitStashDiff` | read |
| `GitSubmodules` | — | `GitSubmoduleList` | read |
| `GitDiffRange` | `from`, `to`, `context`, `ignore_ws` | `GitRangeDiff` | read |
| `GitCompare` | `branch` | `GitRangeDiff`, with the merge base as `from` | read |
| `GitReflog` | — | `GitReflogList`, 100 entries | read |
| `GitWorktrees` | — | `GitWorktreeList`, mogeung's own included | read |
| `GitConflictFile` | `path` | `GitConflictStages` | read |
| `GitFileAtRev` | `sha` (a trailing `^` allowed), `path` | `GitFileAtRevContent` | read |

| Event | Carries | Relay |
|---|---|---|
| `GitCommits` | a page: `skip`, `done`, each commit with refs, parents and a session-attribution hint; echoes `rev`, `all`, `grep`, `author`, `path`, `pickaxe`, `since`, `until`, `repo` | pass |
| `GitCommitDiff` | the files in `Change`'s shapes, hunks carrying the repository's review marks (`R-D17`), `detail` (message, committer, dates, parents, emails, signature letter, branches containing); echoes the diff options | pass |
| `GitLocalChanges` | staged, unstaged, untracked, conflicted, ignored as `!!`; echoes `repo` | pass |
| `GitFileDiff` | one file against `HEAD` (`/dev/null` when untracked); echoes the options | pass |
| `GitAnnotation` | per-line authorship, `truncated`, echoes `rev` | pass |
| `GitRefsInfo` | HEAD, branches with tracking state, tags dereferenced, remotes, `FETCH_HEAD`'s mtime; echoes `repo` | pass |
| `GitStashList` | the stashes; echoes `repo` | pass |
| `GitStashDiff` | one stash's diff | pass |
| `GitSubmoduleList` | paths and status prefix | pass |
| `GitRangeDiff` | `from` → `to`, same shapes | pass |
| `GitReflogList` | where HEAD has been | pass |
| `GitWorktreeList` | the worktrees | pass |
| `GitConflictStages` | `base`, `ours`, `theirs`, empty where a side has none | pass |
| `GitFileAtRevContent` | one file as it stood, `truncated` | pass |

Four answers echo `repo` — commits, local changes, refs, stashes — because a
write re-broadcasts to every window on the session and two may be pointed at
two checkouts; a client drops an answer that is not for the one it shows. The
other git answers carry no `repo`, so an answer in flight for a checkout the
window just switched away from lands — a limit taken knowingly and the first
thing to close if a stale answer is ever seen. `GitCommitDiff`'s read marks
are the *resolved* repository's: with a sibling checkout picked they are that
checkout's sessions', usually none.

## Git writes and fetch

Admitted by ADR-0012 and held at publishing and merging by
[ADR-0014](../decisions/0014-fetch-is-not-publishing.md): no `push`, no `pull`,
no merge, no rebase (`R-D24` holds the question and
[ADR-0022](../decisions/0022-a-fast-forward-is-not-a-merge.md) a draft answer).
All pass the write guard above.

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `GitStage` | `paths` | `GitLocalChanges` re-broadcast | write |
| `GitUnstage` | `paths` | the same | write |
| `GitDiscard` | `paths` — **the one verb with no undo**; an untracked path is deleted | the same | write |
| `GitCommit` | `message`, `amend`, `session_trailer` — only what is staged, never `-a`; the trailer composed by the daemon | the same | write |
| `GitBranchCreate` | `name`, `switch_to` | status, refs and stashes re-broadcast | write |
| `GitSwitch` | `name`, `detach` (`R-D32`) — clears every pinned diff base in that worktree | the same | write |
| `GitStashPush` | `message`, `include_untracked` | the same | write |
| `GitStashPop` | `index` | the same | write |
| `GitStashDrop` | `index` | the same | write |
| `GitResolve` | `path`, `side` (ours, theirs, or what is on disk) — whole-file, every path ends in `git add`, content not inspected | the same | write |
| `GitFetch` | — | `GitFetched` | write |

| Event | Carries | Relay |
|---|---|---|
| `GitFetched` | `updates`, `upstream`, `ahead`, `behind`; always sent, including when nothing moved | pass |

**Every write answers by re-broadcasting what git says**, never by reporting
what it did, so the client holds no model of repository state that could
drift from git's — and sees it one round trip after the click, including when
git did something other than what was asked. The re-read is of the repository
the write **named**. Write failures carry git's own words verbatim, stderr
when there is any and stdout otherwise, because git splits refusals across
both and the commonest — *nothing to commit, working tree clean* — arrives on
stdout with a non-zero exit. `GitFetch` is the one message that reaches a
network beyond this machine: never on a timer, `GIT_TERMINAL_PROMPT=0` and no
stdin so a credential prompt fails rather than parking a thread, always
answered.

## Machine handoffs

Things that reach for another program on the daemon's machine. None steers an
agent: focusing moves *your* window, opening hands a folder to an application
that can edit it (pillar K), and sending is the one fenced exception.

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `LaunchTerminal` | `dir`, `worktree`, `source` (which CLI, `R-J51`), `headless` (`R-J61`), `machine` (a followed daemon's machine id, `R-J135`) | a session appears on the board — mirrored, when it was started elsewhere | here; write, to that machine, when `machine` names a followed one |
| `BrowseFolders` | `path` (absolute, `~` or `~/…`), `machine` (`R-J142`) | `Folders`, to the asker | write — behind the guard here, and across the hop, as a launch's |
| `FocusTerminal` | `session_id` | the terminal app comes forward | desktop |
| `OpenFolder` | `session_id` | the file manager opens there (`R-J34`) | desktop |
| `ProbeIntellij` | `session_id` | `IntellijProbe` (`R-J92`) | desktop |
| `OpenInIntellij` | `session_id` | IntelliJ opens there; refused in words without `.idea` or a launcher | desktop |
| `OpenTerminal` | `session_id` | a terminal window opens in the session's `cwd`, in the application the daemon's `terminal` key names — terminator on Linux, iTerm2 on a Mac when unset; refused in words, naming the key, when it is not there (`R-J143`) | desktop |
| `SendToSession` | `session_id`, `text` | `SentToSession`, to the asker (`R-B54`) | write |

| Event | Carries | Relay |
|---|---|---|
| `IntellijProbe` | `root`, `project`, `launcher` — once per session, so a dead button can say why | theirs |
| `Folders` | `machine` (stamped by the relay), `asked`, `path`, `parent`, `dirs`, `truncated`, `error` | pass |
| `SentToSession` | `session_id`, `target` — an ack, because the gesture is invisible | pass |

`LaunchTerminal` starts a real interactive CLI, unmodified, under tmux; the
recipe is an exhaustive match on `source` with no default arm, a source
without one is an error in words, and a headless Codex launch into a
directory Codex has not been trusted in is refused (`R-J74`) because the
prompt would have no window to appear in
([cross-session-signals.md](cross-session-signals.md#starting-one-and-knowing-whether-you-did-r-b2-r-i3)).
A launch that names a followed machine (`R-J135`) is the one command that
crosses the hop without a session id: `federation::machine_of` reads the
machine off it, the hop is a write — the follow must allow changes — and
`outbound` sends it on stripped of `machine` and with `headless` forced,
because a terminal window on a machine where nobody is looking is the desktop
act the hop refuses; the session then arrives through the follow, mirrored
as that machine's.
`BrowseFolders` (`R-J142`) answers *where could a launch start* from the
daemon whose disk it is, because the window's OS picker could only show the
window's: folder names only, at `path` or its nearest folder that exists.
It reads further than anything else here — any folder's names, not a
session's workspace — so `is_write` gates it beside `AddWorkspaceDir` and
`hop` makes one that names a machine a write, routed by `machine_of` and
stripped by `outbound` as a launch is. The relay stamps the answer with the
follow's machine so a window can drop one meant for a picker it has closed.
`SendToSession` delivers exactly the text, once, into the session's own pane,
and the gate takes no token and no flag; the mechanics are in the same
document.

## Notes, tasks and scratch

The daemon's own store and directory, not a repository: none of these pass
the write guard, and the token covers them beyond loopback.

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `NoteList` | — | `Notes` | here |
| `NoteSave` | `id` (empty mints one), `body`, `session_id`, `seq`, `repo` | `Notes`, then `Tasks` | here |
| `NoteDelete` | `id` | `Notes`, then `Tasks` | here |
| `TaskList` | — | `Tasks` | here |
| `TaskSet` | `note_id`, `ord`, `done` — rewrites the document | `Tasks` | here |
| `ScratchList` | — | `Scratches` | here |
| `ScratchCreate` | `ext`, `folder` | `ScratchContent { fresh: true }` to the asker; `Scratches` broadcast | here |
| `ScratchRead` | `name` | `ScratchContent`, to the asker | here |
| `ScratchWrite` | `name`, `content` | `ScratchSaved`, to the asker | here |
| `ScratchRename` | `name`, `to` — the one verb where the window proposes a name; an existing target refused | `Scratches` | here |
| `ScratchDelete` | `name` — already gone is not an error | `Scratches` | here |
| `ScratchDuplicate` | `name` | `ScratchContent { fresh: true }`; `Scratches` | here |
| `ScratchMkdir` | `path` | `Scratches` | here |
| `ScratchRmdir` | `path` — recursive; the window asks first | `Scratches` | here |

| Event | Carries | Relay |
|---|---|---|
| `Notes` | every note, whole, after any change — two windows cannot drift | theirs |
| `Tasks` | every task with `depth` and `group`, `closed_today`, and since `R-L17` `closed` — the closures since local midnight with their times, newest first, one per task by its words; `default`, so an older window still parses | theirs |
| `Scratches` | every name and folder | theirs |
| `ScratchContent` | one file, `fresh` true exactly once, on a create or duplicate | theirs |
| `ScratchSaved` | the write landed | theirs |

Notes are daemon-owned markdown mirrored one way to `~/.mogeung/notes/*.md`
([ADR-0015](../decisions/0015-markdown-is-the-truth.md)); a task is a `- [ ]`
line addressed by document and position with no id, and `TaskSet` is the only
direction that writes, by rewriting the document. Scratch names are paths of
bare names relative to the scratch folder, minted by the daemon except for a
rename; `check_path` applies the bare-name rule per segment and `resolve`
proves containment on the filesystem, because a symlinked directory is
invisible to any string rule
([ADR-0035](../decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)).
Content goes to the asker and the list goes to everyone, which is how a second
window learns to close a pane whose file has gone.

## Teams (`R-P3`, feature 0051)

A team is the second kind of thing the daemon stores that it could not
recompute — the user's own writing, like a note, and kept the same way:
in the store, mirrored one way to `~/.mogeung/teams/<id>-<slug>.md`, served
whole after every change ([ADR-0015](../decisions/0015-markdown-is-the-truth.md);
[ADR-0043](../decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)
clause 8). What crosses the wire is a **definition, never a command line**: a
seat names its CLI as a `SessionSource` and the launcher (`R-P4`) composes the
command from the variant, which is how ADR-0025 clause 1 survives its own
amendment.

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `TeamList` | — | `Teams`, to the asker, with `allowed`; then `Seats`, `Sends` and `SeatModels` | here |
| `TeamSave` | `team` — whole; an empty `id` mints one, and the daemon stamps `created` and `updated` whatever was sent | `Teams`, broadcast | here |
| `TeamDelete` | `id` — refused while a seat of the team is running, naming the seats (`R-J134`); otherwise the definition and its mirror, the exited seats' records and the files made for its seats go, and the worktrees stay | `Seats` and `Teams`, broadcast | here |
| `TeamBoot` | `id` — starts every seat not running; *running* is read off tmux, so a seat whose pane has gone is forgotten and started again (`R-P14`); a refusal names the seat and comes after the ones before it are on record | `Seats`, broadcast; the seat rows at once; then `TeamBooted` to the asker | here |
| `TeamStop` | `id` — ends every seat's process; the worktrees stay | `Seats`, broadcast | here |
| `TeamWindDown` | `id` — types the policy's wind-down text into every running seat and stops each as its note arrives, its turn ends, or the deadline passes; the team session comes down with the last seat (`R-P15`); `TeamStop` meanwhile cuts it short | `Seats` and `Sends`, broadcast, at each step | here |
| `SeatWorktreeRemove` | `team_id`, `seat` — a stopped seat's worktree, only when clean | an `Error` naming what was removed or why not, to the asker | here |
| `TeamPause` | `id`, `paused` — the switch, on the definition; the only verb that writes `paused` (`R-P6`) | `Teams` and `Sends`, broadcast | here |
| `SendRelease` | `team_id`, `id` — type a held send now; your act, so the chain starts again | `Sends`, broadcast; the seat rows at once | here |
| `SendDrop` | `team_id`, `id` — never deliver it | `Sends`, broadcast; the seat rows at once | here |
| `SeatRestart` | `team_id`, `seat` — stop one seat and start it again; a handover follows as the policy says (`R-P10`) | `Seats`, broadcast; the seat rows at once; a `Sends` when the handover is composed | here |
| `TeamExport` | `id`, `path` (absolute, optional) — the definition whole as JSON, where you say or beside the markdown mirror (`R-P12`) | `TeamExported`, to the asker | here |
| `TeamImport` | `path` (absolute) — mogeung's own `.json`, or an OpenRig `rig.yaml` read as other people's configuration (`R-P12`) | `TeamDraft`, to the asker; **nothing stored** | here |
| `TeamSchema` | nothing — the JSON Schema a team file follows, derived from the daemon's own types (`R-P20`); open like `TeamList` | `TeamSchema`, to the asker | here |

| Event | Carries | Relay |
|---|---|---|
| `Teams` | every team, whole; `allowed` on the answer to `TeamList` only, so a broadcast cannot flip a window's gate | theirs |
| `Seats` | every launch record — team, seat, CLI, the chosen session id, the pane, the tmux session, the directory, and since `R-P5` `signals` (the kind, the count, the last event and its line, the context fill and the model), and since `R-J134` `alive` — whether the seat's own tmux session stands, read off the pane listing every scan, so a record with `alive: false` is a seat that exited and a client counts it as such; **never the seat's token** — after a boot, stop or delete, with every `TeamList` answer, after a hook event or a status reading that moved something, and when any seat's `alive` flips | theirs |
| `TeamExported` | `team_id`, the `path` written (`R-P12`) | theirs |
| `TeamBooted` | what a `TeamBoot` did, seat by seat (`R-P14`): `started` by this boot, `revived` among them had a record but no pane, `skipped` were running and left alone; all three empty is a team already up. After the `Seats` broadcast, so the window closes on an answer and not on rows it cannot tell from the ones already there | theirs |
| `SeatModels` | each CLI's `models` (`id`, `label`, and for a CLI that says per model its `efforts` and `default_effort`), the `efforts` the CLI takes at launch, and the `source` the list came from — Claude Code's aliases and current ids, Codex's own `models_cache.json` read as it is; none for Qwen Code or opencode, whose box stays free text. A seat's `effort` is one of `minimal` · `low` · `medium` · `high` · `xhigh` · `max` · `ultra`, absent for the CLI's default, and `TeamSave` refuses one its CLI cannot take (`R-P16`) | theirs |
| `TeamDraft` | a `team` with no id and no stamps, for the window to show as an unsaved draft, and the import's `notes` — what was and was not carried (`R-P12`) | theirs |
| `TeamSchema` | the `schema`, whole — draft 2020-12, every field with its description and its default, the enum spellings as the wire has them, and the import rule in its own description (`R-P20`) | theirs |
| `Sends` | every team's counters and log (`R-P6`): `chain` against `hop_budget`, `this_hour` against `hourly_cap`, `paused`, `held`, and the `log` of entries — who (`kind`, `from`), whom (`to`), the `text`, `why`, `status`, the `hop` it took, and for a `handover` (`R-P10`) `composed_by` as *<model> @ <host>*; a `press` (`R-P11`) is the readiness Enter, not a hop; a `wind_down` (`R-P15`) is your press with declared words, neither hop nor automatic — oldest first, the last two hundred; and `wind_down` while a team is being wound down or lately was: `started`, `deadline`, a seat each as `asked` / `noted` / `stopped` / `timed_out` / `gone` with the note's excerpt, kept until the next boot; with every `TeamList` answer and after every change | theirs |

**A save is judged like a run is started.** `TeamList` is open; `TeamSave`
and `TeamDelete` pass `runs_allowed` — this machine whatever the bind, anyone
else only with `--allow-run` and the token — because a definition is what a
later click starts, and a socket that may not start a run may not write one
either. The refusal names the clause. `allowed` travels on the list's answer
the way `RunConfigs.allowed` does, so the window greys Save with the sentence
rather than letting a click fail.

**The daemon validates, not the window.** `team::validate` refuses what could
not be addressed or counted — a seat with no name or a bad one, two seats with
one name, a wake aimed at a seat the team lacks or at itself, a cap of zero —
and the error names the field. It does not check that the repository exists or
the CLI is installed: those refusals belong to the launcher, at the moment they
matter. The window's `lib/teams.ts` mirrors the rule so Save is grey with the
same words while you type; the daemon's answer is still the one that counts.

**A seat's row is known from birth** (`R-P4`). A `Session` carries `seat`
— team id, team, seat — and the daemon publishes one per launch record from
the moment the process starts: its id is the session id the daemon chose
where the CLI takes one (Claude Code, Qwen Code), so the CLI's own session is
the same row, or `seat:<team>:<seat>` where the CLI chooses its own; never
`pane:`, never `provisional`. When the real session claims the pane or the id
the tag moves onto it, and a seat whose pane has gone stays as a dead row
until the team is stopped. Nothing on this wire starts a seat but a
definition the daemon holds: the launcher's match per CLI is in `seat.rs`,
and the command line never travels. Since `R-P11` a seat's row also carries
`screen` — the shape its pane showed, from ADR-0044's closed set, the lines
it was read from, and when — and never the capture.

**Two layers, on the wire as an absence.** A seat's `permission` is omitted
when it inherits the team's, and its `guidance` is its own, appended after the
team's by the launcher; the two are never merged in a definition. Policy —
`hop_budget` (3), `hourly_cap` (60), `handover`, `readiness_press`, `wakes`,
`watchdogs` — is per team, and every automatic send it can make is declared
text in it, readable before it fires (ADR-0003's amendment, fence 3).

**A seat's own voice** (`R-P5`, `hooks.rs`). Two REST routes, `POST
/api/seat/hook` and `POST /api/seat/status`, are the one surface an agent's
own process talks to ([ADR-0043](../decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)
clause 3). They sit **outside** the shared-token layer and behind three
fences of their own: a loopback peer, a bearer token that names one seat —
minted at launch, carried in the seat's environment, persisted with the
launch record so a re-adopted seat is still heard, compared in constant time
and never sent to a client — and a body that is JSON with an event name the
receiver has classified. The order is the fence: peer before token, token
before body, so an address that is not loopback learns nothing. An event the
tables know (`CLAUDE_HANDLED`, `OPENCODE_HANDLED`) is counted and becomes the
record's last word; a name on the ignored list is a decision already taken;
a name on neither is counted as `unread` and raised through Health as
`hook:<name>`, the canary's own list — `R-A1`'s discipline, one surface over.
The status route answers with the seat's status-line text and counts nothing:
it fires on every repaint, so it is never kept and is broadcast only when the
fill or the model moved. `Session.seat` carries `signals: hooked | plain` so
the row says which kind of seat it is, because a plain seat's silence is not
a hooked seat's.

**Sends** (`R-P6`, `policy.rs`). Fence 4 of ADR-0003's amendment, on the
wire. Every automatic send into a seat — a seat's own words through
`mogeung-send`, a declared wake, a declared watchdog — is judged by the
policy engine against three facts (the team's pause switch, the chain of hops
since a human last sent, the automatic deliveries in the last hour), then
typed or **held**, and either way logged as a `SendEntry` the `Sends` event
carries. The counters are read off that log, so a restarted daemon counts
what it counted before. A seat's message arrives on `POST /api/seat/send`
with the seat's own token, the recipient as `?to=` and the text as the body,
and is resolved **within the sender's team and nowhere else** (the evening
amendment of 2026-10-09): a name outside it is a `404` with the team's seats
listed, itself a `409`, a seat not running a `409`, and the answer is one line
the generated command prints and turns into an exit code — `200` delivered,
`202` held, anything else refused. A held send rides on the recipient's row as
`Session.held_send` (how many, why the oldest, since when), which ranks the
row as needing you ([attention-ranking.md](attention-ranking.md)); `SendRelease`
types it and starts the chain again, `SendDrop` never does, and both are
judged like your own send. `TeamPause` is the one writer of `paused`:
`TeamSave` keeps what the daemon holds, so a form left open cannot un-pause a
team on its way through. Your own `SendToSession` into a seat is logged beside
the automatic ones as `human`, and resets the chain; into any other session it
is what it always was, and no ledger exists for it. One kind of entry is not
declared text: a `handover` (`R-P10`), composed by the model the daemon is
configured with on a seat's start into a team with history or on
`SeatRestart`, from the team's guidance, the teammates' transcripts and diffs
and the send log — all framed as data — and logged *composed by <model> @
<host>*. Under *after you read it* it arrives held and the seat's row ranks
until you release it whole or drop it; under *automatic* it goes through the
judge as a hop; with no endpoint configured, or a host the config does not
consent to, the entry is a refusal that says the seat started without one.

**A file out, a file in** (`R-P12`, `rig.rs`). `TeamExport` writes the
definition the daemon holds, whole — id, stamps and pause included — as
JSON where you say or beside the markdown mirror as `<id>-<slug>.json`; the
mirror stays one way, and the export is never read back on its own.
`TeamImport` reads a file you named and answers a **draft**: mogeung's own
JSON with the id dropped, the stamps zeroed and the pause cleared, so it is a
new team here and not an overwrite; or an OpenRig `rig.yaml` mapped the way
[ADR-0026](../decisions/0026-other-peoples-run-configurations.md) reads an
IDE's launch file — members to seats by `runtime` through one match, a
runtime that is not a CLI skipped and said so, `agent_ref` followed beside
the rig to the agent's guidance markdown, the summary and culture file to the
team's guidance, the edges appended as declared text rather than invented
wakes, and profiles, skills, plugins and startup actions listed as not
carried. The repository arrives empty in both cases and the window asks for
it. Both verbs are judged like `TeamSave`.

**The spec of that file** (`R-P20`, `team::schema`). `TeamSchema` answers a
JSON Schema (draft 2020-12) generated from `Team` and every type under it
through the same serde attributes the file is written with — field names,
enum spellings, defaults and doc comments — so the spec is the struct and
cannot say one thing while the daemon writes another. Its own description
states the import rule: `id` dropped, `created` and `updated` zeroed, `paused`
cleared. Open like `TeamList`, because it describes the format and carries
nothing of yours; the window saves it as `mogeung-team.schema.json` where you
choose.


**The one write into a worktree** (`R-P9`, `write.rs`). ADR-0019 refused an
editor for the concurrent writer it could not see; its 2026-10-09 amendment
admits one write where the daemon knows the writers — a worktree it made for
a seat — and `FileWrite` is that write and no other. Fence 1 is resolved
before the path is looked at: the session must carry a seat's tag, the
seat's launch record must say its directory is a worktree the daemon made,
and a session you started by hand or a seat in the team's own checkout is
refused in words. Then the path is canonicalised under that one root (`..`,
an absolute path and anything under `.git` refused), and fence 3 is judged:
the file on disk must hash to the `base_hash` the save carries — the
`FileContent` it was opened from — or the save is refused with the current
text and **no override**, you reload and edit again; and no seat may be
mid-turn in that worktree, read from the registry, or the save is refused
until it says `force`. The write is a rename, so a seat reading the file sees
one version or the other. The window's file pane is editable in exactly this
case and says so; it keeps the draft on screen through `R-J38`'s reloads and
sends the hash the draft **began** from, so an agent's write between your
first keystroke and your save is the daemon's refusal rather than a quiet
overwrite.

## Skills and memory

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `FetchKit` | — | `Kit`, every entry without bodies | here |
| `FetchKitDoc` | `path`, canonicalised and refused outside a published root | `KitDoc`, echoing the path | here |

| Event | Carries | Relay |
|---|---|---|
| `Kit` | every memory and skill under `~/.claude` | theirs |
| `KitDoc` | one body, `truncated` | theirs |

Reads of `~/.claude`, never writes; the stakes are higher than a transcript's
because these files change what an agent does next
([claude-code-formats.md](claude-code-formats.md)).

## Runs and signals

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `FetchRunConfigs` | `session_id` | `RunConfigs` | read |
| `RunStart` | `session_id`, `config_id` — **no command field, by design** | `RunStarted`, then `RunOutput`… | write |
| `RunStop` | `run_id` | `RunEnded` | write |
| `FetchRunOutput` | `run_id` | `RunOutputHistory`, to the asker | read |
| `RevealRunEnv` | `session_id`, `config_id`, `key` — one value, by name | `RunEnvValue`, to the asker | write |
| `SetSignalCommand` | `repo`, `command` (empty clears) | `SignalStatus` | here |
| `RunSignal` | `session_id` — on a click, never a timer | `SignalStatus` as it starts and ends | write |
| `FetchSignal` | `repo` | `SignalStatus` | here |

| Event | Carries | Relay |
|---|---|---|
| `RunConfigs` | the configurations and `allowed` — ADR-0025 clause 4 travelling to the client; for a mirrored session the peer's gate *and* the follow's consent | pass |
| `Runs` | every run this daemon owns, on connect | theirs |
| `RunStarted` · `RunOutput` · `RunEnded` | one run's life | pass |
| `RunOutputHistory` | the buffered output, in order | pass |
| `RunEnvValue` | one value, because it was asked for by name | pass |
| `SignalStatus` | a repo's command, whether a run is in flight, the last result | theirs |

Adding a command string to `RunStart` would turn an unauthenticated loopback
port into a remote shell; it is the single change to this file that would
matter most. The rest is [run-and-debug.md](run-and-debug.md).

## Insight, usage and docs

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `InsightSearch` | `query`, on Enter | `InsightSearchResults` | here |
| `FetchDigest` | `day` | `DayDigestReport` | here |
| `FetchRecurring` | — | `RecurringFailures` | here |
| `FetchPromptLibrary` | — | `PromptLibrary` | here |
| `FetchAnalytics` | — | `AnalyticsReport` | here |
| `FetchSubagents` | `session_id` | `SubagentTreeReport` | read |
| `FetchDecisions` | `session_id` | `DecisionReport` | read |
| `FetchFileSessions` | `path` | `FileSessions` | here |
| `FetchDocScan` | `repo` — watched repositories only | `DocReport` | here |
| `FetchUsage` | — | `UsageStats` | here |

| Event | Carries | Relay |
|---|---|---|
| `InsightSearchResults` · `DayDigestReport` · `RecurringFailures` · `PromptLibrary` · `AnalyticsReport` · `FileSessions` · `DocReport` | each echoing its query, day, path or repo so a stale answer drops; each row stating how it matched (A8) | theirs |
| `SubagentTreeReport` · `DecisionReport` | one session's subagent transcripts; decision-shaped sentences, candidates never authority | pass |
| `UsageStats` | burn per session, day, repo and model; the window-limit figure is an estimate and labelled so; `TokenSplit` separates the four input buckets, `ModelBurn.cost_usd` is an option where `null` means unpriced not free, `unpriced_models` and `rates_as_of` ([ADR-0024](../decisions/0024-equivalent-cost-in-dollars.md)) | theirs |

## The model

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `ModelChat` | `id`, `messages` (the whole conversation, every time), `conversation` (omit to keep nothing), `about { session_id, path, anchor }`, `stream`, `agent` (the Ask panel's alone — answer with the sandboxed agent where this daemon can; ignored with `about`) | `ModelChunk`… (and `ModelStep`… for the agent) then one `ModelReply`, to the asker | here |
| `ModelStop` | `id` | no reply of its own; the ask answers with `stopped: true` | here |
| `ReadingGuide` | `session_id` | `ReadingGuideReady` | read |
| `SemanticSearch` | `query` | `SemanticResults`, to the asker | here |
| `BuildSemanticIndex` | — | `SemanticResults` | here |
| `ClusterFailures` | `min_sessions` | `FailureClusters` | here |
| `ChatList` | — | `ChatHistory` | here |
| `ChatLoad` | `id` | `ChatConversation`, to the asker | here |
| `ChatDelete` | `id` | `ChatHistory` | here |

| Event | Carries | Relay |
|---|---|---|
| `ModelChunk` | `delta`, coalesced at 60 ms or 400 characters | theirs |
| `ModelStep` | `step { tool, title, ok }`: one thing the agent did, as it happens (`R-O14`) | theirs |
| `ModelReply` | `text` **or** `error`, `model`, `elapsed_ms`, `stopped`, and for `about`: `cites`, `basis` (`turns` · `code` · `unanswered`), `narration`; for an agent ask: `agent` (what answered), `steps` (all of them), or `agent_note` (why plain chat answered instead) | theirs |
| `ReadingGuideReady` | every file, ranked first, the rest `ranked: false`; `error` on the same message | pass |
| `SemanticResults` | hits shaped like search hits plus `score`, `model`, `built_ms`, `stale`, `refusal` | theirs |
| `FailureClusters` | failures grouped by meaning, `model`, `refusal` | theirs |
| `ChatHistory` | summaries, not turns | theirs |
| `ChatConversation` | one conversation, whole | theirs |

`ModelChat` is the only command that carries free-form text — everything else
names a thing the daemon already has — and three features (the chat, the
draft, ask-the-diff) ride it as a purpose rather than as families of their
own. Since `R-O14` the Ask panel's ask may set `agent`, and the daemon answers it
with opencode in a sandbox where it can build one
([ADR-0046](../decisions/0046-an-ask-may-be-answered-by-a-sandboxed-agent.md));
no other purpose sets it, because the others carry mogeung's evidence. A refusal arrives as an ordinary `error`, never as silence. The handler
is spawned, not awaited, so a cold model cannot freeze the connection. The
rest — consent, streaming, provenance, the guide's rule, the index as a cache
— is [model-and-proxy.md](model-and-proxy.md).

## The config file (`R-J79`)

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `ConfigGet` | — | `Config` | here |
| `ConfigSave` | `text`, whole-file | `Config`, with what the daemon **re-read** | here |

| Event | Carries | Relay |
|---|---|---|
| `Config` | `path`, `text`, the `keys` this daemon understands, `readonly` (why it may not be edited), `error`, `saved` | theirs |

Whole-file because the file has comments in it. The daemon validates with the
parser that will read it — `Config` is `deny_unknown_fields`, so a mistyped
key is refused rather than ignored for ever — and since 2026-10-21 it reads the
file the same way at start: one it cannot read whole stops the daemon, with
the line and the key named, rather than starting it on the defaults
([ADR-0045](../decisions/0045-a-broken-config-file-stops-the-daemon.md));
`Config::check` and `Config::load` describe a fault in one sentence, so the
editor's refusal and the start's read alike. Reading is open; editing is
loopback-only, because the file holds outbound URLs. The three model keys and
`terminal` are re-applied on save and a save wakes the pricing task;
everything else needs a restart, and the window says which is which. The
keys: `listen`, `addr`, `url`, `db`, `poll_ms`, `notify`, `push_url`,
`token`, `ssh_target`, `advertise`, `beacon`, `terminal` (`R-J143`),
`model_url`, `model_name`, `embed_model`,
`allow_remote_model`, `chat_history`, `ask_agent` (`R-O14`, re-applied on save), `llmproxy`, `llmproxy_bin`,
`llmproxy_config`, `llmproxy_port`, `hotkey`, `update_check`, `update_branch`,
`update_interval`, `update_source`, `refresh_pricing`; a flag always beats the
file, and `--allow-run` has no file twin on purpose.

## Identity and federation

`Snapshot` and `/api/health` carry a `DaemonIdentity`: `machine_id` (the
comparison — 16 random bytes in `~/.mogeung/machine-id`, written once),
`host` (display), `claude_home` (which world this daemon watches), `pid`,
`version`, `ssh_target` (how to reach it, when configured), `federation` (the
protocol it speaks, `1`). Hostnames are not the comparison — two VMs off one
image share a name — and unknown on either side means *not this machine*. A
client older than the field ignores it and a newer client falls back to the
address guess rather than refusing everything; pinned both ways.

| Command | Carries | Answered by | Hop |
|---|---|---|---|
| `Follow` | `id` (the window's connection id; the same again replaces), `name`, `url`, `token` (its own field, never in the URL), `allow_writes` — absent, off; the window asks for it **on** for a new follow since 2026-10-21 (`R-J144`, ADR-0013's amendment of that date) | `Peers` | here |
| `Unfollow` | `id` | `Peers` | here |
| `SetPeerWrites` | `id`, `allow` — the per-follow *allow changes* consent | `Peers` | here |
| `FetchPeers` | — | `Peers`, to the asker; **nothing** to a client from another machine | here |

| Event | Carries | Relay |
|---|---|---|
| `Peers` | every follow: `id`, `name`, `url` (never a token), `state` (`connecting` · `up` · `down` · `refused` · `incompatible` · `myself`), `machine_id`, `host`, `version`, `sessions`, `allow_writes`, `detail`, `since_ms`; this machine's windows only | theirs |

**The id namespace.** A mirrored session's id is `@<machine_id>:<id>`, and so
is a run's; every id in a forwarded answer is rewritten on the way back and
stripped on the way out, and the window treats ids as opaque.
`Session.origin` says where it runs and is how a pane knows to attach over
ssh to *that* machine.

**Every variant is classified, with no wildcard arm**, so a message added
next month does not compile until somebody decides what crossing a hop means
for it: `federation::hop` gives each command the class in the tables above,
and `federation::relay_class` sorts each event a relay receives into *board*
(dropped — the watch socket carries it), *pass* (to the asker, ids rewritten)
and *theirs* (dropped — it describes that daemon, and a window would read it
as this one's). **Correlation is by socket**: each window gets its own relay
per peer, opened on its first command for that peer, reopened when the token
or the consent changes. **No re-export**: a socket from another machine never
receives a mirrored session, a queue item for one or a `Peers` message, and a
command it sends about a mirrored id is refused, so following is never
transitive. One answer is rewritten on the way through: `RunConfigs.allowed`
for a mirrored session is the peer's gate and this follow's consent, never
this machine's loopback authority. A follow to a daemon that sends no
`federation` field, or an older one, stops at `incompatible`.

## REST

```
GET  /api/health                         # liveness, and whether it still sees everything; carries `daemon`
POST /api/seat/hook                      # a seat's hook event, with the seat's own bearer token; loopback only (R-P5)
POST /api/seat/status                    # a seat's status-line reading; answers with the line to show, counters included
POST /api/seat/send?to=<seat>            # a seat's words to a seat of its team, plain text; 200 delivered, 202 held, else refused (R-P6)
POST /api/seat/note                      # a seat's note about its own work, plain text; 200 kept beside its files for the next handover (R-P15)
GET  /api/queue
GET  /api/sessions
GET  /api/sessions/{id}
GET  /api/sessions/{id}/events?since=N
GET  /api/sessions/{id}/change
POST /api/sessions/{id}/review_all
POST /api/sessions/{id}/review           {"anchor": "...", "reviewed": true}
POST /api/rescan
GET  /api/repos
GET  /api/repos/{repo}/debt
GET  /api/repos/{repo}/docscan
GET  /api/usage
GET  /api/insight/search?q=...
GET  /api/insight/digest?day=YYYY-MM-DD
GET  /api/insight/recurring
GET  /api/insight/prompts
GET  /api/insight/analytics
GET  /api/insight/file?path=...
GET  /api/sessions/{id}/subagents
GET  /api/sessions/{id}/decisions
GET  /api/sessions/{id}/blast?path=...
GET  /api/sessions/{id}/ls?path=...      # explorer; path optional
GET  /api/sessions/{id}/file?path=...
GET  /api/sessions/{id}/tree
GET  /api/sessions/{id}/search?q=...
GET  /api/sessions/{id}/git/log?skip=N&limit=N&rev=...&grep=...&author=...&path=...&pickaxe=...&all=true&since=EPOCH&until=EPOCH
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
GET  /ws                                 # the WebSocket
```

Every **read** family has a REST twin; the writes, the runs, the model, notes
and scratch, the config and the follow commands are WebSocket-only, and every
git route answers for the session's own repository — there is no `repo`
parameter. `/api/health` returns a `headline`, `blind_ratio`, plain-language
`alerts` and the full `detail`, because *is the board empty because nothing is
happening, or because mogeung went blind?* should not require a window.
