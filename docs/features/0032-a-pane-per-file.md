---
title: A pane per file
status: shipped
updated: 2026-08-07
roadmap: [R-B53]
depends_on: [A14, A15, A16, A30]
---

# 0032 — A pane per file

Every open file becomes its own dockable pane, so the window runs one tab
system instead of two.

## Spec

### Problem

The window docked things twice. dockview held the panes; the Code pane held its
*own* file strip, with its own two-way split and its own "send to the other
side" button — a second, weaker docking system nested inside the first, capped
at two columns and unable to put a file anywhere near an agent.

Asked on 2026-08-06: *"can each of the editor open as its own docker tab?"* —
and the answer was yes, with the interesting half being what happens to a file
pane when you click a different session.

The cost of the old shape had already been paid twice that week. `R-B49` merged
the Agent pane's duplicate header; the Code pane then needed its *own* fix for
the same complaint — three rows of naming — which shipped as a hidden group
header and cost it the ability to be dragged at all. Both were symptoms of one
pane pretending to be a tab strip.

### Assumptions

- **A15**, **A16** — reading worktree files inside mogeung is worth a pane, and
  earns workbench affordances. Both `SUPPORTED`. This changes the shape of the
  workbench, not the bet.
- **A14** — the user arranges panes rather than leaving the default.
  `SUPPORTED`. This is the assumption that makes a pane per file better than a
  strip: if nothing is ever dragged, a strip was fine.
- **A30** — two sessions on screen at once. `SUPPORTED`, and the reason the
  binding question below has the answer it does.

### Acceptance

- [x] Opening a file puts a pane on screen named after that file
- [x] Two files are two panes, arrangeable like any other
- [x] A file pane keeps showing its file when another session is selected
- [x] A file tab closes; every other pane in the centre still cannot
- [x] The preview rule survives — an unpinned file is replaced, a pinned one is
      not — and the *pane* goes with the file it replaced
- [x] A restored layout carries no file panes
- [x] `Alt+C` still does something sensible

### Explicitly out of scope

- **Restoring open files across a restart.** `explorer` is store state, not
  preferences, and never survived a restart. This does not change that.
- **Two files from two different sessions in one group.** Allowed by accident —
  nothing prevents dragging them together — and neither designed for nor
  forbidden.

## Plan

### Approach

A file pane is identified by `file:<session>:<rev>:<path>`, and **reads its own
file out of that id**. That single decision is what makes the pane bound to its
session: there is no lookup of `selected` anywhere in it, so a selection change
cannot reach it.

`ExplorerState` loses `active`, `focus` and `FileTab.group` — all three existed
only to model the internal split, which dockview now does. What is left is
`open`, a list of what is open and its bodies.

### Files touched

| Path | Change |
|---|---|
| `desktop/src/panes/FilePane.tsx` | renamed from `CodePane`; `TabStrip` and the split deleted; renders one file |
| `desktop/src/lib/panes.ts` | `filePaneId`, `parseFilePaneId`, `showFilePane`, `closeFilePane`, `filePanes`; `ensureCodeAlone` deleted |
| `desktop/src/lib/explorer.ts` | `openFile` opens a pane; `closeTab` becomes `closeFile`, by identity |
| `desktop/src/store/index.ts` | `active`, `focus`, `FileTab.group` removed |
| `desktop/src/App.tsx` | `file` component; `syncCodePane` deleted; `file:` and `code` stripped on load |
| `desktop/src/ui/PaneChrome.tsx` | a file tab names its file from its id, and closes |
| `desktop/src/ui/tools/FilesTool.tsx` | the tree marks every open file rather than the active one |
| `desktop/src/lib/keymap.ts` | `Alt+C` repointed to the newest open file |

### Risks and unknowns

- **Panes accumulate.** A file pane never closes itself, which is the price of
  it never emptying itself either. The preview rule is what keeps browsing from
  leaving forty of them.
- **Monaco per visible pane.** Two files open is two editors. Hidden tabs cost
  nothing; a wide split of six files would.

### Test strategy

The state half in `explorer.test.ts` — one entry per `(path, rev)`, the preview
reused, a preview promoted rather than duplicated, closing by identity, one
session's files kept out of another's. The pane half in `panes.test.tsx` — a
file opens a pane, two files make two, the preview's pane goes with it, closing
removes it, and a restored layout carries none.

## Notes

**The binding question was the whole design, and it was asked before any code.**
Offered two options — a file pane that stays put, or one that closes and
reopens as the selection moves — the answer was *stay put*. Everything else
follows from it: the pane reads its file from its own id, so a selection change
has nothing to act on. The alternative would have meant a reconciling effect
watching `selected`, and a lot of dockview churn on every click in the queue.

**Two rows of chrome came back, and that is a fair trade rather than a
regression.** `R-B49` had got the Code pane down to one row by hiding its group
header — which also took away its ability to be dragged, stated at the time as
an accepted cost. A file pane has its dockview tab back (that tab *is* the file
tab now) plus a thin controls row, and it is draggable again. Net: same rows,
one tab system, and a file can sit beside an agent.

**`closeTab(id, index)` became `closeFile(id, path, rev)`.** An index into a
list is fine while one component owns both the list and the click; a pane knows
only which file it is showing, and an index it cannot see would be stale the
moment anything else closed.

**The preview rule needed a second half it never had.** Reusing the unpinned
tab used to be a list edit; now the *pane* of the file being replaced has to be
closed too, or a tab outlives the file it was showing.

**Stripping `code` from old layouts is not cosmetic.** Any layout written before
today names a `code` panel whose component no longer exists, which dockview
restores as a dead tab. It joins `MOVED_TO_DOCK`, which already existed for
exactly this and was the second time this week it earned its keep.

**Unproven.** Whether a pane per file is better *in use* than a strip is the
open question, and it is A14's in a new place: this is only an improvement if
files actually get arranged. If they end up always tabbed together in one
group, the strip was doing the same job with less machinery — and the honest
response then is to say so rather than to add a strip back on top of the panes.
