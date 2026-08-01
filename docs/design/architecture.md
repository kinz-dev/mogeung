---
title: Architecture
status: active
updated: 2026-08-01
covers:
  - crates/mogeungd/src/main.rs
  - crates/mogeung-ui/src/prefs.rs
  - crates/mogeungd/src/state.rs
  - crates/mogeung-ui/src/main.rs
  - crates/mogeung-ui/src/net.rs
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
│  native egui app · curl · anything you care to write      │
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
directories, per session in `~/.mogeung/explorer.json` — is view state, not
authority, the same standing as the keymap and the layout: file *bodies* are
never persisted, and every restore re-asks the daemon.

The git view (`R-D10`–`R-D12`) is the third read surface: log, diffs,
status, refs, stashes, blame, historical file bodies — every one a
fire-and-forget command on the blocking pool, and every one read-only by
protocol until `R-D19` added the first three writes on 2026-07-31, with client-supplied shas, ref names and filters shape-checked
before git sees an argument. See
[wire-protocol.md](wire-protocol.md) for the family and its hygiene rules.

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

Every `--poll-ms` (default 1500):

1. Read `~/.claude/sessions/*.json`; drop entries whose pid is not running.
2. Scan `~/.claude/projects/**/*.jsonl` modified within 14 days. A file over
   4 MiB is followed from near its end rather than read whole.
3. Tail each file from its recorded byte offset; classify and fold every line
   into its session. **Every line is accounted for**, including discarded ones —
   see [health-and-canary.md](health-and-canary.md).
4. Apply liveness to **every** known session, not only ones that moved — a
   session going busy→idle produces no transcript line, and that transition is
   the most important signal we have. The same pass resolves each live session's
   tmux pane (`R-B18`), using one `tmux list-panes` and one `ps` for the whole
   scan rather than a subprocess per session.
5. Recompute diffs for sessions that changed, and for any that just exited.
6. Rank and broadcast the queue, then broadcast health.

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
`server::admit` refuses a bind beyond loopback with no token, so
`mogeung --addr 0.0.0.0:7717` is refused exactly as `mogeungd` would be — and
the window asks *before* spawning the thread, on the main thread, because a
refusal printed from a background thread is a line the window opens over. The
token the window would have presented to a daemon elsewhere is the token it
requires when it is the daemon; that is why `--token` reaches `daemon::host`.

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
list at `~/.mogeung/connections.json` — client state, like the keymap, and
`0600` because it holds tokens. Switching replaces the `Net`, which is how the
old network thread learns to stop: it returns once nobody is listening on its
event channel, rather than reconnecting for ever behind a window that has moved
on. Everything the previous daemon said is then dropped, because it describes a
different machine; what the *user* chose — layout, keymap, prefs — survives.
Terminal panes detach rather than close, so tmux keeps their shells alive on the
machine being left.

**Client state is split by what it is about** (`R-I11`, after
[ADR-0013](../decisions/0013-one-window-one-daemon.md) settled that a window
watches one daemon and watching two machines means two windows). `prefs.json`
is one file written whole, so two windows raced over it and the last to save
won. Scoping the whole file per daemon would have meant choosing a theme once
per machine, so the split is by subject instead: `~/.mogeung/prefs.json` keeps
what describes *this window* — theme, layout, fonts, zoom, geometry, filters —
and `~/.mogeung/state/<machine_id>.json` keeps what is keyed by a session id or
a path on the watched machine: hidden, pinned, labels, bookmarks, editor wrap,
and the terminal panel's tab list. The window adopts a machine's state when the
daemon publishes its identity, which is also when it swaps the terminal tabs —
so a tab rooted at a worktree on the dev box does not follow you to the laptop
that happens to have the same path. Keying on `machine_id` rather than the URL
is the same reason `R-I5` exists: an `ssh -L` tunnel makes a remote daemon
answer on `127.0.0.1`. A pre-`R-I11` file migrates whole into the first machine
adopted, which is always LOCAL.

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

The UI runs a dedicated OS thread with a small tokio runtime holding the
WebSocket, bridged into the egui frame loop over a plain std channel. That keeps
the whole UI synchronous and immediate-mode with no async colouring.

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
(attempts tables, Wayland refuses honestly), and the server takes
`--token` for the remote case.
