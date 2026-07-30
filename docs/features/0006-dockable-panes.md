---
title: Dockable panes
status: shipped
updated: 2026-07-26
roadmap: [R-B20]
depends_on: [A14]
---

# 0006 — Dockable panes

The five detail tabs become tiles you can split, drag and resize, so two of
them can be on screen at once.

## Spec

### Problem

The detail pane shows exactly one of Changes, Transcript, Info, Debt and
Agent. Reading a diff while watching the transcript that produced it means
alternating between two tabs and holding one of them in your head — which is
precisely the work a second pane would do for you.

Asked for directly:

> can we make the tab panel to a movable dockable panel. For example user could
> actually arrange the window and put info and debt side by side.

### Assumptions

- **A14** — the user wants two detail views at once and will arrange them
  rather than leaving the default. `SUPPORTED` by the request itself, with the
  honest caveat that asking for a layout is weaker evidence than keeping one.

### Acceptance

- [x] The default layout is the old tab bar exactly — nothing moves until you
      move it
- [x] A tab can be dragged out to sit beside, above or below another
- [x] Splits can be resized
- [x] The arrangement survives a restart
- [x] A layout that fails to load falls back to the default rather than
      refusing to start
- [x] The tab shortcuts still work, and re-open a pane that was closed
- [x] There is one obvious way back to the default layout

### Explicitly out of scope

- Docking the **queue** or the status bar. The queue is the one thing that is
  always the same shape, and a layout where it can be hidden defeats the point
  of the app.
- Tearing a pane into a separate OS window.
- Per-session layouts. One arrangement for the window.

## Plan

### Approach

`egui_tiles` for the tree — same authors as egui, tracks its versions, and
models exactly the tabs / horizontal / vertical / grid containers wanted here.
`Tab` becomes the pane type, so the pane identity that the keymap already
targets is unchanged.

**The default tree is a single tab container holding all five panes**, which
renders and behaves like the tab bar it replaces. This matters more than it
sounds: a docking system that greets you with a novel arrangement makes you
undo something before you can work.

`Tab` continues to mean *the pane the keyboard is aimed at*, which is a
distinction that did not exist when only one pane was visible. Scroll and tab
actions target it; clicking a pane sets it.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-ui/src/layout.rs` | new — tree construction, persistence, focus |
| `crates/mogeung-ui/src/app.rs` | detail panel renders a tree; keyboard over panes |
| `crates/mogeung-ui/src/keymap.rs` | `ResetLayout` |

### Risks and unknowns

- **The saved tree is `egui_tiles`' own serde shape**, not ours. A version bump
  can change it, and the layout is the one piece of state we cannot regenerate
  from the daemon. Mitigated by falling back to the default on any load error,
  which is the same rule prefs and the keymap already follow.
- **Keyboard focus across several visible panes is genuinely ambiguous.** With
  one pane there was nothing to decide. The answer here — an explicit focused
  pane, moved by clicking or by the tab keys — is a choice, not a discovery.

### Test strategy

The tree is `egui_tiles`' and not worth re-testing. What is ours and testable:
the default layout contains every pane, a corrupt saved layout yields the
default instead of an error, focus cycling visits visible panes in order and
wraps, and a pane closed and re-requested comes back.

## Notes

**The borrow problem has a boring answer.** `egui_tiles` renders through a
`Behavior` that needs `&mut App`, and the tree lives *on* `App`. Taking the tree
out of `App` for the duration of the call — `self.tree.take()`, render, put it
back — makes the two borrows sequential instead of overlapping. The tree is
`Option` purely for that, and is `Some` at every other moment. Worth a comment
where it is declared, because an `Option` field with no `None` state otherwise
reads like an oversight.

**`UiResponse::None` from the pane body, always.** Returning `DragStarted`
would let a pane be dragged by its content, which fights every scroll area,
text selection and terminal inside it. The tab is the handle.

**Every action needed a binding, including this one.** `ResetLayout` failed
`every_action_has_a_default_binding` — an invariant whose stated reason ("an
action with no default is one the user can never discover") the command palette
has actually made obsolete. Kept anyway, and gave reset `Alt+0`: a binding is
still the fast path, and the invariant costs one decision per action. Reset is
deliberately not adjacent to anything pressed often, because it discards an
arrangement you may have spent a while on.

**Closing a pane had to be made reversible before it could be allowed.** A
close button that needed a full layout reset to undo would be a trap, so the
pane's own shortcut re-inserts it. That is what makes `is_tab_closable` safe to
return `true`, and it is pinned by a test rather than left to memory.

**`TAB_ORDER` and `next_tab` are gone.** Cycling now depends on which panes
exist, so a fixed array was not just redundant but wrong — it would have cycled
to a pane that had been closed. Their two tests moved to `layout.rs` and now
exercise the real function. Deleting a test is usually suspicious; these were
replaced, not dropped.

**The "drag a tab out to split" hint retires itself** once anything has been
split. A hint that stays up after you have visibly learnt it is just furniture.
