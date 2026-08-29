---
title: Architecture
status: active
updated: 2026-08-29
covers:
  - crates/mogeungd/src/main.rs
  - crates/mogeungd/src/state.rs
  - desktop/src/store/prefs.ts
  - desktop/src/store/index.ts
  - desktop/src-tauri/src/lib.rs
  - crates/mogeungd/src/usage.rs
  - crates/mogeungd/src/runner.rs
  - crates/mogeungd/src/insight.rs
  - crates/mogeungd/src/docscan.rs
  - crates/mogeungd/src/codex.rs
  - crates/mogeungd/src/qwen.rs
  - crates/mogeung-tray/src/main.rs
---

# Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Clients                                                  │
│  the Tauri window · the tray · curl · anything you write  │
└────────────────────────┬─────────────────────────────────┘
                         │ WebSocket + REST (localhost)
┌────────────────────────┴─────────────────────────────────┐
│  mogeungd — the daemon, and the actual product            │
│                                                           │
│  watcher.rs   live registry + incremental transcript tail │
│  adapter.rs   on-disk .jsonl → TranscriptEvent            │
│  state.rs     scan loop, diff attribution, review state   │
│  git.rs       diffing, risk scoring, hunk anchoring       │
│  api.rs       WebSocket + REST                            │
│                                                           │
│  Store: SQLite (state) + files on disk (nothing copied)   │
└────────────────────────┬─────────────────────────────────┘
                         │ reads only, never writes
┌────────────────────────┴─────────────────────────────────┐
│  ~/.claude/  — Claude Code's own files                    │
└──────────────────────────────────────────────────────────┘
```

## The daemon is the product

Every UI is a client. That buys three things: mogeung keeps working with no
window open, reach from another device is free, and a second client is a
packaging decision rather than a rewrite.

The third was demonstrated and then retired. `R-C3` shipped a phone client as a
self-contained HTML file served from `/`, costing the daemon no change at all —
which was the point. Nobody used it, so it was removed on 2026-07-30 rather than
maintained through every wire change. The claim it proved is what remains: the
REST and WebSocket surfaces are the offer, and taking it up needs nothing added
here.

The attached terminal (`R-B18`) is the one place a client holds an OS resource
of its own — a pty running `tmux attach`. It stays inside the rule because the
client is told *which* pane by the daemon and decides nothing itself, and
because what it holds is a view rather than the session: the pty belongs to
tmux, and closing the window leaves the session untouched. See
[ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md).

The terminal panel (`R-B31`, `R-B33`) is the second, and the only place a
client holds a resource the daemon knows nothing about at all: a shell it
started itself, in a directory it chose, with no session id anywhere in the
state. It stays inside the rule for the reason [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md)
gives — the shell runs under tmux, so what it holds is again a *view*, and a
`claude` started in it is a session mogeung observes like any other rather than
one it owns. Closing the window detaches. The daemon is not told, because there
is nothing it could correctly do with the information.

Against a remote daemon both panes drive tmux **over ssh** (`R-I6`): the pty is
still local — that is what a pty is — but what runs in it is
`ssh -t <target> tmux …` rather than `tmux …`, so the shell opens on the machine
that has the files. The rule is untouched, one layer further out: tmux still
owns the session, it still outlives the window, and it is still reachable from
any terminal — on that host. The ssh destination comes from the daemon's
published identity (`R-I5`); a remote daemon that has not been told one gets a
refusal rather than a guess, because the hostname it reports need not resolve
from here and need not be the name ssh wants. Locally the panel falls back to a
bare pty when tmux is missing; remotely it does not, because that fallback would
trade the right machine for a shell on the wrong one.

The export button (`R-B43`) is the one place a client writes a file the user
can see. It stays inside the rule the same way: the window supplies text and a
readable name, a native save dialog asks where it should go, and the **shell**
does the writing. The split is the whole of it: `dialog:allow-save` is the only
permission added, so the webview can ask for a path but cannot write to one —
the write stays in a command this shell owns. Adding the filesystem plugin
instead would have handed the window a general write verb, which is a different
thing entirely from being able to save the file you are looking at.

Where there is no picker to ask — the plugin absent, or a desktop with no
portal — the shell falls back to `$XDG_DOWNLOAD_DIR`, then `~/Downloads`, then
`~/.mogeung/exports`, sanitising the name and refusing to overwrite. The two
routes differ on overwriting deliberately: a path you chose replaces what is
there because the dialog already asked, and a path nobody chose may destroy
nothing. Either way it is not a worktree write and does not touch pillar K —
the file is a copy of what the daemon already published, going out rather than
in.

The file explorer (`R-B24`) gives the daemon a second read surface: on request
it lists and reads files under a session's *own* root — repo when known, cwd
otherwise. Same shape as everything else: the client asks over the wire and
renders what comes back, never touching the worktree itself. The daemon
canonicalises every path and refuses one that escapes the session root,
symlinks included, because an unauthenticated localhost port must not become
"read any file by asking politely". There is no write path — the roadmap's
"an editor — explicitly not" is a property of the protocol, not just the UI.

**A session can be read through more than one root since `R-J40`**, and the
rule is unchanged rather than relaxed: the extra folders are ones the user
added by hand, kept in `~/.mogeung/workspaces.json` under the session's
*repository* so they outlive it, and a path is served only if it resolves
inside one of them. A relative path still means the session's own root; an
absolute one names itself and is checked against that whitelist. What a client
may name is therefore exactly what somebody authorised through the UI.

**`R-J39` closed the discovery half without moving that line.** A session's
workspace answer also carries *hints*: folders mogeung has seen this session
working in — a `/add-dir` the CLI confirmed, or a repository it wrote files
into — each shown under the tree with a `+` and a `✕`. **Offered, never
added**, and the wording is the design: every channel behind a hint is
retrospective, so it can only say where an agent has already been, which makes
it a shortcut for a click and not a basis for a read boundary. Nothing widens
until somebody presses `+`, and dismissing one is a client preference (keyed by
the same repository root) rather than a fact about the session.

The inference is deliberately narrow, and the shape came from the corpus rather
than from taste: of the folders sessions wrote into outside their own root,
every one that was a **git repository** was a real sibling project and every one
that was not was the harness talking to itself — the agent's own memory
directory (the single most written-to folder on the machine), a per-session
scratchpad, and one loose dotfile that rolled up to `$HOME`. So an edit only
suggests a repository, `~/.claude` is refused through either channel, and a
folder wide enough to contain your home directory is never offered at all.

The workbench (`R-B25`) widened that surface, not the rule: the daemon also
walks the whole tree (for go-to-file) and greps it (for content search), both
under the same containment, both on the blocking pool so a monorepo cannot
wedge the event loop. What the client keeps — open tabs, pins, expanded
directories, per session — is view state, not authority, the same standing as
the keymap and the layout: file *bodies* are never persisted, and every restore
re-asks the daemon.

The git view (`R-D10`–`R-D12`) is the third read surface: log, diffs,
status, refs, stashes, blame, historical file bodies — every one a
fire-and-forget command on the blocking pool, and every one read-only by
protocol until `R-D19` added the first three writes on 2026-07-31, with client-supplied shas, ref names and filters shape-checked
before git sees an argument. See
[wire-protocol.md](wire-protocol.md) for the family and its hygiene rules.

**One outbound network call exists**, and only one: `git fetch`, on an explicit
keystroke (`Ctrl+T`), admitted 2026-08-01 by
[ADR-0014](../decisions/0014-fetch-is-not-publishing.md). Everything else this
process does is localhost, the local filesystem, or the user's own LAN. Worth
stating plainly because "mogeung is entirely local" stops being true as a
blanket claim: a fetch can be slow, hang on DNS, or fail in ways nothing else
here fails. It never runs on a timer, and it never pushes or merges.

`~/.claude` is read and never written, permanently — that is
[ADR-0003](../decisions/0003-observe-do-not-spawn.md) and the diagram above.
The *repository* is a separate question, and was answered differently on
2026-07-30: [ADR-0012](../decisions/0012-write-locally-never-publish.md)
admits local git writes and rules out every remote verb. Unbuilt as of this
writing, and it changes only the git surface — no session, prompt or agent
is touched by any of it. Note that the daemon already had one repository
write before that ADR: `git worktree add`, on an explicit launch-with-
isolation action.

## The scan loop

Once, before the first scan: the repair pass
(`AppState::repair_reingested_history`, gated on `store::SCHEMA_VERSION`). It
runs first so the pass that follows folds onto a clean record rather than on top
of a duplicated one — see
[data-model.md](data-model.md#repair). A failure is logged and start-up
continues: a database that cannot be repaired is still a database worth
watching from.

Then every `--poll-ms` (default 1500):

1. Read `~/.claude/sessions/*.json`; drop entries whose pid is not running.
2. Scan `~/.claude/projects/**/*.jsonl` modified within 14 days. A file over
   4 MiB is followed from near its end rather than read whole.
3. Tail each file from its recorded byte offset; classify and fold every line
   into its session. **Every line is accounted for**, including discarded ones —
   see [health-and-canary.md](health-and-canary.md). The offset is recorded in
   the database once those lines are folded in, so a restart resumes rather than
   re-reading the file whole (`R-A6`, and see
   [data-model.md](data-model.md#read-positions-are-part-of-the-record-r-a6)
   for what re-reading it did).
4. Adopt any live-registry entry no transcript has mentioned. **Claude Code
   writes the `.jsonl` on the first message, not on launch**, so a session you
   have just opened exists only in step 1's registry — and a board that
   discovered sessions from transcripts alone could not see it until you typed
   into it, which is after the moment you needed pointing at it (`R-J30`). It
   happens *after* step 3 on purpose: a **resumed** session is live and has a
   transcript already, and taking it in from the registry first would skip
   step 2's cap and read an 11 MB history whole inside the loop. Nothing is
   guessed — the transcript path stays empty until the file appears, because
   deriving it means deriving Claude Code's project-slug rule, which `A4` says
   will move.
5. Apply liveness to **every** known session, not only ones that moved — a
   session going busy→idle produces no transcript line, and that transition is
   the most important signal we have. In place, under one write lock: the
   pass used to clone the whole board out and back, three copies per tick
   (`R-J57`). The same pass resolves each live session's tmux pane (`R-B18`),
   using one `tmux list-panes` and one `ps` for the whole scan rather than a
   subprocess per session — skipped entirely when nothing is alive — both on
   the blocking pool, like every subprocess and git call the scan makes, so
   the API stays answerable mid-pass.
6. Recompute diffs — **gated on the worktree actually moving** (`R-J53`). A
   growing transcript is the trigger to *look*, not to diff: each session
   rests `CHANGE_PROBE_SECS` between looks, a look is a fingerprint
   (`rev-parse HEAD` + `status --porcelain`, cached per repo within the
   pass), and only a moved or unreadable fingerprint pays for
   `compute_change` — which is a full-worktree diff against the pinned base
   and grows for the life of the session. A session that just exited
   bypasses the gate: it is newly reviewable and the diff is wanted now.
   Untracked files are rendered in-process rather than via `git diff
   --no-index` per file, which used to fork up to 200 short-lived processes
   per tick while an agent worked.
7. Flush sessions whose counter-only updates have coasted long enough
   (`R-J54`): a fold that moved nothing but tallies updates memory and is
   saved-and-broadcast here, together, rather than per tick; anything a
   decision hangs off broadcast the moment it was folded.
8. Rank the queue — over borrows, under the read lock — and broadcast it
   **only if it differs** from the last announcement. A moved diff is
   announced as a `ChangeSummary` (counts and paths; hunks are served per
   connection on request). Health is broadcast when it says something new,
   plus a slow heartbeat (`R-J55`) — see
   [wire-protocol.md](wire-protocol.md).
9. Daily, and on the first pass after start: retention (`R-J57`). Sessions
   dead past `RETENTION_DAYS` lose their rows, events, marks and offsets —
   unless a note anchors to them, or their transcript file is still inside
   the scan window and would only be re-adopted. The same pass checkpoints
   and truncates the WAL.

One scan runs at a time: the interval, a websocket `rescan` and the HTTP
rescan collapse into whichever pass is already underway rather than stacking
subprocess storms.

Polling rather than filesystem events: *stat-ing* a few dozen files every
1.5 s costs nothing, and it avoids every rename and atomic-write edge case
that makes FSEvents miserable. What the poll must never do is treat "the
transcript grew" as "everything downstream is stale" — that is how the tick
rate became a full-worktree diff rate (`R-J53`), and the gates in steps 6–9
are the boundary between polling for *evidence* and repeating *work*.

The Codex pass follows the same rule one directory over (`R-J56`): its
`threads` index is read only when the index file's mtime moved, except while
a Codex session is marked alive.

That gate has to watch **the writer-lock directory too** (`R-J76`), and missing
it made `R-J73` look like it had not worked. A thread nobody has spoken to
writes no index row, so the index and its WAL never move; with no Codex session
yet alive the pass returned early and never reached the lock scan the adoption
reads from. The first user message wrote the row, moved the mtime, opened the
gate — so a session appeared the moment you typed and not before, which is
exactly what a gate closing over a live session looks like from the outside.
A directory's mtime changes when an entry is created or removed, which is a
thread opening or closing, so the stamp now carries it as a fourth element.

## Codex liveness is a registry now, not a guess (2026-08-26)

`R-J70`. It used to be a recency heuristic — alive meant "wrote something in
the last ten minutes and has an unfinished turn" — and that is wrong in the
direction that matters most. A session **waiting for you** is the most
important row on the board and is also the one that has written nothing for
twenty minutes, so it fell off the queue exactly when it mattered, and a
`Done` status meant "gone" rather than "your turn".

Codex `0.149` takes an advisory `flock` on
`~/.codex/thread-writer-locks/<thread-id>.lock` for as long as a thread is
open. The file names the thread and the lock names the process, so it is a real
registry — the equivalent of `~/.claude/sessions/*.json`, and the thing
`R-J30` needed for Claude Code.

Two things follow from the registry, and `R-J73` had to add both after
`R-J70` shipped without them. A thread with **no index row at all** — Codex
writes one on the first user turn — is adopted from its lock alone, so a
session you have just started is on the board before you speak to it rather
than after; its start time is the lock's mtime and its cwd comes from the
process where the platform will say. And the pid is walked to its **pane**, so
the session can actually be hosted: `R-J70` set the pid and said it was
hostable, which was true of the pid and not of the code.

**The lock is read, never taken.** Testing a lock by trying to acquire it is
the obvious implementation and is refused here: this daemon must not compete
with an agent for a resource the agent needs (ADR-0003), and a momentary grab
is precisely the race that would make a Codex thread fail to start. On Linux
the holder is read out of `/proc/locks`, which touches nothing and yields the
pid as well — so a live Codex thread can be *hosted* in an Agent pane rather
than only pointed at. Where the kernel does not publish locks (macOS) the
file's presence is the answer and the pid is unknown; a lock left behind by a
crash reads as alive there, which is the stated cost of not interfering. An
install with no lock directory at all — an older Codex — falls back to the old
heuristic, because concluding "nothing is alive" would be a worse lie than the
guess.

## Who runs the daemon

The window binds the daemon port at start-up. Winning the bind means hosting a
daemon **on a thread in its own process**; losing it means attaching to the one
already there. A hosted daemon dies with the window; an attached one is left
alone. `mogeungd` is still a separate binary and is still how you get a daemon
that outlives every window.

The bind *is* the check — probing first and starting second races two windows
against each other. A thread rather than a child process because a thread cannot
outlive its process, which removes the pid file, the cleanup-on-exit and the
orphan-holding-the-port problem all at once.

This does not weaken the client contract below: the window talks over the same
websocket either way and cannot tell which process the daemon is in.
[ADR-0009](../decisions/0009-the-window-may-host-a-daemon.md).

**A hosted daemon obeys the same admission rule as a standalone one** (`R-I10`).
`server::admit` refuses a bind beyond loopback with no token, and the window
asks it *before* serving on the socket it just won, so a window cannot become a
daemon that `mogeungd` would have refused to be. The check is on the bound
address rather than on what was asked for, which is what makes `0.0.0.0` and a
`:0` port answer the same question honestly.

**Daemons can announce themselves** (`R-I8`), over mDNS as `_mogeung._tcp`,
**only** when `--advertise` says so. The broadcast is a disclosure in its own
right — it names the machine and says there is something here worth reaching —
and no code can tell a home network from conference wifi, so the default is off
and stays off. Browsing produces a list the window renders; nothing dials
anything. A loopback bind refuses to advertise, since nobody could reach it, and
that refusal is what makes every discoverable daemon token-gated by construction:
the only binds that *can* advertise are the ones `admit` already requires a
token for.

**The daemon may start a process you named** (`R-N4`), and never an agent.
[ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md) is the
whole reasoning; the shape here is that `AppState` holds a `Runs`, so a run
outlives the window that started it, and that spawning is **opt-in past
loopback** by `--allow-run`. That flag is read the same way `writes_allowed` is
— from the bind address, once, by the code that bound the socket — so the
start-up refusal and the per-request gate cannot come to disagree. Loopback
needs no flag, because it is the trust boundary the terminal panel already has.
See [run-and-debug.md](run-and-debug.md).

**The daemon can be changed without restarting** (`R-I7`). The window keeps a
saved list — client state, like the keymap — with a name, a URL and an optional
token each. Switching tears the old connection down before the new one is
dialled, so a window that has moved on cannot be reconnected behind by the
socket it left. Everything the previous daemon said is then dropped, because it
describes a different machine; what the *user* chose — layout, keymap, prefs —
survives.
Terminal panes detach rather than close, so tmux keeps their shells alive on the
machine being left.

**Client state is split by what it is about** (`R-I11`, after
[ADR-0013](../decisions/0013-one-window-one-daemon.md) settled that a window
watches one daemon and watching two machines means two windows). The split is by
subject: what describes *this window* — theme, layout, fonts, zoom, filters —
is stored flat, and what is keyed by a session id or a path on the watched
machine is stored under that machine's id: hidden, pinned, labels, colour tags,
bookmarks, editor wrap, and the terminal panel's tab list. The window adopts a
machine's state when the daemon publishes its identity, which is also when it
swaps the terminal tabs — so a tab rooted at a worktree on the dev box does not
follow you to the laptop that happens to have the same path. Keying on
`machine_id` rather than the URL is the same reason `R-I5` exists: an `ssh -L`
tunnel makes a remote daemon answer on `127.0.0.1`.

**A saved preferences file is always older than the build that reads it**, and
the two ways it can be out of date need different answers. A field the file has
never heard of is filled from the defaults on load, at the boundary, once —
which is what stops a newly added key being `undefined` at every read site. A
default that has *changed* cannot be reached that way at all: the whole object
is written on every save, so a file from last month states the old answer
explicitly and would keep it for ever. `PREFS_VERSION` is the seam for those.
A file below the current version is moved onto the new default once and stamped,
and a file already at it is left exactly as its owner set it. Bumping it moves a
setting somebody may have chosen on purpose, so a bump has to be worth that; the
first one is side-by-side becoming the default diff.

Session ids are not stable across `/clear`, which mints a new one for the same
conversation, so anything keyed by one has to be able to *move*. The window
matches a dead session against a live one sharing a pid and a cwd and carries
the label, colour tag and pin across (`migrateSuccession`), plus the selection
and any held panes (`successions`, in the store). The evidence for that is on
the wire — `pid`, `alive`, `cwd`, `last_event_at` — which is why it can be a
client-side rule rather than something the daemon has to be taught. Note what it
costs the daemon to make possible: a dead session **keeps** its last pid, which
`data-model.md` states as a rule precisely because wiping it looks like tidying
up.

**A process that has been open for days has a *line* of dead ids behind it, not
a predecessor** — one per `/clear` — and ordering them is the whole difficulty.
It is done by `last_event_at`, the last line each of them actually wrote.
`started_at` cannot do it: the daemon fills it from the live registry's
`startedAt`, which is when the **process** started, so every id on one pid
reports the same instant and the comparison ties. `R-J15` is that bug; ordering
by a field that does not order was indistinguishable from working for as long as
sessions were short-lived.

The two halves move on different rules, and the difference is the point. Labels,
tags and pins are *identity* — they name the conversation, so they follow it
every pass, idempotently, and stop moving once the live head holds them. The
selection and a held pane are *placement* — where you asked to be looking — so
each predecessor donates its hop **once**. A rule that re-pointed the view every
pass would make a finished session impossible to sit and read, which is a worse
window than the one that leaves the pane blank.

`R-I12` records the argument that all of this belongs to the daemon instead. It
carried more weight when two clients each kept their own copy; with one client
it is a smaller problem and the same argument. What it would fix now is
portability — the state lives in the window's storage on the machine you are
sitting at, not with the sessions it describes.

**The window also asks who it is talking to** (`R-I5`). The daemon publishes a
`DaemonIdentity` — a stable `machine_id` from `~/.mogeung/machine-id`, plus
hostname, watched `~/.claude`, pid and version — on every snapshot and on
`/api/health`. Actions that touch a machine (open-in, jump-to-terminal,
launch-terminal) compare that id against this machine's, rather than guessing
from the address dialled. Both ends resolve identity through one function in
`mogeungd::machine`, deliberately: two processes computing it differently could
disagree about whether they are on the same desk.

## Client contract

Commands are fire-and-forget; their effect returns on the event stream. Clients
are therefore pure projections of daemon state with no local authority and no
request/response correlation layer.

The window holds one WebSocket in the browser layer and pushes every message
into a zustand store; nothing else in the client talks to the daemon. Panes read
that store and render, which is the same discipline the egui client kept with a
tokio thread and a std channel — one connection, one place state arrives.

**A projection has to be able to go stale on purpose.** The one piece of daemon
state the client holds *by value* rather than by subscription is a file's body:
it arrives once, in answer to a `FetchFile` the pane asked for, and no event
ever amends it. So the store carries the other half — a `reload` flag it raises
when it has reason to believe what it holds is out of date (`R-J38`): a
`ChangeUpdated` naming that path, the OS window coming forward, or the pane
being clicked back onto. Raising a flag rather than dropping the body is the
part worth knowing: the old text stays on screen until the new text lands, so a
file being rewritten by an agent does not flash its pane between reads.

### The window

`desktop/` — React, Monaco and dockview, packaged with Tauri, since 2026-08-04
([ADR-0018](../decisions/0018-a-second-client-in-typescript.md)). It was built
beside the egui window and against one unchanged daemon, and on 2026-08-05 it
became the only window:
[ADR-0020](../decisions/0020-the-egui-client-is-retired.md) deleted
`crates/mogeung-ui` and the vendored `egui-term` with it.

The daemon was not changed to make either of those happen and never knew there
were two. That is the property "every UI is a client" was always claiming,
demonstrated twice — `R-C3`'s phone client first, this port second — and both
demonstrations were retired after making their point. The tray is what remains
of it, and it is a real second client: same websocket, no authority, one number.

Two clients cost more than twice one, and the extra is divergence. The `/clear`
label bug is the case worth remembering: found and fixed in the egui client in
July, ported without the fix, and re-reported against the React client a
fortnight later. That is the argument in ADR-0020 for deleting rather than
keeping a client nobody develops.

The Tauri process keeps a small native half, and it is native for one reason
each: it **holds the ptys**, so ADR-0010 and ADR-0011 stay true (what a client
holds is a view of a tmux session; closing the window detaches); it owns the
global shortcut (`R-B10`); and it reads `~/.mogeung/machine-id` so local-versus-
remote is decided by identity rather than by the address dialled (`R-I5`).

`desktop/src-tauri` is deliberately **its own cargo workspace**. As a member it
would put Tauri's Linux system dependencies in the path of
`cargo test --workspace`, and that command is a gate for the daemon.

### Chrome and panes

The window docks things two ways, and which one a thing uses is a decision
rather than a habit —
[ADR-0017](../decisions/0017-the-rail-is-chrome.md).

**Panes** are views of a session. Two *kinds* are left in the centre — **Agent**
and **file** — since `R-B45` moved Changes and Transcript down to join Git,
Insight and Debt in the bottom dock, and Info under the queue. A file is a pane
per open file since `R-B53`, which collapsed the Code pane's own tab strip and
two-way split into dockview's: the window docked things twice until then, and
the inner system was the weaker one. They live in the dockview
tree, are draggable and splittable, and their arrangement is serialised into the
client's own storage.

*Which* session a pane shows stopped being a property of the window on
2026-08-06 and became a property of the pane (`R-B49`). A pane resolves its
session through `PaneScope` rather than reading `selected`: unbound it follows
the queue, as everything did before, and **held** it stays on one session while
the selection moves around it. That is what lets two agents be on screen and
live at once, and it is deliberately confined to the tile tree — chrome follows
the selection by definition, which is half of what ADR-0017 means by the word.

The selection is written back the other way too: activating a **held** Agent
pane makes its session the current one, because everything outside the tile
tree — the file tabs, the dock, Info — describes `selected`, and a pane you are
working in that leaves them all pointed elsewhere is a pane that costs you a
trip to the queue. One-way; a hold is never disturbed by it.

Two rules keep the arrangement durable across a restart. Slots are **numbered**
(`agent`, `agent:2`, …) and never keyed by session, so the serialised layout
names no session and cannot restore a tab pointing at one that ended days ago.
And a hold lives in the *machine-scoped* preferences beside the queue's pins and
labels, for the reason `R-I11` gives: a session id from the dev box means
nothing on the laptop.

**Chrome** is everything that must stay reachable whichever pane is forward: the
Attention queue on the left, the terminal across the bottom, and since
2026-08-03 the tool-window rail on the right (`R-B40`), which holds the worktree
tree (`R-B41`) and global search (`R-F13`). None of these are dock panels; each
is laid out around the dock, so closing every pane cannot take the queue with
it.

The rail holds **as many of its tools as you open**, stacked down one column in
strip order, since `R-J33` on 2026-08-19 —
[ADR-0027](../decisions/0027-the-rail-stacks.md), which is ADR-0017's own
revisit trigger being pulled rather than a change to its rule. So `prefs.rail`
is a *list*, read through `railList` because every file written before that day
holds a single tool's name where the window now iterates, and each section's
share of the height is a **weight** rather than a pixel count: the rail is as
tall as the window, and a saved height would need re-dragging on a machine with
a different screen. The bottom dock is still one tool at a time, and that
asymmetry is deliberate — it is horizontal, and two stacked dock tools would
each get half of the shortest panel in the window.

**The wall is neither** (`R-B50`, 2026-08-07). It is an overlay on a chord —
every session as a tile, positions keyed by session id so they never move —
and it is deliberately not a third docking idea: it cannot be arranged, cannot
be left open beside anything, and holds no state but a boolean. ADR-0017's rule
is about where a thing *lives*; something that exists only while you hold a key
does not live anywhere. It reads the snapshot the window already has and fetches
nothing.

Chrome state rides in the client's preferences rather than in dockview's own
serialisation, for the reason the egui client kept it out of egui's: the widths
and open-or-closed of the surrounding furniture are the user's settings, and a
layout engine's snapshot is the wrong place to keep something that has to
survive the layout being reset.

## The model seam (`R-O1`, 2026-08-28)

`mogeung_core::model` holds the policy — what is configured, whether it is
allowed, and the sentence saying why not — and `mogeungd::model` holds the part
that leaves the machine. The split is the same one `run.rs` uses: pure decisions
where they can be tested with nothing running, effects where the processes are.

**The daemon owns the endpoint, not the client**
([ADR-0031](../decisions/0031-consent-to-a-named-host.md) clause 2,
carried forward from ADR-0030). The
corpus is on the daemon's machine, so a window watching a Mac (`R-I6`) has to
read *that* machine's transcripts with whatever endpoint *that* daemon was
given. A client-side call would be the first piece of local authority in a
client, which the TypeScript port was able to claim it never took.

The endpoint is a **URL in config**, not an in-process model, because the box
with the GPU and the box with the sessions may differ. `curl` makes the call —
`notify.rs`'s trade, for its reasons: one POST on a human-initiated action,
shelling out cannot poison the runtime, and the alternative drags a TLS stack
into a workspace that has managed without one. The body goes down **stdin**, so
neither argv limits nor quoting can ever be part of a chat message.

**The window's hosted daemon reads `model_url`, `model_name` and
`allow_remote_model` from `config.toml`, and nothing else.** That daemon reads
the config file for these three and no others — it takes the defaults for `db`,
`poll_ms` and the rest — and widening that is a separate question. The model
could not wait for it: a hosted daemon has no argv, so with nothing read there
the chat panel would say *no model configured* for ever with no way to change
its mind.

The consent is the third of those, and it was not there when this shipped.
`--allow-remote-model` was given `--allow-run`'s shape, and the shapes differ in
the one way that matters here: `runs_allowed` reads the **bind**, so a hosted
daemon is loopback and never needs the flag, where the model gate reads the
**endpoint**. Flag-only therefore meant *unreachable* on the shape mogeung is
normally run in.
[ADR-0031](../decisions/0031-consent-to-a-named-host.md) replaced it with a key
that names the host it consents to, so moving `model_url` asks again.

## The proxy the daemon owns (`R-O10`, 2026-08-28)

`mogeung_core::llmproxy` holds the policy — which port, what the starter config
says, which hosts it forwards to — and `mogeungd::llmproxy` owns the child
process. The same split as the model seam and `run.rs`, for the same reason.

**This is the first long-lived child mogeungd has ever owned**, and
[ADR-0033](../decisions/0033-a-proxy-of-our-own.md) is where the two things it
crosses are argued. It is off unless `llmproxy = true` in the config file, and
file-only: a flag would start a proxy for one invocation and leave it behind.

The port is **derived** from the daemon's own rather than random, so start-up
recomputes where it left the last one instead of reading a file that could be
describing a process that died last week — `daemon.rs`'s stale-pid argument,
applied to somebody else's process. Start-up **adopts** an llmproxy already
answering there, and shutdown stops it **by address** — `--listen <addr>
--shutdown`, llmproxy's own mechanism — rather than by signal. llmproxy
re-execs itself as `--foreground` and detaches, so the spawned process is gone
within a second and never held the port: a recorded pid names something already
exited, and the process-group kill `run.rs` uses would reach nothing.
`PR_SET_PDEATHSIG` is rejected on top of that — it fires on the death of the
parent *thread* under a runtime free to retire it, macOS does not have it, and
it would not reach a detached grandchild either.

**The window stops the proxy it started, because the daemon cannot.** The
hosted daemon is handed a shutdown future that never resolves, so
`server::run`'s cleanup — `Proxy::shutdown` included — is unreachable on that
path; standalone `mogeungd` reaches it on `ctrl_c` and the window never does.
So the shell's `RunEvent::Exit` calls the same `llmproxy::stop(bin, port)`,
against a target recorded at host time and only when the window actually
hosted: attaching to somebody else's daemon must not stop a proxy that daemon
is still using. Both sides derive the port independently, so that agreement is
a test.

**The admin interface is reachable because the daemon reads its port.**
llmproxy binds admin on a random loopback port, so nobody could find it. It
writes the URL into its own runtime metadata file, which mogeung reads and puts
a button on. Opening it is the desktop shell's first launch that is not a pty,
and it is deliberately the narrowest one that does the job: `open_local_url`
accepts `http://` on a **parsed** loopback host and refuses everything else,
rather than adding the general opener plugin and giving the webview *open
anything*.

**Where it forwards is reported, never gated.** A proxy on `127.0.0.1` passes
ADR-0031 clause 3 without asking. mogeung refuses to extend the gate — routing
is per request, so it could only be sometimes-right — and names the hosts
instead, read from the config file rather than from the running process so the
answer survives the proxy being down.

## The reading guide's state (`R-O3`)

The model's ordering lives in the store keyed by session, beside `changes`, and
is asked for by a button rather than computed on selection — it spends a model
call of up to a minute, and ADR-0031 clause 6 keeps model work off anything
that runs on its own.

**The daemon decides what the guide contains, not the client.** It appends
every file the model did not name, marked `ranked: false`, so no client can
render a shortlist as though it were the whole diff. `--bin judge` found
`claude-opus-5` naming about sixteen files of sixty; putting that rule in the
window would have meant trusting each client to remember it.

The keyword ordering is untouched and is what shows without a model. Switching
the guide off is the old pane exactly — pillar K's refusal of a blend, kept by
never mixing the two scores.

## The two harnesses, and why they are binaries (`R-O2`, `A36`)

`--bin judge` and `--bin why` are the pillar's measurements, and they are
programs rather than tests for one reason: their corpus is **this machine's**,
which no fixture can stand in for and no CI can carry. Each reads the same
`Config` the daemon reads, resolves the same endpoint including mogeung's own
llmproxy, and **exits non-zero when nothing answered** — `--bin sweep`'s rule,
because a broken setup that reads as *no findings* is the failure that costs a
year.

Neither owns a prompt of its own. `judge` calls `mogeungd::guide` and `why`
calls `mogeungd::why`, which is what the shipping panel calls: a harness
grading a different prompt than the one that ships is measuring a feature that
does not exist. `why` also holds `R-O4`'s two candidate **retrievals** —
nearest-in-time (`R-F9`'s) and leading-up — because which one the panel uses is
a finding rather than a preference, and the corpus answered it before the panel
existed.

## What is deliberately absent

No supervisor, no child processes, no writes to `~/.claude`. See
[ADR-0003](../decisions/0003-observe-do-not-spawn.md).

## Diff attribution, and two spellings of one path

`R-J27`. A session's `touched_files` carry the prefix its transcript wrote,
unresolved; `repo_root` is what `git rev-parse --show-toplevel` answers, which
is resolved through every symlink. They differ for any checkout reached through
one — an ordinary way to keep a repo on another volume — and on macOS for every
temp directory, since `/var` is a firmlink to `/private/var`.

Unreconciled, the attribution filter in `state.rs` matches nothing, `retain`
empties the file list, and the session reports **no changes at all** — which
looks exactly like a session that has not touched the worktree. The comparison
resolves on the miss path only, so the ordinary case stays a string compare,
and it canonicalises the longest *existing* ancestor because a deleted file is
precisely what a diff is about.

`R-J62` is the same failure reached by a different road: a touch that is not in
the repo **at all**. Agents write scratch files — a plan under `/tmp`, a note
under `~/.claude` — and those land in `touched_files` beside the real edits.
The filter ran whenever that list was non-empty, so a session whose only
recorded write was a scratchpad matched nothing and reported no changes while
its worktree held real work; one out-of-repo touch was worse than none, because
none skips the filter entirely. Only touches that resolve to a path *inside*
the root are candidates now — `relative_to_root` hands back an absolute path
for one that does not, which is the test — and a session left with no
candidates falls back to the whole worktree diff, the answer a session with no
touches already got. A scratchpad alongside real edits changes nothing: the
out-of-repo path is dropped, not the filter.

## One process table per scan pass (2026-08-26)

`R-J64`. Two passes need to know which tmux pane a live session's process sits
in — the Claude liveness update and `scan_qwen` — and each resolved it for
itself, forking `tmux list-panes` and `ps -axo pid=,ppid=` on the blocking
pool. On a machine running both CLIs that was two of each every 1.5 s, and
`ps` was measured at **18.5 ms** against 680 processes: about 2.4% of a core
spent re-deriving a table that changes when a pane moves and at no other time.

`AppState::process_table` resolves it once and hands both callers the same
`Arc`s — `Arc` rather than a clone because the ancestry map has one entry per
process on the machine, and copying it per caller would trade the fork for a
memcpy. Two gates, cheapest first:

- The whole table is reused inside `PROC_TABLE_TTL_MS` (500 ms), which is what
  makes one scan resolve it once however many callers ask, while still
  resolving afresh on the next tick.
- Past that, the cheap half is re-read every pass and the expensive half is
  re-forked only when it could have moved: the pane list changed, a caller
  asked (a live session with no `tmux_target` yet has no answer to keep), or
  `PROC_PARENTS_BACKSTOP_SECS` elapsed. The backstop exists because an
  unchanged pane list is a *signal* that nothing moved, not a proof — a process
  can be re-parented inside a pane that kept its identity.

If the blocking pool refuses, the last known table is served rather than an
empty one: a spurious "no panes" unhosts every Agent pane for a tick.

## What a fold compares (2026-08-26)

`R-J66`. `significant_change` decides whether a fold is worth announcing now or
may coast under `R-J54`. It used to build a masked clone of each side and
compare those, which had one property worth keeping — a field added to
`Session` was compared by default, so the gate could not silently stop watching
something — and one worth losing: it deep-cloned both sessions including
`recent_tools`, `recent_touches` and `touched_files`, which are 70% of a
session's bytes and are precisely the fields it was about to blank.

It binds every field by name instead. Adding a field to `Session` is now a
**compile error** in that function until someone decides which side of the mask
it belongs on — the same guarantee, with no allocation. The nine masked fields
are the counters and the histories that move whenever a transcript grows.

## Modules added 2026-07-29

The daemon grew five read-side modules, each behind the existing scan or
an on-demand endpoint: `usage.rs` (incremental token-burn scanner, byte
offsets + hour/day buckets — and, since `R-J21`, a fold keyed by **model
and local day** whose token buckets are split the way they are *priced*,
so `pricing.rs` in the core crate can put a dollar figure on them per
[ADR-0024](../decisions/0024-equivalent-cost-in-dollars.md)),
`runner.rs` (the signal runner — the one
deliberate executor, click-only), `insight.rs` (cross-session search /
digest / analytics engines), `docscan.rs` (markdown inventory,
staleness, GC proposals), `codex.rs` (the `~/.codex` adapter; its scan
pass maps threads into the same `Session`), `qwen.rs` (the `~/.qwen`
adapter, `R-I15`; a live registry plus per-session JSONL, so structurally
it is `watcher.rs` rather than `codex.rs`, and its records are Gemini's
rather than Anthropic's). The three readers share only `LineClass` —
[ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)
records why that stays a variant-and-a-module rather than becoming a
trait, and why every "is this Claude?" question is now a named method on
`SessionSource` instead of an equality test that silently meant "not
Claude". A fourth binary,
`mogeung-tray`, subscribes to the queue over the wire and shows the
WAITING count — a client like every other, no local authority. It names
the machine that count is for (`R-I11`), from the same `DaemonIdentity`,
because one tray per daemon means two bare numbers otherwise. The
terminal focus/launch and notification paths gained Linux siblings
(attempts tables, Wayland refuses honestly — and since 2026-08-02 the launch
side treats *spawning* and *launching* as different events, because they are:
a terminal handed flags it does not understand starts, prints usage and exits,
and `spawn` has already returned `Ok`. It waits, asks whether the child is
still alive, and reports that terminal's own stderr when it is not).
The `x-terminal-emulator` alternatives symlink is resolved to whatever it
points at so it gets that program's flags rather than xterm's. The server takes
`--token` for the remote case.
