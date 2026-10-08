<img src="assets/mogeung.png" alt="mogeung" width="200" align="right">

# mogeung

A supervision layer over the agent sessions you already run.

**It watches. It never starts, steers or stops an agent.** You keep running
`claude`, `codex`, `qwen` or `opencode` in as many terminal tabs as you like —
full interactive loop, plan mode, slash commands, permission prompts, untouched.
mogeung reads the files each CLI already writes for itself and adds the layer
that doesn't exist:

- **one queue across every session**, ranked by who needs you — across every
  machine you follow, not only this one
- **a diff per session**, risk-ordered, with per-hunk read marks that survive
  the agent rewriting the file
- **a health panel that says what it cannot see**, because the formats it reads
  are undocumented and going quietly blind must not look like a quiet day
- **a collision warning** when two live sessions edit the same file — the one
  thing no single agent can know about itself

Because it observes rather than wraps, it cannot make any individual session
worse. That was the fatal flaw of the first version — see
[ADR-0003](docs/decisions/0003-observe-do-not-spawn.md), and its 2026-08-29
amendment for the one thing it may now do on your behalf: press send, once, on
a click and a confirmation.

**Status: in daily use.** The premise — that a cross-session queue changes where
you look, and that you run several agents at once — was supported by a week of
use on 2026-08-04 (A1 and A6 in
[the assumption ledger](docs/product/assumptions.md)); whether the *ranking* is
right is still unmeasured. [The roadmap](docs/product/roadmap.md) is the ledger
of what is built, what is awaiting a verdict and what is next.

## Install it

```sh
./scripts/install.sh
```

Builds the release daemon and the window, installs `mogeungd` and the five
tmux launchers — `yolomo` (claude), `yolomop` (claude through `llmproxy`),
`codexmo`, `qwenmo` and `opencodemo` — into `~/.local/bin`, and installs the
window as an application in the launcher (a `.deb` through `dpkg` on Linux,
which asks for `sudo` at that step and no other; an `.app` on macOS).
`install.sh -h` lists `--no-desktop`, `--no-build` and the rest. An installed
window then updates itself from the checkout when a new commit lands on `main`
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
14 days appears. Nothing is ever written to `~/.claude`.

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

`↑`/`↓` move through the queue, `Alt+1` puts the keyboard in it, and
`Alt+2`–`Alt+5`, `Alt+8` and `Alt+9` open the dock along the bottom — Changes,
Transcript, Run, Debt, Insight, Git — with the same chord closing what it
opened. Numbers rather than initials because `Alt+T` is Claude Code's own
*toggle thinking*, and a window that keeps it is a window that steals a key from
the agent it is showing you. Every binding is editable (`Alt+K`).
**`Ctrl+Cmd+M` brings mogeung back** from anywhere, which is the other half of
that round trip.

→ [The attention queue](docs/guide/the-queue.md) ·
[Reviewing changes](docs/guide/reviewing.md) ·
[The Git tool window](docs/guide/git.md) ·
[Away from the desk](docs/guide/away-from-the-desk.md) ·
[Watching a remote machine](docs/guide/remote.md) ·
[Troubleshooting](docs/guide/troubleshooting.md)

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
opencode on `PATH`. macOS and Linux; Windows is not supported and is not
planned.

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
