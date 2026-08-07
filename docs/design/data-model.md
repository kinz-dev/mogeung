---
title: Data model
status: active
updated: 2026-08-07
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

SQLite at `~/.mogeung/mogeung.db`:

- `sessions(id, created_at, json)` — the whole struct as a JSON blob
- `events(session_id, seq, json)`
- `reviewed(session_id, anchor)` — which hunks you have read
- `signals(repo, command, last_run)` — the per-repo signal command (`R-E2`)
- `notes(id, body, created, updated, session_id, seq, repo)` — the user's own
  writing (`R-B35`)
- `tail_offsets(path, offset)` — how far each transcript has been read (`R-A6`)

**`notes` is the odd one, and deliberately.** Everything else here is derived:
lose it and a rescan of `~/.claude` and git rebuilds it. A note cannot be
recomputed from anything, which is why
[ADR-0015](../decisions/0015-markdown-is-the-truth.md) also requires a one-way
mirror to `~/.mogeung/notes/*.md` — the writing must not be reachable only
through a database that only mogeung can open. The mirror is never read back.

`session_id` and `seq` on a note are **tags, not a location**: together they
anchor it to one turn of one transcript, and `seq` works as an anchor because
it is persisted with the events and resumes from `max_seq` at startup. A note
outlives the session being forgotten, which is what tagging rather than nesting
buys.

Blob storage is a deliberate v0.x choice: the schema is still moving, and being
able to change `Session` without a migration matters more than query planning at
this scale. Rows that fail to deserialise are skipped with a warning rather than
refusing to start.

**`alive` and `live_status` are never trusted from storage.** Both are
re-derived from the OS on the first scan after startup.

**`pid` is kept, and is not a liveness claim.** A dead session holding its last
pid is the only evidence that `/clear` moved that pid to a fresh session id,
which is how a client knows the two are one conversation and carries a
hand-applied label across (`store/prefs.ts`). The scan re-derives the pid of
everything actually running, so a remembered pid only ever survives on a session
whose `alive` is false — and every consumer that needs a *live* pid gates on
`alive` first. Wiping it on load broke the label migration across a daemon
restart, the same way wiping it on death broke it inside one run.

**`started_at` is when the *process* started, not the conversation.** It begins
as the transcript file's mtime and is then overwritten every scan from the live
registry's `startedAt`, which Claude Code sets once per process — so a session
`/clear` minted an hour ago reports the moment its terminal was opened, and
every id on one pid reports the same instant. Read it as "how long has this
window been up"; `last_event_at` is the field that says when this conversation
was last doing anything, and it is the one that can order a `/clear` chain
(`R-J15`, and `architecture.md` on succession). Correcting it is a change to
what the Info pane's "started" and every duration derived from it mean, so it is
recorded here rather than quietly fixed.

### Read positions are part of the record (`R-A6`)

`tail_offsets` is the one table that is neither a fold nor the user's writing:
it says how much of each transcript has already *been* folded. It has to be
persisted for the same reason the sessions are, and by the same process — a
tailer that starts empty over a database that remembers the sessions re-reads
every transcript whole, appends the history again under fresh `seq`s, and folds
every counter a second time. That was the shape of the 2026-08-02 bug; a
restart replayed the entire conversation into the transcript pane and added a
copy of it to the database, once per restart.

Written **after** the lines it covers are folded in, so an interrupted pass
re-reads a batch rather than skipping it, and deleted with the session it
belongs to. The offset for a file that shrank is discarded on sight: a
transcript rewritten shorter is a different file and is read again from the
start.

### Repair

`PRAGMA user_version` tracks whether stored *values* need fixing — not the
schema, which is all `CREATE TABLE IF NOT EXISTS`. Version 1 is the repair for
the above: it drops each session's events, zeroes the counters the fold
produces, and re-reads the transcript from scratch under the same size cap a
first sighting gets. Derived state is rebuilt, never patched, and nothing
original is touched —
[ADR-0016](../decisions/0016-rebuild-derived-state.md) has the reasoning and the
alternatives, including why the counters cannot simply be divided down.

Two sessions cannot be rebuilt that way, and each is handled by saying so rather
than by guessing:

- **The transcript is gone.** Nothing remains to recount from, so its event log
  is deduplicated in place — `dedupe_events` keeps the earliest `seq` of each
  otherwise-identical event — and its counters are left as they are. The
  deduplication is a guess by construction: a transcript really can carry the
  same prompt twice on one timestamp, and this collapses that pair. It is the
  last resort, it repairs a log and can do nothing for a counter, and a record
  left visibly odd is better than one divided by an invented number.
- **It is a Codex thread.** Skipped entirely. Codex rollouts are re-read whole
  on every scan and never went through the offset path, so they were never
  duplicated; folding one through the Claude Code adapter would invent history
  rather than repair it.

The pass ends with `VACUUM` when it dropped anything. Deleting rows frees pages
inside the file and returns nothing to the disk, so a database bloated by this
bug would otherwise stay bloated after being fixed.

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
