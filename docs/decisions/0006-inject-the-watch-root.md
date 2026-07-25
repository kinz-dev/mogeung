---
title: Inject the watch root rather than reading the environment
status: active
updated: 2026-07-25
decided: 2026-07-25
---

# ADR-0006 — Inject the watch root rather than reading the environment

## Context

`watcher::claude_home()` resolved `~/.claude` by reading `CLAUDE_CONFIG_DIR`
deep in the call stack. Tests pointed the watcher at synthetic homes by setting
that variable.

A test then failed **only when run in parallel**: five tests racing on one
process-global.

## Decision

`AppState::with_home(store, path)` takes the watch root explicitly.
`watcher::default_home()` resolves the default once, at startup, in `main`.

## Rationale

Serialising the tests would have hidden the smell rather than removed it.
Reading process-global configuration from deep inside a call stack makes
behaviour untestable and non-obvious; resolving it once at the edge does not.

## Consequences

- Tests run in parallel against synthetic homes and never touch real session
  data.
- A future daemon could watch several roots, or a remote one (roadmap `I4`).
- `CLAUDE_CONFIG_DIR` is still honoured, but only at startup.
