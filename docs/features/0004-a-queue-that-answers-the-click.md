---
title: A queue that answers the click
status: shipped
updated: 2026-07-26
roadmap: [R-B19]
depends_on: [A1, A6]
---

# 0004 — A queue that answers the click

A day of real use against the queue panel, and the small things that made it
feel worse than it is.

## Spec

### Problem

Reported after the first day with the attached terminal:

> The left hand side attention window, when you click on the section, it is not
> very smooth or reactive. Often it is lagging.

Plus three concrete gaps: no quick way to dismiss a finished session, no
right-click anywhere, and clicking a session left you on whatever tab you were
last on rather than in front of the agent.

### Assumptions

- **A1** — a cross-session queue changes where you look. `UNTESTED`. This
  feature does not test it; it removes friction that would confound any attempt
  to.
- **A6** — 3–4 concurrent sessions in normal work. `UNTESTED`, and the reason
  the per-frame cost below mattered at all.

### Acceptance

- [x] Clicking anywhere in a card selects it, and the card shows it is a target
      before you click
- [x] Clicking a session under tmux lands on the Terminal tab
- [x] A finished session can be dismissed by a corner `✕` or by right-click
- [x] A **live** session offers neither, and `h` refuses it with a reason
- [x] Selecting a session with a long transcript does not stutter

### Explicitly out of scope

- Dockable panes (`R-B20`). Wanted, and large enough to need its own decision.
- Any change to what the queue *ranks*. This is about reaching the answer, not
  computing it.

## Plan

### Approach

Three unrelated causes behind one complaint, so the fix is three fixes.

**Cost per frame.** The detail panel cloned the entire `Change` and the entire
transcript out of their maps on **every frame**, because rendering needs a value
that outlives the borrow of `self`. Those are the two largest things the client
owns — every hunk of every file, and every message. So the frame rate fell as a
session accumulated work: the app got slower exactly as a session got more worth
looking at, which is the worst possible direction. Both are now `Rc`, treated as
immutable once received; a clone is a refcount bump.

**Where the click lands.** Every label in a card was stealing the click — see
the Notes. Hovering also draws an outline and changes the cursor now, so a card
looks like the target it is.

**What the click means.** Selecting a session and then hunting for the Terminal
tab is two gestures for one intention.

### Files touched

- `crates/mogeung-ui/src/app.rs` — `Rc` for sessions, changes and events;
  card interaction; `may_toggle_hidden`; open Terminal on click
- `crates/mogeung-ui/src/ui.rs` — `icon::HIDE`
- `crates/mogeung-ui/src/term.rs` — Shift+Enter (see
  [0003](0003-attached-terminal.md))

### Risks and unknowns

- **`Rc` hides mutation.** `events` is appended to on arrival via
  `Rc::make_mut`, which silently deep-copies if a handle is still out. It never
  is — render handles are dropped at the end of the frame — but that invariant
  is now load-bearing and invisible.
- Diff rendering is still O(all hunks) per frame. Untouched here, and the next
  thing to bite if someone opens a very large diff.

### Test strategy

The hide rule is a pure function with a test, because it is the one piece with a
safety property rather than a preference. The rest is layout, which the test
suite cannot see — hence a day of use as the acceptance method.

## Notes

**Selectable labels ate every click on a card.** `selectable_labels` defaults to
on in egui, which gives *every* `ui.label` `Sense::click_and_drag()` so its text
can be selected. Since a container's click target is registered before its
children and egui takes the topmost among widgets tied at distance zero, each
label beat the card it was inside. Only the gaps between labels ever selected a
session — which is exactly how the user described it, after I had already
guessed wrong twice: first that it was frame cost, then that the card merely
*looked* unclickable.

Both of those were real and both are fixed, but neither was this. The tell was in
the report and not in the code: *"I have to click on the empty space."* Empty
space is precisely where no label is. A symptom that specific is worth more than
another pass of reading the rendering path, and I should have taken it literally
the first time.

Nothing looked wrong from inside, either. The click *was* received — it started a
one-word text selection instead of selecting the session, and a text selection
you did not ask for is invisible against a card you are already looking at.

Disabled for the queue panel only. The detail pane keeps selectable text, where
copying a path or an error message is the whole point.

**Registering the card's click needs the Ui's own id, not a fresh one.** egui
registers a `Ui`'s widget rect *before* its children specifically so a container
sits behind what it contains, and `Response::interact` reuses that slot. Calling
`ui.interact(rect, new_id, …)` instead looks equivalent and is not: a new id
lands last, and among widgets tied at distance zero egui takes the topmost. The
card would then have swallowed every button inside it — snooze, pin, hide, and
the new `✕` — while still looking correct. The existing code had this right; it
was nearly "cleaned up" into being wrong.

**A live session cannot be hidden** was the interesting bit of the request. It
is the one place where the obvious generalisation — *let the user dismiss
anything* — is wrong, because dismissing a running agent is how you stop being
told about it while it is still changing your files. The rule lives in one
function, used at all four places that offer the action, because a rule written
out four times gets written three ways.

**Unhiding must never be blocked by that rule.** A session hidden while dead
that came back would otherwise be trapped out of sight by the very guard meant
to protect it. Both directions are pinned by tests.
