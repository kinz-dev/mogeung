---
title: The window
status: active
updated: 2026-10-10
covers:
  - desktop/src/wire/client.ts
  - desktop/src/store/index.ts
  - desktop/src/store/prefs.ts
  - desktop/src/lib/keymap.ts
  - desktop/src/lib/queue.ts
  - desktop/src/lib/connections.ts
  - desktop/src/lib/federation.ts
  - desktop/src/lib/dial.ts
  - desktop/src/lib/tmux.ts
  - desktop/src/lib/teamMode.ts
  - desktop/src/lib/gitActions.ts
  - desktop/src/lib/popout.ts
  - desktop/src-tauri/src/lib.rs
  - desktop/src-tauri/src/daemon.rs
  - desktop/src-tauri/src/connections.rs
  - desktop/src-tauri/src/lan.rs
  - desktop/src-tauri/src/popout.rs
  - desktop/src-tauri/src/update.rs
  - desktop/src-tauri/src/arcade.rs
  - scripts/install.sh
---

# The window

`desktop/` — React, Monaco and dockview in a Tauri shell, since 2026-08-04
([ADR-0018](../decisions/0018-a-second-client-in-typescript.md)); the only
window since 2026-08-05 ([ADR-0020](../decisions/0020-the-egui-client-is-retired.md)).
It is a projection of the daemon with no local authority; what it holds of its
own is listed below, each with the reason it is allowed to.

## One socket, one store

The window holds one WebSocket (`wire/client.ts`: reconnect, re-subscribe,
queue sends) and pushes every message into one zustand store — the only place
a `ServerMsg` is handled. Panes read the store and render.

**A projection has to be able to go stale on purpose.** The one piece of
daemon state held *by value* rather than by subscription is a file's body: it
arrives once, in answer to a `FetchFile`, and no event amends it. So the store
carries a `reload` flag it raises when it has reason to believe the body is out
of date — a `ChangeUpdated` naming that path, the OS window coming forward, the
pane being clicked back onto (`R-J38`). The old text stays on screen until the
new text lands, so a file an agent is rewriting does not flash between reads.

The git family reaches the daemon through one module, `lib/gitActions.ts`, and
one door for the log, because eight things narrow a log query and every bug in
that area was a second call site that carried seven. Which repository the Git
window shows is store state per session (`git[id].repo`, `null` for the
session's own root), attached to every send; picking another clears the rest
of the slice, and an answer is dropped unless the `repo` it echoes is the one
the slice shows, because a write re-broadcasts to every window on the session
and two may be pointed at two checkouts (`R-D34`). The Console tab (`R-D29`)
is fed from the store's one `send` and one `ingest`: every `git_*` command is a
row as it is sent, its answer closes the row, and a daemon `error` — which
carries no address on the wire — lands on the latest row still waiting. The
graph's lanes and the file trees are pure functions under `lib/` with their
own tests; nothing about the topology is asked of the daemon, since every
commit row carries its parents.

**Teams and seats are the daemon's, cached here** (`R-P3`, `R-P4`). `teams`
and `seatLaunches` are replaced wholesale on every `teams` and `seats`
message, as `notes` is; `teamsAllowed` is kept only from the answer to
`team_list`, because a broadcast carries no gate. The Teams window
(`ui/TeamsWindow.tsx`, the top bar's Users button, `Alt+Shift+g` — spelled
`Alt+Shift+KeyG` in `MAC_KEYS`) edits a draft and sends it whole; the daemon
validates, stamps and mints, and `lib/teams.ts` mirrors its rules so Save is
grey with the daemon's words before a click. A seat's row on the queue
carries a `team/seat` chip from the `seat` tag the daemon puts on it, so a
seat never passes for a session you started by hand. `teamSends` is the
third cache of the family (`R-P6`): every team's counters and send log,
replaced whole on `sends`, shown in the Teams window and in the mode.

**Gau Geung, the team mode** (`R-P7`, `ui/GauGeungMode.tsx`, `lib/teamMode.ts`)
is the wall's shape with a tile tree inside: a mode on the bare `Alt+g`
(`Alt+KeyG` on a Mac, where `⌥g` composes `©`), `Esc` leaves, the layout
underneath is untouched, and nothing is fetched — teams, launches, sessions,
the queue and the send logs are read as the store already holds them. It
took the chord from the Agent pane, which moved to `Alt+e` (the Mac keeps
`⌘G`). Inside is a second dockview, as the terminal panel is: a **board**,
a **seat pane** that follows the pick until you hold it, and a **terminal
pane per seat** you attach, each a `tmux attach` of its own like the Agent
pane; the arrangement is saved per machine under `mogeung.teamLayout:`, and
the daemon's grouped tmux layout (`R-P8`) is for a terminal and is not
mirrored — the header offers `tmux attach -t geung-<team>` to copy, read off
the launch record's `team_session`. The board shows OpenRig's **five lanes** for seats in a team —
needs you, parked with baton, blocked on agents, now, just finished — each
card carrying mogeung's badge inside, and `lib/teamMode.ts` reads the two
idle lanes off the send log: a seat idle after a message arrived holds the
baton, one idle after sending waits on the seat it told; both rank
`WAITING` on the queue, and the lane is what the mode knows that the queue
does not. Below it the **topology** is a graph and a table of the same
seats, together and with no toggle: an edge is the last send between two
seats, held edges drawn as waiting for you with the hop on them; a row names
its state and its **source** — `(hook)`, `(plugin)`, `(transcript)`, and
`(screen)` once `R-P11` reads a pane — its context fill where a status line
gave one and *unknown · no hooks* where none did, its tokens, and its sends.
The policy strip carries the counters, the pause switch and every held send
with release and drop, through the same verbs the Teams window sends. Tabs
show one team or all of them; each team keeps its own counters and switch.

**A notice is a warning or a toast, never both** (`R-J114`). Only an `error`
is a warning — counted in the status bar, listed in the Warnings window, kept
in the log; an `info` shows as a toast and is not counted. `capNotices` keeps
fifty of the one and ten of the other, so a burst of toasts cannot push a real
failure out of the log.

## Chrome and panes

The window docks things two ways, and which a thing uses is a decision
([ADR-0017](../decisions/0017-the-rail-is-chrome.md)).

**Panes** are views of a session, in the dockview tree: **Agent** (a
`tmux attach`), a **file** pane per open file (`R-B53`), a **diff** pane for
one file of one commit out of the Git window (`R-D30`, id
`diff:<session>:<rev>:<path>`), and the one exception — a **scratch** pane
(`R-L5`), a view of a file in `~/.mogeung/scratch` belonging to no session,
whose id the saved layout keeps because it names nothing that can go stale.
Changes, Transcript, Run, Debt, Insight and Git are tools in the bottom dock,
one at a time; Info sits under the queue.

*Which* session a pane shows is a property of the pane (`R-B49`): it resolves
its session through `PaneScope` rather than reading `selected`, follows the
queue when unbound, and **held** stays on one session while the selection
moves. That is what lets two agents be on screen and live at once, and it is
confined to the tile tree — chrome follows the selection by definition.
Activating a held Agent pane writes the selection back the other way, so the
file tabs, the dock and Info describe the session you are working in; a hold
is never disturbed by it. Slots are numbered (`agent`, `agent:2`, …) and never
keyed by session, so the serialised layout names no session; a hold lives in
the machine-scoped preferences beside pins and labels.

**Chrome** is everything that must stay reachable whichever pane is forward:
the Attention list on the left, the terminal panel across the bottom, the
tool-window rail on the right. The rail holds as many of its tools as you
open, stacked down one column in strip order
([ADR-0027](../decisions/0027-the-rail-stacks.md)); `prefs.rail` is a list read
through `railList` because files written before that day hold one name, and
each section's share of the height is a weight rather than pixels, so a saved
layout survives a different screen. The bottom dock is one tool at a time on
purpose — two stacked dock tools would each get half of the shortest panel in
the window. The terminal panel is its own dockview since `R-B56`: one shell
per split, tabs behind a hover grip, a colour per shell. Chrome state rides in
the client's preferences rather than in dockview's serialisation, because the
surrounding furniture's widths are the user's settings and must survive the
layout being reset.

**The wall** (`R-B50`) is neither: an overlay on a chord, every live session a
tile keyed by id so it never moves, no state but a boolean, fetching nothing.

**Keys are data** (`R-B11`, `R-B12`): an action table plus a map to text
bindings in `lib/keymap.ts`, with the shortcuts window generated from it.
Navigation actions are pane-agnostic — *next* means next thing in whatever
has focus. Only what you changed is stored and loading merges over the
defaults, so an action added later appears bound. Binding parsing rejects
anything it does not fully understand: a first version let `Ctl+J` parse as a
bare `J`, which fired on the wrong key while validation called it fine, and
"this shortcut does nothing" is indistinguishable from "this action is
broken". There are no bare-letter chords: a bare letter is a character in a
scratch file or a terminal (`R-L5`). The global hotkey (`Ctrl+Cmd+M`,
`R-B10`) is the shell's, through `tauri-plugin-global-shortcut`: `Pressed`
only, unminimise before focus, never fatal when another application owns the
chord; a shortcut macOS reserves registers and never fires, which nothing can
detect. The keymap listens in every popout too (`R-J109`), through tinykeys'
matcher rather than its listener, because a popout is another realm whose
`KeyboardEvent` is not `instanceof` this window's.

## What the shell holds, and why it is native

The Tauri process keeps a small native half, native for one reason each:

- **It holds the ptys.** `pty_open` runs `tmux attach` for an Agent pane and a
  shell under `tmux new-session -A` for the terminal panel, so what the window
  holds is a view and closing it detaches
  ([ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md),
  [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md)). `lib/tmux.ts`
  builds the argv and hands tmux the mouse per session (`R-J84`); against a
  remote daemon it wraps the command in `ssh -t <target>` through a login
  shell, and refuses without an `ssh_target` rather than guessing. The shell
  resolves `tmux` through `mogeungd::env` — a Dock-launched window has the
  launchd `PATH` the daemon does (`R-J87`), and `portable_pty`'s builder
  searches its *own* `PATH`, so it gets an absolute program and the repaired
  environment both.
- **It reads `~/.mogeung/machine-id`**, so local-versus-remote is decided by
  identity rather than by the address dialled (`R-I5`).
- **It owns the connection file.** The saved daemons —
  `{id, name, url, token?, note?}` — rest in `~/.mogeung/connections.json`,
  mode `0600`, written by the shell
  ([ADR-0036](../decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md)).
  The id is the identity, not the URL, so two routes to one daemon are two
  entries and an address can be edited. The token is composed into the dialled
  URL at the moment of connecting (`lib/dial.ts`: this machine's daemon takes
  the shell's port and token, a saved remote its token from the file) and is
  never part of what the panel displays; `localStorage` remembers the address
  alone. A browser tab has no shell, falls back to `localStorage`, and says so.
  The switcher's status check (`connections_status`) is the shell's too, so no
  token crosses into the webview to answer it.
- **It listens on the LAN** (`src-tauri/src/lan.rs`): udp/7717 from launch,
  beacons folded into a peer table keyed by machine id, *Found on your LAN*
  drawn from it; a peer greys after four misses and stays
  ([ADR-0040](../decisions/0040-discovery-is-the-beacon.md)). Finding is never
  connecting.
- **It hosts a daemon** (`daemon.rs`) when the port is free, on a thread, from
  `config.toml`, with `admit` checked on the main thread so a refusal is
  visible ([ADR-0009](../decisions/0009-the-window-may-host-a-daemon.md)). It
  stops the llmproxy it started on exit, because the hosted daemon's own
  shutdown path is unreachable ([model-and-proxy.md](model-and-proxy.md)).
- **It opens exactly two kinds of URL**: `open_local_url` takes `http://` on
  a parsed loopback host (the proxy's admin page), `open_artifact_url` takes
  `https://claude.ai/artifact/` plus 22 base58 characters (`R-J115`). Neither
  is the general opener plugin.
- **Export and the clipboard** are the shell's, with one permission each
  (`dialog:allow-save`; `clipboard-manager:allow-write-text`, and `read-text`
  deliberately not, because a pty that could read the clipboard is a remote
  session reading this machine's). Export falls back to `$XDG_DOWNLOAD_DIR`,
  `~/Downloads`, then `~/.mogeung/exports`, sanitising the name and refusing
  to overwrite where no dialog asked.
- **It writes one more kind of file** (`arcade.rs`): a history of play for a
  page in the window that is deliberately undocumented, one JSON line per
  session in `~/.mogeung/<name>.jsonl` for a fixed list of names, refused for
  any other name or any line that is not one JSON object. The daemon never
  reads them; the page loads no store and dials no daemon.

`desktop/src-tauri` is **its own cargo workspace**: as a member it would put
Tauri's system dependencies in the path of `cargo test --workspace`, which is
a gate for the daemon. The flip side is that a change to a public
`mogeung-core` type compiles everywhere that gate looks and can still break
the shell, so `cargo check --manifest-path desktop/src-tauri/Cargo.toml` is
the check to run after one.

## Pop-outs (`R-B55`, `R-J108`)

A pane can be moved into a window of its own, and **dockview owns that
window** ([ADR-0037](../decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md)):
`addPopoutGroup` moves a group's DOM into a second document and keeps one
dockview across both, so a pane can be dragged out, dragged back, and another
docked beside it. No second client, no second socket. It rests on one switch:
`window.open` in a Tauri webview is opt-in, and `popout.rs` supplies the
handler through `WebviewWindowBuilder::on_new_window`, handing back a real
window labelled `popout-*` under the same capability as `main` — which is why
the main window is built in `setup()` rather than by the config. dockview
copies the opener's stylesheets but not the `data-theme` attribute they read,
so `mirrorTheme` carries it across. Closing a popout returns its panes: a
Tauri close button never raises the `beforeunload` dockview listens for, so
`popout.rs` holds the first close and emits `popout:close`, the client raises
`beforeunload` on that group's window itself, and `watchPopouts` closes the
OS window once its group has gone; a second close passes through, so a client
that never answers cannot trap a window. The cost is coupling: one dockview,
one store, one socket, so a popout cannot outlive the main window.

## Client state, split by what it is about (`R-I11`, ADR-0023)

What describes *this window* — theme, layout, fonts, zoom, filters — is stored
flat; what is keyed by a session id or a path on the watched machine — hidden,
pinned, labels, colour tags, bookmarks, wrap and blame per path, holds, the
terminal panel's tab list — is stored under that machine's id, adopted when
the daemon publishes its identity, which is also when the terminal tabs swap.
Judgements stay in the client, machine-scoped, exportable
([ADR-0023](../decisions/0023-judgements-stay-in-the-client.md), `R-J11`).

**A saved preferences file is always older than the build that reads it.** A
field the file has never heard of is filled from the defaults on load, once.
A default that has *changed* cannot be reached that way, because the whole
object is written on every save; `PREFS_VERSION` is the seam: a file below the
current version is moved onto the new default once and stamped, and a bump
moves a setting somebody may have chosen, so a bump has to be worth that. A
behaviour can also change without a bump when it moves under a key that is
new (`filesClickOpens`, `R-J122`): no saved file holds the key, so every
window gets the new default and nobody's choice moves.

**Succession.** Session ids are not stable across `/clear`, which mints a new
one for the same conversation, so anything keyed by one has to be able to
move. The window matches a dead session against a live one sharing a pid and
a cwd and carries the label, colour tag and pin across (`migrateSuccession`),
plus the selection and any held panes. The evidence is on the wire — `pid`,
`alive`, `cwd`, `last_event_at` — which is why it is a client rule; what it
costs the daemon is that a dead session keeps its last pid
([data-model.md](data-model.md)). A process open for days has a *line* of dead
ids behind it, ordered by `last_event_at` — `started_at` is the process's
start and ties across the line (`R-J15`). Identity (labels, tags, pins) follows
the conversation every pass, idempotently; placement (the selection, a held
pane) hops once per predecessor, so a finished session can be sat and read.

**Switching daemons** tears the old connection down before the new one is
dialled, drops everything the old daemon said, keeps what the user chose, and
detaches terminal panes rather than closing them. The window acquires this
machine's own daemon at every launch, whichever board it shows (`R-J102`).

## Following, as the window sees it

Three things are per session rather than per daemon: where a pane attaches
(`reachForSession` — the origin's `ssh_target`), which live sessions a branch
switch's warning names (the switched session's machine only), and whether the
screen-acting buttons are offered (never for another machine's). The Attention
list groups by machine, this machine first (`groupQueue` in `lib/queue.ts`,
which the keyboard walks too), and marks every remote row *on HOST*.
`lib/federation.ts` renders a follow's state and the sentence beside it; the
follow itself is the daemon's ([architecture.md](architecture.md#one-daemon-following-others-r-i18-feature-0047-2026-09-23)).

## Updating itself (`R-J105`, ADR-0041)

Every build is stamped with its commit (`crates/mogeungd/build.rs`, read by
both binaries through `mogeungd::version`), and `install.sh` records what it
installed and from which checkout in `~/.mogeung/install.json`. `update.rs`
reads that checkout's branch on the window's schedule (`update_check`,
`update_branch`, `update_interval`, `update_source` in `config.toml`); asked
to, it checks the commit out in `~/.mogeung/update/tree`, runs
`install.sh --build-only` there as you, then `--no-build --elevate pkexec`,
whose one root step is `dpkg -i` through the system's polkit dialog — the
shell never handles a password. Each stage's output streams to the window and
is kept in `~/.mogeung/update/build.log` and `install.log`; `install.sh`'s
exit status says how it ended (3 a dismissed dialog, 4 not authorised). The
restart is a detached waiter that starts the installed binary only once this
process has exited, because the window usually hosts the daemon on its port.
Nothing here runs on the daemon side.
