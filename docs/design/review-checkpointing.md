---
title: Review checkpointing and risk ordering
status: active
updated: 2026-07-28
covers:
  - crates/mogeungd/src/git.rs
  - crates/mogeung-core/src/change.rs
  - crates/mogeung-core/src/review.rs
  - crates/mogeung-ui/src/diff.rs
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

Anchors are computed over **whitespace-normalised** content (`R-D2`): leading
indentation and runs of internal spaces are collapsed, so reformatting or
re-indenting does not resurrect a hunk you have already read.

Normalisation stops there, deliberately. String contents, case and the `+`/`-`
sign all still count. Going further would start marking genuinely different code
as already-reviewed — a silent false negative, which is the one failure this
system must never have. Pinned in both directions by
`reindenting_does_not_make_a_hunk_unread` and `normalisation_stops_at_whitespace`.

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


## Diff base (`R-D7`)

The base is **the last commit made before the session started**, not `HEAD` when
mogeung first saw it.

`HEAD`-at-first-sight is wrong whenever the daemon starts *after* an agent has
already committed: those commits are inside the base, so the work is invisible
and the session looks like it did nothing. Resolved with
`git rev-list -1 --before <session start> HEAD`, falling back to `HEAD` when the
repo has no commit that old.

## Presentation

All of it is pure text-in/spans-out in `crates/mogeung-ui/src/diff.rs`, so the
interesting parts are testable without a window.

**Syntax highlighting (`R-D4`)** is a tokenizer, not a parser: strings,
comments, numbers and one shared keyword set across languages. No tree-sitter,
no grammars, no language detection. It will mis-colour things. The property that
*is* enforced is losslessness — colouring must never alter the text, pinned by a
test over unicode, unterminated strings and empty lines. If it ever needs to be
correct rather than helpful, replace it wholesale rather than patching it toward
accuracy.

**Word diff (`R-D5`)** finds the common prefix and suffix on word boundaries and
marks the middle as changed. Not a minimal edit script — a real Myers diff would
highlight less — but O(n), and it never produces the confetti that
character-level diffing makes of reformatted code.

The `-`/`+` marker is stripped before comparing. The first implementation did
not, so every pair differed at position 0 and the whole line lit up, which is no
better than having no word diff at all. Pinned by
`the_marker_never_widens_the_highlight`.

**The file list** is one line per file: a marker that is green `✓` when fully
read and a risk-coloured `●` when not, the path elided from the *left*
(`…/src/state.rs`), and the churn. Risk label and flags live on the hover.

Eliding from the left is the point — leading directories are identical across
most of a repo, so truncating from the right would leave a column of
`crates/mog…` telling you nothing. Pinned by a test.

**Side by side (`R-D6`)** zips equal-length runs of removals and additions so a
modified line sits opposite the line it replaced, which is what makes the word
diff meaningful. Lopsided runs get blanks. A test asserts no line is ever
dropped or invented.

## Review debt (`R-D8`)

Counted in **hunks**, because the hunk is the unit the review UI checks off — a
metric you cannot act on in the units you measure it in is decoration.

Built from diffs already computed rather than by re-walking git, so it costs
nothing and always agrees with what the Changes tab shows. The limitation that
follows: it covers sessions mogeung knows about, not the whole history of the
repository.

An empty repo reports **1.0, "nothing outstanding"** rather than 0%. A false
alarm on an empty set is how a metric loses credibility on day one.

## Blast radius (`R-D9`)

`git grep -w` for symbols pulled out of a hunk's added and removed lines by
matching declaration keywords (`fn`, `def`, `class`, `func`, `struct`, …).

**This is grep, not a compiler.** It over-reports common names and misses
anything dynamic. The UI says so on the panel itself rather than in
documentation nobody reads. Test references are called out first: "did anything
test this?" is the question with teeth.
