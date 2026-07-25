---
title: Every transcript line is classified; there is no catch-all
status: active
updated: 2026-07-25
decided: 2026-07-25
---

# ADR-0007 — Every transcript line is classified; there is no catch-all

## Context

mogeung reads two private, undocumented Claude Code formats
([A4](../product/assumptions.md)). The parser is deliberately forgiving —
unknown shapes are skipped rather than fatal — because a schema change must
degrade the board rather than stop the watcher
([ADR-0002](0002-structured-transcript-not-a-terminal.md)).

That posture is right, and it creates the problem. `parse_line` returned
`Option<Parsed>`, and `None` meant two unrelated things: *bookkeeping we
classified and chose to skip*, which happens thousands of times a day, and *a
type from a release we have never seen*, which is the single thing we most need
to know. A `_ => None` arm made them identical.

The cost was not hypothetical. Three event types — `queue-operation` (190
occurrences in the corpus), `pr-link` and `frame-link` — were discarded silently
throughout v0.2. They were documented nowhere and nobody could have discovered
them, because nothing recorded that a line had been thrown away.

This matters more here than in most parsers. mogeung's job is to answer "is
anything waiting for me?". The failure mode of a forgiving parser is a board
that is *quietly incomplete*, and an incomplete board looks exactly like a quiet
afternoon. The tool fails by being reassuring.

## Decision

**Every transcript `type` is named in exactly one of two lists**,
`adapter::HANDLED` or `adapter::KNOWN_IGNORED`, and anything else raises an
alert naming the type.

`parse_line` returns a five-way `LineOutcome` — parsed, ignored, barren,
unknown, malformed — never a bare `Option`. Adding a type to `KNOWN_IGNORED` is
a deliberate, reviewable act carrying a comment that says why it holds nothing
we need.

A committed golden corpus asserts that no shape observed in real transcripts
classifies as unknown or malformed.

## Alternatives

**Keep `Option`, log unknown types at `warn!`.** Cheapest, and it was the
obvious move. Rejected because a daemon's log is not somewhere anyone looks —
the whole product exists because *you should not have to go and check*. A signal
that requires you to already suspect a problem is not a canary.

**Alert on a ratio of unparsed lines rather than on individual types.** Rejected
because it inverts the economics. A new event type may be rare and still carry
the thing that matters; a threshold guarantees the first occurrences are
swallowed, which is precisely the window in which a format change is worth
catching.

**Parse defensively — try to extract common fields from any object.** Rejected
as guessing. It would produce plausible-looking data from shapes nobody has
examined, which is worse than a gap, because a gap is visible.

**Do nothing until a format change actually bites.** Rejected because the
failure is silent by construction, so "it bit us" is not an event that occurs.
You discover it weeks later, having trusted an incomplete board the whole time.

## Consequences

**Good.** A format change is detectable on the first line. The classification
lists double as documentation of the format — the table in
[claude-code-formats.md](../design/claude-code-formats.md) is now exhaustive
rather than illustrative. Writing it down immediately found three types we did
not know existed.

**Bad — this list is a snapshot of one machine.** Another user, or a newer CLI,
will hit types we have never seen, and their first run may show alerts for
things that are entirely benign. The alert text has to make "mogeung does not
understand this" sound like what it is rather than like a fault, or we have
built a nag.

**Bad — it is maintenance.** Every Claude Code release can add a type, and each
one needs a human decision and a comment. That is the price of the guarantee;
the alternative is the silence we just spent a feature removing.

**Ruled out:** any future `_ => ignore` arm in the transcript parser. If that
appears, this ADR has been abandoned.

## Revisit if

- Claude Code publishes a documented, versioned transcript schema — then the
  lists become redundant and the schema version is the canary.
- Unknown-type alerts prove noisy in practice across other machines. The fix
  would be to alert once per type rather than to stop classifying; if even that
  is too much, the decision to revisit is *how we notify*, not *whether we know*.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
