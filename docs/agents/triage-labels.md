---
title: Triage labels for the engineering skills
status: active
updated: 2026-08-24
---

# Triage labels

The skills speak in five canonical triage roles. This repo has no label
system — its tracker is markdown — so a role is recorded as a `triage:` key in
the feature doc's frontmatter, or beside the roadmap row when there is no
feature doc yet.

| Role in the skills | Written here      | Meaning                                 |
| ------------------ | ----------------- | --------------------------------------- |
| `needs-triage`     | `needs-triage`    | Not yet evaluated                       |
| `needs-info`       | `needs-info`      | Waiting on the reporter                 |
| `ready-for-agent`  | `ready-for-agent` | Fully specified, ready for an AFK agent |
| `ready-for-human`  | `ready-for-human` | Requires human implementation           |
| `wontfix`          | `wontfix`         | Will not be actioned                    |

The strings are the canonical names unchanged, because nothing here used other
names first.

`triage:` is new with this file and is not retrofitted onto existing feature
docs. Its absence means the doc predates it, **not** `needs-triage`.

It is deliberately not the same axis as the roadmap's status glyphs (✅ shipped
· ⏳ built, awaiting the dogfooding verdict · 🗑 removed · blank not started),
which say what happened to the work rather than whether it is ready to start.
A refused idea is struck through and keeps its blank box — it is not
`wontfix`, because `wontfix` is a triage outcome and a strike-through is an
argument. See [the roadmap](../product/roadmap.md) for both conventions.
