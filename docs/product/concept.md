---
title: Concept
status: active
updated: 2026-10-08
---

# mogeung

**A supervision layer for people whose job became watching agents.**

This document is the *why* and the *shape*. It deliberately does not list
features ([roadmap.md](roadmap.md)), explain how the code works
([../design/](../design/)), or justify past choices ([../decisions/](../decisions/)).
When those overlap, they win — a concept doc that duplicates them becomes a
stale copy of everything.

The original pre-implementation thesis is preserved at
[../archive/2026-07-25-original-concept.md](../archive/2026-07-25-original-concept.md).
It is worth reading for what it got wrong.

---

## 1. The problem

An IDE is built around one assumption: **a human types code into files, one
character at a time.** Completion, refactoring, inline errors, go-to-definition
— all of it optimises that loop. The file tree is the root object; the cursor is
the centre of the world.

That is now a fraction of the work. The rest looks like this:

- prompting and steering several agents at once, across repos and worktrees
- reading large volumes of code you did not write, arriving faster than you can
  keep up with
- deciding what to test, and checking whether what an agent *claimed* matches
  what it *did*
- holding an architecture in your head while it is modified underneath you

The bottleneck moved from **writing** to **reviewing, verifying and
remembering**. No tool owns that.

Bolting a chat panel onto a file editor does not fix it, because the file is
still at the centre. The centre moved.

## 2. The thesis

Three claims. Each is falsifiable, and each has an entry in
[assumptions.md](assumptions.md).

**I. The root object is the change, not the file.**
Files are a projection of accumulated changes. What you reason about is: what
moved, is it safe, have I read it.

**II. Attention is the scarce resource.**
With one agent you need no tool. With four, your bottleneck is not reading code
— it is *knowing which of the four to look at*. Everything else is downstream of
answering that. ([A1](assumptions.md), [A6](assumptions.md))

**III. Supervision must be additive.**
Any tool that inserts itself between you and the agent must earn back what it
takes away. A layer that observes takes away nothing, so it cannot lose that
trade. This was learned expensively —
[ADR-0003](../decisions/0003-observe-do-not-spawn.md).

A fourth claim, inherited and still unproven:

**IV. Derived beats written.**
A hand-maintained `PROGRESS.md` is prose frozen at a moment, so it rots. State
computed from diffs, tests and commits cannot lie and never needs garbage
collecting. We apply this to ourselves — `STATUS.md` is generated — before
proposing it to anyone else. ([A10](assumptions.md))

## 3. What mogeung is

A daemon that watches the agent sessions you run in your own terminals —
Claude Code, Codex, Qwen Code and opencode — and a window that shows you two
things:

**One queue across every session, ranked by who needs you.** Blocked on an
approval, waiting on you, failed, rate-limited, unreviewed, stalled, running. Ranking is strict — a session can never
jump into a more urgent tier — and every row states why it sits where it does.
([../design/attention-ranking.md](../design/attention-ranking.md))

**A diff per session that remembers what you have read.** Risk-ordered rather
than alphabetical, and anchored by content hash so that when an agent rewrites a
file, only genuinely new work comes back unread.
([../design/review-checkpointing.md](../design/review-checkpointing.md))

It writes nothing to `~/.claude`. It starts no agent: it may launch the real CLI
unmodified in a terminal, run a process the repository itself names
([ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md)), and —
on a click and a confirmation — press send for you, once
([ADR-0003](../decisions/0003-observe-do-not-spawn.md), amended 2026-08-29).

## 4. Principles

**Observe, never wrap.** No spawning, and no steering in the sense
[ADR-0003](../decisions/0003-observe-do-not-spawn.md) defines it — being in the
path between you and an agent, or between an agent's output and its own next
input. The exceptions are the real CLI launched unmodified, and a one-shot send
by your hand.

**The daemon is the product.** Every UI is a client with no local authority.
mogeung keeps working with no window open, and a second client is a packaging
decision rather than a rewrite.

**Ranking is never a black box.** Every queue position carries a plain-language
reason. A heuristic you cannot interrogate is one you will stop trusting.

**Heuristics must admit what they are.** Risk scoring is keyword matching over
diff text. It is a *reading order*, never a safety guarantee, and the UI must
not imply otherwise.

**Degrade, never crash.** Everything rests on undocumented file formats that
change without warning. The parser ignores what it does not recognise — which
makes the dangerous failure "quietly seeing less", so that must be made loud.

**Assumptions are written down before they are built on.** The whole first
implementation was lost to one that was not. ([assumptions.md](assumptions.md))

## 5. Non-goals

- **Not an editor.** No language intelligence, no refactoring engine, no
  debugger. "Open in IntelliJ" is a first-class action, permanently.
- **Not an agent.** It never acts. A local model you point it at may read the
  evidence and answer you, in a column of its own that never becomes evidence
  or a tier ([ADR-0031](../decisions/0031-consent-to-a-named-host.md)); none is
  required.
- **Not a chat client.** You already have one, and it is better than anything we
  would build.
- **Not multi-user or cloud.** Single developer, local-first. It reaches
  machines you own over ssh or a token, and follows their daemons into one queue
  ([feature 0047](../archive/features/0047-daemon-federation/spec.md)); never a
  cloud, never a second person.
- **Not a replacement for git.** It reads git, and may stage, commit, branch,
  stash, resolve and `fetch` **locally** — ADR-0012 (2026-07-30) admitted the
  writes and [ADR-0014](../decisions/0014-fetch-is-not-publishing.md), which
  superseded it, moved the line from the network to publishing and merging,
  because fetching reads someone else's server and changes nothing there.
  **No pull, no push.**

## 6. What success and failure look like

**Success**, concretely: for a week, with three or four terminals open, you stop
opening tabs to check on agents — the queue tells you. And you stop re-reading
code you already reviewed.

**Failure modes, in order of likelihood:**

1. **You never run three sessions at once.** Then the queue has nothing to rank
   and the product has no reason to exist. ([A6](assumptions.md))
2. **The queue is accurate but you ignore it**, because glancing at terminals is
   already good enough. ([A1](assumptions.md))
3. **A Claude Code update changes the file formats** and mogeung quietly goes
   blind. ([A4](assumptions.md))
4. **Risk ordering is noise**, so you read alphabetically anyway.
   ([A3](assumptions.md))

**The first two were tested.** A week of use, reported 2026-08-04, carried
70–80% of the interaction with agents, so A1 and A6 are `SUPPORTED`; what is
still unmeasured is whether the *ranking* is right, as opposed to the queue
existing. Only the third is an engineering problem, and the canary exists to
make it loud.

## 7. Where the rest lives

| | |
|---|---|
| [roadmap.md](roadmap.md) | What could be built next, ranked |
| [assumptions.md](assumptions.md) | What we believe and have not checked |
| [../decisions/](../decisions/) | Why things are as they are |
| [../design/](../design/) | How it works today |
| [../guide/](../guide/) | How to use it |
