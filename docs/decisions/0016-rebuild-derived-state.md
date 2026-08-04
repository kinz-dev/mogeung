---
title: Derived state is rebuilt from its source, never patched
status: active
updated: 2026-08-02
decided: 2026-08-02
---

# ADR-0016 — Derived state is rebuilt from its source, never patched

## Context

The daemon's database holds two kinds of thing, and until now nothing said so
out loud:

- **Derived** — sessions, their counters, and the transcript events. Every byte
  of it is a fold over `~/.claude` and git, and both of those are still on disk.
- **Original** — the user's own writing: notes, and which hunks they have marked
  read. [ADR-0015](0015-markdown-is-the-truth.md) already treats notes as the
  one thing here that cannot be recomputed.

A bug reported 2026-08-02 turned that distinction from bookkeeping into a
problem. The tailer's byte offsets lived only in the process, while the sessions
they belonged to were persisted. So a restart found every session already known
and every transcript unread, folded each file in again from byte 0, and appended
the whole history a second time under fresh `seq`s — which collide with nothing,
so the primary key never complained. The user's database had grown to 793 MB and
held **1.29 million events of which 16 thousand were real**: single events stored
up to 538 times, transcripts that replayed the same conversation on screen once
per restart, and `turns`/`tool_calls`/`tokens_out` counted once per restart too.

Fixing it forward is not interesting — the offsets are persisted, the tailer is
seeded from them, and `R-A6` has tests that fail without it. The decision is what
to do about the databases already carrying the damage, because the two halves of
the damage are not equally repairable:

| | can be repaired in place? |
|---|---|
| the event log | yes, by collapsing rows that differ only in `seq` |
| `turns`, `tool_calls`, `tokens_out` | **no** — they are sums, and nothing recorded how many times each was added |

## Decision

**When derived state is wrong, delete it and fold the source in again. Do not
attempt to correct it in place.**

The repair drops every event for a session, zeroes every counter the fold
produces, and re-reads the transcript from the beginning under the same size cap
a first sighting gets. It runs once, gated on `PRAGMA user_version`, before the
first scan. What it does not touch is anything original: notes, reviewed hunks,
signal commands, the pinned diff base.

The in-place collapse survives only as the last resort for a session whose
transcript is **gone**, and it is labelled as the guess it is: two events with
the same timestamp and the same kind are usually a duplicate and occasionally
two real prompts. On the corpus this was validated against, every one of 113
sessions still had its file, so the guess was never needed.

## Alternatives

**Divide the counters by the duplication factor.** Tempting, because the factor
looks knowable — count the copies, divide. It is wrong: files grow between
restarts, so early events were copied 538 times and late ones 89. There is no
single divisor, and a per-event one still cannot repair a sum. This is inventing
a number that looks like a measurement, which is exactly what
[ADR-0005](0005-tokens-not-dollars.md) refuses elsewhere.

**Collapse the event rows and leave the counters.** Cheap, and it fixes the
symptom the user actually sees — the transcript. Rejected because the board
would keep ranking on counters inflated ninetyfold while the transcript beside
them looked right, which is worse than either being wrong on its own.

**Tell the user to delete the database.** Correct, and what the author would have
done. Rejected as advice to give a stranger: it also deletes their notes and
every hunk they have marked read, and it loses the sessions whose transcripts
have aged past the 14-day window. A repair keeps all three.

**Make ingest idempotent instead — derive `seq` from the line's position in the
file, so a re-read overwrites rather than appends.** The stronger fix, and the
one that would have made this class of bug impossible. Rejected *for now* because
notes anchor to `(session_id, seq)`: changing what a `seq` means detaches the one
kind of data in here that cannot be recomputed. Worth revisiting behind a note
migration.

## Consequences

Easy: any future fold bug has a stated remedy that is one function, and the
database stops being something to be careful with. On the reported corpus the
repair removed 1,294,810 rows and took the file from 793 MB to 10.7 MB.

Hard, and worth stating: the repair is **destructive by design** and runs
automatically at startup. It deletes real rows on the strength of the claim that
they can be rebuilt — a claim that is only true because mogeung never writes to
`~/.claude` and can therefore trust it to still be there. It also re-reads the
whole corpus once, which is the cost of a first run, paid again.

`VACUUM` is part of it. Without it the freed pages stay in the file and a user
who ran the repair would see a 793 MB database and conclude nothing happened.

Ruled out: a migration that edits a derived value. If a value is wrong and it was
derived, the fold that produced it is what gets run again.

## Revisit if

A derived field appears that is genuinely expensive to rebuild — a fold over
something remote, or one that cannot be re-read at all. That is the case this
decision does not cover, and it would need its own answer rather than a strained
reading of this one.

Also revisit if position-derived `seq` becomes affordable. If notes can be
migrated to an anchor that does not depend on ingest order, idempotent ingest
makes the repair unnecessary rather than routine.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
