---
title: How this project's documentation works
status: active
updated: 2026-07-25
---

# The doc system

mogeung is documentation-driven: docs decide what gets built, and code is the
consequence. This file is the rulebook. Read it before writing any doc.

The system is the product's own thesis applied to us — **progress is derived,
not written**, and every doc carries the metadata that makes staleness
mechanically checkable. If we cannot keep our own docs honest, the doc layer on
the roadmap is not worth building.

## The one rule that matters

> **Never create a markdown file at the repository root.**
> Unsure where something goes? It goes in `docs/features/<current-feature>`.

Doc sprawl does not happen because people are careless. It happens because
nobody said where to put things, so everyone defaults to `PLAN.md` at the root.

## Where things go

| Folder | Holds | Lifecycle |
|---|---|---|
| `product/` | Thesis, roadmap, assumptions | Long-lived, edited freely |
| `decisions/` | ADRs — why we chose what we chose | **Immutable.** Supersede, never edit |
| `design/` | How the system works **today** | Rewritten freely; no history |
| `features/` | Work in flight: spec, plan, notes | Archived on ship |
| `guide/` | User documentation | Follows released behaviour |
| `archive/` | Superseded docs | Never deleted, never read by default |
| `_templates/` | Skeletons to fill in | — |

The split between `design/` and `decisions/` is what stops rot. A design doc is
always "how it works now" and may be rewritten wholesale. An ADR is never
rewritten — the record of *why we were once wrong* is the most valuable thing in
this repo, and editing destroys it.

## Frontmatter

Every doc starts with:

```yaml
---
title: Attention ranking
status: draft | active | superseded | archived
updated: 2026-07-25
---
```

Design docs add `covers:`, listing the code they describe:

```yaml
covers:
  - crates/mogeung-core/src/attention.rs
  - crates/mogeungd/src/state.rs
```

This is load-bearing. `scripts/check-docs.sh` asks git whether any covered path
changed after `updated` — so a doc that has drifted from its code says so out
loud instead of quietly lying.

Feature specs add `depends_on:`, naming assumptions from
[product/assumptions.md](product/assumptions.md):

```yaml
depends_on: [A1, A4]
```

## Generated files — never hand-edit

- `STATUS.md` — from feature frontmatter, git, and test counts
  (`scripts/gen-status.sh`)

If you find yourself hand-editing a generated file, the generator is wrong. Fix
the generator.

## The loop

```
roadmap entry
  → docs/features/NNNN-slug.md  (spec: what, why, acceptance, depends_on)
  → plan                        (how — an agent drafts it, a human approves)
  → implement
  → notes                       (surprises worth remembering)
  → ADR if a durable choice was made
  → update docs/design/
  → move to docs/archive/, regenerate STATUS.md
```

## Features: start light

A feature begins as **one file**: `docs/features/0007-collision-warning.md`,
with `## Spec`, `## Plan`, `## Notes` sections. Split it into a folder
(`0007-collision-warning/spec.md`, `plan.md`, `notes.md`) only when it actually
gets big enough to need it.

Over-structure is still sprawl. Three near-empty files per feature is ceremony
you will resent by week three.

## Assumptions come first

No feature spec may be written without naming the assumptions it rests on. If a
spec depends on an `UNTESTED` assumption, **the work is to test the assumption,
not to build the feature.**

This rule exists because v0.1 was built and thrown away over an unexamined
assumption. See [product/assumptions.md](product/assumptions.md).

## Checks

```sh
./scripts/check-docs.sh    # frontmatter lint, staleness, orphans
./scripts/gen-status.sh    # rewrite STATUS.md
```
