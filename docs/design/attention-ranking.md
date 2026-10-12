---
title: Attention ranking
status: active
updated: 2026-10-10
covers:
  - crates/mogeung-core/src/attention.rs
---

# Attention ranking

One queue across every session, answering "where do I look right now?".

**How ranking is called.** `rank` is generic over `Borrow<Session>` since
`R-J57`: the scan loop ranks borrowed sessions under its read lock instead of
cloning the whole board once per tick. Nothing about the ordering changed —
the genericity is a calling convention, and `&[Session]` callers (the HTTP
queue, the snapshot, every test) compile as before.

## Reasons

Scored in this order. The order the code *evaluates* them in is stated below
the table: snooze first, then `Failed`; for a live session `RateLimited`,
`AwaitingPermission`, `AwaitingInput`, `Stalled`, `Running`; for an ended one
`Idle` (reviewed), `Idle` (no changes), `NeedsReview`.

| Reason | Badge | Score | Fires when |
|---|---|---|---|
| `AwaitingPermission` | `APPROVE` | 1100 | Alive, registry `idle`, **and a tool call has no result** — or, since `R-P11`, a seat's pane shows a permission menu, a plan approval or a question, with *(screen)* in the detail |
| `AwaitingInput` | `WAITING` | 1000 | Alive and the registry reports `idle` — or, since `R-P6`, the team's policy holds a send for this seat, whatever it is doing — or, since `R-P11`, a seat's pane shows a trust dialog, a login or *press Enter*, named with *(screen)* |
| `Failed` | `FAILED` | 900 | An API error was recorded — or, since `R-P11`, a seat's pane shows a shell where the agent was, *(screen)* |
| `RateLimited` | `LIMIT` | 850 | Alive and a rate limit was hit; cleared when the session moves again — or, since `R-P11`, a seat's pane shows a rate-limit banner |
| `NeedsReview` | `REVIEW` | 800 | Exited, changed files, not all read |
| `Stalled` | `STALLED` | 700 | Alive and busy, silent ≥ `stall_secs` |
| `Running` | `running` | 100 | Alive and busy, recently active |
| `Idle` | `idle` | 0 | Reviewed, exited with no changes, **or snoozed** |

**`Failed` is checked before liveness**, so a live session that hit an API error
still ranks as failed. The error clears when a new human turn appears — you
responded, so it no longer needs you.

**Snooze is checked before everything**, including failure. "Stop telling me
about this one" has to mean it, or you would never trust it enough to use it.
The row ranks `idle` with the detail *snoozed* and stays visible; the window
marks it with a small clock and offers *Wake*.

**`RateLimited` is neither `Failed` nor `AwaitingInput`** (`R-G1`): there is
nothing to fix, and typing does not help. It usually means several sessions
went dark at once, and the queue should look like exactly that. Cleared when a
new human turn or real assistant output shows the session moving again.

### `APPROVE` vs `WAITING` (`R-B4`)

The registry says `idle` in both cases, and they want opposite things from you:
one needs a decision about work **already in flight**, the other needs a new
instruction. They are told apart by an unmatched `tool_use` — a tool call with
no corresponding `tool_result` later in the transcript.

`APPROVE` outranks `WAITING` because the agent cannot proceed until you answer,
whereas a session waiting for a task has finished what you asked. A new human
turn clears the open-tool list: you cannot be blocked on a prompt you have
already answered.

An open tool on a *busy* session is not a prompt — the tool simply has not
returned yet — so both conditions are required.

## Scoring

`score = base + tiebreak`, where `tiebreak = (waited / 30).clamp(0, 99)`.

The gap between tiers is 100 and the tiebreak caps at 99, so **a session can
never be promoted past a more urgent tier** no matter how long it has waited. A
brand-new `FAILED` always outranks a three-day-old `REVIEW`. Pinned by a test.

`waited` is time-since-idle for `AwaitingInput` and `AwaitingPermission`, and
session duration otherwise.

## Why `WAITING` is not a heuristic

Claude Code publishes `status: idle` in its own live registry, so blockage on a
human is a **fact we are told**, not something inferred. In v0.1 this could only
be guessed after the fact from permission denials, and it was the largest
documented gap. See [ADR-0003](../decisions/0003-observe-do-not-spawn.md).

## Sessions from another machine (`R-I21`, 2026-09-23)

A daemon that follows others ([feature 0047](../archive/features/0047-daemon-federation/spec.md))
ranks **one board**: its own sessions and the ones it mirrors go through
`attention::rank` together, in `AppState::rank_board`, so a peer's waiting
session outranks a local idle one by the same rule two local ones would. No
second ranking exists, in the daemon or in the window — which is
[ADR-0013](../decisions/0013-one-window-one-daemon.md)'s whole reason for
putting federation in the daemon.

The one exception is a peer that has gone quiet. Its sessions are kept —
a session that simply vanishes when Wi-Fi drops is worse than one marked old —
but they are **not ranked**: they are appended below everything live as
`Idle`, score `-1`, with the detail *"on HOST — not heard from; last known, not
current"*. Last-known *waiting* must not sit at the top of the queue for ever,
and ranking a stale row by its stale reason would do exactly that. They go
stale a minute after the peer was last heard, and rank normally again the
moment it is.

The notifier reads the merged queue, so a waiting session on a followed
machine notifies here — labelled *"… — on HOST"* — and counts in the tray.

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
(`waiting for you`, `22 file(s), +4110 -213 unread`). The ranking must never be
a black box.

Since `R-J65` that string is **static**, and the clock travels beside it as
`since` — the instant the row's wait began, which is the same anchor
`waiting_secs` measures from. The window renders `detail — fmtDur(now - since)`
and gets the sentence it always showed. Rows with no clock (`Running`,
`NeedsReview`, `Failed`, `RateLimited`) carry no anchor; snoozed rows carry none
either, because their countdown runs to a *deadline* the window already holds as
`snoozed_until`.

## The queue is gated (`R-J8`, `R-J65`)

The ranking is computed every pass and broadcast only when it differs from the
last one sent. `AttentionItem` derives `PartialEq`, and the derive is
load-bearing: every field of an item is part of the comparison, so a field that
changes by construction — a rendered duration inside `detail`, which is what the
string carried for a year — disables the gate silently. That is why `detail` is
static and the clock travels as `since`. The gate does not reach silence and
cannot: the tiebreak moves every 30 s for anything waiting, so a board with a
waiting row genuinely re-ranks and is genuinely re-sent then — a handful of
broadcasts a minute, not none. Two tests hold both halves: one tick apart an
unchanged board ranks identically, and a minute apart the tiebreak still moves.
Gating applies to the broadcast, never to a request: a `Rescan` or an explicit
fetch is always answered, unchanged or not
([wire-protocol.md](wire-protocol.md#the-contract)).

## What ranking does not read

`attention.rs` is covered by this document, so every commit to that file asks
whether a new field is an input. The answer has been *no* each time, most
recently on 2026-10-07, and each refusal is deliberate rather than pending:

- **Workspace and discovery fields** (`R-J39`, `R-J40`): a folder you added, or
  one you were offered, says nothing about whether a session needs you.
- **`api_ms` and `tool_ms`** (`R-J63`): time on the API is not urgency. A
  session that has burned an hour of API time may be finished, may be looping,
  and may be doing exactly what it was asked; wiring it in would denominate
  attention in a misleading unit, which is the specific mistake
  [ADR-0005](../decisions/0005-tokens-not-dollars.md) made and
  [ADR-0024](../decisions/0024-equivalent-cost-in-dollars.md) kept out of the
  queue when it brought dollars back for one view only. The numbers answer
  *where did this session's time go*, which nothing else mogeung reads can, and
  the Info pane is where they belong.
- **`artifacts`** (`R-J115`) and **`custom_title`** (`R-J120`): a page a
  session published, or a name you gave it, is not a reason to look.
  `custom_title` pins `title`, which `label()` reads for the notifier and the
  window; `rank` never reads a label.
- **`provisional`** (`R-J75`): a tmux pane running an agent that has not
  written a line yet is published as a session
  ([ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md))
  carrying only what the pane can prove — alive, a pid, a cwd where the
  platform will say, and zero of everything else. Ranking does not look at the
  flag. It looks at the evidence, which places the row at `running` with the
  detail *working*. Rebuilt every scan with its clocks set to now, it can never
  become `STALLED` and never earns the within-tier tie-break, so it sits at
  exactly 100, the floor of the tier, until the agent writes its first line and
  the real session takes the pane. `R-J75` names that as its cost; this
  document names it as what ranking on evidence rather than on a flag has to
  do.
- **`seat`** (`R-P4`, `R-P5`): which team a session belongs to and whether
  its hooks reach the daemon are facts about the row, not reasons to look. A
  seat ranks on the same queue as every session you started by hand, from the
  same evidence; what its hooks said lands on the launch record, not here.
  What **is** read, since `R-P6`, is `held_send` — and it is a reason, not
  the tag: the policy engine held a message for this seat (the fourth hop,
  the cap, the pause), and nothing moves until you release or drop it. It
  ranks `WAITING` with the hold as its detail — *a send held for it — the
  hop budget of 3 is spent* — and the hold's time as its clock, after an
  approval (a seat blocked on a prompt cannot take the message anyway) and
  before plain idle. An observed session never carries the field, because
  only the policy engine writes it and the engine addresses launch records.
- **`screen`** (`R-P11`, [ADR-0044](../decisions/0044-mogeung-reads-a-seats-pane-and-no-other.md)):
  read, and on the ADR's terms. A gate read off a seat's pane ranks — a
  permission menu, a plan approval or a question as `APPROVE`; trust, login
  and *press Enter* as `WAITING`; a rate-limit banner as `LIMIT`; a shell
  where the agent was as `FAILED` — with *(screen)* in the detail so a
  reading is never mistaken for a fact, and the read's time as its clock.
  The arm sits **after** the registry's own facts and the policy's hold and
  before plain idle, which is clause 4's *a hook's fact outranks a screen's
  reading*: a seat the registry already calls idle with an open tool is
  `APPROVE` on evidence, and the screen adds the reason only where the
  evidence is silent. The quiet shapes — at the prompt, working — and
  `unknown` never rank. An observed session never carries the field: the
  capture takes a pane from a launch record and from nowhere else.
