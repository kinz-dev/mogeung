---
title: Data model
status: active
updated: 2026-07-25
covers:
  - crates/mogeung-core/src/session.rs
  - crates/mogeung-core/src/change.rs
  - crates/mogeung-core/src/transcript.rs
  - crates/mogeungd/src/store.rs
---

# Data model

Deliberately small. Everything in the UI is a view over these.

| Type | What it is |
|---|---|
| `Session` | An observed Claude Code session. Identified by **Claude Code's own** session id — mogeung mints no identifiers |
| `LiveStatus` | `Busy` / `Idle` / `Unknown`, from the live registry |
| `TranscriptEvent` | One typed event with a per-session monotonic `seq` |
| `Change` | A session's diff: ordered `FileChange`s |
| `FileChange` | Path, status, stats, risk flags and score, `Hunk`s |
| `Hunk` | Content anchor, header, lines, flags, score, `reviewed` |
| `AttentionItem` | A session's queue position with reason, score, and explanation |

## Session identity

Sessions are keyed by Claude Code's uuid. mogeung generating its own id would
mean maintaining a mapping for no benefit — the session belongs to the CLI, not
to us.

`Session::label()` falls back in descending usefulness: `ai-title` → last prompt
→ registry name → short id.

## Persistence

SQLite at `~/.mogeung/mogeung.db`, three tables:

- `sessions(id, created_at, json)` — the whole struct as a JSON blob
- `events(session_id, seq, json)`
- `reviewed(session_id, anchor)` — which hunks you have read

Blob storage is a deliberate v0.x choice: the schema is still moving, and being
able to change `Session` without a migration matters more than query planning at
this scale. Rows that fail to deserialise are skipped with a warning rather than
refusing to start.

**`alive`, `live_status` and `pid` are never trusted from storage.** They are
re-derived from the OS on the first scan after startup.

## Ephemeral state

`Change` is recomputed from git on demand and cached in memory only — never
persisted. Only the `reviewed` anchors need to survive a restart, and they are
what make checkpointing durable.
