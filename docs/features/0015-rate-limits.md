---
title: Rate limits and token burn
status: in-progress
updated: 2026-07-29
roadmap: [R-G1, R-G2, R-G3]
depends_on: [A4, A20]
---

# 0015 — Rate limits and token burn

Pillar G, built at the 2026-07-29 *"finish the R-* items in one-go"* ask.

## Spec

### Problem

Hitting the five-hour window kills every live session at once, with no
warning and — with overage disabled — no recourse but waiting. mogeung
already parses per-message token usage and throws it away at any
granularity coarser than a session. When the limit lands, the evidence
is a synthetic assistant line the parser treats as ordinary text.

### Assumptions

A4 (`AT RISK`) — everything here reads the undocumented transcript
format. A20 (`AT RISK`) — **the roadmap's premise for R-G1 was wrong**:
a 2026-07-29 sweep of all 235 local transcripts found no
`rate_limit_event` line type anywhere. What exists is a synthetic
assistant message (`message.model == "<synthetic>"`, all-zero usage,
text like *"You've hit your session limit · resets 8pm"*). The honest
build keys on that signature, and additionally classifies a
`rate_limit_event` line type as handled-if-it-ever-appears so the
canary captures its shape instead of alarming.

### Acceptance

- [x] When a session hits the limit, the queue says so — a distinct
      attention state, not a silent idle — and shows the reset time
      parsed from the synthetic message when present
- [x] A burn view shows tokens per session, per repo, and per day
      (tokens, never dollars — ADR-0005) — the Insight pane's Analytics
      view
- [x] A rolling five-hour burn figure is visible; when past limit-hits
      exist, the warning threshold is derived from them and the UI
      labels it an estimate — never an authoritative quota
- [x] A `rate_limit_event` line, if a future CLI emits one, is captured
      with its raw shape rather than firing the unknown-type canary
- [x] Health/canary stays quiet across the full local corpus

### Explicitly out of scope

- Dollar amounts (ADR-0005), plan names, quota guesses presented as fact.
- Per-model burn breakdowns — wait for want.

## Plan

### Approach

Adapter: detect the synthetic-limit signature in assistant lines →
`EventKind::LimitHit { resets: Option<String> }`; add `rate_limit_event`
to `HANDLED` with a capture-shape arm. State: fold burn into per-day,
per-repo aggregates (persisted with `#[serde(default)]`); a limit-hit
sets a session flag + timestamp. Attention: new reason tier for
limit-hit. Wire: `UsageStats` request/response pair with echo. UI: burn
table + rolling window figure in the Info/health area; estimate label.

### Test strategy

Corpus fixture with the real synthetic shape (values synthetic);
adapter unit tests for signature and reset-time extraction; aggregation
tests over folded lines; e2e for the stats endpoint.

## Notes

**The roadmap's premise died on contact with the corpus.** R-G1 was
written as "the CLI emits `rate_limit_event`; currently discarded".
Zero exist across 235 transcripts. What exists is a synthetic assistant
message (`message.model == "<synthetic>"`) whose *prose* carries the
reset time. The build keys on that signature, keeps `rate_limit_event`
in `HANDLED` as a capture-shape arm anyway (its first real appearance
should be recorded, not alarmed on), and A20 was filed `AT RISK` to
record the gap between the roadmap's belief and disk.

**A limit is not a failure and not "waiting for you".** Both neighbours
were wrong: `Failed` implies something to fix, `AwaitingInput` implies
typing helps. It got its own tier (850, between them) because several
sessions usually go dark at once and that moment should look like
exactly what it is.

**Warning without a quota.** The CLI publishes no token quota, so the
threshold is the smallest observed pre-hit five-hour burn, at hour
granularity, and every surface that shows it says "est." — the pillar-K
"no half-measures" bullet applied to numbers.

**Hour buckets, not message timestamps.** The scanner keeps per-file
day and hour aggregates plus a byte offset, so the first report pays
for the corpus (~67 MB) and later reports read only appended bytes. The
window maths is off by up to an hour at the edges, which is fine for a
labelled estimate and 300× smaller than remembering timestamps.
