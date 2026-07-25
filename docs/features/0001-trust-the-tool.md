---
title: Trust the tool
status: shipped
updated: 2026-07-25
roadmap: [R-A1, R-A2, R-A3, R-A4, R-A5]
depends_on: [A4]
---

# 0001 — Trust the tool

## Spec

### Problem

mogeung reads two undocumented, private file formats. The parser is built to
ignore what it does not recognise rather than crash, which is the right
posture — but it means **the realistic failure is mogeung quietly seeing less
than it should**, and that looks exactly like a quiet day.

This is not hypothetical. Scanning the real corpus on this machine (52
transcripts, 68 MB, 20,648 lines) turns up 13 distinct event types. The parser
handles five and documents four more as deliberately ignored. Three —
`queue-operation`, `pr-link`, `frame-link` — are handled by nothing and
documented nowhere. They are silently swallowed by a catch-all arm today, and
nobody would ever find out.

That is the bug this feature exists to make impossible: not "we do not parse
`pr-link`", but **"we cannot tell the difference between a type we chose to skip
and a type we have never heard of."**

A second, related blindness: `AppState::ensure_session` carries the comment
*"Read it in full so history is complete, but only if it is not enormous"* — and
performs no size check. The guard it does have (`skip_to_end` when the file is
older than `HISTORY_DAYS`) is **unreachable**, because `scan_transcripts` has
already filtered those files out. The largest transcript here is 11.2 MB and is
read and parsed in full, synchronously, on first sight.

### Assumptions

| # | Assumption | Status |
|---|---|---|
| [A4](../product/assumptions.md) | Claude Code's on-disk formats are stable enough to depend on | `AT RISK` |

`A4` is `AT RISK`, not `UNTESTED` — we have evidence (verified against 2.1.219
and 2.1.220) and reason to doubt it holds forever. The rule that `UNTESTED`
assumptions must be tested rather than built on does not block this work;
**this work is the instrumentation that keeps `A4` honest over time.**

This feature deliberately does *not* depend on `A1` or `A6`, the two `UNTESTED`
assumptions that decide whether mogeung is worth building at all. It is the
prerequisite that makes the week of dogfooding meaningful: without it, "the
board looked empty" is unfalsifiable.

### Acceptance

- [ ] The daemon distinguishes four outcomes per transcript line: parsed,
      known-and-ignored, recognised-but-yielded-nothing, and never-seen.
- [ ] A transcript type nobody has classified raises a visible alert naming the
      type — no silent catch-all.
- [ ] Malformed JSON is counted and surfaced rather than dropped.
- [ ] A change in the observed Claude Code version raises an alert, because that
      is when formats move.
- [ ] A health view states what mogeung has seen and, explicitly, what it
      *cannot* see.
- [ ] A transcript larger than a stated cap is followed from its tail instead of
      read whole, and the fact that history was skipped is visible rather than
      silent.
- [ ] A committed corpus of anonymised line shapes parses with zero unknown and
      zero malformed outcomes, and fails loudly if a shape stops being handled.

### Explicitly out of scope

- Recovering from a format change. The goal is to *notice*, not to adapt.
- Parsing the three newly-named types into anything. They are classified as
  ignored; whether any deserves surfacing is a separate decision.
- Any change to ranking, diffing or review.

## Plan

### Approach

**Classification, not booleans.** `adapter::parse_line` returns `Option<Parsed>`
today, and `None` conflates "bookkeeping we skip" with "a type from the future".
It becomes `LineOutcome`, with an explicit `KNOWN_IGNORED` list. Adding a type to
that list is now a deliberate, reviewable act.

**Counting lives beside the scan, not inside the parser.** The parser stays
pure and synchronous; `ScanHealth` accumulates outcomes across scans, and the
daemon derives alerts from it.

**Alerts are facts, not thresholds.** A first unknown type is an alert on sight.
No ratio tuning, no smoothing — the interesting event is rare and discrete.

**The cap is visible.** A skipped tail is recorded per session and shown, so
"this session's early history was never read" is a stated fact rather than a
silent hole in the transcript.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeungd/src/adapter.rs` | `LineOutcome`, `KNOWN_IGNORED` |
| `crates/mogeungd/src/health.rs` | new — `ScanHealth`, alert derivation |
| `crates/mogeungd/src/state.rs` | record outcomes; cap oversized reads |
| `crates/mogeungd/src/watcher.rs` | tail-follow a file from an offset |
| `crates/mogeungd/src/store.rs` | remember versions and classified types |
| `crates/mogeung-core/src/health.rs` | new — wire types |
| `crates/mogeung-core/src/wire.rs` | `ServerMsg::Health` |
| `crates/mogeungd/src/api.rs` | enrich `/api/health` |
| `crates/mogeung-ui/src/app.rs` | health window + alert indicator |
| `crates/mogeungd/tests/fixtures/corpus.jsonl` | new — anonymised shapes |
| `crates/mogeungd/tests/corpus.rs` | new — golden test |

### Risks and unknowns

- **The corpus must carry no private data.** It is built from *shapes* observed
  in real transcripts — key names and block structure — with every value
  replaced by synthetic content written by hand. No prompt text, no file paths,
  no code.
- **`KNOWN_IGNORED` is a snapshot of one machine.** Another user will hit a type
  we have not seen. That is the alert working, not failing — but the first-run
  experience must not be a wall of warnings.
- **A version bump is not itself a problem**, so the alert must read as "watch
  this", not "something broke". Crying wolf trains the user to ignore it, which
  is the exact failure this feature exists to prevent.

### Test strategy

Every test free and offline. The corpus test is the one that would fail today:
`queue-operation`, `pr-link` and `frame-link` currently classify as unknown.

## Notes

### The feature found three real bugs, two of them mine

**1. Three event types were being discarded silently.** `queue-operation` (190
occurrences), `pr-link` (6) and `frame-link` (2) existed in the corpus
throughout v0.2 and were documented nowhere. They are now classified as ignored
— which may or may not be right, but it is now a *decision* rather than an
accident.

**2. The size guard was unreachable.** `ensure_session` promised "only if it is
not enormous" and checked nothing. Its one guard compared file age against
`HISTORY_DAYS`, which `scan_transcripts` filters on beforehand, so it could
never fire. Two transcripts on this machine (5.0 MB and 4.0 MB of history) were
being read and parsed in full inside the scan loop.

**3. The version alert was confidently wrong** — see below.

### The version watch cried wolf on its first real run

The first implementation kept versions in encounter order and reported a change
between the last two seen. Against the real corpus it announced:

> Claude Code changed from 2.1.209 to 2.1.210

while the machine was running **2.1.220**. Transcripts are scanned
newest-file-first and each carries whatever release wrote it, so encounter order
is unrelated to time; the alert had grabbed two historical releases and printed
them backwards.

This is worth recording because it is the exact failure the feature exists to
prevent, committed by the feature itself. A canary that fires wrongly is worse
than no canary — it teaches you to dismiss the panel, and then the real alert
arrives and you dismiss that too.

Ordering now comes from each line's own `timestamp`. `Health` also reports
`current_version`, because "what am I running now" is the question a user
actually has.

**The lesson generalises:** every alert this project adds should be run against
real data before it is believed. Synthetic tests all passed; only the 30-session
corpus exposed it.

### What the unit tests could not have caught

All three bugs survived a passing suite. The corpus test (`R-A3`) and the live
run against `~/.claude` are what found them. That is an argument for making the
golden corpus a habit rather than a one-off — and for the roadmap's instinct
that dogfooding beats more tests.

### Deliberate omissions

- **Counters are in-memory** and reset on daemon restart. Persisting "have I
  seen this type before" matters once an alert has been dismissed a few times.
  Not yet needed.
- **`Barren` does not alert**, only counts. A threshold there would be the ratio
  tuning this design explicitly avoids; if a shape moves, the count is visible
  in the panel and that is enough for now.
- **`history.jsonl` is still unread.** Out of scope; roadmap section `F`.

### Verification

Against the author's real corpus — 30 sessions, 9,757 lines in a scan:

```
headline      : Reading everything it recognises
blind ratio   : 0.0
running       : 2.1.220
lines         : 7754 parsed / 1971 ignored / 32 barren / 0 unknown / 0 unreadable
alerts        : 3 × history-skipped (non-urgent), correct in each case
```

36 tests → 63, all free and offline. Nothing spawns an agent.
