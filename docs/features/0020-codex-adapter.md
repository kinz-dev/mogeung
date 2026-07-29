---
title: Codex adapter
status: in-progress
updated: 2026-07-29
roadmap: [R-I1]
depends_on: [A4, A23]
---

# 0020 — Codex adapter

R-I1, built at the 2026-07-29 one-go ask. The roadmap's own framing:
this item exists to test whether the Session model generalises (A23).

**Honesty note.** The local `~/.codex` (CLI 0.145.0) is a fresh
install: the session index schema is real and was read from disk
(`state_5.sqlite`, `threads` table, 42 migrations), but it holds zero
sessions and no rollout files exist anywhere on this machine. The
adapter is therefore built against the real index schema plus synthetic
rollout fixtures derived from the binary's own persistence taxonomy —
and stays ⏳ with an explicit *unverified against real Codex use* caveat
until real sessions exist. That is one notch better evidenced than
R-I2 (no `~/.gemini` at all, descoped), and the spec says so rather
than blurring the two.

## Spec

### Problem

A Codex session running beside Claude Code sessions is invisible to the
queue. Either the Session model absorbs a second agent CLI or the
product is Claude-specific and should say so.

### Acceptance

- [x] With Codex threads present, sessions appear in the queue with
      cwd/repo, title, live status, and token totals, marked with their
      source *(scan-loop integration landed: `state::scan_codex`, pinned
      by `tests/codex_scan.rs` — a trailing approval request reaches the
      APPROVE tier)*
- [x] The index is read via `state_*.sqlite` globbed by version —
      never a hardcoded `state_5` — read-only, WAL-safe, and
      `rollout_path` is treated as possibly stale (Codex itself has
      read-repair for it)
- [x] Rollout parsing handles `.jsonl` and `.jsonl.zst`, both
      `sessions/` and `archived_sessions/` roots, and degrades on
      unknown line kinds exactly like the Claude parser — classified,
      counted, never panicking; the canary covers Codex too
      *(module-level: counts are produced and named; feeding them into
      `Health` awaits integration)*
- [x] Waiting-vs-working derives from the rollout tail
      (`turn_started` / `turn_complete` / trailing approval request)
      and is labelled a heuristic in the health panel
- [x] On this machine (schema real, zero sessions) mogeung reports the
      empty Codex install truthfully — present, watched, no sessions —
      rather than nothing at all (pinned by `tests/codex_scan.rs`)

### Explicitly out of scope

- Gemini CLI (R-I2 — no local data at all; descoped with a roadmap
  note, not built blind).
- Writing anything under `~/.codex`, including WAL/SHM side effects —
  reads use SQLite read-only mode.

## Plan

### Approach

Daemon `codex.rs`: discovery (`~/.codex` present?), index read via
rusqlite `mode=ro` with a column-tolerant row mapper (`#[serde(
default)]` discipline applied to SQL — select by name, tolerate
absence), rollout tailer reusing the `Tailer` byte-offset shape with a
zstd decode path, line classifier mirroring `adapter.rs`'s
`LineOutcome` so Codex drift is as loud as Claude drift. Map into
`Session` with a `source` field (serde-defaulted for old rows). Scan
loop grows a Codex pass behind the same cadence.

### Risks and unknowns

- 42 migrations in a young tool: columns will move. Select-by-name and
  per-row degrade are the whole defence.
- Every rollout-shape claim is inference from binary strings; the
  fixtures encode today's best understanding and the canary reports
  what reality disagrees with.

### Test strategy

Fixture SQLite DBs (built in tempdirs at both the current schema and a
column-poorer older shape); synthetic rollout fixtures incl. a `.zst`
one and an unknown-kind line asserting canary behaviour; e2e: empty
install reports honestly.

## Notes

**2026-07-29 — R-I1 core module built, standalone.** `crates/mogeungd/src/codex.rs`
plus `crates/mogeungd/tests/codex.rs`; no daemon file was touched beyond
`lib.rs` registration and two Cargo deps (`ruzstd` runtime decode, `zstd`
dev-only for building compressed fixtures). Integration into the scan
loop / `Session` / `Health` is deliberately not here — the public surface
for it is `CodexInstall::{discover, state_db, list_threads,
resolve_rollout, read_thread_rollout}`, `read_rollout`, `RolloutTailer`,
`parse_rollout_line` and the pure `derive_status`.

What is evidence and what is inference:

- **Evidence.** The `threads` schema, the `state_<N>.sqlite` naming, WAL
  in active use, and the empty-install condition were all read from the
  real `~/.codex` (CLI 0.145.0). The reader opens
  `SQLITE_OPEN_READ_ONLY`, selects `*` and maps columns by name; absent
  columns degrade per-field and are reported in
  `ThreadIndex::missing_columns`. `*_ms` stamps win over unix-seconds
  twins. An unreadable database is an empty index with `error` set.
- **Inference.** Every rollout line shape. The six top-level kinds and
  the item taxonomy under `response_item`/`event_msg` come from the
  binary's persistence strings, not from observed files — zero rollouts
  exist on this machine. Accordingly the classifier treats unknown
  outer kinds *and* unknown nested item kinds as counted
  `Unknown` outcomes (nested ones named `outer/inner`), and the tests
  lean on the degradation paths. Token fields use Codex's names
  (`cached_input_tokens`, `reasoning_output_tokens`); a test pins that
  Claude's `cache_read_input_tokens` is *not* picked up. Whether usage
  snapshots are per-turn or cumulative is unverified, so the rollout
  keeps only `last_usage` and callers are pointed at the index's
  `tokens_used`.

Status heuristic (pure, tested, and to be labelled a heuristic in any
UI): walking the tail backwards, an approval request seen before any
turn boundary → waiting; `turn_started` or a bare user message →
working; `turn_complete`/`turn_aborted` (or nothing at all) → done;
mid-turn chatter is skipped so answered approvals do not read as
waiting.

One bug the fixture tests caught before any real data could: a
payloadless `response_item` envelope initially fell back to reading its
own `type` as the item kind, manufacturing fake nested drift
(`response_item/response_item`). The fallback is now restricted to
`session_meta`/`turn_context`, where it cannot be confused.

Still ⏳ overall: unverified against real Codex use, by construction.
The first real session on this machine is the actual test.