---
title: Performance findings (agent 2) — the scan loop and the wire
status: superseded
updated: 2026-08-26
---

# Performance findings, agent 2 — 2026-08-26

A source-level review of where the daemon and window still spend, taken from
the code at `ca05300` rather than from a running process. It complements
[`2026-08-26-finding-agent1.md`](2026-08-26-finding-agent1.md), which measured the live binary. The
two overlap on purpose: where they agree, confidence is high; where this one
adds items, they are the costs that a `/proc` snapshot does not surface — forks
the code *can* spawn, allocations the board makes, and a diff it computes and
then throws away.

Nothing here is built. This is evidence and a plan.

## What a tick does

`server::run` drives one task on a `tokio::time::interval` (default
`poll_ms = 1500`, floored at 250 in `server.rs`). Each fire calls
`AppState::scan` (`crates/mogeungd/src/state.rs:922`), which, in order:

1. `watcher::scan_live` — reads and JSON-parses every
   `~/.claude/sessions/*.json`, one `kill(pid, 0)` syscall each.
2. `watcher::scan_transcripts` — walks `projects/`, `stat`s every `.jsonl`,
   sorts them all by mtime.
3. Per transcript: `Tailer::read_new` (a second `stat`, then a read) and
   `apply_lines` (JSON-parse each new line, fold into the `Session`).
4. The liveness pass — when any Claude session is alive, fork `tmux
   list-panes` and `ps -axo` once on the blocking pool.
5. `maybe_recompute_change` for each touched session, gated by a 10 s
   fingerprint (`CHANGE_PROBE_SECS`).
6. `scan_codex`, `scan_qwen`, `refresh_collisions`, `flush_quiet_sessions`.
7. `publish_queue`, `publish_health`, `maybe_run_retention`.

Steps 3–6 already carry the `R-J53`–`R-J59` treatment: byte-offset tailing,
per-repo fingerprint dedup within a pass (`pass_fps`), quiet-coasting of
counter-only updates (`R-J54`), a health heartbeat gate (`R-J55`), a Codex
index-mtime gate (`R-J56`), and daily retention (`R-J57`). What remains is the
rest of the loop.

## Finding 1 — the queue gate is defeated by a live clock

**Largest, cheapest fix; confirmed by both this review and agent 1's
measurement.**

`publish_queue` ranks the whole board and then tries to stay silent:
`if *last == queue { return; }` (`state.rs:822`). But `AttentionItem` derives
`PartialEq` over *all* its fields including `detail`
(`crates/mogeung-core/src/attention.rs:84`), and `classify` bakes a ticking
clock into `detail` (`attention.rs:109`):

- `AwaitingInput` / `AwaitingPermission` → `fmt_dur(waited)`
- `Stalled` → `fmt_dur(silent)`
- snoozed → `fmt_dur(left)`

`fmt_dur` renders the seconds component, so while *any* session is waiting,
stalled, or snoozed, its row differs from the previous tick **by construction**,
and the `*last == queue` gate can never fire. The whole queue is re-broadcast to
every window every 1.5 s — agent 1 measured this at ~1.07 MB/min, the dominant
steady-state wire traffic. This is exactly the `R-J55` failure mode — a
by-construction-volatile field defeating a change gate — fixed for `Health`
(`health_equivalent`, `state.rs:4168`) and never applied to `Queue`.

The clock is also redundant: the window already renders its own elapsed time
from `session.last_event_at` / `status_since` it holds in the store
(`desktop/src/ui/QueuePanel.tsx:307`). The daemon ships a second clock beside
one the client computes for free.

**Fix.** Stop embedding elapsed time in `detail`. Either:

- **(a, preferred)** carry an anchor timestamp on `AttentionItem`
  (`waiting_since`, `#[serde(default)]`) and let the client render the
  countdown from its existing `fmtDur`/`secsSince`; keep `detail` for the
  genuinely-static text (`"22 file(s), +4110 -213 unread"`, the error string).
- **(b, minimal)** leave `detail` as-is on the wire and make the gate compare a
  stable projection `(session_id, reason, score)` instead of the whole item.

Both stop the per-tick broadcast; only the payload shape differs. Pin the
invariant with a test that would fail today: two `rank()` calls a minute apart
over an unchanged board must produce an equal queue.

## Finding 2 — `compute_change` diffs the whole worktree, then discards most of it

`recompute_change_inner` runs a full `git diff --unified=3 <base>` over the
entire worktree (`git.rs:979`, `compute_change_inner`), then — for an
attributed session — immediately `retain`s it down to `touched_files`
(`state.rs:~2260`). For a session that edited three files in a tree with three
hundred dirty files, ~99% of that diff work is thrown away. The same call also
reads up to `MAX_UNTRACKED = 200` untracked files **in full** (each up to
`MAX_FILE_BYTES = 512 KiB` → ~100 MB materialised per recompute) to synthesise
added-file diffs.

This is the biggest per-session spike in the loop, and it runs on every probe
where the fingerprint moved — for a busy agent, every 10 s.

**Fix.** When `touched_files` is non-empty, narrow the command itself:
`git diff --unified=3 <base> -- <touched…>` and
`git ls-files --others --exclude-standard -- <touched…>`, so git does only the
work the session actually owns. Fall back to the whole-worktree diff only when
there is no attribution (the "credit it with the whole worktree" case the
`R-J62` comment describes). Cache each untracked file's synthesised
`FileChange` by `(path, mtime)` so an unchanged untracked file is not
re-read and re-parsed on the next probe. Note `retain` is itself O(files ×
touched); pushing the filter into the pathspec removes that loop entirely.

## Finding 3 — `scan_qwen` has no idle gate; `scan_codex` does

`scan_codex` (`state.rs:1660`) records an index stamp and skips the whole read
when nothing is alive *and* the stamp is unchanged — the `R-J56` gate.
`scan_qwen` (`state.rs:1856`) has the same shape (a live registry plus a
transcript tree) but **no gate at all**: it early-returns only when `~/.qwen`
does not exist, and otherwise walks `projects/*/chats[/archive]`, `stat`s every
transcript, and `ScanCache::update`s each one, every tick — whether or not a
Qwen session has been alive for a fortnight.

**Fix.** Give Qwen the Codex treatment: stamp the mtime of the registry dir and
the transcript roots, keep a `qwen_seen` stamp, and skip the walk when nothing
Qwen-side is alive and the stamps are unchanged. A session that dies stops
writing, so "alive bit + directory mtime" is enough to know when a re-walk is
owed.

## Finding 4 — the live registry is re-read and re-parsed every tick, forever

`watcher::scan_live` does `read_to_string` + `serde_json::from_str` + a
`kill(pid, 0)` for *every* file in `~/.claude/sessions/` on every tick. The
module doc itself notes these files are not cleaned up on exit, so the set is
"every session that ever ran here" and the cost grows without bound while the
useful information (the handful of live pids) stays tiny.

**Fix.** Keep a `path → (mtime, parsed LiveEntry)` cache. Re-read and re-parse
only when a file's mtime changed; drop entries whose file vanished. The
`kill(0)` is one syscall and can stay per file; it is the JSON parse, not the
signal, that is the cost.

## Finding 5 — duplicate work, one pass apart

- **Two `tmux` + `ps` forks a tick when both CLIs are live.** The Claude
  liveness pass forks both when `!live_by_id.is_empty()`
  (`state.rs:~1040`); `scan_qwen` forks the identical pair again when its own
  live list is non-empty. On a machine running Claude *and* Qwen, that is two
  `tmux list-panes` and two `ps -axo pid=,ppid=` every 1.5 s. Compute
  `(panes, parents)` once per pass and pass it into both. (Agent 1 timed
  `ps -axo` at ~18.5 ms here — half of that pair, gone.)
- **Duplicate `stat` per transcript.** `scan_transcripts` already returns
  `size` from its own `metadata()`; `Tailer::read_new` stats the same path again
  to decide whether to open it. Thread the known size in; only `stat` when it
  has not been handed over.
- **`contains_key` under the read lock, inside the per-file loop.** The loop
  does `self.sessions.read().await.contains_key(&f.session_id)` once per file
  (`state.rs:922`). Take one read lock at the top of the pass, build a
  `HashSet` of known ids, and filter lock-free.
- **`labels` built before the gate.** `publish_queue` builds the full
  `id → label()` map — one clone + one `label()` call per session, every tick —
  *before* the `*last == queue` check, for a `notify_for` that in steady state
  notifies nothing. Build it lazily, only for ids that actually cross into the
  notification set.

## Finding 6 — per-event SQLite writes and per-fold session clones

- **One transaction per event.** `emit` (`state.rs:898`) calls
  `store.append_event` per event, each an autocommit `INSERT` under the store's
  single `std::sync::Mutex<Connection>`, alongside `M` `save_session` and `M`
  `save_tail_offset` writes per pass. A busy tick is `N + 2M` tiny
  transactions. Batch a session's events into one `rusqlite::Transaction` (and
  fold in its tail offset) so the lock is held once per session per pass.
- **Unbounded `touched_files`, O(n) per touch.** `apply_lines` does
  `if !s.touched_files.contains(&f) { s.touched_files.push(f) }` with no cap
  (its sibling `recent_touches` is capped at `MAX_RECENT_TOUCHES = 200`). A
  long session that edits many files grows a `Vec` that is rescanned linearly on
  every new touch (O(n²) over the session), and the whole vec is serialised into
  the session row on every `save_session` and into every `SessionUpdated` /
  `Snapshot` frame. Keep a `HashSet` shadow for the membership test, and cap the
  retained paths to the most recent N (attribution only needs recent paths; the
  rest fall back to the whole-worktree diff, which Finding 2 already narrows).
- **Redundant full-`Session` clones per fold.** `apply_lines` takes
  `before = s.clone()`; `put_or_coast` then runs a full structural `PartialEq`
  (`s != *before`) *and* `quiet_view(after)` (another full clone, then a masked
  compare); `put` clones once more for the insert. That is roughly three deep
  clones plus two deep compares per touched session per tick, and each clone
  copies every `touched_files`/`recent_touches`/`verify_runs` element. Compute
  the quiet view against a `&Session` (it only needs a masked borrow), compare
  once, and clone only the single copy that is actually stored and broadcast.
- **`Change` is cloned per probe and per request.** `recompute_change_inner`
  does `changes.insert(id, change.clone())`, and `change_for_request` does
  `.get(id).cloned()`. For a session with a large diff that is a full hunk-tree
  copy on every 10 s probe and every pane open. Consider holding `Change` behind
  a per-session lock or an `Arc` so the cache and the reply share one copy.

## Finding 7 — small, mechanical

- **Release profile is `opt-level = 2`** (`Cargo.toml`). The daemon is a
  long-lived hot loop, not a build-time cost; `opt-level = 3` (with
  `lto = "thin"` if the link time tolerates it) is a free steady-state win with
  no behaviour change.
- **`refresh_collisions` takes a write lock over the whole board every tick**
  (`state.rs:1209`), blocking API readers, and iterates all sessions even when
  nothing could have changed. Early-out when no live session has a touch inside
  `COLLISION_WINDOW_SECS` and no session currently carries a collision.

## Plan

Ordered by blast radius, not size — each phase ships and reverts on its own.

### Phase 1 — the clock and the fork (no protocol break)

1. Move the queue's clock out of the wire (`Finding 1`): `waiting_since` on
   `AttentionItem` with `#[serde(default)]`, or — minimal — gate on a stable
   `(id, reason, score)` projection. Test: two `rank()` calls a minute apart over
   an unchanged board compare equal.
2. Compute `(panes, parents)` once per pass and share it between the Claude
   liveness update and `scan_qwen` (`Finding 5`).

### Phase 2 — stop doing work the answer already implies

3. Narrow `compute_change` to the touched paths when attribution is present;
   cache untracked-file diffs by `(path, mtime)` (`Finding 2`).
4. Give `scan_qwen` the `R-J56` gate — stamp the registry and transcript
   directory mtimes, skip when unmoved and nothing Qwen-side is alive
   (`Finding 3`).
5. Cache `scan_live` by `(path, mtime)`; skip re-parse and re-`kill` on
   unchanged files (`Finding 4`).

### Phase 3 — mechanical per-tick waste

6. Thread the known `size` into `Tailer::read_new`; hoist the `contains_key`
   to one read lock per pass; build `notify` labels lazily
   (`Finding 5`).
7. Batch `emit`'s event inserts into one transaction; cap + `HashSet`-shadow
   `touched_files`; collapse the redundant `Session` clones in the fold
   (`Finding 6`).
8. Bump the release profile to `opt-level = 3` (`Finding 7`).

## Watch, do not act yet

- **Renderer memory growth.** Agent 1 measured `+0.24 MB/min` in the
  `WebKitWebProcess`. `term.dispose()` runs on hide, so this is bounded by
  *visible* panes; `scrollback` is the lever if it ever matters.
- **Repeated snapshots.** Agent 1 saw two `snapshot` frames in a 30 s window
  with one subscribe; there is a single send site, so it is most likely a
  client reconnect — but at ~1.08 MB a reconnect loop is expensive, and the
  `Lagged → reconnect` path in `ws_conn` re-sends the whole snapshot. Worth
  confirming *before* Phase 2 makes the snapshot cheap enough to hide.
- **Unbounded `events` table.** Rows are only dropped at 30-day retention; a
  very chatty long-lived session accumulates a lot of them. `load_recent_events`
  already bounds the wire, so this is disk, not wire — revisit if the DB grows.

## Not a performance issue, filed while here

None this pass. (`R-J62` and the `cost-state` canary on Claude Code 2.1.246
belong to agent 1's measurement session; see [the
roadmap](../product/roadmap.md).)
