---
title: Data model
status: active
updated: 2026-07-29
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


## Fields added after v0.2

Every one carries `#[serde(default)]`. The store keeps whole sessions as JSON
blobs, so a row written by an older build must still load rather than being
dropped as unreadable — and `load_sessions` only warns on a bad row, which means
a missing default would silently lose sessions rather than fail loudly.

| Field | For |
|---|---|
| `open_tools: Vec<OpenTool>` | Permission-prompt detection (`R-B4`) |
| `snoozed_until: Option<DateTime>` | Snooze (`R-B5`) |
| `collisions: Vec<Collision>` | Cross-session file overlap (`R-B3`) |
| `loop_signal: Option<String>` | Thrashing advisory (`R-B7`) |
| `recent_touches: Vec<Touch>` | Timestamped edits, capped at 200, feeding collisions |
| `recent_tools: Vec<String>` | Last 12 `tool:target` keys, feeding loop detection |
| `tmux_target: Option<String>` | The pane this session can be attached in (`R-B18`) |

`recent_touches` exists because `touched_files` is cumulative: "we both edited
this file at some point today" is not a collision, and answering the question
needs *when*, not just *what*.

`tmux_target` is derived, not stored in any meaningful sense — it is re-resolved
from the OS on every scan alongside `alive`/`pid`, and cleared when a session
dies. A stale one would offer a terminal tab that attaches to nothing. It lives
on `Session` rather than behind its own endpoint so a client can decide whether
to *offer* the tab without asking a second question
([ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md)).

## Session fields added 2026-07-29

All `#[serde(default)]`, so rows persisted by older builds still load:

- `limit_hit_at` / `limit_resets` — the five-hour limit, recognised from
  the CLI's synthetic assistant message; cleared by the next human turn
  or real assistant output (`R-G1`).
- `verify_runs` — build/test/typecheck-shaped commands with outcomes,
  paired to their `tool_result` by id; `claims` — "tests pass"-shaped
  prose bound to that evidence, `contradicted` when the run said
  otherwise (`R-E1`/`R-E3`). Both capped.
- `source` — which CLI wrote the session (`claude_code` default,
  `codex`); the Codex scan maps `~/.codex`'s thread index into this same
  struct, which is A23's test (`R-I1`).

The store also grew a `signals` table (per-repo signal command and its
last run, `R-E2`) — a real table rather than a blob because it is keyed
by repo, not session.
