---
title: Features in flight
status: active
updated: 2026-08-03
---

# Features

One file per unit of work: `NNNN-slug.md`, from
[`_templates/spec.md`](../_templates/spec.md), with `## Spec`, `## Plan` and
`## Notes` sections.

Split into a folder (`NNNN-slug/spec.md`, `plan.md`, `notes.md`) only when a
feature actually grows large enough to need it. Over-structure is still sprawl —
three near-empty files per feature is ceremony, not organisation.

Numbers come from [`../product/roadmap.md`](../product/roadmap.md) ordering, not
from the roadmap item ids. A spec must name the roadmap item it implements and
the assumptions it depends on.

On ship: update `docs/design/`, write any ADRs, move the file to
`docs/archive/`, and regenerate `STATUS.md`.

**In flight**, both `shipped` and neither dogfooded — planned and built
2026-08-03 from four screenshots of RustRover:

- [0027 — the right tool-window rail](0027-right-rail.md), and the worktree
  tree moving into it (`R-B40`, `R-B41`)
- [0028 — global search](0028-global-search.md), the rail's second tool window
  (`R-F13`)

They are one piece of work in two specs: the rail without a tool window in it
is nothing, and the search panel has nowhere to live until the rail exists.
The docking rule both depend on is
[ADR-0017](../decisions/0017-the-rail-is-chrome.md).
