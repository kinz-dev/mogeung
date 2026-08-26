---
title: Performance pass 3 — the chatter, not the disk
status: draft
updated: 2026-08-26
roadmap: R-J64
depends_on: [A4]
---

# 0037 — Performance pass 3

Merged from two independent reviews taken the same day against `ca05300`:
agent 1 measured the running binary, agent 2 read the source. Where they agreed
the item is here with both kinds of evidence; where they disagreed the
measurement won. Both source documents are in
[`../archive/`](../archive/) — this file supersedes them.

## Spec

### Problem

`R-J53`–`R-J59` fixed the I/O storm that was making the whole machine slow, and
the fix is holding: against the installed binary the daemon reads **0 bytes**
from disk over a 190 s window, file refaults are **0**, and system memory
pressure reads `0.00` at every horizon.

What is left is not I/O. It is **chatter** — work redone at the poll rate that
nothing consumes. Per 1.5 s tick the daemon makes 3,750 read syscalls, 230
write syscalls and 2.1 MB of logical reads, and pushes 100 KB/s at the
websocket. The renderer now costs nearly twice the daemon (8.6% vs 4.9% CPU),
and most of that is work the daemon hands it.

This is the same shape as `R-J8` and `R-J53`, arriving on the wire instead of
the disk.

### Assumptions

Rests on `A4` (the agent CLIs' formats are undocumented and move without
warning) only in so far as the scan loop must keep degrading rather than
panicking. Nothing here changes what is parsed. No `UNTESTED` assumption blocks
this work.

### Acceptance

- [ ] With the board idle and one session waiting, the daemon broadcasts the
      queue at most a few times a minute rather than on every tick.
- [ ] Only one `ps` process is forked per scan pass, whichever CLIs are live.
- [ ] `cargo test --workspace`, `npm test` and `./scripts/check-docs.sh` pass.
- [ ] The Changes pane still shows the same files it does today — measured
      before and after on a session with in-repo attribution.

### Explicitly out of scope

- **The snapshot projection** (agent 1's Finding 2). Splitting `Session` on the
  wire is worth doing and is the largest payload win available, but it changes
  the wire contract and belongs in its own pass, judged on reconnect latency
  rather than on the steady-state numbers here.
- **Narrowing `compute_change` with a git pathspec** (agent 2's Finding 2).
  See *Rejected* below — the proposed fix reintroduces `R-J62`.

## Plan

### What was verified, and what did not survive

Every claim from both reviews was checked against the code and this machine.

| Claim | Verdict | Evidence |
|---|---|---|
| Queue gate defeated by a live clock | **Confirmed** | 2 of 223 rows differ, seconds only; 28.5 KB rebroadcast at 0.69/s = 1.07 MB/min |
| Duplicate `tmux`+`ps` forks | **Confirmed** | `state.rs:989/993` and `state.rs:1921/1925`; `ps -axo` timed at 18.5 ms on 680 processes |
| Per-event SQLite writes | **Confirmed** | `emit` loops `append_event`, autocommit + mutex + `to_string` per row; matches 230 writes/tick |
| `quiet_view` clone chain | **Confirmed, understated** | `significant_change` clones *both* sides, and clones the three fattest vecs to zero them |
| `opt-level = 2` | **Confirmed** | `Cargo.toml:27` |
| `compute_change` cost | **Real, framing wrong** | 54 ms / 6.0 MB per probe ≈ 36 MB/min — the dominant `rchar` source. But no live session currently discards it, and the hot repo has **0** untracked files, not the 200 × 512 KiB worst case quoted |
| `scan_qwen` "has no gate at all" | **False** | `state.rs:1917` gates both forks on `live.is_empty()`. It lacks a gate on the *walk* — which covers 7 transcripts |
| `scan_live` "grows without bound" | **Contradicted** | `~/.claude/sessions` holds **10 files, 44 KB** against 222 known sessions |

### Approach

Ordered by measured payoff. Each ships and reverts on its own.

**1. One process table per pass (`R-J64`).** The Claude liveness pass and
`scan_qwen` each fork `tmux list-panes` + `ps -axo pid=,ppid=`, gated on their
own live list. On a machine running both — this one — both fire, so `ps` is
forked twice per tick at 18.5 ms each. Resolve `(panes, parents)` **once** per
pass and hand it to both. Then gate the `ps` fork itself: `tmux list-panes`
costs 2.1 ms and *is* the thing that changes when a pane moves, so re-fork `ps`
only when the pane list changed, when a live session has no `tmux_target`, or
on a slow backstop cadence.

**2. The queue's clock moves to the client (`R-J65`).** `classify` bakes
`fmt_dur(…)` into `detail` for the `AwaitingInput`, `AwaitingPermission`,
`Stalled` and snoozed branches, and `AttentionItem` derives `PartialEq` over
all fields — so `publish_queue`'s `*last == queue` gate cannot hold while any
session is waiting. Carry the anchor timestamp instead (`waiting_since`,
`#[serde(default)]`) and let the window render it with the `fmtDur` /
`secsSince` it already calls at `QueuePanel.tsx:307`.

> **The obvious cheaper fix does not work.** Gating on a
> `(session_id, reason, score)` projection still ticks, because
> `score = base_score() + (waited / 30).clamp(0, 99)` (`attention.rs:201`).
> And even the real fix will not reach silence: that tiebreak genuinely
> reorders the queue every 30 s. Expect ~40 broadcasts/min → ~4–8/min.

**3. Folds stop cloning what they mask; events batch per session (`R-J66`).**
`significant_change(before, after)` calls `quiet_view` on both sides, and
`quiet_view` deep-clones the session — including `recent_tools`,
`recent_touches` and `touched_files`, 70% of a session's bytes — *specifically
to zero them*. Compare the surviving fields directly, cloning nothing.
Alongside, `emit` writes one autocommit transaction per event, each taking the
store mutex and running its own `serde_json::to_string`; batch a session's
events into one transaction per pass.

**4. `opt-level = 3` (`R-J67`).** The daemon is a long-lived hot loop, not a
build-time cost. One line, no behaviour change.

### Rejected

**Narrowing `compute_change` with a git pathspec.** Agent 2 proposed
`git diff <base> -- <touched…>` to skip work later thrown away. It would
silently reintroduce `R-J62`. The `retain` uses **suffix** matching —
`f.path.ends_with(t.as_str())` — because `touched_files` carries the prefix the
transcript wrote while git reports paths resolved through symlinks (`R-J27`,
documented in [architecture.md](../design/architecture.md)). A git pathspec has
no suffix semantics: hand it a path spelled the other way and it matches
nothing, `git diff` returns empty, and the Changes pane blanks — the exact
failure fixed in `ca05300`, except now inside git where the `is_absolute()`
guard cannot see it and no test covers it.

The underlying observation is still the best unclaimed win in either document:
that diff is **36 MB/min of text**, the daemon's dominant logical-read source.
Pursuing it needs the spelling reconciled *before* it reaches a pathspec, plus
a test built from a symlinked checkout. Filed, not built.

**Dropped as below the noise floor.** A Qwen mtime gate (7 transcripts), a
`scan_live` mtime cache (10 files), the duplicate transcript `stat`, the
per-file `contains_key`, lazy `notify` labels, and the `refresh_collisions`
early-out. Each is microseconds per tick. Do them only when already editing
that function.

### Files touched

- `Cargo.toml` — release profile
- `crates/mogeung-core/src/attention.rs` — `waiting_since` on `AttentionItem`
- `crates/mogeungd/src/state.rs` — shared process table, fork gate, `quiet_view`
- `crates/mogeungd/src/store.rs` — batched event append
- `desktop/src/wire/types.ts`, `desktop/src/ui/QueuePanel.tsx` — client clock

### Risks and unknowns

- **The fork gate must not go stale.** A session moved between panes has to be
  noticed. Gating on the tmux pane list is safe because that list is what
  changes when a pane moves; a backstop cadence covers the rest.
- **`waiting_since` is additive.** `#[serde(default)]` keeps a client built
  before the change parsing the queue unchanged.
- **Batching events changes durability granularity.** A crash mid-pass loses a
  pass's events rather than none — acceptable, since `R-A6`'s tail offsets are
  written after the fold and an interrupted pass re-reads its batch.

### Test strategy

A test per change, each of which fails on today's code:

- Two `rank()` calls a minute apart over an unchanged board compare equal.
- One scan pass with both CLIs live resolves the process table once.
- `significant_change` agrees with the old clone-based answer across a table of
  session pairs (counter-only, meaningful, mixed).
- A batched append stores the same rows in the same order as the per-event one.

## Notes

Built 2026-08-26. All four landed; the deferred items stayed deferred.

**The acceptance criteria moved once, and the test says why.** "Two `rank()`
calls a minute apart compare equal" was written into the plan and is *false by
design*: `score` carries `(waited / 30).clamp(0, 99)` so the longest wait sorts
first inside a tier, and a waiting board therefore re-ranks every 30 s and
should be re-sent then. The real invariant is **one tick** apart, which is what
was failing. Both are now tests — the second one exists to stop a future reader
"finishing the job" by freezing the ordering.

**Wording changed in one row.** `Stalled` read `busy but silent for 8m30s` and
now reads `busy but silent — 8m30s`, so that every clocked row joins its text
and its duration the same way and the window needs one rule rather than four.
The other three are byte-identical to what they showed before.

**`significant_change` gained a compile-time guarantee it did not have.** The
clone-based version compared new fields by accident — `clone()` included them.
The destructured version would have *silently dropped* them, which is worse, so
it binds all forty fields by name: adding one to `Session` now fails to compile
until someone decides which side of the mask it belongs on. A table-driven test
moves each field in turn and asserts which side it lands on.

**A "flaky test" that turned out to be a real bug — `R-J68`.**
`run::tests::output_is_bounded_and_the_loss_is_stated` kept failing mid-pass and
passing on isolated re-runs, so the first read was "pre-existing flake, not
ours" — `run.rs` is not in this diff. Measured instead of assumed: **1 failure
in 20** isolated runs, and reliably under the load of a full workspace run,
which was enough to block `gen-status.sh` and therefore the hand-back.

It was a race in the product, not the test. `start` spawns a pump per pipe and
a waiter that calls `child.wait()` then `finish`; `child.wait()` returns when
the process is gone and says nothing about whether the pumps have drained the
pipes, so a run could be marked `Exited` with its last lines still in flight.
The waiter awaits both pumps now. **0 failures in 30** after, plus a new test
that repeats the shape twenty times so a probabilistic bug has a deterministic
guard.

Out of scope for a performance pass, and fixed anyway: it was standing between
this work and the checks that must pass before handing it back, and a run that
reports a truncated log while claiming to have finished is a worse bug than
anything else on this list.

**Verified on the reinstalled binary, 2026-08-26.** Queue broadcasts
0.69/s → 0.10/s and 1.07 → 0.15 MB/min; fork CPU 1.22% → 0.24%; disk writes
0.27 → 0.02 MB/min; refaults 0, memory pressure 0.00. `R-J67` showed **no
measurable difference** and is recorded as such rather than claimed as a win.

**One claim in this document was wrong and is corrected in `R-J66`.** Both
reviews cited *230 write syscalls per tick* as the per-event SQLite inserts,
and this plan repeated it. Thread-level attribution on the running process
shows writes are WebKit's — `ReceiveQueue` 305/s, GTK main 161/s,
`VBlankMonitor` 50/s — against ~20/s across every tokio worker. `syscw` was
measuring the window, never the store. The batching still helped; the number
offered as proof of it did not belong to it.

**The verification found a new one, `R-J69`.** With the queue's per-tick
broadcast gone, `snapshot` became 94% of all wire traffic — and not from the
reconnect loop the *Watch* section above guessed at. Every connect shipped the
board **twice**: the daemon pushes a snapshot on connect, and the window then
asked for a re-send it had not missed. Fixed in the client, where the
redundancy was, and verified on the reinstalled bundle: a connection that never
sends `subscribe` gets **one** snapshot and every live update; one that does
still gets **two**, because the daemon is deliberately unchanged.

**Not done, deliberately.** The snapshot projection (1.08 MB → ~160 KB) and the
narrowed `compute_change` are both still open — the first because it changes
the wire contract and deserves its own pass, the second because the obvious
implementation reintroduces `R-J62`. See *Rejected* above; the 36 MB/min of
diff text remains the largest unclaimed win.
