---
title: Domain docs for the engineering skills
status: active
updated: 2026-08-24
---

# Domain docs

How the engineering skills consume this repo's domain documentation. This is a
single-context repo: one product, one vocabulary, three crates and a client.

## Before exploring, read these

- `docs/design/context.md` — the glossary, if it exists.
- [`docs/decisions/`](../decisions/0003-observe-do-not-spawn.md) — the ADRs.
  Read the ones touching the area you are about to work in. **They live in
  `decisions/`, not `docs/adr/`**, and they are immutable: supersede, never
  edit.
- `docs/design/` — how the system works today.
- [`docs/product/assumptions.md`](../product/assumptions.md) — what we believe
  and have not checked. Nothing here is designed without it.

If `context.md` does not exist, **proceed silently.** Do not flag its absence
and do not create it upfront; `/domain-modeling` writes it when a term
actually gets resolved. When it is written it goes in `docs/design/`, carries
`title:`/`status:`/`updated:` frontmatter, and needs a truthful `covers:` list
naming the modules that define the vocabulary — an invented one to silence the
staleness check is worse than the warning it hides.

**Never put a `CONTEXT.md` at the repository root.** `check-docs.sh` fails on
it, and [`docs/README.md`](../README.md) calls that the one rule that matters.

## Use the glossary's vocabulary

When your output names a domain concept — a roadmap row, a refactor proposal,
a hypothesis, a test name — use the term as the glossary defines it. Do not
drift to synonyms it avoids. If the concept is not there yet, that is a
signal: either you are inventing language the project does not use, or there
is a real gap worth noting for `/domain-modeling`.

## Flag ADR conflicts

If your output contradicts an ADR, say so rather than silently overriding:

> _Contradicts ADR-0003 (observe, do not spawn), but worth reopening because…_

That one in particular is load-bearing: mogeung never starts, steers or stops
an agent, and v0.1 was thrown away over it.
