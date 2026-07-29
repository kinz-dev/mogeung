---
title: Verification — claims against evidence
status: in-progress
updated: 2026-07-29
roadmap: [R-E1, R-E2, R-E3, R-E4, R-E5]
depends_on: [A4, A7, A21]
---

# 0016 — Verification

Pillar E, built at the 2026-07-29 one-go ask. The observer pivot made
this cheap: the full transcript is on disk, so "did it actually run the
tests" is a read, not an interrogation.

## Spec

### Problem

An agent says "all tests pass" and the only way to know is to scroll
its transcript looking for a Bash call. Sessions end having edited code
and verified nothing, and that fact is invisible in the queue. The
trust layer that made concept.md interesting has no smallest slice.

### Assumptions

A21 (`SUPPORTED`) — the evidence is already parsed. A7 (`UNTESTED`) —
whether surfacing it changes behaviour is exactly what the dogfooding
week measures. A4 (`AT RISK`) as everywhere.

### Acceptance

- [x] A session that edited files and never ran a build/test/typecheck
      command wears an **unverified** mark in the queue and detail
      header (R-E4)
- [x] The detail view lists verification evidence: each build/test
      command the session ran, when, and whether its result looked like
      success or failure (R-E1)
- [x] Assistant claims of the "tests pass / build succeeds" family are
      extracted and each is bound to the nearest matching evidence — or
      visibly to none; the claim list never guesses beyond its
      heuristic and says how it matched (R-E3)
- [x] A per-repo signal command (e.g. `cargo test`) can be configured
      and run **by explicit click only** — never automatically — with
      the result attached to the session; the fence: mogeung runs
      checks, it never runs agents (R-E2, ADR-0003 respected)
- [x] When a configured coverage command emits lcov, the changed lines
      of the session's diff show covered/uncovered counts; absent
      coverage data the feature says "no data", never a made-up number
      (R-E5)

### Explicitly out of scope

- Automatic runs on file change (a watcher that runs code is a step
  toward acting; explicit click only).
- Semantic claim understanding — the ledger is honest keyword+shape
  heuristics, labelled as such (the pillar-K "no half-measures" bullet
  applies to presentation, so present it as matching, not judgment).

## Plan

### Approach

Adapter: classify Bash `tool_use` commands into verify kinds
(build/test/typecheck/coverage) by honest patterns; pair with results
via existing open-tool pairing; fold into `Session.verify_evidence`.
Claim extraction over assistant text with a small pattern set. R-E2/E5:
daemon `signal_runner` module — per-repo command config in the store,
spawn_blocking, capture exit+tail, lcov parse for E5; wire pair +
REST twin; UI: evidence panel in Info tab, unverified badge in queue
row, run button.

### Test strategy

Adapter unit tests (command classification, claim patterns, pairing);
corpus fixtures for claim lines; runner tests with a fake command
(`sh -c 'exit 1'`); lcov parser unit tests; e2e for endpoints.

## Notes

**The exit code was already there.** A Bash `tool_result` carries
`is_error`, which is the exit status one layer up — pairing it with the
`tool_use` id made "did the tests actually run, and how did they end"
a fold-time fact with no new parsing.

**`contradicted` earns the pillar.** Binding claims to runs mostly
produces confirmations; the case that matters is prose saying "all
tests pass" over a run that exited nonzero. Pinned by an integration
test, because that is the exact lie the trust layer exists to catch.

**A failed check is not "unverified".** R-E4 initially read as "no
passing check"; that is wrong — a session that ran the tests and saw
them fail *did* verify, and the mark must not shame it. `unverified()`
asks only whether any check completed.

**Coverage refuses to invent zeros.** A changed line with no `DA`
record is unknown (uninstrumentable or uninstrumented), not a miss;
a file with no lcov record reports nothing rather than 0%. "No data"
is rendered in words.

**Sidechain prose is excluded from claims**, same reasoning as
`last_activity`: a subagent's "tests pass" is about its subtask.
