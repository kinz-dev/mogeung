---
title: The attention queue
status: active
updated: 2026-07-25
---

# The attention queue

The left panel ranks every session by who needs you. The badge is the category;
the dim line beneath it is the evidence.

| Badge | Meaning | Detail shows |
|---|---|---|
| `WAITING` | Alive and idle — it has finished its turn and wants you to type | `waiting for you — 4m12s` |
| `FAILED` | An API error was recorded | the error, e.g. `server_error` |
| `REVIEW` | Exited, changed files, not all read | `3 file(s), +47 -12 unread` |
| `STALLED` | Alive and busy, silent 5+ minutes | `busy but silent for 8m30s` |
| `running` | Alive and busy, producing output | its current tool call |
| `idle` | Nothing to do | `reviewed` / `ended with no changes` |

Uppercase wants a human. Lowercase is informational. **show quiet** controls
whether `idle` sessions are listed at all.

## Ordering is strict

Scores are 1000 / 900 / 800 / 700 / 100 / 0, and the tiebreaker (longest wait
first) is capped at 99. A session can never jump into a more urgent tier, so a
brand-new `FAILED` always outranks a three-day-old `REVIEW`.

## `WAITING` is a fact

Claude Code publishes `busy`/`idle` in its own live registry. mogeung is not
guessing.

`FAILED` is checked before liveness, so a live session that hit an API error
still shows failed. It clears when you send a new prompt.

## The small badge

`live·busy`, `live·idle` or `live` is raw liveness from the registry,
independent of the ranking. Ended sessions show none.

## Top bar

`N waiting for you` (red) · `N need you` (amber, all uppercase categories) ·
`queue clear`.

## Tuning

`stall_secs` defaults to 300 in `crates/mogeung-core/src/attention.rs`. Five
minutes is generous on purpose — a long build is normal silence, not a stuck
agent. Raise it if `STALLED` fires on healthy work.
