---
title: Observe sessions, never spawn them
status: active
updated: 2026-07-25
decided: 2026-07-25
supersedes: The v0.1 spawning model
---

# ADR-0003 — Observe sessions, never spawn them

**This is the most important decision in the project.**

## Context

v0.1 spawned agents: the user typed an intent into mogeung, which ran
`claude -p` in a git worktree and presented the result. It was built, tested,
documented and committed.

On first real use the verdict was: *"a handicapped Claude Code with a single
session."*

## The failure, precisely

1. **The attention queue is worth zero at N=1.** A ranked list of one item is a
   label. The entire product only pays at three or four concurrent sessions.
2. **To feed that queue, v0.1 removed the interactive loop** — no steering, no
   permission prompts, no plan mode, no slash commands. Each individual session
   became *worse* than simply running `claude`.

Together: strictly worse than a terminal until N≥3, while making N≥3 awkward to
reach. The genuinely novel part was the review layer, and v0.1 gated it behind a
worse front-end for something that already has a good one.

The root cause was not a bad plan. It was an **unexamined assumption** — that
populating the queue required spawning — which was never written down and so was
never reviewable. See [assumptions.md A2](../product/assumptions.md).

## Decision

**mogeung observes. It never starts, steers or stops an agent.**

It reads what Claude Code already writes for itself:

- `~/.claude/sessions/<pid>.json` — live registry with first-party `busy`/`idle`
- `~/.claude/projects/<slug>/<id>.jsonl` — the transcript

The single exception: mogeung may open a **real interactive `claude` in a
terminal**, optionally in a fresh worktree. That wraps nothing and addresses the
"reaching N≥3 is awkward" half of the failure.

## Consequences

- Purely additive: it cannot degrade a session, because it does not touch one.
- **"Waiting for you" became a fact rather than an inference.** v0.1 could only
  detect blockage after the fact from permission denials; the live registry
  publishes it directly. The largest documented gap in v0.1 closed for free.
- New dependency on two undocumented file formats — now the top operational risk
  ([A4](../product/assumptions.md)). See
  [claude-code-formats.md](../design/claude-code-formats.md).
- Deleted: the supervisor, permission modes, model selection, follow-ups,
  cancel, the New Run dialog.
- Kept unchanged: the git diff engine, risk scoring, hunk anchoring, review
  checkpointing, the daemon/client split.
- A whole category became possible: the transcript corpus is queryable
  (roadmap section F).

## The durable lesson

Owning the conversation loop was never a *requirement* for the review and
attention layer. It was an assumption — and it was the expensive kind, because
it cost the entire product. [assumptions.md](../product/assumptions.md) exists
to catch the next one.
