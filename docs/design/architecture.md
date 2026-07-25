---
title: Architecture
status: active
updated: 2026-07-25
covers:
  - crates/mogeungd/src/main.rs
  - crates/mogeungd/src/state.rs
  - crates/mogeung-ui/src/main.rs
  - crates/mogeung-ui/src/net.rs
---

# Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Clients                                                  │
│  native egui app · (future) thin web client · curl        │
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
window open, reach from another device is free, and a native shell or web client
later is a packaging decision rather than a rewrite.

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
   the most important signal we have.
5. Recompute diffs for sessions that changed, and for any that just exited.
6. Rank and broadcast the queue, then broadcast health.

Polling rather than filesystem events: a few dozen files every 1.5 s costs
nothing, and it avoids every rename and atomic-write edge case that makes
FSEvents miserable.

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
