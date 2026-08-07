---
title: Session labels
status: shipped
updated: 2026-08-07
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

- [x] A session can be given a label from its card (right-click) and by key
      (`L`) for the selected session; typing a new label replaces the old;
      saving an empty one removes it
- [x] The label shows on the queue card as a colour badge alongside `PIN` and
      the state badge; the colour is stable for a given label text and the
      same for every session sharing that label
- [x] `label:x` in the queue filter narrows to sessions whose label contains
      `x`; plain free text also matches labels; the filter hint mentions it
- [x] Clicking the badge filters the queue to that label, the same gesture as
      clicking the repo name
- [x] Labels survive a restart, and a corrupt prefs file costs the labels
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

**Labels survive `/clear` (found in dogfooding, 2026-07-28 — twice).**
Claude Code's `/clear` keeps the process but mints a new session id, so a
label keyed by id died with the old one — reported as "the label will be
reset?!". The live registry is per-*pid*, which makes succession a fact
rather than a guess: when a dead session and a live one share a pid *and*
cwd (pids get reused; directories disambiguate), the label and pin move
to the successor. Conservative by rule — a label never overwrites one the
successor was given by hand, and two live sessions never trade state.
`Prefs::migrate_succession`, pinned by tests.

The first attempt shipped broken and the report came straight back: the
daemon wiped `pid` in the same scan that marked a session dead, so the
client never saw two sessions sharing one — the migration's evidence was
destroyed one layer below it. Dead sessions now keep their last pid
(`state.rs` says why in place); `focus_terminal` and the Info pane's pid
stat gate on `alive`, so nothing treats the remembered pid as live.

**And a third time, on 2026-08-05, in the other client.** Reported the same
way — "when I clear a conversation the label I applied manually is gone" —
because the React client was ported from `prefs.rs` *without*
`migrate_succession`. The state it keys by session id was ported; the one
function that knows session ids do not survive `/clear` was not. Nothing
about the port made that visible: labels worked, filtering worked, and the
missing piece only shows up on a gesture no test in that client made.

It is now `migrateSuccession` in `desktop/src/store/prefs.ts`, run wherever
sessions arrive, moving the colour tag as well — tags are keyed by id the
same way, and would have been the next report. Its tests mirror the Rust
ones case for case, deliberately: two clients agreeing on *what counts as a
successor* is the only thing stopping the same label landing on different
sessions in two windows.

The daemon had a matching hole one layer down, and it is the July bug again:
`AppState::load` wiped `pid` on every session it read back from the
database, so a `/clear` that straddled a daemon restart lost the evidence
either client needs. The death path had already been fixed and says why in
place; the load path was never looked at. Both now keep it.

**A fourth time, on 2026-08-07 — and the migration was running.** `R-J15`.
Reported as *"the ATTENTION label disappears and the current tmux session
goes blank, I have to click on the session list again"*, which is two
failures wearing one gesture.

*The label.* Succession picked the predecessor by `started_at`, and
`started_at` is not what its name promises: the daemon overwrites it every
scan from the live registry's `startedAt`, which Claude Code writes once per
**process**. A terminal open since Tuesday has a `/clear` for every topic it
has been through, and every one of those dead ids reports the same
`started_at` — the moment the terminal was opened. The comparison meant to
find the newest predecessor therefore always tied, and the tie kept whichever
id the session map happened to hold first, which is the *oldest*. The label
moved onto a conversation that had ended two clears ago, or did not move at
all. This was invisible for as long as processes were short-lived, which is
the honest reason three fixes and their tests all passed over it.

It is `last_event_at` now — the last line each session actually wrote, the
one field that does order a chain — and the rule reads the whole **line**
rather than the immediate predecessor: each thing moves from the most recent
id that still has one. That second part repairs a hop nothing was open to
make, which the old rule stranded permanently by asking an id that had never
held the label whether it held the label.

*The blank pane.* Nothing ever moved `selected`, or a pane held by `R-B49`.
The successor arrived correctly named while the Agent pane below it stayed
attached to an id whose tmux pane died with the old conversation — so the
label survived and the window still had to be re-clicked, which is what the
report is describing. Both now follow, and the split in how is deliberate:
a label, tag or pin is *identity*, so it follows every pass and settles once
the live head holds it; the selection and a held pane are *placement*, so
each predecessor donates its hop **once**. A view that re-points itself on
every tick would make a finished session impossible to sit and read — a worse
window than the one that leaves a pane blank, and one the tests in
`store/succession.test.ts` now forbid.
