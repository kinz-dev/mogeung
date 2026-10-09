---
title: Features in flight
status: active
updated: 2026-10-09
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

## Status, and where a file lives

A feature's `status:` follows its roadmap rows' glyphs, and nothing else:

| `status:` | Meaning | Lives in |
|---|---|---|
| `draft` | Spec written, not approved; nothing built | `docs/features/` |
| `active` | Approved and being built; some rows blank | `docs/features/` |
| `shipped` | Built and installed; at least one row still ⏳ | `docs/features/` |
| `archived` | Every row ✅ (or 🗑 or struck): the dogfooding verdict is in | `docs/archive/features/` |

On the last verdict the file moves to `docs/archive/features/` with
`status: archived`, every link to it is re-pointed, and `STATUS.md` is
regenerated. The first such sweep was 2026-10-08, when 43 verified features
moved in one go; before that, "archived on ship" was a rule nobody had followed
and `STATUS.md` listed every feature ever written as in flight.

## What is in flight

**`STATUS.md` is the generated list** — it reads every file in this folder, so
a hand-kept copy here would only drift. As of 2026-10-09 the folder holds
eight: [0051](0051-gau-geung.md) (`draft`, the Gau Geung epic: decided,
phase 0 run, the definitions and the launcher built, not yet booted for real), [0045](0045-run-configurations.md) (`draft`, the next priority,
awaiting approval), [0044](0044-more-than-one-repository.md) (`active`,
`R-D35` open),
and five `shipped` awaiting a verdict — [0038](0038-a-local-model.md),
[0040](0040-a-connection-manager.md), [0046](0046-found-on-the-lan.md),
[0048](0048-opencode-adapter.md), [0050](0050-prices-that-keep-up.md).
