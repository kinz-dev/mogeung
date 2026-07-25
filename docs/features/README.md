---
title: Features in flight
status: active
updated: 2026-07-25
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

**Nothing is in flight.** Priority is unresolved pending the dogfooding week —
see [roadmap item 0](../product/roadmap.md#0-the-non-feature).
