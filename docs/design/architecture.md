---
title: Architecture
status: active
updated: 2026-10-21
covers:
  - crates/mogeungd/src/main.rs
  - crates/mogeungd/src/server.rs
  - crates/mogeungd/src/state.rs
  - crates/mogeungd/src/seat.rs
  - crates/mogeungd/src/hooks.rs
  - crates/mogeungd/src/policy.rs
  - crates/mogeungd/src/write.rs
  - crates/mogeungd/src/handover.rs
  - crates/mogeungd/src/screen.rs
  - crates/mogeungd/src/rig.rs
  - crates/mogeungd/src/env.rs
  - crates/mogeungd/src/machine.rs
  - crates/mogeungd/src/version.rs
  - crates/mogeungd/src/lan.rs
  - crates/mogeung-core/src/lan.rs
  - crates/mogeungd/src/discovery.rs
  - crates/mogeungd/src/federation.rs
  - crates/mogeungd/src/pricing_refresh.rs
  - crates/mogeungd/src/codex.rs
  - crates/mogeungd/src/qwen.rs
  - crates/mogeungd/src/opencode.rs
  - crates/mogeungd/src/usage.rs
  - crates/mogeungd/src/runner.rs
  - crates/mogeungd/src/insight.rs
  - crates/mogeungd/src/docscan.rs
  - crates/mogeungd/src/bin/beacon.rs
  - crates/mogeung-core/src/config.rs
  - crates/mogeung-tray/src/main.rs
---

# Architecture

The map. Each section says how a part works today and names the document that
owns the detail; history lives in the roadmap's rows and the ADRs, not here.

```
┌──────────────────────────────────────────────────────────────┐
│  Clients — every one a projection with no local authority     │
│  the window (desktop/, Tauri + React) · the tray · curl       │
└────────────────────────────┬─────────────────────────────────┘
                             │ WebSocket + REST, loopback by default
┌────────────────────────────┴─────────────────────────────────┐
│  mogeungd — the daemon, and the product                       │
│                                                               │
│  watcher.rs · adapter.rs   Claude Code: registry + transcripts │
│  codex.rs · qwen.rs · opencode.rs   the other three CLIs       │
│  state.rs      the scan loop, signals, attribution, launches   │
│  git.rs        diffs, risk order, hunk anchors, local writes   │
│  federation.rs one daemon following others                     │
│  api.rs · server.rs   the wire, admission, the gates           │
│                                                               │
│  Store: SQLite (derived state, notes) + files (nothing copied) │
└────────────────────────────┬─────────────────────────────────┘
                             │ reads only, never writes
┌────────────────────────────┴─────────────────────────────────┐
│  ~/.claude · ~/.codex · ~/.qwen · ~/.local/share/opencode      │
└──────────────────────────────────────────────────────────────┘
```

## The daemon is the product

Every UI is a client. mogeung keeps working with no window open, reach from
another machine is the same socket, and a second client is a packaging
decision rather than a rewrite — demonstrated twice (a phone page, `R-C3`,
removed unused; the TypeScript window, which replaced the egui one against an
unchanged daemon, [ADR-0020](../decisions/0020-the-egui-client-is-retired.md)).
The tray is what remains of the second-client claim: same socket, one number.

A client holds no authority, and four things it does hold are inside that rule
for a stated reason each:

- **The Agent pane** is a pty running `tmux attach` to a pane the daemon
  named; what it holds is a view, and closing the window detaches
  ([ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md)).
- **The terminal panel** is a shell the window started under tmux, so anything
  typed into it — a `claude` included — outlives the window and is observed
  like any other session ([ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md)).
  Against a remote daemon both panes run `ssh -t <target> tmux …` on the
  machine that has the files (`R-I6`), with no bare-pty fallback there.
- **Export and the clipboard** go through the Tauri shell with one permission
  each (`dialog:allow-save`; `clipboard-manager:allow-write-text`, and never
  read), so the webview can ask for a path or hand over text and cannot write
  a file or read the clipboard on its own.
- **What the daemon will read out** — a session's worktree, the folders added
  to it, `~/.claude`'s skills and memory — is bounded by the daemon, not the
  client: [read-boundary.md](read-boundary.md).

The repository is written locally and never published (ADR-0012, then
[ADR-0014](../decisions/0014-fetch-is-not-publishing.md) for `fetch`); the
CLIs' own files are never written; a process is started only when a
configuration the repository contains names it
([ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md)).

## The scan loop

Once, before the first scan, the passes `PRAGMA user_version` gates run — the
re-ingestion repair and the two backfills ([data-model.md](data-model.md#repair))
— so the first pass folds onto a clean record. Then every `--poll-ms` (default
1500):

1. Read `~/.claude/sessions/*.json`; drop entries whose pid is not running.
2. Scan `~/.claude/projects/**/*.jsonl` modified within 14 days. A file over
   4 MiB is followed from near its end rather than read whole.
3. Tail each file from its recorded byte offset; classify and fold every line
   into its session. **Every line is accounted for**, including discarded ones
   ([health-and-canary.md](health-and-canary.md)). The offset is recorded once
   those lines are folded in, so a restart resumes rather than re-reads
   (`R-A6`, [data-model.md](data-model.md#read-positions-are-part-of-the-record-r-a6)).
4. Adopt any live-registry entry no transcript has mentioned. Claude Code
   writes the `.jsonl` on the first message, not on launch, so a session you
   have just opened exists only in the registry (`R-J30`). After step 3 on
   purpose: a resumed session has a transcript already, and taking it in from
   the registry first would read an 11 MB history whole inside the loop. The
   transcript path stays empty until the file appears — deriving it means
   deriving the project-slug rule, which `A4` says will move.
5. Apply liveness to **every** known session, in place under one write lock,
   because busy→idle produces no transcript line and is the most important
   signal there is. The same pass resolves each live session's tmux pane from
   **one process table per pass** (`R-J64`): `tmux list-panes` and `ps` once,
   reused inside `PROC_TABLE_TTL_MS`, the expensive ancestry half re-forked
   only when the pane list changed, a caller needed it, or a backstop elapsed;
   if the blocking pool refuses, the last table is served rather than an empty
   one, because a spurious *no panes* unhosts every Agent pane for a tick.
6. Recompute diffs — **gated on the worktree actually moving** (`R-J53`). A
   growing transcript is the trigger to *look*, not to diff: a look is a cheap
   fingerprint, only a moved one pays for `compute_change`, and a session that
   just exited bypasses the gate. [review-checkpointing.md](review-checkpointing.md).
7. Flush sessions whose counter-only updates have coasted long enough
   (`R-J54`); anything a decision hangs off was broadcast the moment it was
   folded. `significant_change` binds every `Session` field by name, so a new
   field is a compile error until someone says which side of the mask it is on
   (`R-J66`); the eleven masked fields are the counters, the two timings and
   the histories that move whenever a transcript grows.
8. Rank the queue over borrows and broadcast it **only if it differs** from
   the last announcement; a moved diff travels as a `ChangeSummary`; Health
   when it says something new, plus a slow heartbeat
   ([wire-protocol.md](wire-protocol.md#the-contract)).
9. Daily, and on the first pass after start: retention (`R-J57`) and the WAL
   checkpoint ([data-model.md](data-model.md#persistence)).

One scan runs at a time; the interval, a websocket `rescan` and the HTTP
rescan collapse into whichever pass is underway. Polling rather than
filesystem events: stat-ing a few dozen files every 1.5 s costs nothing and
avoids every rename and atomic-write edge case. What the poll must never do is
treat "the transcript grew" as "everything downstream is stale" — the gates in
steps 6 to 9 are the line between polling for *evidence* and repeating *work*.

**The other three CLIs** follow the same rule one directory over. Codex's
`threads` index is read only when its mtime moved — or the writer-lock
directory's, which a thread opening or closing changes (`R-J56`, `R-J76`) —
except while a Codex session is alive. Codex liveness is an advisory `flock`
on `~/.codex/thread-writer-locks/<thread>.lock`, a real registry: **read,
never taken** (a daemon must not compete with an agent for a resource it
needs), from `/proc/locks` on Linux where it also yields the pid and therefore
a hostable pane, by the file's presence on macOS where the pid is unknown and
a crash's leftover lock reads as alive. A thread with no index row yet is
adopted from its lock, so a session is on the board before you speak to it
(`R-J73`). Qwen Code is structurally Claude's shape — a registry and per-session
JSONL — with Gemini-shaped records; opencode is a database, opened read-only,
with only the sessions whose `event_sequence` moved re-read, and liveness a
process match by `$PWD` and start time. One `Session` holds all four
([ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md),
[data-model.md](data-model.md)).

### Beside the scan: the price table's task (`R-J124`)

A second loop, spawned by `server::run`, wakes a minute after start-up and
then every six hours, or at once when a config save wakes it (`pricing_kick`).
It runs the usage report, so a model in use with no price becomes a Health
alert without anyone opening Analytics; and if `refresh_pricing` in the config
names `platform.claude.com` and a reading is due, it fetches the pricing and
deprecations pages through `curl` (`pricing_refresh.rs`). The daemon's first
network call with no person behind it, on
[ADR-0042](../decisions/0042-a-timer-may-read-a-public-page.md)'s terms:
consent names the host, nothing about the machine is sent, weekly at most,
compare-only. Its record lives in `pricing/` beside the database.
`refresh_pricing` is deliberately read from the file at each pass rather than
being an `Options` field, so a save takes effect without a restart and the
window's field-by-field `Options` cannot break on it again.

## Who runs the daemon

The window binds the daemon port at start-up. Winning the bind means hosting a
daemon **on a thread in its own process**; losing it means attaching to the one
already there. The bind *is* the check — probing first and starting second
races two windows — and a thread cannot outlive its process, which removes the
pid file, the cleanup-on-exit and the orphan-holding-the-port problems at once
([ADR-0009](../decisions/0009-the-window-may-host-a-daemon.md)). `mogeungd` is
how you get a daemon that outlives every window, and `scripts/mogeungd`
(`R-J101`) builds and manages it — a pid file and a log under `~/.mogeung`, a
stop that sends SIGINT because graceful shutdown hangs off Ctrl-C, and a
refusal rather than a kill when the port is held by a window.

**A hosted daemon obeys the same admission rule** (`R-I10`). `server::admit`
refuses a bind beyond loopback with no token, checked on the bound address
before anything is served, and the window asks it before serving on the socket
it just won. The hosted daemon reads `listen`, `token`, `ssh_target`,
`advertise`, `beacon`, `terminal` (`R-J143`) and the three model keys from
`config.toml`, so a plain launch can be LAN-connectable or model-equipped from
the file alone; it takes the defaults for `db`, `poll_ms` and the rest. A file
it cannot read whole hosts nothing, and the window shows the line and the key
as the reason nothing is serving
([ADR-0045](../decisions/0045-a-broken-config-file-stops-the-daemon.md)). It is acquired at **every**
launch, whatever board the window shows (`R-J102`): which daemon runs and
which one is watched are separate choices.

**The daemon's identity** is a `DaemonIdentity` — a stable `machine_id` from
`~/.mogeung/machine-id`, hostname, watched `~/.claude`, pid, version, optional
`ssh_target`, federation protocol — on every snapshot and on `/api/health`.
Both the daemon and the shell resolve it through one function in
`mogeungd::machine`, so two processes on one desk cannot disagree; the window
compares ids rather than guessing from the address it dialled, which is what
makes an `ssh -L` tunnel read as remote (`R-I5`).

**Gates are decided per connection, not only from the bind** (`R-J103`,
`R-J104`). The run verbs, the chat and send-to-session are open to a client on
this machine — the peer address `server::run` records on every request — and
judged by the bind and the flags for everyone else, so a window hosting a
LAN-bound daemon from its own `config.toml` is not refused its own panel. The
config editor still reads the bind. The exact table is in
[wire-protocol.md](wire-protocol.md#security).

### The daemon announces itself

Every daemon, `mogeungd` and the one a window hosts alike, sends a small UDP
beacon to each interface's broadcast address every 15 s, as a burst of two a
second apart: version, machine id, hostname, Claude home, API port, how it can
be reached (`lan` beyond loopback and therefore token-gated, `ssh` with the
configured target, `local` otherwise), and how many sessions are live and need
someone. **Never the token**, and no field to put one in — the codec in
`mogeung-core/src/lan.rs` is shared with the shell that listens, so the two
ends cannot disagree. [ADR-0039](../decisions/0039-the-daemon-announces-itself-by-default.md)
makes it the default (`beacon = false` or `--no-beacon` silences it);
[ADR-0040](../decisions/0040-discovery-is-the-beacon.md) makes directed
broadcast the transport, measured across an Orbi mesh where multicast did not
pass between two Wi-Fi clients. *Gone* is four missed beacons, because the
loss the probe measured is the receiving laptop's Wi-Fi power save. **The bind
does not move with the beacon**: presence is not access, and a found daemon is
an offer a hand takes up by typing the token.

The mDNS advert (`--advertise`, `_mogeung._tcp`) stays opt-in as a record for
other tools' browsers; a loopback bind refuses to advertise; nothing of
mogeung's browses it. `discovery::Watcher` and `discovery::browse` live on for
one caller, `--bin beacon`'s probe, which counts mDNS resolutions beside
beacons so the measurement can be re-run.

### The daemon may own a process

A process you named — a run configuration the repository contains, by id,
never a command string — with the run outliving the window that started it
and spawning opt-in beyond loopback by `--allow-run`, read from the bind once
by the code that bound the socket so the start-up refusal and the per-request
gate cannot disagree ([run-and-debug.md](run-and-debug.md)). A signal command
you configured, on a click (`R-E2`). And its own llmproxy
([model-and-proxy.md](model-and-proxy.md)). Never an agent you started:
`run::AGENTS` is the list refused by name, and the same list is what
recognises an agent in a tmux pane ([cross-session-signals.md](cross-session-signals.md)).

**And since 2026-10-09, a seat** (`R-P4`, feature 0051): an agent the daemon
starts itself from a team definition it holds, in Gau Geung mode
([ADR-0003](../decisions/0003-observe-do-not-spawn.md)'s amendment). `seat.rs`
composes each CLI's argv and environment from the definition through one
match per `SessionSource` — session id where the CLI takes one, permission
mode in the CLI's own flag, the model and since `R-P16` the thinking level
(`--effort`, Codex's `model_reasoning_effort` override, opencode's agent
`variant` beside its model; Qwen takes none, and `validate` refuses one there),
the two layers of guidance by a flag or a file
under `~/.mogeung`, a home of its own only when asked
([ADR-0043](../decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md))
— and starts it in a tmux session of its own, in a worktree cut on
`geung/<team>/<seat>` or the team's checkout. The **launch record** (pane id,
chosen session id, tmux session, directory) is persisted so a restarted
daemon re-adopts the seat; the scan publishes a seat's row from the record
from birth, never as a `pane:` row, and a post-pass (`tag_seats`) carries the
tag onto the CLI's own session once it claims the pane or the chosen id
([ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md)'s
amendment). A stop ends the processes and forgets the records; the worktrees
stay, and removing one is a separate verb that refuses a dirty tree. Boot,
stop and remove are judged like a run start. Since `R-P14` a boot reads
*running* off tmux rather than off the record — the record outlives the
process so the exit can be waked on — so a seat whose pane has gone is
forgotten and started again, the team session is rebuilt only when something
started, and `TeamBooted` tells the asker what happened. Since `R-P15` there
is a second way down: `TeamWindDown` types the policy's wind-down text into
every running seat and finishes each as its note arrives (`POST
/api/seat/note`, through `mogeung-note` on the seat's `PATH`), as its turn
ends, or at the deadline; the note is kept as `note.md` beside the seat's
files and is the first evidence the next handover quotes for that seat, and a
boot after a stop hands each seat its own earlier session and note. A team
winding down wakes and nudges nobody; `Sends` carries how it is going. Since
`R-J134` the record says *running* itself: `alive` is whether the seat's own
tmux session stands, read off the `list-panes` the scan already takes
(`session_in_panes`), recomputed as the daemon starts and broadcast as
`Seats` when any seat flips — so a window counts exited seats as exited
rather than as records. A team's delete refuses while a seat runs, naming
it, and otherwise forgets the exited records and removes the files made for
its seats; the worktrees stay.

**And a seat talks back** (`R-P5`, since 2026-10-10): `hooks.rs` is the first
surface in mogeung that an agent's **own process** reaches — ADR-0043 clause
3, and the *new daemon surface* that ADR names. When the server is up it
records its bind in `AppState::listen_addr`, and the launcher hands every
seat whose CLI can carry a hook the way back: a token minted per seat and the
receiver's loopback URL in the seat's environment, then whatever the CLI
accepts at launch — a `--settings` document with hooks and a status line for
Claude Code, a plugin file named in the inline config for opencode, `hooks`
in the `settings.json` of a home of mogeung's own for Qwen Code, and nothing
for Codex, whose hooks live in its config beside trust hashes. Every file is
under `~/.mogeung/seats/<team>/<seat>/`. The receiver (`POST /api/seat/hook`,
`/api/seat/status`) is merged **outside** the shared-token layer and admits a
loopback peer with one seat's token and no other; what arrives is data —
classified against `CLAUDE_HANDLED` / `OPENCODE_HANDLED` the way transcript
lines are, an unknown name raised through the Health canary as `hook:<name>`
— and becomes the launch record's `signals`: the kind (*hooked* or *plain*,
on the row's tag), the count, the last event and its line, the context fill
and the model from the status line. Hook bodies are kept beside the seat's
files as `signals.jsonl`, capped, for the week of captures; status readings
are never kept. The token is persisted with the record so a restarted daemon
still hears a seat it re-adopts, and stripped before the record crosses the
wire.

**And a seat is sent to** (`R-P6`, since 2026-10-10): `policy.rs` is fence 4
of ADR-0003's amendment — *every automatic send is visible, budgeted and
stoppable* — as a ledger per team and a judge over it. The ledger is the send
log; the counters (the chain of hops since a human last sent, the automatic
deliveries in the last hour) are read off it, so there is no second state to
keep right and a restart counts what it counted before. The judge answers
*deliver* or *hold* from the team's pause switch, the hop budget and the
hourly cap, and holds when it cannot read the definition at all — a daemon
that cannot reach its own policy behaves as if paused. **One function types
into a seat**, `AppState::type_into`, and it takes a launch record and no
other kind of address: there is no value an observed session could be, which
is fence 1 at the type level; `send.rs`'s three tmux invocations are still
the only code below it that reaches a pane, and a test seam over them is the
only thing a test replaces. Three things reach the judge: a seat's own words,
through `mogeung-send` — a generated command first on every seat's `PATH`
under its files (set by `env` on the command, since tmux overrides a
`-e PATH`; the guidance names the directory too — `R-P17`), posting to `POST /api/seat/send` with the seat's token, the
recipient resolved within the sender's team only; a declared wake, fired by a
`Stop` or `SessionEnd` hook (`hooks::seat_event`), by a plain seat's registry
going busy to idle, or by its pane going; and a declared watchdog, on the
scan's tick once per silence. A held send lands on the recipient's row as
`held_send` and ranks it as needing you; the Teams window shows the counters,
the pause switch (which acts at once and is the one writer of `paused`), what
is held with *release* and *drop*, and the log. The status line a Claude seat
shows carries the counters too.

**And the team has a session for a terminal** (`R-P8`, since 2026-10-10):
*"ideally I want it to have a special grouped layout"* — so `tmux attach -t
geung-<team>` is the TUI client this feature has instead of one of its own.
Every seat keeps its own session (`R-P4`), because the scan, the send path
and the window all key on the seat's pane and a pane belongs to one window;
`seat::layout_team` builds the team's session **around** them after a boot:
a window named `all` whose panes are nested tmux clients, one attached to
each seat's session (`env -u TMUX tmux attach`, so the inner tmux forgets it
is inside) and tiled, so every seat is on screen at once and keys typed into
a pane reach its seat; then every seat's own window linked in by name
(`link-window`), so one seat can take the whole screen with a window switch;
the status line on, which lists the seats. A stop kills the team session
first, and the seats stand: tmux closes only windows linked nowhere else,
and a nested client merely detaches — pinned by `tests/team_tmux.rs` against
a real server, with `sleep` for seats. The window does not mirror this
layout; it attaches per pane and arranges its own grid, and the mode offers
the attach command to copy. The launch record carries the session's name as
`team_session`.

**And a human writes in a seat's worktree** (`R-P9`, since 2026-10-10):
`write.rs` is the narrow part of ADR-0019's amendment — the hash a
`FileContent` carries, the judge over *moved* and *busy*, containment for a
path that may not exist yet, and a rename for the write itself.
`AppState::write_seat_file` resolves the seat from the session's tag and
refuses before the path is looked at unless the launch record says the
directory is a worktree the daemon made; `FileWrite` is the verb, behind the
write guard and the runs gate. [read-boundary.md](read-boundary.md#the-one-write-r-p9)
has the fences in order.

**And a seat is handed over to** (`R-P10`, since 2026-10-10): the one
automatic send that is not declared text, under ADR-0003's later amendment.
`handover.rs` is the prompt and the reading of the answer; `state.rs`
gathers the evidence from what the daemon already holds — the team's two
layers of guidance, the teammates' last turns from the store and their
diffs, the send log, and on a restart the seat's own earlier session — asks
the model through the one seam (ADR-0031: consent to a remote host is the
seam's to judge, and with no endpoint the seat starts without a handover and
the log says so), and hands the text to the policy engine as a `Handover`
send: held on the seat's row under *after you read it*, judged and typed as
a hop under *automatic*, nothing under *none*. Every line of evidence is
framed as data another agent wrote, and the prompt says so before any of it
appears. The trigger is a boot that joins a team with history, or
`SeatRestart`, which stops one seat and starts it again with its earlier
session as predecessor; composition runs in the background, so a boot is
answered by rows. The entry is logged *composed by <model> @ <host>* and the
text is kept whole.

**And a seat's pane is read** (`R-P11`, since 2026-10-10):
[ADR-0044](../decisions/0044-mogeung-reads-a-seats-pane-and-no-other.md)
admitted reading a pane the daemon made and no other, and `screen.rs` is
the reading. `SeatPane` has one constructor and it takes a launch record,
which is clause 1 as a type; `classify` is a keyword matcher over the last
forty lines through one `match` per CLI into a closed set of shapes, with
`unknown` as a value, pinned by every fixture phase 0 captured. The scan
reads each seat's pane once per rest (`screen_tick`, before the rows are
built so the shape rides on them), skips an unchanged capture by its hash,
raises an unknown screen as `screen:<cli>` through Health, and makes the one
press — Enter, for *press Enter*, under the team's `readiness_press` — once
per screen as a logged automatic send. A gate ranks the row with *(screen)*
in the detail, after the registry's facts; the seat's card on the board shows the
shape, when it was read and the lines it was read from.

**And a team crosses a file boundary** (`R-P12`, since 2026-10-10): `rig.rs`
is the door. Out, the definition whole as JSON where you say or beside the
mirror; in, a draft and never a save — mogeung's own file as a new team, or an
OpenRig `rig.yaml` read as other people's configuration, members to seats by
runtime, guidance followed beside the rig, edges as text, the rest listed as
not carried. The window shows the draft with the notes and you press Save.

### The PATH a launcher does not give (`R-J87`)

A bundle launched from the Dock, Finder or Spotlight gets the launchd `PATH`
and reads no profile, so `/opt/homebrew/bin` is absent and `tmux` is invisible
— and the code it breaks is written to treat a missing tmux as ordinary, so
every pane list came back empty and the Agent tab said *not running under
tmux* about a session in `tmux ls`. `env.rs` repairs it, and does two things
because a child needs two: `env::which` resolves the program to an absolute
path against the repaired list (`posix_spawnp` searches the *caller's* `PATH`,
so `.env` alone would not find it), and `env::command` hands the child the
repaired `PATH`, because a headless launch creates the tmux server every agent
under it inherits from. Directories are appended, never prepended, and only
when they exist; the variable itself is never written, since `setenv` mutates
state other threads read and this daemon is routinely hosted in-process. The
shell calls `mogeungd::env` rather than copying it
([the-window.md](the-window.md)).

## One daemon following others (`R-I18`, feature 0047, 2026-09-23)

A daemon may follow daemons on other machines and show their sessions on its
own board — the shape [ADR-0013](../decisions/0013-one-window-one-daemon.md)
named when it refused an aggregating window, admitted by its amendments of
2026-09-23. **The window still watches one daemon and computes nothing**;
everything below happens in `federation.rs`.

```
this machine                                      a followed machine
┌──────────┐  one socket  ┌──────────────────┐    ┌──────────────┐
│ window   │ ───────────→ │ mogeungd         │    │ mogeungd     │
│          │              │  own sessions    │    │              │
│          │              │  + Federation ───┼──→ │ watch socket │  board only
│          │              │    mirrored map  │    │              │
│          │  relay per   │    Relays ───────┼──→ │ relay socket │  one per window
│          │  window,peer │                  │    │              │
└──────────┘              └──────────────────┘    └──────────────┘
```

- **The follow list** is `~/.mogeung/peers.json`, `0600`, holding each
  follow's token ([ADR-0036](../decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md)'s
  amendment): written only by a *Follow* from a window on this machine, never
  served, never re-exported.
- **A watch socket per follow** carries the peer's board — snapshot, session
  updates and removals — with a `Bearer` header, a ping every 15 s, silence
  for 45 s counted as lost, and a redial backoff from 1 s to 30 s. It never
  carries a command.
- **Mirrored sessions live in a map of their own**, not in
  `AppState.sessions`. Liveness, collision detection, persistence and
  retention all walk that map and would each do the wrong thing to another
  machine's session; merging only at the outputs — the snapshot, the ranked
  queue, the notifier — keeps them out without a guard in every pass.
- **Staleness is measured from when a peer was last heard.** A minute later
  its sessions are marked `origin.stale`, ranked below everything live and
  greyed; the watch loop sleeps no later than that moment, so a 30 s backoff
  cannot make it late.
- **A relay socket per (window, peer)** carries forwarded commands and their
  answers back to exactly the window that asked; every message is classified
  with no wildcard arm ([wire-protocol.md](wire-protocol.md#identity-and-federation)).
- **Writes cross only under two gates**: the follow's *allow changes* consent,
  and the peer's own gates judging the relay as the client it is.

Measured 2026-09-23 on one box: the follower's snapshot grows from ~348 KB to
~712 KB; steady state costs about +0.1 % CPU and +4.5 MB; a warm relayed read
adds 10–30 ms; the first command to a peer pays ~140 ms to open its relay.

## Client contract

Commands are fire-and-forget and their effect returns on the event stream, so
a client is a pure projection with no correlation layer. A snapshot arrives
unsolicited on connect. The scan's broadcasts are change-gated; a request
never is. [wire-protocol.md](wire-protocol.md) is the contract;
[the-window.md](the-window.md) is the one client that matters, and the tray
is the other.

## What is deliberately absent

No supervisor over an agent, and no writes to the CLIs' own files
([ADR-0003](../decisions/0003-observe-do-not-spawn.md)). The children the
daemon does own are a process you named, a signal you configured and its own
llmproxy; none of them is an agent. No write to a worktree file
([ADR-0019](../decisions/0019-a-viewer-not-an-editor.md)); no push, pull or
merge; no TLS of its own; no timer that sends anything to anyone.

## Modules, and the document that owns each

| Path | What it is | Owned by |
|---|---|---|
| `crates/mogeungd/src/main.rs` · `server.rs` | flags, config, admission, the two loops, the gates | this document; [wire-protocol.md](wire-protocol.md) |
| `watcher.rs` · `adapter.rs` | Claude Code's registry and transcript tail; every line classified | [claude-code-formats.md](claude-code-formats.md) |
| `codex.rs` · `qwen.rs` · `opencode.rs` | the other three readers, one `Session` | this document; [health-and-canary.md](health-and-canary.md) for their canaries |
| `state.rs` | the scan loop, signals, attribution, launches, handoffs, git wrappers, notes | this document; [cross-session-signals.md](cross-session-signals.md); [read-boundary.md](read-boundary.md) |
| `git.rs` | diffs, risk order, anchors, the local write verbs | [review-checkpointing.md](review-checkpointing.md) |
| `store.rs` · `notes.rs` · `scratch.rs` | SQLite, the markdown mirror, the scratch directory | [data-model.md](data-model.md); [wire-protocol.md](wire-protocol.md#notes-tasks-and-scratch) |
| `api.rs` | the WebSocket handler, the reply lane, REST | [wire-protocol.md](wire-protocol.md) |
| `federation.rs` (+ `mogeung-core/src/federation.rs`) | following, mirroring, relays, the hop classes | this document; [wire-protocol.md](wire-protocol.md#identity-and-federation) |
| `health.rs` (both crates) | the canary, the tracker, alerts | [health-and-canary.md](health-and-canary.md) |
| `attention.rs` (core) | the ranking | [attention-ranking.md](attention-ranking.md) |
| `notify.rs` · `send.rs` · `env.rs` | banners and push; one-shot send into a pane; the repaired `PATH` | [cross-session-signals.md](cross-session-signals.md) |
| `workspace.rs` | the folders a session may be read through | [read-boundary.md](read-boundary.md) |
| `kit.rs` (both crates) | skills and memory under `~/.claude`, read only | [claude-code-formats.md](claude-code-formats.md) |
| `run.rs` (both) · `detect.rs` · `runconfig.rs` · `runner.rs` | run configurations, the daemon-owned process, the signal runner | [run-and-debug.md](run-and-debug.md) |
| `team.rs` (both) · `seat.rs` · `hooks.rs` · `policy.rs` · `write.rs` · `handover.rs` · `screen.rs` · `rig.rs` | team and seat definitions, their mirror; the seat launcher and its records; the hook receiver and what a seat is handed to reach it; the policy engine — the send log, the judge, the counters; the one write into a seat's worktree; the handover's prompt and reading; the pane read into a closed set of shapes; a definition out as a file and a file in as a draft | this document; [wire-protocol.md](wire-protocol.md#teams-r-p3-feature-0051); [data-model.md](data-model.md); [read-boundary.md](read-boundary.md#the-one-write-r-p9) |
| `model.rs` · `llmproxy.rs` (both) · `guide.rs` · `why.rs` · `embed.rs` | the model seam, the proxy, the three model features | [model-and-proxy.md](model-and-proxy.md) |
| `usage.rs` · `pricing.rs` (core) · `pricing_refresh.rs` | token burn by model and day, the price table and its weekly reading | this document; [health-and-canary.md](health-and-canary.md#the-price-tables-canary-r-j124-2026-10-08) |
| `insight.rs` · `docscan.rs` | search, digests, failures, prompts, blame; the markdown inventory | [cross-session-signals.md](cross-session-signals.md#the-insight-layer); [wire-protocol.md](wire-protocol.md#insight-usage-and-docs) |
| `lan.rs` (both) · `discovery.rs` · `machine.rs` · `version.rs` | the beacon, the mDNS advert, identity, the build stamp | this document |
| `bin/sweep.rs` · `bin/runconfigs.rs` · `bin/beacon.rs` · `bin/judge.rs` · `bin/why.rs` | the instruments: corpus sweep, run-config sweep, the LAN probe, the two model harnesses | [claude-code-formats.md](claude-code-formats.md); [run-and-debug.md](run-and-debug.md); this document; [model-and-proxy.md](model-and-proxy.md) |
| `crates/mogeung-core/src/config.rs` | `~/.mogeung/config.toml`, every key, `deny_unknown_fields`; a file that cannot be read whole is a `ConfigError` with the line, the key and the fix, and stops the daemon rather than starting it on the defaults (ADR-0045) | [wire-protocol.md](wire-protocol.md#the-config-file-r-j79) |
| `crates/mogeung-tray` | the tray: one socket, the waiting count, the daemon it counts for | this document |
| `desktop/` | the window | [the-window.md](the-window.md) |
