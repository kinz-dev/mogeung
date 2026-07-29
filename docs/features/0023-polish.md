---
title: Polish — geometry, config, CLI, empty states, diff speed
status: in-progress
updated: 2026-07-29
roadmap: [R-J1, R-J2, R-J3, R-J4, R-J5, R-J6]
depends_on: [A19]
---

# 0023 — Polish

The `J` pillar, numbered and built. Comfort and speed rather than capability:
nothing here shows you anything mogeung could not already show you.

## Spec

### Problem

Pillar `J` was six items of prose in the roadmap, unnumbered, and so the
question *"what is left to build?"* could not be answered from the file that
exists to answer it. Checking the prose against the code found it half true —
"persist UI state" was already done for nineteen fields and never done for the
window itself.

The individual pains, in the order they are met:

- The window opens at 1440×900 every time, wherever it was left. Everything
  else about the layout is remembered, which makes the one exception worse
  than a consistent lack of memory would be.
- Both binaries are configured by flags only, so a preference means editing a
  script or retyping an invocation.
- Nothing about a running daemon is reachable from a terminal. There are forty
  REST endpoints and no way to reach them without `curl` and a JSON reader.
- Seventeen places say a variant of "nothing here", where nothing distinguishes
  *no data* from *the fetch failed*.
- A large diff draws every line of every hunk each frame, whether visible or
  not. Whether that is actually slow has never been measured.
- There is no light theme.

### Assumptions

- **A19** — the git pane earns commercial-grade reading depth. `SUPPORTED`.
  Only relevant negatively: `R-J5` and `R-J6` scale with the number of panes,
  and A19's own note says an unused section is a removal candidate. That is why
  both are sequenced last.

No item here rests on an `UNTESTED` assumption, which is what makes the pillar
safe to build during the dogfooding week rather than after it. That is also its
honest limitation: polish cannot be wrong the way a feature can, so nothing
here tests anything.

### Acceptance

- [x] The window reopens at the size and position it was closed at, and a
      position on a monitor that no longer exists does not lose the window
- [x] A setting written in `~/.mogeung/config.toml` is honoured by both
      binaries, and the matching flag still overrides it
- [x] A corrupt config file starts the daemon anyway, with a warning
- [x] `mogeung queue` prints the queue in a terminal; `--json` prints the body
- [x] Every empty state says which of "no data" and "could not load" it is
- [x] A large real diff scrolls without dropping frames — or the measurement
      shows it already did, and `R-J2` closes unbuilt
- [x] (`R-J6`) The light theme is legible in every pane, diffs included
- [x] (`R-J6`) Switching theme takes effect without restarting, and survives one

All six are built and installed; like everything else awaiting
[item 0](../product/roadmap.md#0-the-non-feature), they are ticked as *done*,
not as *judged*.

"Legible" is asserted by contrast tests over every pair the palettes promise —
text on each of four surfaces, accents on panels, diff text on its own tint and
on the word-diff band, syntax tokens on both tints, badge lettering on every
fill, read against unread, and each graph lane against its neighbours. It is
also *looked at*: both themes were captured from a running window. What was not
visually confirmed is the diff pane specifically, because synthetic input never
reached egui through XWayland — the tests cover it, a screenshot does not.

### Explicitly out of scope

- Any new view, signal or datum. This pillar adds no capability.
- A CLI that wraps all forty endpoints. Six are chosen; see the plan.
- Theme *customisation*. Two themes, not a palette editor.

## Plan

### Approach

**`R-J1` — geometry in our own store.** `prefs.rs` is already the client's
view-state store; geometry joins it as one more field. Deliberately not
eframe's `persistence` feature, which would write a second store in a different
format holding a divergent copy of the same kind of state. Restoring requires
reading prefs before the viewport is built, so the load moves ahead of
`run_native` and `App::new` takes what was already read.

A restored position is validated against the monitors that exist now. Failing
that check drops the position and keeps the size — a window that opens in the
wrong place is a nuisance, and a window that opens off-screen is lost.

**`R-J2` — measure, then decide.** The renderer walks every line of every hunk
each frame, and with syntax on, every token is its own label. Whether that
matters is an open question, not a known defect, so the first commit is the
measurement and it is allowed to close the row.

If it is slow, the obstacle is `horizontal_wrapped`: `ScrollArea::show_rows`
needs a uniform row height. Diff lines would move to fixed-height rows that
scroll sideways rather than wrap — which is what diff viewers do, and the
precedent is already set here (`8795688`, narrow panes scroll sideways instead
of folding rows).

**`R-J3` — file under flags.** One `config.toml` for both binaries; precedence
is flag, then file, then default. Parse failures warn and fall back, following
the rule the transcript parsers already follow: degrade, never refuse. The
window keeps its hand-rolled argument parsing.

The environment layer named in the first draft of this plan was dropped: no
`MOGEUNG_*` setting exists today, so it would have been a new interface nobody
asked for, sitting between the two that were.

**`R-J4` — six subcommands, not forty.** `mogeung` with no arguments still
opens the window; a leading bare word dispatches instead. `queue`, `sessions`,
`health`, `rescan`, `diff`, `search`, each with `--json`.

**`R-J5`, `R-J6` — last.** Both scale with pane count, and the dogfooding week
may delete panes.

`R-J6`'s shape: a `Palette` struct of ~35 fields, two `const` instances, and a
process-global `pal()` behind them. The names survived — `RED` became
`pal().red` — so the 232 call sites changed mechanically and read the same
afterwards. A preference (`dark`/`light`/`system`) lives in `prefs.json` beside
the rest of the view state, with `Alt+T` to cycle and a selector in the
settings window.

### Files touched

- `crates/mogeung-ui/src/prefs.rs` — geometry field
- `crates/mogeung-ui/src/main.rs` — early prefs load, viewport from it, CLI
  dispatch, config file
- `crates/mogeung-ui/src/app.rs` — geometry capture, empty states, palette
- `crates/mogeung-ui/src/ui.rs` — palette
- `crates/mogeung-ui/src/theme.rs` — the two palettes and the switch (`R-J6`)
- `crates/mogeung-ui/src/cli.rs` — subcommands (`R-J4`)
- `crates/mogeung-ui/src/keymap.rs` — `CycleTheme`
- `crates/mogeung-core/src/config.rs` — the config file both binaries read
- `crates/mogeungd/src/main.rs` — config file

### Risks and unknowns

- **The measurement may close `R-J2`.** That is the point of doing it first,
  but it means the pillar's one performance item may deliver nothing but a
  recorded number.
- **Geometry across monitors** is the item most likely to have a case we do not
  own hardware to hit — a scaled second display, a monitor that returns
  bounds late.
- **`R-J6` is a mechanical change of 232 sites** and mechanical changes at that
  scale are where a wrong colour hides. The compiler catches the refactor; it
  cannot catch a legible-in-dark, invisible-in-light choice.
- **Polish during the dogfooding week edits the thing being judged.** Kept to
  items that cannot change a verdict.

### Test strategy

Round-trip and precedence tests, which is most of what this pillar can be
tested for: geometry survives a save/load, an off-screen position is discarded,
a flag beats a file value, a malformed file yields defaults. The CLI is tested
against a stubbed server rather than a live daemon. Nothing spawns an agent.

## Notes

**The `R-J2` measurement, in full.** Release build, largest commit in this repo
(`dd899ec`, 7,399 diff lines), the real renderer driven headlessly:

| lines | before | after |
|---|---|---|
| 500 | 1.8 ms | 0.34 ms |
| 2,000 | 6.7 ms | 0.28 ms |
| 7,399 | **30.3 ms** | **0.28 ms** |

30ms is 33fps while scrolling, so the row was worth building — and the debug
build's 400ms was worth ignoring, which is why the measurement was taken twice.
Afterwards the cost is flat in diff size, which is the actual property wanted:
a diff costs what is on screen. The uncommitted working tree at the time was
7,271 lines, so the worst case was not hypothetical.

**Uniform row height was the whole design problem.** `ScrollArea::show_rows`
and every cheaper culling scheme need to know a row's height without laying it
out, and a wrapped line's height is only knowable by laying it out. Estimating
it from monospace advance width almost works — and fails on word-boundary
wrapping, where the estimate under-counts and the content jitters as you
scroll, which is worse than being slow. Diff rows now do not wrap, and the
Changes tab scrolls sideways like the git panes already did.

**Two of the six items were smaller than the roadmap thought, and one was
bigger.** `R-J5` claimed seventeen ambiguous empty states; reading all
seventeen found the one-go pass had already distinguished loading from empty
almost everywhere (*"reading prompt history…"*, *"scanning transcripts…"*,
*"computing diff…"*), and an unreadable directory never reaches the empty
branch at all because the daemon answers it with an error. Two were genuinely
ambiguous and were fixed. Meanwhile `R-J1` needed a monitor-validity rule that
the one-line roadmap entry did not hint at.

**Wayland reports no window position, and the first version of `R-J1` therefore
did nothing at all.** `outer_position()` returns `NotSupported` there, so egui
leaves both `outer_rect` and `inner_rect` empty — and the tracker, which keyed
everything off `outer_rect`, silently never sampled, never wrote a file and
never complained. Every unit test passed. It was caught by launching the window
against a throwaway `HOME` and finding no `prefs.json` at all, which took about
a minute and is the only reason this shipped working.

The size now comes from the drawing surface, which every platform has, and the
position from `outer_rect` where there is one. Under Wayland the window keeps
its size and the compositor places it — that platform's answer, not a
degradation of ours. Verified both ways on Wayland: a fresh window stored
1440×900 with no position, and a hand-written 1180×742 came back at 1180×742.

Two things fell out of the same rewrite. The zoom factor has to be applied to
both size and position, because egui points shrink as the global zoom grows
while `with_inner_size` is in logical pixels — without it, launching zoomed
would store a smaller window and every restart would shrink it again. And a
not-yet-mapped window reports a size below the minimum, which would otherwise
overwrite a good remembered one with a placeholder.

**`println!` panics on a closed pipe.** `mogeung diff … | head` died with a
Rust backtrace — found in the first minute of using the thing, not in any test,
because the unit tests call the renderers directly and never touch stdout. It
now treats `EPIPE` as the end of the job.

**`R-J6` found a defect in the palette it was extending.** The first run of the
contrast tests failed three times, and every failure was in the *dark* theme —
the one that had already shipped. Two were my thresholds being arbitrary and
were argued down in the test comments. The third was real: white lettering on
an amber badge is **2.36:1**, and amber is `NeedsReview` and `RateLimited`, so
it is the badge most often on screen. Badge ink is now chosen by the fill's
luminance.

That rule is narrower than it first looks. Picking *maximum* contrast would
flip nearly every dark badge to dark ink — the dark accents are all mid-tone,
so dark lettering wins on all of them — which is a redesign of a theme this
work was not asked to touch. The line is drawn at luminance 0.30, which catches
amber (2.36 → 7.8) and urgent (2.84 → 6.5) and leaves the rest as they shipped.

The remaining gap is stated rather than hidden: badge text is 11px, so AA wants
4.5:1, and the dark palette's mid-tone accents sit at 3.0–3.9 with white
lettering. The light palette clears 5:1 on every fill. Closing the dark gap
means either darkening every accent or inverting every badge, and both are
redesigns rather than fixes.

**Two palettes, hand-written, because inverting one does not work.** Lightness
inversion is the shortcut that produces the washed-out look giving "light mode
added later" away. Concretely: a 55%-multiplied blue selection becomes a navy
block with black text on it; the dark diff tints become garish mints; amber
becomes unreadable. Each wanted a decision, not a formula.

Three things only a running window showed. `Visuals::light()` has to be the
base rather than `Visuals::dark()` with colours overwritten, because egui
derives shadows and disabled states from it — a light window on a dark base
gets black shadows over white panels. `graph_colors` had to stop being a
`const` array, since a `const` cannot ask which theme is running. And the light
graph lanes needed re-picking: the first attempt put teal 76 channel-steps from
blue, and two lanes that close cannot be told apart where they cross.

**The daemon's flags had to lose their clap defaults.** `default_value` makes a
value you typed indistinguishable from one you did not, so the file could never
know whether it was allowed to fill a gap. Both defaults moved into constants
applied after the merge, which is also the only way `resolve` could be tested.
