---
title: Sharpen triage, reach and review
status: shipped
updated: 2026-07-25
roadmap: [R-B1, R-B2, R-B3, R-B4, R-B5, R-B6, R-B7, R-B8, R-B9, R-C1, R-C2, R-C3, R-C4, R-C5, R-D1, R-D2, R-D3, R-D4, R-D5, R-D6, R-D7, R-D8, R-D9]
depends_on: [A1, A3, A6, A8]
---

# 0002 — Sharpen triage, reach and review

Roadmap pillars `B` (queue), `C` (notifications and reach) and `D` (review
depth), built together because they share the same three files and would have
conflicted if split.

## Spec

### Problem

v0.2 plus [feature 0001](0001-trust-the-tool.md) gives a queue you can trust and
a diff that remembers what you read. What it does not give you is a way to *act*
quickly: every interaction is a mouse click, the queue cannot be searched or
grouped, a session you have consciously decided to ignore keeps shouting, and
"waiting for you" cannot tell a permission prompt apart from a finished turn —
which are the two most common states and want opposite responses.

Reach is worse: mogeung only exists while its window is open and you are looking
at it. And review depth stops at a unified diff, so reformatting resurrects
hunks you already read, and nothing answers "what else does this touch?".

### Assumptions

| # | Assumption | Status | Why this rests on it |
|---|---|---|---|
| [A1](../product/assumptions.md) | A cross-session queue changes where the user looks | `UNTESTED` | Every `B` item makes the queue faster to act on. Worthless if the queue is ignored |
| [A6](../product/assumptions.md) | The user runs 3–4 concurrent sessions | `UNTESTED` | `R-B3` collision warning cannot fire at N=1 |
| [A3](../product/assumptions.md) | Keyword risk heuristics are good enough for reading order | `UNTESTED` | `R-D8` ranks unread files by the same score |
| [A8](../product/assumptions.md) | Per-session attribution by edited files is accurate enough | `AT RISK` | `R-B3` inherits it exactly — an agent editing via shell is invisible |

**This work proceeds on two `UNTESTED` assumptions, against the standing rule in
[assumptions.md](../product/assumptions.md).** That was an explicit instruction
after the rule was put to the user, and it is recorded here rather than
quietly: if `A1` or `A6` turn out false, most of pillar `B` and all of pillar
`C` was wasted effort, and this spec is where that will be visible.

### Acceptance

- [ ] The queue is navigable and actionable from the keyboard alone.
- [ ] "Blocked on a permission prompt" and "finished, waiting for a task" are
      different rows with different urgency.
- [ ] A session can be silenced without being forgotten, and the silence holds.
- [ ] Two live sessions editing one file warn both sides, and stop warning when
      it is over.
- [ ] A session repeating itself is visible without being promoted.
- [ ] mogeung can reach you when its window is not in front, without becoming
      something you mute.
- [ ] The queue and a diff are usable from a phone.
- [ ] Reformatting does not resurrect a read hunk; genuinely different code
      never collides with it.
- [ ] Work an agent committed before mogeung noticed the session is still in the
      diff.
- [ ] Review notes become a prompt without mogeung sending anything.
- [ ] "How much has nobody read?" and "what else uses this?" are answerable.

### Explicitly out of scope

- Any path that sends text to a session — [ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md).
- Real syntax parsing, or a real call graph. Both stay heuristics that say so.
- Terminals other than Terminal.app for `R-B2`.
- Authentication, despite `R-C3` inviting a phone onto the network.

## Plan

### Approach

**Core first, then daemon, then UI**, because all three pillars land in
`app.rs`, `wire.rs` and `state.rs`. Splitting them across parallel workers would
have cost more in merge conflicts than it saved.

**New signals are session fields, not a side table.** They ride the existing
`SessionUpdated` broadcast, so no new plumbing and no cache to invalidate. All
carry `serde(default)` — the store keeps sessions as JSON blobs and only *warns*
on an unreadable row, so a missing default would silently drop sessions.

**Presentation logic is pure functions in a separate module.** Highlighting,
word diff and side-by-side pairing are text-in/spans-out in `ui/diff.rs`, so the
parts worth testing are testable without a window.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/session.rs` | `OpenTool`, `Touch`, `Collision`; 6 new fields |
| `crates/mogeung-core/src/attention.rs` | `AwaitingPermission` tier; snooze suppression |
| `crates/mogeung-core/src/review.rs` | new — `ReviewDebt`, `BlastRadius` |
| `crates/mogeung-core/src/wire.rs` | 4 commands, 2 events |
| `crates/mogeungd/src/state.rs` | open tools, touches, collisions, loops, snooze, debt, blast, focus |
| `crates/mogeungd/src/git.rs` | normalised anchors, session-aware base, symbol extraction |
| `crates/mogeungd/src/notify.rs` | new — transition-only notification |
| `crates/mogeungd/src/web.rs` | new — the thin web client |
| `crates/mogeung-ui/src/diff.rs` | new — highlight, word diff, side-by-side |
| `crates/mogeung-ui/src/app.rs` | filter, grouping, keys, ambient, prompt, debt, blast |

### Risks and unknowns

- **`R-C2` (menu-bar item) was not built.** See Notes.
- **Collision warnings inherit `A8` exactly.** A false negative is silent.
- **Anchor normalisation is a one-way door**: loosening it further risks marking
  unread code as read, which is worse than any amount of re-reading.

### Test strategy

Free and offline throughout. The pure functions get property-style tests
(losslessness, no line dropped); the cross-session signals get a synthetic
`~/.claude` with two live sessions, which is the only way to exercise them.

## Notes

### `R-C2` was not built, and that is a real gap

A menu-bar item needs to outlive the window to be worth anything — the whole
point is glancing without opening mogeung. Inside the egui app it would die with
the window; done properly it is a **fourth binary** with its own event loop and a
`tray-icon` dependency.

That is a bigger commitment than the rest of pillar `C` combined, and it is the
item most likely to be made redundant by the dogfooding week: if `--notify`
banners turn out to be enough, a menu-bar item is dead weight. Left undone
deliberately, and `R-C2` stays open on the roadmap. **Everything else in B, C
and D shipped.**

### Two bugs I wrote, both found by running it

**The word diff highlighted whole lines.** It compared the `-`/`+` marker as
content, so every pair differed at position 0, the common-prefix scan stopped
immediately, and the entire line lit up — no better than not having a word diff.
Caught by a test that asserted the *useful* behaviour rather than the
implemented one.

**The permission detector fired on the existing test fixture.** The v0.2
discovery fixture ended on an `Edit` tool call with no result, so it correctly
classified as `APPROVE` and broke an assertion expecting `WAITING`. The fixture
was wrong, not the detector: it depicted a session that had asked to edit a file
and never got an answer. Fixed by closing the tool call, and a new test now
covers both sides of the distinction.

That is twice in two features that the first real run found something the tests
did not. It is becoming the pattern worth trusting.

### Judgement calls worth recording

**Snooze beats failure.** A mute button that failure can override is one nobody
presses.

**Loop detection is advisory, never a tier.** Repetition is suggestive, not
proof, and a heuristic that rough must not be able to reorder the board.

**`APPROVE` outranks `WAITING`.** The agent has work in flight it cannot finish;
a session waiting for a task has already done what you asked.

**Grouping keeps global rank.** Repos are ordered by their most urgent session,
so the top of the panel is still the top of the queue.

### Verification

Against the author's real corpus, live:

- Review debt on this repo: **59 hunks, 0 read, 14 files**, riskiest first —
  which is this feature's own uncommitted diff.
- Blast radius on `session.rs`: found all **7** newly added symbols and **22**
  references across the workspace, tests called out.
- Web client served at `/`, 10.5 KB, self-contained.

91 tests → 98, all free and offline.
