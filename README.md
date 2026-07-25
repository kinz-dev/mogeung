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

Because it observes rather than wraps, it cannot make any individual session
worse. That was the fatal flaw of the first version — see
[ADR-0003](docs/decisions/0003-observe-do-not-spawn.md).

**Status: v0.2.** Working and tested; the core premise is still unproven. See
[docs/product/assumptions.md](docs/product/assumptions.md).

## Run it

```sh
cargo build --release

./target/release/mogeungd    # terminal 1 — the daemon
./target/release/mogeung     # terminal 2 — the window
```

Nothing to configure, no repos to register. If you have run `claude` anywhere in
the last 14 days, it appears. Nothing is ever written to `~/.claude`.

→ [Getting started](docs/guide/getting-started.md)

## The queue

```
WAITING  → alive and idle: it is waiting for you to type
FAILED   → hit an API error
REVIEW   → exited, and left changes nobody has read
STALLED  → alive and busy, but silent past a threshold
running  → working normally
```

`WAITING` is not a heuristic — Claude Code publishes `busy`/`idle` in its own
live registry, so mogeung is told, not guessing.

→ [The attention queue](docs/guide/the-queue.md) ·
[Reviewing changes](docs/guide/reviewing.md) ·
[Troubleshooting](docs/guide/troubleshooting.md)

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

Rust 1.85+, `git`, Claude Code 2.1.x on `PATH`, macOS.

Two caveats worth knowing up front: everything rests on **undocumented** Claude
Code file formats that an update can change, and the daemon has **no
authentication** — it binds localhost, and anyone who can reach the port can
read your transcripts. Do not expose it.

## Develop

```sh
cargo test --workspace      # 63 tests, all free — nothing spawns an agent
./scripts/check-docs.sh     # frontmatter, staleness, orphans
./scripts/gen-status.sh     # rewrite STATUS.md
```

[AGENTS.md](AGENTS.md) is the entry point for agents working on this repo.
