---
title: Reachable by keyboard
status: shipped
updated: 2026-07-26
roadmap: [R-B21, R-B22]
depends_on: [A1, A13]
---

# 0005 — Reachable by keyboard

A command palette, a keyboard-driven settings window, a filter that behaves,
and one theme instead of egui's defaults with our colours on top.

## Spec

### Problem

The app is built for someone who navigates by keyboard, and that is exactly
what made it hard to use. **A shortcut you have not memorised is
indistinguishable from a feature that does not exist.** Thirty-four actions
shipped, most of them reachable only by a key you had to already know, and the
one window that could have taught you them needed a mouse to operate.

Three concrete failures behind the general one:

- Pressing `/` to focus the queue filter *also typed a `/` into it*, every time.
- The keyboard settings window was click-only — the mouse was required to
  configure the keyboard.
- Nothing in the window told you what was possible. The queue's hint line named
  six bindings and was wrong about which six mattered.

### Assumptions

- **A1** — a cross-session queue changes where you look. `UNTESTED`.
- **A13** — the user prefers to drive by keyboard and will reach for a palette
  before a menu. `SUPPORTED` — stated directly, and the whole keymap system
  exists because of it.

### Acceptance

- [x] `Cmd+K` opens a palette listing every action, filtered as you type
- [x] Each palette row shows the binding, so using it teaches the shortcut
- [x] An unbound action is still reachable
- [x] The keyboard settings window is fully operable without a mouse — move,
      rebind, reset, search
- [x] `/` focuses the filter without typing a `/` into it
- [x] The filter can be driven end to end: type, arrow, Enter, Escape
- [x] Surfaces, type sizes and radii come from one place

### Explicitly out of scope

- Dockable panes (`R-B20`), still wanted and still its own job.
- Chorded / multi-key sequences. The keymap stores single chords, and `Cmd+K`
  removes most of the pressure for a second modifier layer.

## Plan

### Approach

**The palette is the answer to discoverability, not a menu bar.** It costs one
binding to learn and it shows the binding for everything else, which means it
is a discovery surface that makes itself unnecessary — the opposite of a menu,
which you keep going back to. It is generated from `Action::ALL`, so an action
added later appears in it with no extra work.

Matching is a fuzzy subsequence score shared with the settings window's search,
so the two search boxes in the app cannot behave differently.

### Files touched

- `crates/mogeung-ui/src/palette.rs` — new; matching, cursor, tests
- `crates/mogeung-ui/src/app.rs` — palette window and rows, keymap window
  cursor/search, filter key handling, theme call
- `crates/mogeung-ui/src/ui.rs` — `apply_theme`, surface colours
- `crates/mogeung-ui/src/keymap.rs` — `Action::CommandPalette`

### Risks and unknowns

- **Modal key handling is now four layers deep** — capture, palette, settings,
  terminal, filter — each returning early before the ordinary dispatch. The
  order between them is load-bearing and nothing enforces it but comments.
- The palette re-scores every action on every frame it is open. Thirty-four
  actions, so it does not matter today, and would if actions became sessions.

### Test strategy

The scorer is a pure function and carries the tests, including one that asserts
**every action ranks first for its own label** — a property that catches a
label and a scorer disagreeing, which is the failure that would make the
palette quietly useless for one entry.

## Notes

**Two of the scorer's own tests failed on the first run, and both were worth
having.** The word-start bonus was larger than the contiguity bonus, so typing
`term` ranked the nonsense string `t e r m` — four word starts — above
`terminal`. Fixed by making a contiguous run worth at least as much as a word
start. The second failure was a bad test rather than bad code: it compared
against a label that did not match at all, so it panicked on `unwrap` instead of
comparing scores. Replaced with a synthetic pair that isolates the one property
it was trying to state.

**`/` typing a `/` into the box it just opened** is the same class of bug as the
selectable-labels one in [0004](0004-a-queue-that-answers-the-click.md): egui
delivers `Key` and `Text` for one press, and the field is drawn later in the
same frame with focus already granted. The fix is to drop pending `Text` events
when the shortcut fires. It now happens in two places, which is a hint that a
third will need it.

**Click-outside-to-dismiss did not work as first written.** It asked egui
whether the pointer was over the palette's layer within the whole screen rect —
true wherever the pointer is, so the condition could never fire. Testing against
the area's own rect is both simpler and correct. Worth noting because it would
have looked like a missing feature rather than a bug.
