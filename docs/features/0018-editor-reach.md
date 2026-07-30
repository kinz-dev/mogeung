---
title: Editor reach — git ergonomics, navigation, comforts
status: shipped
updated: 2026-07-30
roadmap: [R-B27, R-B28, R-B29]
depends_on: [A13, A15, A16]
---

# 0018 — Editor reach

R-B27–R-B29, built at the 2026-07-29 one-go ask. Still a viewer, never
an editor — pillar K's line is restated here because this is the batch
where "it's nearly an editor anyway" pressure peaks.

## Spec

### Problem

The Editor pane shows a file but not what changed in it: no gutter
against HEAD, no blame without switching panes, no outline, no way to
jump to a symbol or line, no preview for the markdown the agents
produce in bulk.

### Assumptions

A13, A15, A16 — all `SUPPORTED`; this extends the workbench those asks
built. The write fence is unchanged.

### Acceptance

- [x] An open file shows a gutter marking lines changed vs HEAD;
      next/prev-change keys walk them (`n`/`p`, pointer over the pane —
      the Git pane's hunk keys); lines this *session* changed are
      marked distinctly from other uncommitted changes (R-B27)
- [x] The current line's blame (sha, author, age, summary) is visible
      on demand; a file can be compared side-by-side with any revision
      reachable from the Git pane (R-B27) — the blame hover card already
      existed (R-D10/11); "vs HEAD" pairs the committed version beside
      the worktree in one click, and any other revision rides the
      existing rev tabs + split
- [x] A symbol outline lists the file's functions/types; go-to-symbol
      (the outline's filter box) and go-to-line (`Ctrl+G`) jump;
      occurrences of a right-clicked word highlight (R-B28). Folding is
      **descoped**, not half-shipped — see Notes
- [x] Markdown renders as a toggleable preview; images preview
      (local-disk read, refused against a remote daemon); word wrap
      toggles per file and persists; copy path and copy `path:line`
      exist; the header shows size/lines/language; bookmarks
      set/jump/list via the context menu and the `marks` menu (R-B29)

### Explicitly out of scope

- Any write path, any LSP, any semantic rename/refs — outline and
  occurrences are syntactic (pillar K).
- Sticky scroll if egui makes it disproportionate — noted as dropped in
  Notes if so, not silently.

## Plan

### Approach

R-B27 daemon: reuse existing diff plumbing for a per-file
lines-changed-vs-HEAD query (wire pair with echo); session-changed
lines intersect with `touched_files` hunks already computed. Blame
rides the existing blame endpoint. R-B28/29 are client-side: a light
line-based symbol scanner (regex per language family over the cached
body — honest and labelled, no tree-sitter dependency added), fold
ranges from indentation, occurrences from word match; markdown preview
via egui_commonmark if the dep is acceptable, else a minimal renderer;
image preview via egui's image support on the existing byte fetch.

### Test strategy

Symbol-scanner and fold unit tests per language fixture; gutter-merge
tests (session vs uncommitted); wire echo tests; manual acceptance in
the running app for the visual items.

## Notes

**The single-galley viewer decided everything.** The body is one laid-out
galley whose row geometry the find bands, blame column and line numbers
already share. The diff gutter, occurrence bands and go-to-line all ride
the same geometry for free — and folding *cannot*: hiding line ranges
would re-number every row and make each of those gutters lie. Folding
(and sticky scroll, same reason) is descoped rather than shipped wrong;
if dogfooding wants it, the viewer needs per-line rendering first.

**Nothing new crossed the wire for the diff gutter.** `GitDiffFile`'s
answer, already cached for the Git pane, parses into changed line
numbers client-side — the patch-text lesson from feature 0013 again.
Session-changed lines come from the session diff already in hand;
they draw as a second, narrower bar in the attribution colour.

**Occurrences are right-click, not cursor-follow.** A selectable egui
label exposes no cursor; the honest gesture available is "highlight
this word" on the context menu, which also carries copy `path:line`
and bookmarks — verbs on the line, the blame menu's shape.

**Word wrap persists per file, not per tab** — wrap is a property of
prose files (a README wants it, a lockfile never), and a tab is too
short-lived to remember it.
