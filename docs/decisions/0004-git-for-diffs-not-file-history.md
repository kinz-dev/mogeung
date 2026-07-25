---
title: Use git for diffs, not Claude Code's file-history
status: active
updated: 2026-07-25
decided: 2026-07-25
---

# ADR-0004 — Use git for diffs, not Claude Code's file-history

## Context

Observed sessions need a diff. Claude Code keeps pre-edit file backups in
`~/.claude/file-history/<session-id>/`, which would give perfect per-session
before/after content with no git involvement — including for sessions that
committed their work, and for repos with several sessions running at once.

## Decision

**Diff with git.** Attribute a diff to a session by *which files that session
edited*, taken from its `Edit`/`Write` tool calls and `file-history-delta`
records.

## Rationale

The backup blobs are named by opaque hash plus version (`ac336a0064b654b7@v2`)
with no reliable mapping back to a path. The mapping appears to live in
transcript delta records, but was absent for the sessions inspected. Building on
it would mean reverse-engineering an undocumented index inside an undocumented
format.

The git engine already existed, was tested, and handles untracked files —
exactly what an agent creating new modules produces.

## Consequences

- A session working outside a git repo shows no changes.
- The base commit is repo HEAD **when mogeung first saw the session**, so
  sessions predating mogeung diff meaninglessly ([A9](../product/assumptions.md)).
- Committed work disappears from the diff as the base moves with HEAD. Roadmap
  `D7`.
- Two sessions editing the *same* file both show it. Git cannot separate them
  and neither can we ([A8](../product/assumptions.md)).
- Revisit if `file-history` gains a documented index — it would fix all four.
