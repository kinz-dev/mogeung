<img src="assets/mogeung.png" alt="mogeung" width="200" align="right">

# mogeung

A supervision layer over the agent sessions you already run — and, since
2026-10-09, a place to run a team of them.

**It watches. It never starts, steers or stops an agent it did not start
itself.** You keep running `claude`, `codex`, `qwen` or `opencode` in as many
terminal tabs as you like — full interactive loop, plan mode, slash commands,
permission prompts, untouched. mogeung reads the files each CLI already writes
for itself and adds the layer that doesn't exist:

- **one queue across every session**, ranked by who needs you — across every
  machine you follow, not only this one
- **a diff per session**, risk-ordered, with per-hunk read marks that survive
  the agent rewriting the file
- **a health panel that says what it cannot see**, because the formats it reads
  are undocumented and going quietly blind must not look like a quiet day
- **a collision warning** when two live sessions edit the same file — the one
  thing no single agent can know about itself

And the one thing it *does* start: a **team**. In Gau Geung mode mogeung
launches real CLIs in real tmux panes, one worktree each, lets them hand each
other the baton under a budget you hold, and ranks the seat that needs you on
the same queue as everything else. See [A team of agents](#a-team-of-agents-gau-geung).

Because it observes rather than wraps, it cannot make any individual session
worse. That was the fatal flaw of the first version — see
[ADR-0003](docs/decisions/0003-observe-do-not-spawn.md). Its amendments are the
whole story of what mogeung may do on your behalf: press send, once, on a click
and a confirmation (2026-08-29); and own a **seat** — a session it started
itself, in a team — behind five fences and a hop budget (2026-10-09). Every
session you start by hand keeps the old sentence whole.

**Status: in daily use.** The premise — that a cross-session queue changes where
you look, and that you run several agents at once — was supported by a week of
use on 2026-08-04 (A1 and A6 in
[the assumption ledger](docs/product/assumptions.md)); whether the *ranking* is
right is still unmeasured. The team mode is the newest part: built 2026-10-09
and 2026-10-10, first real team booted 2026-10-11, and every one of its rows
stays ⏳ until it is seen to work in use. [The roadmap](docs/product/roadmap.md)
is the ledger of what is built, what is awaiting a verdict and what is next.

## Install it

```sh
./scripts/install.sh
```

Builds the release daemon and the window, installs `mogeungd` and the five
tmux launchers — `yolomo` (claude), `yolomop` (claude through `llmproxy`),
`codexmo`, `qwenmo` and `opencodemo` — into `~/.local/bin`, and installs the
window as an application in the launcher (a `.deb` through `dpkg` on Linux,
which asks for `sudo` at that step and no other; an `.app` on macOS). The
daemon is compiled once, inside the window's workspace, and the script says
how long it took. `install.sh -h` lists `--no-desktop`, `--no-build` and the
rest. An installed window then updates itself from the checkout when a new
commit lands on `main`
([ADR-0041](docs/decisions/0041-the-window-updates-itself-through-the-os.md)).

One executable. It starts a daemon if none is watching, attaches to one if there
is, and a daemon it started stops when you close the window. For a daemon that
outlives every window — so notifications keep firing while nothing is on screen:

```sh
./scripts/mogeungd start -- --notify   # builds, starts in the background; stop, restart, status, logs
```

Open the window as usual and it attaches to that daemon instead of hosting one.
See [ADR-0009](docs/decisions/0009-the-window-may-host-a-daemon.md).

Nothing to configure, no repos to register. A session run anywhere in the last
14 days appears. Nothing is ever written under `~/.claude`, `~/.codex`,
`~/.qwen` or opencode's directories — not for a seat either
([ADR-0043](docs/decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)).

→ [Getting started](docs/guide/getting-started.md)

### Two builds, on purpose

`cargo build --release` at the repo root builds the daemon and **not** the
window: `desktop/src-tauri` is deliberately its own cargo workspace, so a
machine without node or webkit's headers can still build, test and run the part
that does the watching. The window is a TypeScript client in a Tauri shell; it
replaced the original egui client on 2026-08-05
([ADR-0020](docs/decisions/0020-the-egui-client-is-retired.md)), and the daemon
never knew there were two — which is the claim "every UI is a client" was
making all along.

→ [Building it, and what it needs installed](desktop/README.md#building-a-binary-you-can-hand-to-someone)

## The queue

```
APPROVE  → blocked on a permission prompt it needs you to answer
WAITING  → alive and idle: it is waiting for you to type
FAILED   → hit an API error
LIMIT    → hit a rate limit; several usually go dark together
REVIEW   → exited, and left changes nobody has read
STALLED  → alive and busy, but silent past a threshold
running  → working normally
idle     → reviewed, ended with no changes, or snoozed
```

`WAITING` is not a heuristic — Claude Code publishes `busy`/`idle` in its own
live registry, so mogeung is told, not guessing. `APPROVE` splits that: an
unanswered tool call means the agent is *blocked*, not merely finished.
Ranking is strict: a session can never jump into a more urgent tier by waiting.

The list groups by machine and folder. Seats of a team sit under a **Team**
heading at the top, a heading per team; a machine you follow is headed by the
name you gave its connection. An Agent pane's tab wears the same badge after
the session's name, so the tab and the row never disagree.

`↑`/`↓` move through the queue, `Alt+1` puts the keyboard in it, and
`Alt+2`–`Alt+5`, `Alt+8` and `Alt+9` open the dock along the bottom — Changes,
Transcript, Run, Debt, Insight, Git — with the same chord closing what it
opened. `Alt+E` is the Agent pane, `Alt+W` the wall, `Alt+G` the team board
and `Alt+Shift+G` the Teams window. Numbers rather than initials because
`Alt+T` is Claude Code's own *toggle thinking*, and a window that keeps it is a
window that steals a key from the agent it is showing you. Every binding is
editable (`Alt+K`). **`Ctrl+Cmd+M` brings mogeung back** from anywhere, which
is the other half of that round trip.

**New session** and **Teams** open in OS windows of their own — modal, one
copy, each remembering its size and zoom. New session can start a session on a
machine you follow: the picker names it as you named its connection, browses
*that* machine's folders, and the far daemon runs the launch headless; the
session comes back mirrored like every other. Beside the anchor in an Agent
pane's header, three buttons open the session's folder in the file manager, a
terminal there, or IntelliJ — on the machine the folder is on. Which terminal
is the `terminal` key in that machine's `~/.mogeung/config.toml`, edited in
the Configuration window; terminator on Linux and iTerm2 on a Mac until set.

→ [The attention queue](docs/guide/the-queue.md) ·
[Reviewing changes](docs/guide/reviewing.md) ·
[The Git tool window](docs/guide/git.md) ·
[Away from the desk](docs/guide/away-from-the-desk.md) ·
[Watching a remote machine](docs/guide/remote.md) ·
[Troubleshooting](docs/guide/troubleshooting.md)

## A team of agents (Gau Geung)

The one place mogeung starts an agent. A **team** is a named set of **seats**
with one policy; a seat is the unmodified CLI in a tmux pane, started by the
daemon from a definition you wrote in the window, usually in a fresh worktree
of the team's repository on branch `geung/<team>/<seat>`. Every other session
is *observed*, and nothing promotes it — birth decides ownership.

- **Define** a team in the Teams window (`Alt+Shift+G`, or its button in the
  top bar): the repository, the team's guidance and permission mode, and per
  seat its CLI, model and thinking level, a worktree or the checkout, its own
  guidance, and optionally a blank home of its own under `~/.mogeung`. Seats
  start in auto mode unless the team says otherwise.
- **Boot** starts every seat that is not running, each a real CLI in tmux,
  linked into one session `geung-<team>` you can attach to from any terminal.
  A restarted daemon re-adopts its seats; a seat that exited says so.
- **The board** (`Alt+G`) is a pane of the main tile tree: OpenRig's five
  lanes — *needs you · parked with baton · blocked on agents · now working ·
  just finished* — the topology as a graph and a table together, and the
  policy strip with the counters. A seat opens as a card under its team, and
  a terminal per seat attaches beside it.
- **Seats talk** with `mogeung-send <seat> "…"`, on every seat's `PATH`. The
  daemon judges each message against the team's policy and types it into the
  teammate's pane, once, as a person would. A chain may travel **three hops**
  with no human between before the next delivery is **held** and ranks on the
  queue as needing you; a team may make **sixty** automatic sends an hour; one
  **pause** switch holds them all; typing in a pane always wins. Wakes and
  watchdogs are declared text in the policy, readable before they fire. A seat
  talks only to its own team.
- **Handovers** on a seat's start or restart are composed by your configured
  model (`model_url` in `config.toml`) — what the team is doing, what the
  others did, what this seat picks up first — and held for you to read, or
  delivered as a hop, as the policy says. Without a model the seat starts cold
  and the log says so.
- **The screen is read** — a seat's pane, and no other
  ([ADR-0044](docs/decisions/0044-mogeung-reads-a-seats-pane-and-no-other.md)).
  A permission menu, a plan approval, a question, a trust dialog, a login or a
  rate-limit banner ranks the seat with *(screen)* in its detail. mogeung
  answers none of them; the one press it may make is Enter, for the one shape
  that asks for it, and that is off by default.
- **Edit in a seat's worktree** from the file pane — the one place the viewer
  is an editor — with a save that refuses when the file moved under you or the
  seat is mid-turn.
- **Wind down** types the policy's wind-down message into every running seat;
  each writes its last words with `mogeung-note`, then stops, and the next
  boot's handover quotes them. *Stop now* cuts it short; *Delete* removes a
  team whole once nothing runs.
- **Import and export**: a team out as JSON, a file in as a draft — mogeung's
  own or an OpenRig `rig.yaml` — and the team file's JSON Schema to download
  from the Teams window, derived from the type it is written with so it cannot
  drift.

What a seat needs beyond the daemon: `tmux`, `curl` (so its hooks and
`mogeung-send` reach the daemon), a git repository to work in, and its CLI on
`PATH`. **Nothing is written under your agent homes** — a seat is configured
through its CLI's own flags at launch
([ADR-0043](docs/decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)),
so the `claude` you type in a terminal behaves exactly as it would on a machine
that has never heard of mogeung.

→ [Gau Geung, the team mode](docs/guide/gau-geung.md) ·
[Feature 0051](docs/features/0051-gau-geung.md) ·
[Pillar P of the roadmap](docs/product/roadmap.md)

## Away from the window

```sh
mogeungd --notify                                   # desktop banners, macOS and Linux
mogeungd --notify --push-url https://ntfy.sh/your-topic
```

Notifications fire on the *transition* into needing you, once — never on a state
that is merely continuing. The tray shows the waiting count with no window open,
and a daemon on another machine can be followed into this window's queue.

## Documentation

| | |
|---|---|
| [docs/README.md](docs/README.md) | How the doc system works — **read before writing docs** |
| [Concept](docs/product/concept.md) | The thesis |
| [Assumptions](docs/product/assumptions.md) | What we believe and have not checked |
| [Roadmap](docs/product/roadmap.md) | The ledger: every item, its status and what is next |
| [Decisions](docs/decisions/) | ADRs — why things are the way they are |
| [Design](docs/design/) | How it works today |
| [Guide](docs/guide/) | User documentation |
| [Features](docs/features/) | Work in flight; verified features are under [docs/archive/features](docs/archive/features/) |
| [STATUS.md](STATUS.md) | Generated from git, tests and specs |

The changelog stopped on 2026-08-05 and is archived at
[docs/archive/2026-08-05-changelog.md](docs/archive/2026-08-05-changelog.md);
the roadmap's rows are the record since.

## Requirements

Rust 1.85+, `git`, `tmux`, and at least one of Claude Code, Codex, Qwen Code or
opencode on `PATH`. On Linux, `mold` as well (`sudo apt install mold`): the
repository's `.cargo/config.toml` links with it, and a build without it stops at
the first link with *cannot find 'ld'*, which looks like a broken toolchain and
is not. A team's seats want `curl` too. macOS and Linux; Windows is not
supported and is not planned.

The caveat worth knowing up front: everything rests on **undocumented** file
formats that a CLI update can change, and going quietly blind is the failure
mode the health panel exists to make loud.

On exposure — the daemon binds loopback and needs no token there. Beyond
loopback it **refuses to start without a shared secret**, and there is no
`--insecure`; the safest route stays an ssh tunnel.
→ [Watching a remote machine](docs/guide/remote.md)

## Develop

```sh
./scripts/start.sh          # build + run both; --fresh for a throwaway db
mprocs                      # both side by side, plus test/docs on a keypress

cargo test --workspace      # all free — nothing spawns an agent
cd desktop && npm test && npm run check   # the window's own suite, and tsc
./scripts/check-docs.sh     # frontmatter, staleness, orphans
./scripts/gen-status.sh     # rewrite STATUS.md
```

[AGENTS.md](AGENTS.md) is the entry point for agents working on this repo.
