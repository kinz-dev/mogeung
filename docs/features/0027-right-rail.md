---
title: The right tool-window rail
status: shipped
updated: 2026-08-19
roadmap: [R-B40, R-B41, R-J33]
depends_on: [A13, A14, A15, A16, A28]
---

# 0027 — The right tool-window rail

Asked for 2026-08-03 as three requests with four screenshots of RustRover. Two
of them are this feature: the rail, and the file tree that lives in it. The
third is [feature 0028](0028-global-search.md), which is the rail's second tool
window.

The docking rule this rests on is
[ADR-0017](../decisions/0017-the-rail-is-chrome.md).

## Spec

### Problem

The worktree tree is trapped inside the Editor tab.

The concrete moment: you are reading a transcript where the agent claims it
changed `watcher.rs`, and you want to know whether that file is where it says
it is. The tree exists — it is two clicks away — but reaching it means leaving
the transcript, looking, and coming back to find your place. The same is true
from the Git pane, from the terminal, and from Changes.

The tree is also only as tall as the Editor tab and only visible when the
Editor is forward, which is the wrong shape for the one view that is a map of
everything else.

The right edge of the window is empty. The left edge already proves the
pattern: the Attention panel is a docked panel that collapses to a strip and
has never been in the tile tree.

### Assumptions

- **A13** (`SUPPORTED`) — keyboard-first navigation. The rail needs a binding
  per tool, not only a click target.
- **A14** (`SUPPORTED`) — the user wants more than one thing on screen at once
  and will arrange it. The rail is that want at the window's edge rather than
  inside the tile tree.
- **A15 / A16** (`SUPPORTED`) — the explorer earned a pane, and then earned
  workbench affordances. Nothing here re-litigates that; it moves where the
  tree lives.
- **A28** (new, `UNTESTED`) — *a file tree visible beside every tab is worth
  30px of permanent chrome.* This is the bet.

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is to
> test the assumption, not to build the feature.

A28 is testable cheaply here, which is what makes building it honest rather
than a dodge. The tree is not written — it **moves**, and `explorer_dir`,
`SessionExplorer`, expansion state, reveal and `open_in_explorer` all already
exist. The rail shell is a mirror of `queue_panel`. So the cost of testing A28
is a panel and a strip, not a file explorer.

**Removal condition, agreed in advance:** if the rail sits collapsed through a
week of use, Files goes back inside the Editor and the rail comes out with it.
That condition is also ADR-0017's first revisit trigger.

### Acceptance

- [x] A strip on the right edge is always visible, with one icon per tool
      window, and cannot be closed away entirely
- [x] Clicking an icon opens that tool; clicking the lit icon collapses back to
      the strip
- [x] The open tool's width is draggable, and the width survives a restart
- [x] The Files tree is visible with any tab forward — Transcript, Git, Agent,
      Changes — not only the Editor
- [x] Clicking a file in the rail opens it and raises the Editor tab.
      **Amended on contact with the code:** single-click opens as a *preview*
      tab and double-click pins, which is the tree's existing IntelliJ
      behaviour and predates this row. Forcing a pin from the rail would have
      made every glance at a file permanent
- [x] Ctrl+wheel over the tree zooms the tree alone, and a zoom set before this
      shipped is still in effect (`R-B39`'s behaviour, carried over)
- [x] With no session selected the rail says so, rather than showing an empty
      tree that looks like a failed listing
- [x] The Editor tab draws no tree of its own, and its empty state says where
      the tree went rather than telling you to pick from something that is no
      longer on screen
- [x] Each tool has a keyboard binding, and the bindings appear in the keymap
      window like every other action

### Explicitly out of scope

- **Dragging a tool window into the centre, or a tab into the rail.**
  ADR-0017. It will be asked for; the answer is written down.
- **Two tool windows open at once.** The rail shows one. IntelliJ splits its
  rail into two stacks and that is a different panel, not a bigger one.
  **Reopened 2026-08-19** — asked for directly, and answered by stacking the
  tools in one column rather than by splitting the rail:
  [ADR-0027](../decisions/0027-the-rail-stacks.md), `R-J33`. The line above is
  left as it was written, because the reason it gives is still why the *two
  stacks* were not built.
- **A left rail.** The Attention panel owns the left edge and is not being
  generalised into a tool-window host.
- **Vector icons.** This window's chrome is text glyphs (`«`, `»`, `↻`) and
  coloured letter chips. An icon font or an SVG loader is a dependency and a
  light/dark theming problem, for a decoration.
- **Writing to worktree files.** Pillar K, unchanged. The tree changes address,
  not permissions.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

Two stages, matching the two roadmap rows. The first is useless alone and is
still worth separating: it is the half that can be got wrong in a way the
second would hide.

**`R-B40` — the rail shell.** A `RailTool` enum (`Files`, `Search`), a
`rail_panel` declared immediately after `queue_panel`, and two preferences.
Collapsed it is `Panel::right("rail-strip").exact_size(30.0).resizable(false)`
with one glyph button per tool and a tooltip carrying the name and its binding.
Expanded it is `Panel::right("rail")` with a header, a `»` to collapse, and a
body dispatched on the active tool. The strip stays visible beside the open
panel with the active tool lit — that is what the screenshots show, and it is
what makes switching tools one click instead of two.

**`R-B41` — Files.** `explorer_tab` loses its `Panel::left("explorer-tree")`
block; the same body — header, `ScrollArea::both` with wrap forced off, then
`self.explorer_dir(ui, "", 0)` — becomes the Files tool body. The wrap comment
travels with it: the scroll area alone does not stop rows folding at the pane
edge, and a tree whose rows fold onto two lines stops reading as a tree.

Clicking a file from the rail routes through the existing `open_in_explorer`,
which already pins, reveals and raises the Editor. Nothing new is needed for
the bridge; it was built for search results and this is the same shape.

### Files touched

- `crates/mogeung-ui/src/app.rs` — `rail_panel` beside `queue_panel`;
  `explorer_tab` gives up its tree panel; `editor_group`'s rect comment needs
  rechecking once the tree is no longer taking a slice of that ui
- `crates/mogeung-ui/src/prefs.rs` — `rail_open: Option<RailTool>` and
  `rail_width: f32`, both `#[serde(default)]` so an existing `prefs.json` loads
- `crates/mogeung-ui/src/keymap.rs` — an action per tool plus a toggle,
  registered like `ToggleQueuePanel`
- `docs/design/architecture.md` — on ship, not before

### Risks and unknowns

- **Panel declaration order is load-bearing.** A `CentralPanel` claims whatever
  is left, so every edge panel must be declared before `detail_panel`. The rail
  goes directly after `queue_panel`, which also puts it *over* the status bar's
  row — exactly what the queue already does. Choosing differently for the two
  edges would be visible as an asymmetry.
- **`max_rect` versus `available_rect_before_wrap`.** An `egui::Panel` moves
  its parent's *cursor*, not its `max_rect`, so a leftover region drawn after a
  panel still claims the whole pane. `Self::zoom_over` exists for exactly this
  and documents it; the rail adds new leftover regions, and `editor_group`'s
  reason for using `available_rect_before_wrap` changes when the tree leaves.
- **Panel widths do not survive a restart today.** `eframe` is pulled in
  without the `persistence` feature and `App` has no `fn save`, so egui's own
  `PanelState` dies with the process — every panel width in this window already
  resets on launch. The rail's width has to ride in our prefs like
  `terminal_panel` does, written on pointer-up only so a drag does not touch
  the disk on every frame.
- **The Editor tab stops being self-contained.** Accepted in ADR-0017 and worth
  restating: with the rail collapsed there is no tree anywhere, and that is a
  regression for anyone who used the Editor alone.
- **"No session selected" becomes reachable.** The tree is per-session
  (`ensure_session`) and was previously only drawn inside a tab that already
  had one. The rail is always visible, so the empty state is now a real state
  and needs the `R-J5` treatment.
- **`egui_tiles` drag onto the rail is unverified.** Dropping a tab over the
  rail must do nothing rather than something surprising. Check before shipping;
  it is the one place the two docking models touch.

### Test strategy

- Prefs round-trip for `rail_open` and `rail_width`, and an old `prefs.json`
  without either field still loading — `prefs.rs` has both patterns already.
- A test that would fail today rather than one documenting what works: assert
  the Editor tab renders no tree. That is the assertion that the *move*
  happened, which is the part of this that can silently half-land.
- The rest is visual and belongs on the dogfooding checklist: clicking,
  dragging, the strip surviving a collapse, the zoom carrying over, and the
  empty state.

## Notes

Built 2026-08-03, in one pass with [0028](0028-global-search.md).

**The plan's biggest miss was the fetch block.** The tree does not fetch its own
listings — `explorer_tab` did, in a block whose own comment says it lives in the
paint "so a pane that is *docked* visible works without ever having been
switched to". Moving the tree without moving that would have shipped a rail that
showed `listing…` for ever unless you also opened the Editor. It came out as
`explorer_fetch`, called by both, and every branch already guarded on `pending`
— so running twice in a frame sends once, which is what makes two callers safe.
That guard was load-bearing before this row existed and nobody had had to notice.

**`editor_group`'s comment was wrong the moment the tree left.** It explained
that group 0 uses `available_rect_before_wrap` because the *tree panel* had
taken a slice of that ui. The tree is gone; the reason is now the split panel.
The code was right either way and the comment would have quietly misled whoever
read it next.

**The Editor's empty state was the honest cost of ADR-0017 showing up
immediately.** "Pick a file to read it" is useless advice when the thing you
pick from is on the other side of the window and possibly collapsed. It now
names the two keys that get you a file — the rail, and go-to-file — which is
the first place the trade recorded in the ADR is actually visible to a user.

**Two smaller things.** `Prefs` is one `serde_json::from_str`, so an unknown
tool name from a newer build would have cost every setting in the file; `rail`
deserialises forgivingly for that reason, with a test that fails without it.
And the strip is declared *before* the open panel so it keeps the outermost
edge — declared the other way, opening a tool slides the strip inboard and
moves the very button you are about to press to close it.
