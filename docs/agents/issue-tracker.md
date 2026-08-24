---
title: Issue tracker for the engineering skills
status: active
updated: 2026-08-24
---

# Issue tracker: the roadmap and the feature docs

mogeung does not use GitHub Issues. None has ever been filed, and the repo has
a remote only to hold the code. Work lives in two files in this tree, and the
engineering skills read and write there:

- **`docs/product/roadmap.md`** — the ranked backlog. One table row per item,
  id of the form `R-` plus a pillar letter and a number. An item exists when
  it has a row; the row is the ticket.
- **`docs/features/NNNN-slug.md`** — the spec for work in flight, with
  `## Spec`, `## Plan` and `## Notes` sections, and frontmatter naming the
  roadmap id and the assumptions it rests on.

Read [the doc system's rules](../README.md) before writing to either.

## Conventions

- **Create an issue** — add a row to the roadmap table under the right pillar,
  with effort (S/M/L) and a blank status box. Do not invent an id that is
  already taken; ids are never reused, even for refused items.
- **Open a ticket for work** — write `docs/features/NNNN-slug.md` from
  `docs/_templates/spec.md`, with `roadmap:` naming the row and `depends_on:`
  naming the assumptions. NNNN is the next free number.
- **Read a ticket** — read the feature doc, then the roadmap row, then every
  assumption in `depends_on:` from
  [`docs/product/assumptions.md`](../product/assumptions.md).
- **Comment on a ticket** — append to the feature doc's `## Notes`.
- **Close** — set the roadmap row's status glyph and the feature doc's
  `status:`, then move the doc to `docs/archive/` and run
  `./scripts/gen-status.sh`.

Every write here is a doc change, so **`./scripts/check-docs.sh` must pass
before handing back.** It is not optional and it needs no network.

## The rule that outranks the skills

A feature spec may not be written against an `UNTESTED` assumption. If one of
the `depends_on:` assumptions is untested, **the work is to test it, not to
build the feature.** A skill that proposes implementation work against an
untested assumption is wrong, and this file is why.

## When a skill says "publish to the issue tracker"

Add the roadmap row. Write the feature spec only if the work is starting now.

## When a skill says "fetch the relevant ticket"

Read the feature doc plus its roadmap row and assumptions.

## Wayfinding operations

Used by `/wayfinder`. The **map** is the feature doc itself — its `## Notes`
section holds Decisions-so-far and open Fog. **Child tickets** are checklist
items under a `## Open questions` heading in that same doc, each tagged
`research` / `prototype` / `grilling` / `task`. Blocking is written as plain
prose on the item ("blocked by: the sweep landing"). Do not split a feature
into a folder of files to model this; [`docs/README.md`](../README.md) calls
that ceremony.

## Pull requests as a request surface

**No.** This is a solo repo and commits land straight on `main`.
