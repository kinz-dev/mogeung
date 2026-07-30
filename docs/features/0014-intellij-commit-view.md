---
title: IntelliJ-style commit view — file tree, one file's diff, details
status: shipped
updated: 2026-07-30
roadmap: [R-D18]
depends_on: [A13]
---

# 0014 — IntelliJ-style commit view

The R-D18 backlog row, asked for directly during dogfooding (2026-07-28)
and then made concrete with a screenshot of IntelliJ's Git log
(2026-07-29): commit list on the left; for the selected commit, its
changed files as a selectable directory tree with the commit's details
beneath, and the diff pane showing **one chosen file at a time** instead
of every file in a single scroll. Read-only throughout, unchanged from
[feature 0011](0011-git-depth.md)'s permanent write fence.

## Spec

### Problem

Selecting a commit in the log shows every file's diff in one scroll. A
forty-file agent commit is minutes of scrolling; R-D14's by-directory
index softens it with jumps, but the reader still swims in one long
document and never gets IntelliJ's basic contract: *pick a file, see
that file*. The commit's details (message, refs) are a header **inside**
that scroll, so they leave the screen the moment you read past them —
and the one question the details never answer is "which branches contain
this commit?", which matters exactly when checking whether a fix has
reached main.

### Assumptions

- **A13** — everything here is read-only git; no new assumption. The
  only daemon addition is one more read (`git branch -a --contains`).

### Acceptance

- [x] Selecting a multi-file commit shows its changed files as a
      directory tree in a sidebar: folders collapsible, single-child
      directory chains flattened to one row (`a/b/c/`), files colored by
      status with churn counts and read marks
- [x] Clicking a file shows **only that file's diff**; a root "all
      files" row restores the whole-commit scroll; switching selection
      in the log resets the focus
- [x] The commit's details sit under the tree, always visible while
      reading hunks: full message, author/committer with dates, sha,
      clickable parents, refs, and **the branches that contain it**
- [x] `n`/`p` with a file focused walk that file's hunks and then step
      into the next/previous file, IntelliJ-style, updating the tree
      selection as they cross file boundaries
- [x] Log rows show author and age dimmed at the end of the row, the
      IntelliJ columns adapted to a narrow pane
- [x] Range and stash diffs get the same tree and focus behavior (they
      are files-lists through the same pane); details remain
      commit-only
- [x] Nothing anywhere writes

### Explicitly out of scope

- A diff-in-a-dialog on double-click (IntelliJ's actual gesture) — the
  inline pane with read marks *is* mogeung's review surface.
- Tree state persistence across selections — the tree opens expanded;
  collapse is a within-look gesture.
- CI status columns from the screenshot — mogeung has no CI source.
- R-D14's index UI is **removed**, not kept alongside: two ways to
  navigate the same list is one too many. Its `group_by_dir` grouping
  logic is the seed of the tree.

## Plan

The tree is a pure function `file_tree(&[FileChange])` in `gitview.rs`
next to `group_by_dir` (which it replaces), building nested nodes with
single-child chains flattened — testable without a window. Focus is one
`Option<String>` on `GitView`, cleared when the diff selection changes.
The sidebar is an `egui::SidePanel::show_inside` within the diff panel,
resizable, tree above, details below; the diff render filters its file
list by the focus before anything else touches it, so hunk numbering,
`n`/`p`, and the patch button all follow for free. "In N branches" is a
new `branches: Vec<String>` on `CommitDetail` (serde-default, so old
daemons still parse), filled by one extra read in `show_commit`.

### Risks and unknowns

- `SidePanel::show_inside` nested in an egui_tiles pane is new here;
  if it fights the pane's own layout, fall back to a fixed-width column.
- `git branch -a --contains` walks history; on huge repos it could be
  slow. It rides `GitShow` (per-commit, user-initiated), not the log.

### Test strategy

UI: tree building (chain flattening, no file lost, dirs before files),
focus filtering, the n/p cross-file walk as a pure function over hunk
counts. Daemon: the `--contains` parse, and detail parsing with and
without the new field. Nothing spawns an agent.

## Notes

(dogfooding pending)
