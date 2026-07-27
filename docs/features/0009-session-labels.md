---
title: Session labels
status: in-progress
updated: 2026-07-27
roadmap: [R-B26]
depends_on: [A13, A17]
---

# 0009 — Session labels

Name a session yourself. The label shows on its queue card as a colour badge —
the same furniture as `WAITING` and `PIN` — and the filter learns `label:`.

## Spec

### Problem

Sessions are named by their derived title — the last prompt, mostly — and at a
dozen sessions those names stop distinguishing anything: three say "fix the
tests", two are experiments you think of as *the risky one* and *the safe one*,
and none of them say which is which. The queue exists to answer "where do I
look"; a name the user chose is the strongest key their memory has, and today
there is nowhere to put it.

Asked for directly:

> can we add a feature to add a Label to the sessions list? so that I can name
> and Label it myself, also we need to be able to filter by label. "\<label\>"
> should appear with a color badge like "WAITING", "PIN" does.

### Assumptions

- **A13** — the user drives by keyboard. `SUPPORTED`.
- **A17** — hand-applied session labels are worth maintaining by hand.
  `SUPPORTED` by the direct ask, with the standing A15 caveat: whether labels
  are still being applied in week two is what actually decides it.

### Where the state lives

Client-side, in `~/.mogeung/prefs.json`, exactly where pinned and hidden
already live — a label is triage view-state, and `prefs.rs` opens by arguing
why that class of state is not daemon state. The cost is the same one pins
already pay: the web client does not see labels. If that ever hurts in
practice, labels move into the daemon's store alongside snooze — a migration,
not a redesign, and it takes pins and hidden with it or none of them.

### Acceptance

- [ ] A session can be given a label from its card (right-click) and by key
      (`L`) for the selected session; typing a new label replaces the old;
      saving an empty one removes it
- [ ] The label shows on the queue card as a colour badge alongside `PIN` and
      the state badge; the colour is stable for a given label text and the
      same for every session sharing that label
- [ ] `label:x` in the queue filter narrows to sessions whose label contains
      `x`; plain free text also matches labels; the filter hint mentions it
- [ ] Clicking the badge filters the queue to that label, the same gesture as
      clicking the repo name
- [ ] Labels survive a restart, and a corrupt prefs file costs the labels
      nothing more than it already cost the pins

### Explicitly out of scope

- Multiple labels per session. One label is a name; a tag system is a
  different feature and there is no evidence anyone wants it yet.
- Sharing labels across clients — see above.
- Auto-labelling, label suggestions, or any inference. The entire point is
  that the user chose the name.

## Plan

*Drafted and approved with the ask 2026-07-27; implemented the same day.*

### Approach

`Prefs` gains `labels: BTreeMap<SessionId, String>` with `label`/`set_label`
(set with an empty string removes — one door for both). The filter's `Query`
gains a `label` field (`label:` / `l:`), and `matches` takes the label as a
parameter since a `Session` does not carry it; the label also joins the
free-text haystack so plain typing finds named sessions.

The badge colour is derived, not chosen: FNV-1a over the label text picks
from a fixed palette of badge-safe colours, so the same label is always the
same colour, on every card, with no colour-picker UI to build or persist.

Editing is a small centred window with one text field — Enter saves, Escape
cancels, empty removes — opened from the card's context menu or the `L`
binding for the selected session. A new `LabelSession` action joins the
keymap and therefore the palette for free.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-ui/src/prefs.rs` | `labels` map, accessors, tests |
| `crates/mogeung-ui/src/filter.rs` | `label:` field, label in the haystack, tests |
| `crates/mogeung-ui/src/ui.rs` | `label_color` palette hash |
| `crates/mogeung-ui/src/app.rs` | badge on the card, click-to-filter, context menu, edit window |
| `crates/mogeung-ui/src/keymap.rs` | `LabelSession`, default `L` |

### Risks and unknowns

- **Labels with spaces** filter imperfectly on badge-click: the filter box
  splits on whitespace, so `label:big refactor` parses as `label:big` plus
  free text `refactor`. Both still have to match — the label is in the
  haystack — so the result set is right in practice; the box just reads
  oddly. Accepted for v1 over inventing quoting, which the filter
  deliberately does not have.
- **Palette collisions**: two labels can hash to one colour. Eight colours,
  a handful of labels — acceptable, and the text disambiguates.

### Test strategy

Prefs: set/replace/remove round-trip through JSON, partial-file tolerance.
Filter: `label:` narrows, `l:` alias, free text matches labels, sessions
without labels are excluded by a label query. Colour: stability and palette
membership. The edit window is egui and not re-tested.

## Notes

**The badge leads the row.** It renders before `PIN` and the state badge, not
after — it is the one badge the user wrote, so it is the one they scan for.
Clicking it filters, the same gesture the repo name already taught.

**`filter::matches` stayed pure.** A `Session` does not carry the label (it
is client state), so the function takes it as a parameter rather than the
module learning about `Prefs`. Every call site is the one in
`visible_queue`; the tests pass labels in directly.

**The editor is the palette's little sibling**: one re-focused text field,
Enter saves, empty removes, Escape cancels — and it previews the badge in
its actual colour while you type, so the colour a label gets is never a
surprise. The `L` binding opens it for the selected session; the context
menu says "Label…" or "Edit label…" depending on which it is.

**Nothing changed on the wire or in the daemon.** The whole feature is
client-side, which is what made it an `S`.
