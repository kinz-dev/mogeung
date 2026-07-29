---
title: Git reach — forensics, ergonomics, refs, conflicts, review-state
status: shipped
updated: 2026-07-28
roadmap: [R-D13, R-D14, R-D15, R-D16, R-D17]
depends_on: [A13, A18, A19]
---

# 0013 — Git reach

The R-D13–R-D17 backlog rows, built in one pass at an explicit *"let's do
R-D10 to R-D17 as much as you can"* — the third one-go commitment in this
area (A16, A19 precedent). Read-only throughout, unchanged from
[feature 0011](0011-git-depth.md).

## Spec

### Problem

Five gaps, all reading-side:

- **Forensics** — "when did this string appear?" has no answer but
  paging; a hunk worth showing another agent cannot leave the pane as
  text; the attribution dots cannot become a filter; hunks are
  mouse-only.
- **Ergonomics** — ±3 lines of context is often too little and there is
  no more; a forty-file agent commit is one flat list; whitespace noise
  cannot be muted; the git diffs ignore the side-by-side preference.
- **Ref reach** — "what would merging this branch bring" needs a
  terminal; remote branches are invisible; reflog does not exist; the
  worktrees mogeung itself creates are not shown.
- **Conflicts** — markers in a diff are the whole story; ours/base/theirs
  cannot be read side by side.
- **Review-state** — R-D8 knows which hunks a human has read, and the
  log does not show it.

### Acceptance

- [x] `find:` in the log filter runs a pickaxe (`-S`) search; `●` next to
      the filter narrows the log to this session's probable commits
- [x] A commit's context menu offers "Copy as patch"; each file header
      offers the same for that file — text a terminal or another agent
      can apply
- [x] `n`/`p` step through the hunks of the git diff panel
- [x] The diff panel can widen context (±3 → ±10 → ±30 → all) and
      toggle whitespace-ignore and side-by-side; a multi-file diff opens
      with a by-directory file index that jumps to each file
- [x] A branch's context menu offers "Diff against current from the
      merge base" (three-dot semantics); remote branches list and scope
      the log like local ones
- [x] A reflog section shows where HEAD has been; a worktrees section
      lists `git worktree list`, naming the session running in each
- [x] A conflicted file opens as a three-column base/ours/theirs
      read-only view
- [x] A viewed commit's diff shows which hunks a human has already read
      (R-D8's marks), the details header counts them, and the log row
      wears a quiet "read" badge once known
- [x] Everything has REST twins; nothing anywhere writes

### Explicitly out of scope

- "Commits on A not on B" as a list (ahead/behind counts exist; the
  scoped log covers most of it) — wait for want.
- Review-state for *unviewed* commits (would mean diffing every log row;
  the badge appears as commits are read, which is the honest cost).
- Conflict *resolution*, and any diffing between the three stages —
  three readable columns first.
- Everything in [feature 0011]'s permanent write fence.

## Plan

Same shapes as 0011/0012: wire pairs in the fire-and-forget form with
REST twins, caches and stray-drop in `gitview.rs`, rendering in the
existing pane. Patch text is rebuilt client-side from the `Hunk` lines
already cached — no new wire needed. Diff options (context, whitespace)
ride the four diff commands and echo back for stray-dropping. Compare
resolves the merge base daemon-side and answers as the existing
`GitRangeDiff`. Review marks reuse `parse_unified`'s `reviewed` set,
fed for the first time from the repo's stored anchors instead of an
empty set.

### Risks and unknowns

- The reviewed-anchor union per repo could be large; it is fetched per
  `GitShow`, not per log page, which bounds it.
- Reflog subjects are freeform; only the separators are trusted, as
  everywhere.
- The file index's scroll-to must reuse the frame-late `scroll_to_me`
  pattern the explorer's reveal already uses.

### Test strategy

Daemon: parsers (reflog, worktree porcelain, conflict stages fallback),
merge-base compare argument construction, context clamping, pickaxe
validation riding the existing filter checks. UI: patch-text round-trip
from hunks, file-index grouping, attribution filter, diff-option changes
clearing caches and dropping strays. E2e: hostile sweep grows the new
commands.

## Notes

**"Copy as patch" needed no wire at all.** The hunks cached for rendering
kept their raw headers and signed lines, so the patch is rebuilt
client-side, `/dev/null` ends included — the daemon was never asked.

**Review marks crossed views for free, which was the point of anchors.**
Feeding `parse_unified` the repo's reviewed-anchor union (instead of the
empty set the git pane had always passed) made R-D17 mostly a one-line
daemon change: a hunk read in the Changes tab is the *same content hash*
seen through a commit. The log badge appears only for commits whose diff
has been fetched — computing it for every row would mean diffing the
whole log, and the spec chose honesty over completeness there.

**Diff options had to echo.** Context and whitespace changes invalidate
every cached diff; without the echo, a slow answer cut the old way would
land in a cache that thinks it is current. `opts_match` treats a `None`
echo as "cut with defaults", which is also what makes `GitCompare`'s
answer (always default-cut) acceptable exactly when the pane is at
defaults.

**A compare's answer is adopted, not requested.** The client cannot know
the merge base, so it cannot pre-register the (from, to) key; a
`compare_pending` flag lets the next unsolicited range answer become the
selection, and *only* then — an unsolicited range must never steal the
pane (pinned by a test).

**The attribution filter hides the graph.** Lanes computed over all
commits drawn against a filtered subset would connect dots that are not
adjacent — a lying graph is worse than none.

**`n`/`p` are plain key presses, not keymap actions**, gated on pointer
position and nothing focused — the pane-zoom gate. Rebindability can come
later if the keys prove wanted; wiring two actions through the keymap,
palette and help for an experiment felt like scope creep in a batch this
size.
