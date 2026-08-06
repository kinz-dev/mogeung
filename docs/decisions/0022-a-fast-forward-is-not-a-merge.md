---
title: A fast-forward is not a merge; pushing is still publishing
status: proposed
updated: 2026-08-07
decided: —
---

# ADR-0022 — A fast-forward is not a merge; pushing is still publishing

> **`status: proposed`.** This is the ADR `R-D24` says it needs, written so the
> decision can be taken rather than taken *by* an implementation. Nothing in it
> has shipped. ADR-0014 refused `push` "permanently as far as this ADR reaches",
> and overturning a sentence like that is the repository owner's call, not a
> side effect of a backlog sweep.

## Context

[ADR-0012](0012-write-locally-never-publish.md) drew the line at the network.
[ADR-0014](0014-fetch-is-not-publishing.md) moved it to **publishing and
merging**, admitted `fetch`, and left `push` and `pull` out with different
reasons for each:

- **`push`** — "the act that makes an agent's work visible to other people".
  Refused on principle, and the principle is ADR-0003's.
- **`pull`** — refused *on the `R-D21` precedent*, because a pull merges, and a
  merge "changes files underneath a possibly-running agent". ADR-0014 says of
  it, in as many words: *"It may well be right later. It is not the same
  question."*

That leaves `R-D24` naming two verbs that were refused for reasons which are
not the same strength. One is a value; the other is a hazard with a known
mitigation. Treating them as a pair is what has kept the row stuck.

Two things have changed since ADR-0014, and one has not.

**Changed:** `R-D25` shipped `fetch` on 2026-08-01 and it was dogfooded on
2026-08-03 without incident, so the outbound-network machinery — the
`GIT_TERMINAL_PROMPT=0` non-interactivity, the always-report rule, the
explicit-action-only rule — exists and has been exercised. And `R-D21`'s
running-agent warning exists too, which is the mitigation ADR-0014 said a merge
verb would need.

**Not changed:** [A24](../product/assumptions.md) is still `UNTESTED`. Its
sentence is *"a **read-only** daemon is safe to reach over a trusted network
with a shared token, without TLS"*, and the word doing the work is *read-only*.
ADR-0012 already noticed the coupling and guarded it — write verbs are refused
unless the bind is loopback or a token was presented. `push` is a different
order of the same problem: every write admitted so far changes files on **this**
machine, where the worst case is a bad commit you can reset. A push admitted
over the same socket changes a **shared** remote, where the worst case is
someone else's afternoon and there is nothing to reset. The token that guards a
local commit is being asked to guard publication.

CLAUDE.md's own rule follows from that: *if an assumption is `UNTESTED`, the
work is to test it — not to build the feature.*

## Decision

**Split the row. `pull --ff-only` is admissible; `push` is not, and not yet.**

- **`pull` is admitted only in its non-merging form** — `git pull --ff-only`,
  which either moves the branch pointer or refuses. ADR-0014's objection was to
  *merging*, not to the word "pull": a fast-forward cannot conflict, cannot
  produce a merge commit, and cannot leave the worktree half-resolved. It is
  `fetch` followed by a pointer move, and both halves are already decided.
  It carries `R-D21`'s running-agent warning anyway, because the files do change
  underneath whatever is reading them, and it reports what moved — including
  "nothing", per ADR-0014's third constraint.
- **A merging `pull` stays refused.** If `--ff-only` fails, the answer is the
  refusal and the reason, never a fallback to a real merge. A verb that
  silently escalates from safe to unsafe when the safe form does not apply is
  worse than no verb.
- **`push` stays refused**, and the condition for reopening it is written down
  below rather than left to the next person who wants it.

## Alternatives

**Admit both, and be a git client.** Rejected for the reason ADR-0014 gave and
this ADR has no new evidence against: the asymmetry between reading someone
else's server and writing to it is the whole of pillar K's line. Nothing in a
day of using `fetch` speaks to whether publishing is safe.

**Admit `push` behind a confirmation dialog.** Rejected, and it is the tempting
one. A dialog converts a policy into a habit — the third time you see it you
stop reading it — and it does nothing at all about the remote-daemon case,
where the guard has to hold for a client that is not the person who would read
the dialog. If push is ever admitted it needs A24 answered, not a modal.

**Keep both refused and close `R-D24` as "explicitly not".** Rejected because it
overstates what has been decided. ADR-0014 itself flagged `pull` as a separate
question that might be right later; retiring the row would file that as settled
when it is not.

**Ship `pull --ff-only` without an ADR, since ADR-0014 only refused merging.**
Rejected as the reading that gets projects into trouble. ADR-0014's *decision*
section says "It may not `pull`" without qualification; the qualification lives
in the reasoning. When the sentence and the reasoning disagree, a new ADR is
the cheap way to find out which one the author meant.

## Consequences

Easy: "get me up to date" becomes a keystroke for the overwhelmingly common
case — a branch that is purely behind — and the stale-`origin/main` failure that
prompted ADR-0014 becomes fixable rather than merely visible.

Hard: **two verbs that look identical to a user now behave differently.** A
`pull` that refuses because you have local commits will read as a bug until the
refusal explains itself, and that message is most of the work. There is also a
new failure the product has never had: a fast-forward moves files under a
running agent, and `R-D21`'s warning is a warning, not a lock.

Ruled out: any automatic form of either verb, permanently, under ADR-0014's
first constraint.

## Revisit if

**For `push`:** A24 is resolved — either settled `SUPPORTED` with the
read-only qualifier removed deliberately, or replaced by an assumption that
speaks to a writing daemon. Until then the row stays open and this stays the
answer.

**For a merging `pull`:** someone is refused by `--ff-only` often enough to be
annoyed, which would be evidence that diverged branches are the normal case
here rather than the exception, and would make the warning worth designing.
