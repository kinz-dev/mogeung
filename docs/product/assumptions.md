---
title: Assumption ledger
status: active
updated: 2026-07-25
---

# Assumption ledger

Every belief the product rests on, and whether we have actually checked it.

**This file exists because of a specific failure.** v0.1 was built and thrown
away not because the plan was bad, but because an assumption underneath it — *"we
must spawn agents in order to populate the queue"* — was never written down, and
so was never reviewable. It survived a whole design document, an implementation,
and a commit before contact with reality killed it in one sentence.

## Status values

| Status | Meaning |
|---|---|
| `UNTESTED` | We believe it. We have no evidence. **Build carefully.** |
| `SUPPORTED` | Evidence exists and points our way |
| `AT RISK` | Evidence suggests it may not hold |
| `REFUTED` | Known false. Linked to the decision that responded |

## The rule

No feature spec may be written without a `depends_on:` naming its assumptions.

**If a spec depends on an `UNTESTED` assumption, the work is to test the
assumption — not to build the feature.**

## Ledger

| # | Assumption | Status | Evidence | Resolution |
|---|---|---|---|---|
| A1 | A cross-session attention queue changes where the user looks | `UNTESTED` | Never used in anger. v0.1 died before reaching the question | — |
| A2 | mogeung must spawn agents to populate the queue | `REFUTED` | v0.1 use, 2026-07-25: "a handicapped Claude Code with a single session" | [ADR-0003](../decisions/0003-observe-do-not-spawn.md) |
| A3 | Keyword heuristics over diff text are good enough for reading order | `UNTESTED` | Ranked `auth.rs` above a lockfile once, in a test | — |
| A4 | Claude Code's on-disk formats are stable enough to depend on | `AT RISK` | Undocumented private files. Verified against 2.1.219/2.1.220 only | Canary planned (roadmap `R-A1`) |
| A5 | Content-hash hunk anchors keep review marks stable across rewrites | `SUPPORTED` | Verified live: `auth.rs` stayed read while a rewritten `main.rs` came back unread | — |
| A6 | The user will run 3–4 concurrent sessions in normal work | `UNTESTED` | The whole product depends on this. Never measured | — |
| A7 | Reviewing agent output is a distinct activity worth its own tool | `UNTESTED` | Stated in [concept.md](concept.md) §1, never validated | — |
| A8 | Per-session diff attribution by edited files is accurate enough | `AT RISK` | Cannot separate two sessions editing the same file | — |
| A9 | Git is the right diff base for observed sessions | `SUPPORTED` | Works, but the base is HEAD-when-first-seen; sessions predating mogeung diff meaninglessly | — |
| A10 | Doc sprawl is a real and painful problem worth tooling | `UNTESTED` | Stated as the opening complaint; two versions shipped without touching it | — |

## Notes on the most dangerous ones

**A1 and A6 are the product.** If either is false, mogeung has no reason to
exist, and no amount of polish on the review layer compensates. They are also
the cheapest to test: use it for a week. Everything on the roadmap is
speculation until they are resolved.

**A4 is the operational risk.** Everything rests on two undocumented file
layouts. The parser degrades rather than crashing, so the realistic failure is
mogeung quietly seeing *less* than it should — the worst kind, because it looks
like "nothing is happening" rather than an error.

**A10 deserves scrutiny.** It was the opening complaint and remains untouched
after two versions. Either it matters and we have been avoiding it, or it
mattered less than stated. Worth deciding honestly rather than drifting.
