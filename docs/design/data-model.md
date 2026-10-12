---
title: Data model
status: active
updated: 2026-10-12
covers:
  - crates/mogeung-core/src/session.rs
  - crates/mogeung-core/src/change.rs
  - crates/mogeung-core/src/transcript.rs
  - crates/mogeungd/src/store.rs
  - crates/mogeungd/src/notes.rs
  - crates/mogeung-core/src/team.rs
  - crates/mogeungd/src/team.rs
---

# Data model

Deliberately small. Everything in the UI is a view over these.

| Type | What it is |
|---|---|
| `Session` | An observed agent session from any of the four CLIs (`SessionSource`). Identified by **the CLI's own** session id — mogeung mints no identifiers |
| `LiveStatus` | `Busy` / `Idle` / `Unknown`, from the live registry |
| `TranscriptEvent` | One typed event with a per-session monotonic `seq` |
| `Change` | A session's diff: ordered `FileChange`s |
| `ChangeSummary` | A `Change` with the hunk bodies left out — per-file counts, paths, review tallies. What the scan loop broadcasts when a diff moves; the hunks travel per connection, on request |
| `FileChange` | Path, status, stats, risk flags and score, `Hunk`s |
| `Hunk` | Content anchor, header, lines, flags, score, `reviewed` |
| `AttentionItem` | A session's queue position with reason, score, a static `detail` and the `since` clock |

## Session identity

Sessions are keyed by the CLI's own id; mogeung generating its own would mean
maintaining a mapping for no benefit. A mirrored session from a followed
machine is `@<machine_id>:<id>`, and a tmux pane running an agent that has
written nothing is `pane:%N`; neither is ever written to the store.

`Session::label()` falls back in descending usefulness: `title` → last prompt
→ registry name → short id. `title` is the CLI's generated `ai-title` until
you `/rename` the session, and your name from then on (`custom_title`).

**A prompt is something you typed.** A slash command's printed output
(`<local-command-stdout>`) arrives as a `user` line with a plain-string
content, the same shape a human prompt has, so it used to label a session
*"Compacted (ctrl+o to see full summary)"*; those lines are barren now. The
command **echo** is still a turn and still becomes a label — that half really
is you — rewritten to the command you typed (`/clear`, `/add-dir …`) rather
than the markup around it (`R-J42`). A label stored before that fix is
repaired when the daemon loads it; the event log is left as written.

## Every field of a `Session`

Every field added after v0.2 carries `#[serde(default)]`, because the store
keeps whole sessions as JSON blobs and a row written by an older build must
still load — `load_sessions` only warns on a bad row, so a missing default
would silently lose sessions rather than fail loudly. **Persisted** says
whether the value survives a restart or is re-derived on the first scan.

| Field | For | Persisted |
|---|---|---|
| `id`, `cwd`, `transcript_path` | identity and where it lives; `transcript_path` is `<opencode.db>#<id>` for opencode, a locator that opens nothing | yes |
| `title`, `name`, `last_prompt` | the label chain above | yes |
| `custom_title` (`R-J120`) | the name you gave with `/rename`; pins `title` against the CLI's re-emitted `ai-title` | yes |
| `repo_root`, `git_branch` | git's resolved root; the branch from the repository's `HEAD` each pass (`R-J110`), overriding the transcript's stamp | yes; branch re-read |
| `pid`, `alive`, `live_status`, `status_since` | liveness from the OS and the registry | `pid` kept, the rest re-derived |
| `version` | the CLI version behind the newest line | yes |
| `started_at`, `last_event_at` | when the *process* started (the registry's `startedAt`, so every id on one pid reports the same instant); the last line this conversation wrote, which is what orders a `/clear` chain (`R-J15`) | yes |
| `turns`, `tool_calls`, `tokens_in`, `tokens_out`, `api_ms`, `tool_ms` (`R-J63`) | the counters and the two timings from `cost-state`; none is a ranking input | yes |
| `last_activity` | the current tool call, for the row | yes |
| `touched_files`, `base_sha`, `files_changed`, `insertions`, `deletions` | attribution and the diff's pinned base and tallies | yes |
| `error` | the API error that ranks `FAILED`; cleared by the next human turn | yes |
| `reviewed` | every hunk read | yes |
| `open_tools` (`R-B4`) | unmatched `tool_use`s — the permission-prompt signal | yes |
| `snoozed_until` (`R-B5`) | the snooze deadline | yes |
| `collisions` (`R-B3`), `loop_signal` (`R-B7`) | the cross-session signals, recomputed every scan | recomputed |
| `recent_touches`, `recent_tools` | timestamped edits capped at 200, the last 12 `tool:target` keys — the inputs to the two above | yes |
| `tmux_target` (`R-B18`) | the pane this session can be attached in; a stale one would offer a tab that attaches to nothing | re-derived |
| `limit_hit_at`, `limit_resets` (`R-G1`) | the rate limit, from the CLI's synthetic message; cleared by movement | yes |
| `verify_runs`, `claims` (`R-E1`, `R-E3`) | build/test/typecheck-shaped commands with outcomes; "tests pass"-shaped prose bound to them, `contradicted` when the run said otherwise; both capped | yes |
| `source` (`R-I1`, `R-I15`, `R-I25`) | which CLI wrote the session, `claude_code` default; asked through named methods (`in_claude_live_registry()`, `has_claude_event_history()`) rather than equality, so a fifth CLI is a compile error at every site that must think about it ([ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)) | yes |
| `announced_dirs` (`R-J39`) | folders the CLI confirmed with `/add-dir`, used only to *offer* a workspace folder | yes |
| `artifacts` (`R-J115`) | pages this session published on claude.ai, newest first; `url` worked out by the daemon from the UUID's base58 form | yes; skipped when empty |
| `provisional` (`R-J75`) | a pane row, rebuilt every scan | never |
| `origin` (`R-I21`) | the machine a mirrored session runs on: `machine_id`, `host`, `ssh_target`, `stale` | never |
| `seat` (`R-P4`, `R-P5`) | the seat this session is, when the daemon started it: `team_id`, `team`, `seat`, and `signals` — `hooked` when its hooks reach the daemon, `plain` when it is known from its transcript and screen only | yes; re-tagged every scan from the launch records |
| `held_send` (`R-P6`) | a send the team's policy is holding for this seat — how many, why the oldest, since when — which ranks the row as needing you; never set on an observed session, because only the policy engine writes it and it addresses launch records | no; derived from the send log every scan |
| `screen` (`R-P11`) | what this seat's pane showed at the last read: a `shape` from ADR-0044's closed set, the `excerpt` it was read from, `read_at`; a gate ranks the row with *(screen)*; never set on an observed session, because the capture takes a pane from a launch record and nowhere else; the capture itself is never kept | no; the scan's memory, re-read every rest |

**`alive` and `live_status` are never trusted from storage**; both are
re-derived from the OS on the first scan. **`pid` is kept and is not a
liveness claim**: a dead session holding its last pid is the only evidence
that `/clear` moved that pid to a fresh id, which is how the window carries a
label across; every consumer that needs a *live* pid gates on `alive` first.
**`started_at` is when the process started, not the conversation**; read it
as *how long has this window been up* and use `last_event_at` for ordering.
Absorbing the second, third and fourth CLI added no field here, which is most
of what [A23](../product/assumptions.md) was asking.

## Persistence

SQLite at `~/.mogeung/mogeung.db`:

- `sessions(id, created_at, json)` — the whole struct as a JSON blob
- `events(run_id, seq, json)` — the column is still called `run_id`, a v0.1
  name that survived; it holds the session id
- `reviewed(run_id, anchor)` — which hunks you have read; the same column name
- `signals(repo, command, last_run)` — the per-repo signal command (`R-E2`);
  a real table because it is keyed by repo, not session
- `notes(id, body, created, updated, session_id, seq, repo)` — the user's own
  writing (`R-B35`)
- `note_tasks(note_id, ord, text, done, depth, grp)` — a **cache** of where the
  `- [ ]` lines in each document are, how deeply each is nested, and the
  heading above it (`R-L3`, `R-L10`)
- `task_events(note_id, text, done, at)` — append-only, the history a checkbox
  cannot keep (`R-L3`)
- `teams(id, created, updated, json)` — a team of seats and its policy, the
  whole struct as a JSON blob with the seats inside it (`R-P3`, feature 0051);
  a blob for the reason sessions are one, so a seat gaining a field needs no
  migration — which is how the policy gained its wind-down message and the
  minutes a seat has to answer it (`R-P15`) with no change here. A wind-down
  in progress is ephemeral, held in memory until the next boot; a seat's last
  words land as `note.md` beside its files under `~/.mogeung`, not in a table
- `seat_launches(team_id, seat, json)` — which panes are seats: the launch
  record a restarted daemon re-adopts from (`R-P4`). Derived, not the user's
  writing, so not mirrored; a stop deletes the row. Since `R-P5` the blob
  carries the seat's receiver `token` — persisted so a re-adopted seat is
  still heard, stripped before the record is served — and its `signals`
  (kind, count, last event, context fill, model), which are live state
  rewritten in memory as hooks arrive and saved only as the record is; since
  `R-P8` `team_session`, the name of the team's own tmux session for a
  terminal
- `sends(team_id, id, at, json)` — a team's send log (`R-P6`): every
  automatic send and every one of yours into a seat, as a `SendEntry` blob —
  who, whom, the text, why, what became of it, the hop it took. **The
  counters are derived from this table**, never stored beside it: the chain
  is the run of hop deliveries since the last entry a human caused, the hour
  is the automatic deliveries newer than sixty minutes, so a restarted daemon
  counts what it counted before. Kept to the last two hundred per team plus
  every held entry, which is a question for you and is not pruned
- `chats(id, title, turns, n_turns, created, updated)` — the chat panel's
  conversations (`R-O9`)
- `tail_offsets(path, offset)` — how far each transcript has been read (`R-A6`)
- `semantic_index(id, session_id, line, role, ts, preview, vec)` and
  `semantic_meta(k, v)` — the **similar** list's embeddings and what built them
  (`R-O6`)

Blob storage is a deliberate v0.x choice: the schema is still moving, and
being able to change `Session` without a migration matters more than query
planning at this scale. Rows that fail to deserialise are skipped with a
warning rather than refusing to start.

**Three kinds of row, by what losing them costs.**

- **Derived** — `sessions`, `events`, `reviewed`, `tail_offsets`,
  `semantic_index`: lose them and a rescan of the CLIs' homes and git rebuilds
  them. `semantic_index` is the clearest case, a cache dropped and rewritten
  wholesale on a rebuild because a half-rebuilt index would mix two models in
  a space the cosine assumes is one; `vec` is little-endian `f32`s in a BLOB,
  since a thousand floats as JSON is ten times the bytes.
- **Yours** — `notes` and `teams`: neither can be recomputed from anything,
  which is why [ADR-0015](../decisions/0015-markdown-is-the-truth.md) requires
  a one-way mirror — `~/.mogeung/notes/*.md` for a note, `~/.mogeung/teams/*.md`
  for a team ([ADR-0043](../decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)
  clause 8) — never read back. A team's mirror says what each seat will be
  started with, the inherited mode marked as the team's; since `R-P12` an
  export may put the same definition beside it as `<id>-<slug>.json`, whole
  and also never read back on its own — importing a file is the explicit
  act, and it answers a draft rather than writing the store; and the daemon
  stamps `created` and `updated` and mints the id, whatever a client sent. `session_id` and `seq`
  on a note are tags, not a location, so a note outlives the session being
  forgotten. `note_tasks` and `task_events` are the opposite case on purpose:
  the first is a cache of what the documents already say and is dropped and
  recreated when its shape changes; the second is history and is never
  dropped. Delete both, restart, and `prepare` rebuilds every task from the
  markdown — the only casualty is *when* things were closed, which the ADR
  names as the acceptable loss, and a test asserts exactly that.
- **Convenience** — `chats`: a conversation accumulates simply by using the
  panel, so it is capped at 200, oldest *touched* first out, and not mirrored,
  because half of it is model output
  ([ADR-0032](../decisions/0032-the-chat-panel-remembers.md)). `turns` is one
  JSON array, read, written and sent whole; `n_turns` is stored rather than
  derived with `json_array_length`, which a test caught erroring the entire
  listing when one row's JSON is malformed.

**Rows are rent, and retention collects it** (`R-J57`). A session dead past
`RETENTION_DAYS` (30) is deleted — row, events, marks, tail offset — daily and
on the first scan after start, unless a note anchors to it or its transcript
file is still inside the scan's mtime window and would only be re-adopted.
The database opens with `journal_size_limit` set and the retention pass runs
`wal_checkpoint(TRUNCATE)`: without both, SQLite reuses the WAL but never
shrinks it, and the per-tick write pattern had left it at 80 % of the
database's own size.

### Read positions are part of the record (`R-A6`)

`tail_offsets` is the one table that is neither a fold nor the user's
writing: it says how much of each transcript has already *been* folded. A
tailer that starts empty over a database that remembers the sessions re-reads
every transcript whole, appends the history again under fresh `seq`s, and
folds every counter a second time — once per restart. Written **after** the
lines it covers are folded in, so an interrupted pass re-reads a batch rather
than skipping it; deleted with its session; discarded on sight for a file that
shrank, because a transcript rewritten shorter is a different file.

### Repair

`PRAGMA user_version` tracks whether stored *values* need fixing — not the
schema, which is all `CREATE TABLE IF NOT EXISTS`. Each pass checks its own
number, so a database already past it is not touched when the constant moves.

- **Version 1** repairs the above: drops each session's events, zeroes the
  counters the fold produces, and re-reads the transcript from scratch under
  the same size cap a first sighting gets. Derived state is rebuilt, never
  patched ([ADR-0016](../decisions/0016-rebuild-derived-state.md)). Two
  sessions cannot be rebuilt that way and are handled by saying so: one whose
  transcript is gone has its event log deduplicated in place (`dedupe_events`
  keeps the earliest `seq` of each otherwise-identical event — a guess by
  construction, since a transcript can carry the same prompt twice on one
  timestamp) and its counters left visibly odd rather than divided by an
  invented number; one that is not a Claude Code session is skipped, asked as
  `source.has_claude_event_history()`, because the other readers never went
  through this offset path. The pass ends with `VACUUM` when it dropped
  anything, since deleting rows returns nothing to the disk.
- **Version 2 is a backfill** (`R-J115`): `backfill_artifacts` reads each
  Claude Code transcript once, folds **only** `artifact-comment-monitor`
  lines, and moves no counter.
- **Version 3 is a backfill** (`R-J120`): `backfill_custom_titles` folds only
  `custom-title` lines, last one wins, and gives a session renamed before this
  build its name and its pin.

## Ephemeral state

`Change` is recomputed from git on demand and cached in memory only — never
persisted. Only the `reviewed` anchors need to survive a restart, and they are
what make checkpointing durable ([review-checkpointing.md](review-checkpointing.md)).
Mirrored sessions and provisional pane rows are rebuilt from the peer's
snapshot and from tmux respectively; nothing in SQLite refers to either.
