---
title: Architecture
status: active
updated: 2026-08-06
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
readable name, and the **shell** decides where it lands — `$XDG_DOWNLOAD_DIR`,
then `~/Downloads`, then `~/.mogeung/exports` — sanitising the name and refusing
to overwrite. There is deliberately no file picker: one would put a dialog and a
filesystem plugin on a capability list whose own description says it is
deliberately small, and would hand the webview the right to write anywhere the
user can. A fixed destination and telling you the path costs one line of UI and
no new authority. It is not a worktree write and does not touch pillar K — the
file is a copy of what the daemon already published, going out rather than in.

The file explorer (`R-B24`) gives the daemon a second read surface: on request
it lists and reads files under a session's *own* root — repo when known, cwd
otherwise. Same shape as everything else: the client asks over the wire and
renders what comes back, never touching the worktree itself. The daemon
canonicalises every path and refuses one that escapes the session root,
symlinks included, because an unauthenticated localhost port must not become
"read any file by asking politely". There is no write path — the roadmap's
"an editor — explicitly not" is a property of the protocol, not just the UI.

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
4. Apply liveness to **every** known session, not only ones that moved — a
   session going busy→idle produces no transcript line, and that transition is
   the most important signal we have. The same pass resolves each live session's
   tmux pane (`R-B18`), using one `tmux list-panes` and one `ps` for the whole
   scan rather than a subprocess per session — both on the blocking pool, like
   every subprocess and git call the scan makes, so the API stays answerable
   mid-pass.
5. Recompute diffs for sessions that changed, and for any that just exited.
   Untracked files are rendered in-process rather than via `git diff
   --no-index` per file, which used to fork up to 200 short-lived processes
   per tick while an agent worked.
6. Rank the queue and broadcast it — **only if it differs** from the last
   announcement, and the same gate applies to each recomputed diff
   (`ChangeUpdated`). Health is broadcast every pass, deliberately ungated:
   it is small and doubles as the daemon's heartbeat.

One scan runs at a time: the interval, a websocket `rescan` and the HTTP
rescan collapse into whichever pass is already underway rather than stacking
subprocess storms.

Polling rather than filesystem events: a few dozen files every 1.5 s costs
nothing, and it avoids every rename and atomic-write edge case that makes
FSEvents miserable.

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

Session ids are not stable across `/clear`, which mints a new one for the same
conversation, so anything keyed by one has to be able to *move*. The window
matches a dead session against a live one sharing a pid and a cwd and carries
the label, colour tag and pin across (`migrateSuccession`). The evidence for
that is on the wire — `pid`, `alive`, `cwd`, `started_at` — which is why it can
be a client-side rule rather than something the daemon has to be taught. Note
what it costs the daemon to make possible: a dead session **keeps** its last
pid, which `data-model.md` states as a rule precisely because wiping it looks
like tidying up.

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

**Panes** are views of a session — Changes, Transcript, Info, Debt, Agent,
Editor, Git, Insight. They live in the dockview tree, are draggable and
splittable, and their arrangement is serialised into the client's own storage.

**Chrome** is everything that must stay reachable whichever pane is forward: the
Attention queue on the left, the terminal across the bottom, and since
2026-08-03 the tool-window rail on the right (`R-B40`), which holds the worktree
tree (`R-B41`) and global search (`R-F13`). None of these are dock panels; each
is laid out around the dock, so closing every pane cannot take the queue with
it.

Chrome state rides in the client's preferences rather than in dockview's own
serialisation, for the reason the egui client kept it out of egui's: the widths
and open-or-closed of the surrounding furniture are the user's settings, and a
layout engine's snapshot is the wrong place to keep something that has to
survive the layout being reset.

## What is deliberately absent

No supervisor, no child processes, no writes to `~/.claude`. See
[ADR-0003](../decisions/0003-observe-do-not-spawn.md).

## Modules added 2026-07-29

The daemon grew five read-side modules, each behind the existing scan or
an on-demand endpoint: `usage.rs` (incremental token-burn scanner, byte
offsets + hour/day buckets), `runner.rs` (the signal runner — the one
deliberate executor, click-only), `insight.rs` (cross-session search /
digest / analytics engines), `docscan.rs` (markdown inventory,
staleness, GC proposals), `codex.rs` (the `~/.codex` adapter; its scan
pass maps threads into the same `Session`). A fourth binary,
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
