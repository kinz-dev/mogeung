---
title: Performance findings — where the daemon and the window still spend
status: superseded
updated: 2026-08-26
---

# Performance findings, 2026-08-26

A second look at resource use, taken against the **running installed binary**
(pid 52254, ~1 h uptime, 4 live sessions, 222 sessions known) after the
`R-J53`–`R-J59` pass shipped and took its verdicts.

The headline: **the I/O problem is gone and did not come back.** What is left is
not I/O at all — it is chatter. The daemon asks the window to do work forty
times a minute that nothing consumes, which is the same shape `R-J8` and
`R-J53` both had, arriving now on the wire instead of the disk.

Nothing here is built. This is evidence and a plan.

## How this was measured

Everything below is a delta between two reads of the same counter on the live
process, not an inference from code. Reproduce with:

```sh
P=$(pgrep -f mogeung-desktop | head -1)
cat /proc/$P/io                     # read_bytes vs rchar, syscr, syscw
cat /proc/$P/status                 # VmRSS, Threads
CG=$(head -1 /proc/$P/cgroup | cut -d: -f3)
grep workingset_refault_file /sys/fs/cgroup$CG/memory.stat
head -2 /proc/pressure/memory
```

Per-thread attribution comes from `/proc/$P/task/*/io`; wire traffic from a
raw websocket client that subscribes and counts frames by `ev`. `strace` is
**not** available against a running process here — `kernel.yama.ptrace_scope`
is `1` — which is why the fork costs below are timed by re-running the same
commands rather than traced.

## Baseline: where it goes now

|  | daemon | renderer (`WebKitWebProcess`) |
|---|---|---|
| CPU | 4.9% (3.7 self + 1.2 forks) | 8.6% |
| RSS | 317 MB, flat over the window | 403 MB, +0.24 MB/min |
| disk reads | **0 KB** in 190 s (1.3 MB in the first hour) | 0 KB |
| file refaults | **0 pages** | — |
| memory pressure | `0.00` at avg10/60/300 | — |

`R-J53`'s fix is holding: logical reads run at ~41 MB/min while physical reads
are zero, so the page cache is fully effective — the exact inversion of the
~0% hit rate that was driving system-wide reclaim. WAL sits at 3.9 MB against
its 4 MB `journal_size_limit`.

**The renderer now costs nearly twice the daemon**, and most of that is work
the daemon hands it. Per 1.5 s tick the daemon makes 3,750 read syscalls, 230
writes and 2.1 MB of logical reads, and pushes 100 KB/s at the websocket.

## Finding 1 — the queue gate is defeated by a live duration string

**The largest single cost, and the cheapest to fix.**

`publish_queue` compares the whole ranked list against `last_queue` and stays
silent when they match. They almost never match. Two consecutive payloads:

```
rows: 223 → 223
rows whose detail changed : 2   'waiting for you — 51m22s' → 'waiting for you — 51m24s'
rows whose score changed  : 0   'waiting for you — 25s'    → 'waiting for you — 27s'
rows whose reason changed : 0
```

Nothing about the ranking moved. Two rows out of 223 differ in the *seconds of
a humanised duration*, and all **28.5 KB is rebroadcast to every window** —
measured at 0.69/s, **1.07 MB/min**, the dominant steady-state traffic. Every
window then parses it and re-renders the queue panel roughly forty times a
minute.

The cause is in `rank()`: `detail` is built with `format!("waiting for you — {}",
fmt_dur(waited))` from `Utc::now()` (`crates/mogeung-core/src/attention.rs:160`).

This is **exactly the `R-J55` failure mode** — a by-construction-volatile field
defeating a change gate — which was diagnosed and fixed for `Health` and never
applied to `Queue`.

The sharpest part: the client already renders its own clock.
`desktop/src/ui/QueuePanel.tsx:307` is
`{fmtDur(secsSince(session.last_event_at, now))} ago`, using the same `fmtDur`.
The daemon is shipping a second clock forty times a minute to sit beside one
the window computes for free.

## Finding 2 — the snapshot is 1.08 MB, and 85% of it is never displayed

223 sessions at 5.0 KB each. By field:

| field | share | avg |
|---|---|---|
| `recent_touches` | 40.0% | 2033 B |
| `recent_tools` | 19.4% | 988 B |
| `verify_runs` | 14.6% | 743 B |
| `touched_files` | 10.6% | 539 B |

All four are working data, not board data. Collision detection consumes
`recent_touches` **server-side**; the window shows these only for the
*selected* session. What the board renders is id, label, status and counts.

This lands on every reconnect — and per `R-J59`'s own note, after a laptop
sleep that is every window at once.

Caps are in place and holding (`recent_touches` 200, `verify_runs` 30,
`recent_tools` `LOOP_HISTORY`); observed maxima are 200 / 30 / 12. The problem
is not unbounded growth, it is shipping bounded history for 223 sessions to
render about six fields.

## Finding 3 — `ps -axo` every tick costs 1.2% of a core, continuously

Timed on this machine (680 processes): **18.5 ms per call**. At 1.5 s ticks
that is precisely the 1.2% children-CPU measured on the process — about a
quarter of the daemon's entire CPU budget.

`process_parents()` (`crates/mogeungd/src/state.rs:4387`) exists only to map a
tmux pane pid to a session pid through the ancestry walk. That mapping changes
when a session is attached or moved — rarely. `tmux list-panes` beside it costs
**2.1 ms**, nine times cheaper, and *is* the thing that changes when panes move.

`R-J57` already stopped these two forks firing on a wholly idle machine
(`live_by_id.is_empty()`). On a machine actually in use they fire every tick.

## Finding 4 — smaller, adjacent

- **Duplicate stat per transcript per tick.** `scan_transcripts` already
  returns `size` from its own `metadata()` call; `read_new` immediately stats
  the same path again to decide whether to open it. ~90 redundant stats/tick.
- **90 lock acquisitions per tick.** `self.sessions.read().await.contains_key(…)`
  runs *inside* the per-file loop rather than once for the pass.
- **223 wasted allocations per tick.** `publish_queue` builds the full `labels`
  map — 223 id clones plus 223 `label()` calls — *before* the gate, for a
  `notify_for` that usually notifies nothing.
- **Qwen never got `R-J56`'s gate.** `scan_qwen` walks the sessions directory
  and the transcripts directory every tick. Codex has an mtime gate for the
  identical shape; Qwen has none.
- **`refresh_collisions` takes a write lock over the whole board every tick**,
  blocking API readers, even when no live session touched anything.

## Plan

### Phase 1 — the clock and the fork

Highest value, self-contained, no protocol break.

1. **Move the queue's clock to the client.** Replace the duration inside
   `detail` with the timestamp it derives from — `waiting_since` on the queue
   item, `#[serde(default)]` so older clients are unaffected — and let
   `QueuePanel` render it with the `fmtDur` / `secsSince` it already calls.
   `detail` keeps its reason-specific text (`"22 file(s), +4110 -213 unread"`,
   `"server_error"`), which is genuinely static. Pin the invariant with a test:
   two `rank()` calls a minute apart over an unchanged board must compare equal.
2. **Gate `process_parents()` on the tmux pane list.** Keep the cheap
   `tmux list-panes` every tick; fork `ps` only when that output changed, when
   a live session has no `tmux_target`, or on a slow cadence as a backstop. The
   pane list is what moves when a pane moves, so this cannot go stale silently.

Expected: ~1 MB/min off the wire, ~40 parse-and-re-render cycles per minute out
of every window, and roughly a quarter of the daemon's CPU.

### Phase 2 — the snapshot's shape

3. Split `Session` on the wire into what the board renders and what a pane
   needs. The snapshot carries the board projection; `recent_touches`,
   `recent_tools`, `verify_runs` and `touched_files` arrive via a
   `fetch_session_detail` verb answered **on the reply lane `R-J59` already
   built**. Every added field `#[serde(default)]`, so a client built before the
   split still parses. Expected: 1.08 MB → ~160 KB.

Bigger blast radius than Phase 1 — it touches the wire contract — so it wants
its own pass rather than riding along.

### Phase 3 — the remaining per-tick work

Mechanical, and each independently revertible.

4. Thread the known `size` from `scan_transcripts` into `read_new`; hoist the
   `contains_key` to one read lock per pass.
5. Build `labels` lazily, for the ids that will actually notify.
6. Give Qwen the `R-J56` treatment — stamp the sessions and transcript
   directory mtimes, skip when unmoved and nothing Qwen-side is alive.
7. Early-out `refresh_collisions` when no live session has touches inside the
   window and no session currently carries a collision.

## Watch, do not act yet

- **Renderer +0.24 MB/min** (~115 MB over an eight-hour day). `term.dispose()`
  does run on hide, so `R-J58` frees memory as well as CPU and this is bounded
  by *visible* panes. If it ever needs action, `scrollback: 10_000` per
  terminal is the lever — and the pane's own comment already argues that tmux
  copy-mode holds the real history.
- **Two `snapshot` messages arrived in a 30 s window** when only one subscribe
  was ours. There is a single send site (`state.rs:768`) and `R-J59` put it on
  the reply lane, so this is most likely a client reconnect — but at 1.19 MB a
  reconnect loop is expensive. Worth confirming *before* Phase 2 makes it cheap
  enough to hide.

## Not a performance issue, filed while here

`R-J62` (fixed, `ca05300`) and `R-J63` (open — the `cost-state` canary alert on
Claude Code 2.1.246) both came out of this measurement session. Neither is a
resource problem; see [the roadmap](../product/roadmap.md).
