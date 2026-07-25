---
title: Attention ranking
status: active
updated: 2026-07-25
covers:
  - crates/mogeung-core/src/attention.rs
---

# Attention ranking

One queue across every session, answering "where do I look right now?".

## Reasons

Evaluated in this order — the first match wins.

| Reason | Badge | Score | Fires when |
|---|---|---|---|
| `AwaitingInput` | `WAITING` | 1000 | Alive and the registry reports `idle` |
| `Failed` | `FAILED` | 900 | An API error was recorded |
| `NeedsReview` | `REVIEW` | 800 | Exited, changed files, not all read |
| `Stalled` | `STALLED` | 700 | Alive and busy, silent ≥ `stall_secs` |
| `Running` | `running` | 100 | Alive and busy, recently active |
| `Idle` | `idle` | 0 | Reviewed, or exited with no changes |

**`Failed` is checked before liveness**, so a live session that hit an API error
still ranks as failed. The error clears when a new human turn appears — you
responded, so it no longer needs you.

## Scoring

`score = base + tiebreak`, where `tiebreak = (waited / 30).clamp(0, 99)`.

The gap between tiers is 100 and the tiebreak caps at 99, so **a session can
never be promoted past a more urgent tier** no matter how long it has waited. A
brand-new `FAILED` always outranks a three-day-old `REVIEW`. Pinned by a test.

`waited` is time-since-idle for `AwaitingInput`, and session duration otherwise.

## Why `WAITING` is not a heuristic

Claude Code publishes `status: idle` in its own live registry, so blockage on a
human is a **fact we are told**, not something inferred. In v0.1 this could only
be guessed after the fact from permission denials, and it was the largest
documented gap. See [ADR-0003](../decisions/0003-observe-do-not-spawn.md).

## Configuration

```rust
pub struct AttentionConfig {
    pub stall_secs: i64,            // 300
    pub review_needs_changes: bool, // true
}
```

Five minutes is deliberately generous: a long build or test run is normal
silence, not a stuck agent.

## Transparency

Every item carries a `detail` string stating why it ranks where it does
(`waiting for you — 4m12s`, `busy but silent for 8m30s`). The ranking must never
be a black box.
