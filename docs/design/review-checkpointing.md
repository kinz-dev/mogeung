---
title: Review checkpointing and risk ordering
status: active
updated: 2026-07-25
covers:
  - crates/mogeungd/src/git.rs
  - crates/mogeung-core/src/change.rs
---

# Review checkpointing and risk ordering

The product's most distinctive feature: **never read the same code twice**.

## Hunk anchors

A hunk's identity is a hash of its **content**:

```
sha256(path + "\n" + each added/removed line)   → first 16 hex chars
```

Line numbers and context lines are excluded. That is the whole trick:

- A hunk that moves within its file keeps its anchor and stays **read**.
- A hunk that is rewritten gets a new anchor and returns **unread**.

Pinned by tests both ways. Verified live: after a follow-up that touched only
`main.rs`, `auth.rs` stayed read while the rewritten `main.rs` came back unread.

**Known limitation:** reformatting or re-indenting changes the hash, so a purely
cosmetic edit makes a hunk unread again. Roadmap `D2`.

## What the diff covers

Computed against the commit the repo was on when mogeung **first saw** the
session, and covering three sources:

1. Committed work since that base
2. Uncommitted edits
3. **Untracked files** — via `ls-files --others` plus `diff --no-index`

Untracked files matter because they are exactly what an agent creating new
modules produces, and plain `git diff` misses them entirely. `add -N` was
rejected deliberately: mogeung must never mutate your index.

## Attribution

Several sessions can share a working tree, so the diff is filtered to the files
that session actually edited, from its `Edit`/`Write` calls.

Two sessions editing the *same* file will both show it. Git cannot separate them
([A8](../product/assumptions.md)).

## Risk ordering

Files sort by risk, never alphabetically.

**Path flags:** auth, secrets, migration, money, CI config, infra, dependency
manifests. Lockfiles, generated code, vendored trees, fixtures and snapshots are
flagged `Noise` (negative weight) and hidden by default.

**Content flags:** `unsafe`, error handling, concurrency, network I/O, secrets,
widened public API, large deletions, and deleted tests.

Weights run from `Secrets` (100) down to `Noise` (−60). A file scores as its
**riskiest hunk**, so one dangerous change cannot be averaged away by
surrounding boilerplate.

**This is keyword matching over diff text, not analysis.** It will produce false
positives (a variable named `password_field`) and false negatives (a subtle auth
bug in a boring file). It is a reading order and must never be presented as a
safety guarantee ([A3](../product/assumptions.md)).
