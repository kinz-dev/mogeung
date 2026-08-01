---
title: Fetch is admitted; publishing is not
status: active
updated: 2026-08-01
decided: 2026-08-01
supersedes: 0012
---

# ADR-0014 — Fetch is admitted; publishing is not

## Context

[ADR-0012](0012-write-locally-never-publish.md), written 2026-07-30, admitted
local git writes and put `fetch`, `pull` and `push` on the far side of one
line: **the network**. That fence shipped as `R-D19`–`R-D22` and holds.

A day of using it found the flaw, and it is not a matter of taste. The
repository this was built in sat on a `main` that was six commits behind its
origin, with `git status` reporting nothing wrong, because the `origin/main`
ref on disk had not been updated since before the merge. mogeung's own Git pane
would have rendered **0 ahead, 0 behind** — a confident number that was simply
false, with no way in the product to make it true.

That is not a missing feature. It is a **shipped feature that lies**, and it
was made likelier by ADR-0012's own successes: committing from the pane means
visiting a terminal less often, and a terminal is where fetching happens.
`R-D23` was filed as the counterweight — render the age of the last fetch, or
read as unknown — and it is worth having either way. But it is a label on a
stale number rather than a way to refresh one, and "3 behind, as of nine days
ago" is an honest way of saying you still have to leave.

The question ADR-0012 did not ask is whether *the network* was ever the right
line, or whether it was a proxy for something else drawn in a hurry. Three
verbs were grouped because they share a transport. They do not share a risk:

| | touches the remote | changes local files | reversible |
|---|---|---|---|
| `fetch` | reads | no | nothing to reverse |
| `pull` | reads | **yes** — merges, can conflict | usually |
| `push` | **writes** | no | socially, no |

`fetch` writes remote-tracking refs under `.git` and nothing else. It cannot
conflict, cannot touch the working tree, cannot lose an edit, and cannot be
seen by anybody else. `push` is the one that publishes — the act ADR-0003's
"never surprise anyone with what the agent did" is really about — and `pull`
is a merge, which is the branch-switch hazard from `R-D21` with a network call
in front of it.

## Decision

**mogeung may `fetch`. It may not `pull` and it may not `push`.**

The line moves from *the network* to **publishing and merging**. In, on top of
everything ADR-0012 admitted:

- `fetch`, including `--prune`, on an explicit action only

Out, and still out:

- `push`, and anything else that makes this machine's work visible elsewhere
- `pull`, `rebase --onto` a remote ref, and anything else that merges. The
  reason is not the network: it is that a merge changes files underneath a
  possibly-running agent, and `R-D21` already established that we warn before
  doing that rather than doing it silently. A merge verb, if ever built, needs
  that warning and is therefore a separate decision.
- anything touching a session, a prompt, or an agent — ADR-0003 untouched and
  permanent

Three constraints travel with this and are not separable from it:

1. **Never automatic.** No poll, no fetch-on-open, no fetch-on-focus. A fetch
   reaches someone else's server, and a tool that does that on a timer is a
   tool nobody can reason about on a metered connection or a locked-down
   network. It happens when a human asks and at no other time.
2. **Never interactive.** `GIT_TERMINAL_PROMPT=0` and no stdin, so a
   credential prompt fails loudly instead of blocking a daemon thread for ever
   on a question nobody can see. A private remote that needs a passphrase must
   report that it does, not hang.
3. **The result is reported, including "nothing changed".** A sync that
   silently succeeds is indistinguishable from one that silently did nothing,
   and the whole point of this ADR is to stop the pane from being confidently
   wrong.

## Alternatives

**Keep ADR-0012 as written and ship `R-D23` alone.** The honest minimum: label
the number with its age. Rejected as *sufficient* rather than as wrong — it is
being built anyway. It converts a lie into an admission, which is better, but
it leaves the product unable to answer a question it now displays. A tool whose
answer to "am I up to date?" is "I cannot tell you, go elsewhere" has chosen
its own purity over the user's afternoon.

**Admit `pull` as well, since that is what "get latest" means.** Rejected on
the `R-D21` precedent rather than on principle. A pull merges, and a merge
changes files under agents that are reading them; `R-D21` established that
mogeung warns before doing that, using knowledge git does not have. A pull verb
that skipped the warning would contradict a decision one week old, and one that
included it is a bigger design than this. It may well be right later. It is not
the same question.

**Admit `push` too, and be a git client.** Rejected, permanently as far as this
ADR reaches. Push is the act that makes an agent's work visible to other
people, and everything in this project's first three ADRs is about not doing
things on the user's behalf that they would have wanted to see first. The
asymmetry is the point: reading someone else's server changes nothing;
writing to it cannot be taken back.

**Fetch automatically, so the number is always fresh.** Rejected under
constraint 1. It is the version that makes the feature invisible and therefore
appealing, and it is also the version that makes mogeung phone a remote from a
laptop on a train.

## Consequences

Easy: ahead/behind becomes a number worth reading, and `R-D23`'s honesty label
gets to say "as of a moment ago" rather than "as of last Tuesday". The stale
`origin/main` that prompted this becomes a one-keystroke fix.

Hard, and worth stating: **this is the first time mogeung makes an outbound
network connection at all.** Everything before it was localhost or the user's
own LAN. That is a genuine change in what the process does, it means a fetch
can be slow, hang on DNS, or fail in ways nothing else here fails, and it means
"mogeung is entirely local" stops being true as a blanket claim. The guide has
to say so.

It also puts a network call behind a keystroke, and keystrokes are cheap. A
person leaning on `Ctrl+T` is hitting a real server. Rate limiting is not
built; if that becomes a problem, it is the fix.

Ruled out: `pull` and `push` from the window, until a further ADR says
otherwise. `R-D24` keeps its number and now means those two only.

## Revisit if

Fetch turns out to be the wrong granularity — most likely because what people
actually want is "fetch and tell me if I should merge", and the answer is
always followed by a trip to the terminal anyway. If that trip happens every
single time, the merge question deserves reopening on evidence rather than on
this ADR's caution.

Also revisit if constraint 1 chafes: if the honest answer to "why is this
stale" is repeatedly "I forgot to press it", automatic fetch was the right
design and this ADR was too careful.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
