---
title: mogeung writes to the repository, and never publishes
status: superseded
updated: 2026-08-01
superseded_by: 0014
decided: 2026-07-30
---

# ADR-0012 — mogeung writes to the repository, and never publishes

## Context

The Git pane reached reading parity with a commercial client
([feature 0011](../features/0011-git-depth.md)) and stopped dead at the write
half. Five documents say so, and one of them says how to change it:

> **Any write operation** — staging, committing, reverting, branching.
> Permanently, unless a future ADR argues otherwise.
> — [feature 0010](../features/0010-git-view.md)

> Scope creep toward a git client is the real danger; the out-of-scope list is
> the fence, and every "just add staging" impulse goes through an ADR or dies.
> — same file

[A19](../product/assumptions.md) predicted the moment: *"the read-only fence is
restated in the spec precisely because this is where 'just add staging'
pressure will arrive."* It arrived on 2026-07-30, asked directly — *"Do we
support git commit and push in the GIT UI now? if not, can we plan for this"*.

This is a real decision and not an obvious one, because the read-only claim is
load-bearing in **three separate ways** that have historically been treated as
one, and they turn out to have different answers.

**1. Product identity.** [ADR-0003](0003-observe-do-not-spawn.md) records why
v0.1 was thrown away: it owned the conversation loop, and a supervision layer
that inserts itself between you and the agent has to earn back what it takes
away. [ADR-0008](0008-build-the-prompt-never-send-it.md) then generalised the
guard — *"'just paste it for me' is one keystroke from 'just send it'"*.

**2. Protocol safety.** The daemon takes commands from a socket where `--token`
is optional and there is no TLS (`server.rs`). [A24](../product/assumptions.md)
is `UNTESTED` and its wording is exact: *"a **read-only** daemon is safe to
reach over a trusted network with a shared token, without TLS"*. The word
read-only is doing the work in that sentence.

**3. Scope discipline.** A bright line needs no judgement. "The pane never
writes" is checkable by grep; "the pane writes only sensible things" is an
argument to be had every time.

The fence conflated (1) with a proxy for it. In v0.1 they genuinely were the
same thing, because everything mogeung did went through the agent. They are not
the same thing now: **committing a diff you have just finished reading does not
re-acquire the conversation loop.** No verb below touches a session, a prompt,
or a model.

The fence is also already not literally true. `state.rs` runs `git worktree
add` when a session is launched with isolation. The rule the code actually
follows is *no writes from the pane, only from an explicit user action* — which
is narrower, more defensible, and worth saying out loud rather than discovering
later.

## Decision

**mogeung may write the working tree and the local repository. It may not talk
to a remote.**

The line is **the network**, not the repository. In:

- stage, unstage, discard
- commit, amend
- branch create, switch
- stash push, pop, drop
- conflict resolution — choose a side, mark resolved

Out, and still out:

- `fetch`, `pull`, `push`, and anything else that reaches a remote
- anything touching a session, a prompt, or an agent — ADR-0003 is untouched
  and permanent

Two constraints travel with the decision and are not separable from it:

**Write verbs require an authenticated caller.** A loopback bind is
authenticated by the operating system; a non-loopback bind requires `--token`.
A write verb arriving on an unauthenticated non-loopback socket is refused, not
warned about. This keeps [A24](../product/assumptions.md)'s premise alive rather
than silently voiding it.

**Writes fail loudly.** The parsing rule — *degrade, never panic* — is correct
for reads because a read that half-works still shows you something true. It is
exactly wrong for writes. A `git commit` that half-works must surface git's own
stderr verbatim and change nothing in the UI's model of the repo.

## Alternatives

**Compose the command, never run it** — build `git add … && git commit -m …`
from the selection and hand it to the shell tab unexecuted, ADR-0008's shape
one layer down. This was the recommendation: it needs no new daemon verb, keeps
A24 untouched, and gets `push` for the same price because it is all just text.
It lost on ergonomics — staging eight of twelve changed files through a command
line you must read before running is worse than eight checkboxes, and a second
syntax between you and git is a cost paid on every commit. **It remains the
fallback**, and it is what we do if A26 fails.

**The full client, including push** — lost to A24 alone. A read-only daemon on
an optional plaintext token is a bet already placed. A publish-capable one is a
different bet and nobody has placed it. `push` is also the one verb in the set
whose consequences leave the machine and cannot be undone from inside git.

**Keep the fence** — lost because the pane already renders staged and unstaged
state it is forbidden to act on. The workflow that produces is *read here,
retype there*, which is the hole the pane was built to close, one level down.

## Consequences

**Easier.** Review and commit without leaving the pane. A commit composed from
a session's diff can carry a trailer naming that session, which is the concrete
stepping stone toward `R-F2` prompt-blame that feature 0010 promised and could
not reach.

**Harder, and this is the real cost.** The bright line is gone, replaced by a
reasoned one, and reasoned lines are weaker under pressure. The next request
will be `push`, and it will be argued on the same shape of reasoning that won
here. The mitigation is that this ADR names the network as the line rather than
naming a list of verbs — a list invites additions, a principle can be pointed at.

**A new class of test.** Every verb so far could be tested against read-only
fixtures. Writes need temp repositories built and torn down per test, and
`discard` needs one that proves it cannot escape the session root. This is a
standing cost on every future git feature, not a one-off.

**One genuinely unrecoverable verb.** Everything here is undoable with git
itself — reflog covers commit, amend, switch and stash — except `discard`,
which destroys uncommitted work that git never saw. It is the only verb that
requires confirmation, and the only one where a bug loses data rather than
time.

**One document becomes false.** [concept.md](../product/concept.md) says *"Not
a replacement for git. It reads git; it never writes to your repo."* The first
sentence survives; the second does not.

## Revisit if

- **A26 fails** — the verbs go unused for a dogfooding week because the shell
  tab is right there and faster. Then this was a large cost for nothing, and
  the composed-command alternative above is the correct build.
- **A write verb corrupts or loses work.** One incident is enough; the fence
  existed partly because a read-only tool cannot do this.
- **`push` is asked for.** Not a reason to reverse this ADR, but the reason it
  names a principle rather than a list. That request needs its own ADR, and it
  needs A24 resolved first, not assumed.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
