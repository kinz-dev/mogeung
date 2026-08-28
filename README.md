<img src="assets/mogeung.png" alt="mogeung" width="200" align="right">

# mogeung

A supervision layer over the Claude Code sessions you already run.

**It watches. It never starts, steers or stops an agent.** You keep running
`claude` in as many terminal tabs as you like — full interactive loop, plan
mode, slash commands, permission prompts, untouched. mogeung reads the files
Claude Code already writes and adds the layer that doesn't exist:

- **one queue across every session**, ranked by who needs you
- **a diff per session**, risk-ordered, with per-hunk read marks that survive
  the agent rewriting the file
- **a health panel that says what it cannot see**, because the formats it reads
  are undocumented and going quietly blind must not look like a quiet day
- **a collision warning** when two live sessions edit the same file — the one
  thing no single agent can know about itself

Because it observes rather than wraps, it cannot make any individual session
worse. That was the fatal flaw of the first version — see
[ADR-0003](docs/decisions/0003-observe-do-not-spawn.md).

**Status: v0.2.** Working and tested; the core premise is still unproven. See
[docs/product/assumptions.md](docs/product/assumptions.md).

## Install it

```sh
./scripts/install.sh
```

Builds the release daemon and the window, installs `mogeungd`, `yolomo`,
`yolomop` and `qwenmo` (the tmux launchers for `claude`, `llmproxy --claude`
and `qwen`) into
`~/.local/bin`, and installs the window's `.deb` with `dpkg -i` — after which
**mogeung is an application in the launcher** like any other. It asks for `sudo`
once, at the dpkg step and nowhere else. `--no-desktop` stops after the daemon;
`--no-build` installs what is already built.

To build without installing anything:

```sh
cd desktop && npm install && npm run tauri build
# → desktop/src-tauri/target/release/mogeung-desktop
#   desktop/src-tauri/target/release/bundle/{deb,rpm,appimage}/
```

One executable. It starts a daemon if none is watching, attaches to one if there
is, and a daemon it started stops when you close the window.

For a daemon that outlives every window — so notifications keep firing while
nothing is on screen — run it separately:

```sh
cargo build --release
./target/release/mogeungd --notify   # keeps watching with no window open
```

Open the window as usual and it attaches to that daemon instead of hosting one.
See [ADR-0009](docs/decisions/0009-the-window-may-host-a-daemon.md).

Nothing to configure, no repos to register. If you have run `claude` anywhere in
the last 14 days, it appears. Nothing is ever written to `~/.claude`.

→ [Getting started](docs/guide/getting-started.md)

### Two builds, on purpose

`cargo build --release` at the repo root builds the daemon and **not** the
window: `desktop/src-tauri` is deliberately its own cargo workspace, so a
machine without node or webkit's headers can still build, test and run the part
that does the watching.

The window was a second client for a day — React and Monaco beside the original
egui one, both against one unchanged daemon
([ADR-0018](docs/decisions/0018-a-second-client-in-typescript.md)) — and on
2026-08-05 it became the only one
([ADR-0020](docs/decisions/0020-the-egui-client-is-retired.md)). The daemon
never knew there were two, which is the claim "every UI is a client" was making
all along.

→ [Building it, and what it needs installed](desktop/README.md#building-a-binary-you-can-hand-to-someone)

## The queue

```
APPROVE  → blocked on a permission prompt it needs you to answer
WAITING  → alive and idle: it is waiting for you to type
FAILED   → hit an API error
REVIEW   → exited, and left changes nobody has read
STALLED  → alive and busy, but silent past a threshold
running  → working normally
```

`WAITING` is not a heuristic — Claude Code publishes `busy`/`idle` in its own
live registry, so mogeung is told, not guessing. `APPROVE` splits that: an
unanswered tool call means the agent is *blocked*, not merely finished.

`j`/`k` to move, `enter` to jump to that session's terminal, `r` to mark read,
`s` to snooze, `/` to filter. `Alt+1` focuses the queue itself; `Alt+2`–`Alt+5`
and `Alt+9` open the dock along the bottom — Changes, Transcript, Insight, Debt,
Git, left to right — and the same chord closes what it opened. Numbers rather
than initials because `Alt+T` is Claude Code's own *toggle thinking*, and a
window that keeps it is a window that steals a key from the agent it is showing
you. Every binding is editable (`Alt+K`). **`Ctrl+Cmd+M` brings mogeung back**
from anywhere, which is the other half of that round trip.

→ [The attention queue](docs/guide/the-queue.md) ·
[Reviewing changes](docs/guide/reviewing.md) ·
[Away from the desk](docs/guide/away-from-the-desk.md) ·
[Watching a remote machine](docs/guide/remote.md) ·
[Troubleshooting](docs/guide/troubleshooting.md)

## Away from the window

```sh
./target/release/mogeungd --notify                  # macOS banners
./target/release/mogeungd --push-url https://ntfy.sh/your-topic
```

Notifications fire on the *transition* into needing you, once — never on a state
that is merely continuing.

## Documentation

| | |
|---|---|
| [docs/README.md](docs/README.md) | How the doc system works — **read before writing docs** |
| [Concept](docs/product/concept.md) | The thesis |
| [Assumptions](docs/product/assumptions.md) | What we believe and have not checked |
| [Roadmap](docs/product/roadmap.md) | The ranked backlog |
| [Decisions](docs/decisions/) | ADRs — why things are the way they are |
| [Design](docs/design/) | How it works today |
| [Guide](docs/guide/) | User documentation |
| [STATUS.md](STATUS.md) | Generated from git, tests and specs |

## Requirements

Rust 1.85+, `git`, Claude Code 2.1.x on `PATH`. macOS and Linux; Windows is not
supported and is not planned.

The caveat worth knowing up front: everything rests on **undocumented** Claude
Code file formats that an update can change, and going quietly blind is the
failure mode the health panel exists to make loud.

On exposure — the daemon binds loopback and needs no token there, because
anyone who can reach `127.0.0.1` can already read `~/.claude` directly. Beyond
loopback it **refuses to start without a shared secret**, and there is no
`--insecure` to talk it out of that: the port serves every transcript on the
machine and can open terminals on it. The safest route stays an ssh tunnel, which
needs no token at all.

→ [Watching a remote machine](docs/guide/remote.md)

## Develop

```sh
./scripts/start.sh          # build + run both; --fresh for a throwaway db
mprocs                      # both side by side, plus test/docs on a keypress

cargo test --workspace      # 310 tests, all free — nothing spawns an agent
cd desktop && npm test      # 149 more, the window's own
./scripts/check-docs.sh     # frontmatter, staleness, orphans
./scripts/gen-status.sh     # rewrite STATUS.md
```

[AGENTS.md](AGENTS.md) is the entry point for agents working on this repo.
