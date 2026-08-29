---
title: <decision, stated as a claim>
status: active
updated: <YYYY-MM-DD>
decided: <YYYY-MM-DD>
---

# ADR-<NNNN> — <decision, stated as a claim>

## Context
The situation forcing a choice. What is true that makes this a real decision
rather than an obvious one.

## Decision
What we chose, in the active voice.

## Alternatives
What else was considered, and the specific reason each lost. An ADR with no
alternatives is not recording a decision.

## Consequences
What this makes easy, what it makes hard, and what it rules out. Include the
bad parts — this section is why the ADR is worth keeping.

## Revisit if
The condition under which this should be reconsidered.

## Amendment — <YYYY-MM-DD>
*Only when there is one. Delete this heading otherwise.*

What changed, in a sentence. Then: the clause it replaces quoted or named, the
reason, and the fences the change comes with. Never edit the text above — a
reader has to be able to see what was believed before, or the document stops
being evidence.

---
*ADRs are immutable. A decision that is **narrowed** changes by an
`## Amendment — YYYY-MM-DD` section appended here, with `updated:` bumped. A
decision genuinely **reversed** is superseded: write a new ADR and set
`status: superseded` plus `superseded_by:` here.*
