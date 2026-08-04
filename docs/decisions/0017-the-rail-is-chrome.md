---
title: The rail is chrome; the tile tree is content
status: active
updated: 2026-08-03
decided: 2026-08-03
---

# ADR-0017 — The rail is chrome; the tile tree is content

## Context

This window already has two ways to dock something, and a third was asked for
on 2026-08-03.

The first is the tile tree: `egui_tiles` holds Changes, Transcript, Info, Debt,
Agent, Editor, Git and Insight as draggable, splittable tabs
([feature 0006](../features/0006-dockable-panes.md), assumption A14). The
second is plain edge panels outside that tree — the Attention queue on the left
and the terminal across the bottom, neither of which is a tile and neither of
which can be dragged into one.

The ask was a JetBrains-style tool-window rail on the right: an always-present
icon strip that expands into a panel, holding a file tree and, later, a search
panel. Presented as four screenshots of RustRover showing exactly that.

What makes this a real decision rather than an obvious one is that the file
tree already exists **inside** the Editor tab, with a draggable divider
(`R-B37`) and its own Ctrl+wheel zoom (`R-B39`), both built the week before.
A rail with a Files tool window is therefore a second tree over the same
worktree — and with the Editor tab forward and the rail open, the arrangement
the feature exists for is exactly the arrangement that shows the tree twice.

Without a rule, "where does the next pane go" becomes a coin flip, and the two
systems drift until neither answers "where do I find things".

## Decision

**The tile tree holds views of a session. The edge panels hold tools that
outlive the selection.** A thing that is read and arranged is a tile; a thing
that must stay reachable whichever view is forward is chrome.

The right rail is chrome. Concretely:

- The rail is an `egui::Panel::right` declared beside `queue_panel`, not a node
  in the tile tree. Its tool windows cannot be dragged into the centre, and
  tiles cannot be dragged into it.
- The rail always shows *something*: collapsed it is a 30px icon strip, never
  nothing. This is the rule the Attention strip already follows, for the same
  reason — a panel you can lose entirely is one you have to rediscover.
- **A thing lives in exactly one of the two.** The Editor tab therefore gives
  up its worktree tree to the rail's Files tool window. `R-B37`'s drag and
  `R-B39`'s zoom move with it rather than being deleted.

## Alternatives

- **The rail as a launcher for existing tiles** — clicking Files raises the
  Editor tab. One docking model, no new chrome. Lost because it cannot produce
  the arrangement that was asked for: the screenshots show the tree beside the
  editor *and* the terminal at once, and a tab cannot be beside a tab.
- **Two trees sharing `SessionExplorer` state** — both drawn, expansion and
  reveal always in agreement. Genuinely safe, and rejected on the narrower
  ground that the duplicate appears precisely in the layout the feature is for.
  This is the alternative to return to if the Editor tab proves to need a tree
  of its own.
- **Two independent trees** — two expansion sets over one worktree, silently
  disagreeing. Rejected outright.
- **Make the rail part of `egui_tiles`**, as a right-docked container. Lost on
  the collapsed strip: the tile tree has no concept of one, and the strip is
  the half of this feature that makes closing the panel safe rather than
  destructive.

## Consequences

- Any tool that must be reachable from every tab now has an obvious home, and
  the second and third (Search, and one day Notes) cost a variant and a body
  rather than a design argument.
- The rail is one more thing competing for horizontal space, on the same edge
  the Editor's split and the symbol outline already use. On a narrow window
  the centre gets thin, and nothing here prevents that.
- **The Editor tab stops being self-contained.** With the rail collapsed there
  is no tree in it at all, and opening a file by name goes through the palette.
  That is a real regression for anyone who used the Editor alone, accepted
  because the strip keeps the tree one click away.
- Dragging a tool window into the centre is now impossible. IntelliJ allows it,
  the screenshots come from IntelliJ, and it will be asked for.
- The rail shows one tool at a time. IntelliJ splits each rail into two stacks;
  we do not, so Files and Search cannot be open together.
- The `editor-tree` zoom key is kept as the rail tree's key, so a preference
  written by `R-B39` keeps working instead of silently resetting to 1.0.

## Revisit if

- The rail sits collapsed through a week of use. Then it is chrome nobody
  wants: Files goes back inside the Editor and the rail comes out with it.
- Two tool windows are genuinely wanted at once. That is the two-stack rail,
  and it changes the panel's shape rather than extending it.
- The Editor tab proves to need a tree of its own after all — the shared-state
  alternative above is the fallback, and it is a smaller reversal than it looks.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
