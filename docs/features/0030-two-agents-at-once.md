---
title: Two agents at once
status: shipped
updated: 2026-08-09
roadmap: [R-B49, R-J31]
depends_on: [A30, A14, A11]
---

# 0030 — Two agents at once

A pane can be aimed at a session that is not the selected one, so two agents
can be on screen and live at the same time.

## Spec

### Problem

The centre holds one Agent pane and it shows whatever the queue has selected.
Two sessions waiting means clicking one, answering it, clicking the other —
and the first disappears the moment you look away from it. Watching a long
build in one session while answering a permission prompt in another is not
possible at all, which is the exact moment two agents running at once is worth
anything.

Asked for directly, 2026-08-06:

> it will be convenient if I have show two Agent panel side-by-side (dockable
> and allow me to move around) it will be very useful. But the current design
> is on the lefthand side it is the ATTENTION lists and Agent tab will only
> show the one being selected.

The splitting half already works — the centre is a dockview tree and a tab can
be dragged beside itself. What does not exist is a pane whose session is not
`selected`: every pane calls `useSelectedSession()`, so two Agent panes would
be two views of one session, which is not what was asked for.

### Assumptions

- **A30** — the user will keep two agent sessions on screen at once and arrange
  them, rather than returning to one. `SUPPORTED` by the ask, with the standing
  caveat that wanting a layout is weaker evidence than keeping one.
- **A14** — the user wants two detail views at once and will arrange them.
  `SUPPORTED`, and worth separating from A30 rather than leaning on: A14 is two
  views *of one session* (`R-B20`), and this is one view *of two sessions*. The
  docking machinery is shared; the bet is not.
- **A11** — an embedded terminal renders Claude Code's TUI well enough to
  answer a prompt. `SUPPORTED`, in daily use since 2026-08-04. Two attached
  terminals is a quantitative extension of a question already answered.

None is `UNTESTED`, so this is buildable rather than an experiment to run
first.

### Acceptance

- [x] An action adds a second Agent pane; both attach to their own
      `tmux_target` and both render live output at the same time
- [x] A pane can be held on the session it is showing, and then clicking a
      queue row moves the *unheld* panes only
- [x] A pane's tab says which session it is showing, not the word "Agent"
- [x] Whether a pane is held is visible on the tab, without hovering
- [x] Which pane the keyboard is aimed at is visible without clicking anything
- [x] A held pane whose session ends says so and stays where it is, rather
      than silently adopting another session
- [x] The bottom dock, `InfoDock` and the status bar still describe the queue
      selection, exactly as they do today
- [x] A layout saved with three Agent panes restores three, and the saved JSON
      names no session
- [x] There is one header row above the terminal, not two
- [x] The Code tab names the file it is showing, for the same reason
- [x] The Code pane has one header row, not three
- [x] Clicking an Agent pane makes its session the current one
- [x] Clicking a queue row puts that session on screen even when every pane is
      held — raising the pane that holds it, or splitting one that does not
      exist yet (`R-J31`, 2026-08-09)

*"Pinned" became **held** during the build — see the first note below.*

### Explicitly out of scope

- **The wall** — a grid of every live session, `Alt+W`-fashion. Designed in the
  same conversation and deliberately deferred to `R-B50`: it is a bet against
  the ranked queue, and it wants a verdict on this row's pinning before it is
  worth writing. This row is its prerequisite either way, because a wall tile
  click has to land in a pane without changing the selection, which is exactly
  what pinning introduces.
- **Pinning dock tools or rail tools.** Those are chrome, and chrome follows
  the selection by definition — [ADR-0017](../decisions/0017-the-rail-is-chrome.md).
- **Typing into two agents at once.** A broadcast input would be steering a
  fleet, and this product observes
  ([ADR-0003](../decisions/0003-observe-do-not-spawn.md)). Two panes means two
  places to type, one at a time.
- **Per-session layouts**, and **tearing a pane into an OS window**. Both were
  out of scope for `R-B20` and stay out for the same reasons.

## Plan

### Approach

Three changes, and the first is the only conceptual one.

**A session-binding boundary below the pane.** A small context sits where
`ZoomPane` already wraps every pane: it resolves to `selected` by default, and
to a fixed session id when the pane is pinned. `useSelectedSession()` resolves
through it. The consequence is that `AgentPane` never learns it can be pinned —
and neither does `CodePane`, or anything added later, which all become pinnable
for free. Putting the pin *inside* `AgentPane` would make it agent-only and
need re-doing the first time two diffs are wanted side by side.

**Numbered slots, not session-keyed panel ids.** Panels are `agent`, `agent:2`,
`agent:3`. The obvious alternative — a panel id per session — is rejected
because dockview persists the layout, and a layout that names a session
restores a tab pointing at something that ended three days ago. A numbered slot
plus a pin means the *arrangement* survives a restart while the *binding* is
free to be dropped when the session it names is gone. This is also what forces
`PANES` in `App.tsx` to stop being a fixed singleton list, and the
"every pane always present" loop in `onReady` to guard the base slot only.

**One header instead of two.** Today the dockview tab (30px, 11px, uppercase)
and `AgentPane`'s own `PaneHeader` (28px, 10px, uppercase) both say `AGENT`,
which is 58px of chrome per pane saying one word twice — and with the centre
split, each half pays all of it. The pane's header goes; its three controls
(the `tmux_target` chip, the host chip, *raise its window*) move into the
group's right header actions, which is per-group and therefore correct when two
panes sit in separate groups. `text-transform: uppercase` comes off `.dv-tab`
at the same time: it was designed for a fixed vocabulary the app chose
(`CHANGES`, `DEBT`), and a session label shouted as `MOGEUNG/MAIN` is wider and
slower to scan than `mogeung/main`. `sectionLabel` in `styles.ts` is **not**
touched — it heads every panel in the window, and shrinking it here would
ripple into the queue, the dock and the rail.

### Files touched

| Path | Change |
|---|---|
| `desktop/src/lib/paneScope.tsx` | new — the binding boundary, and `paneKind` for reading a slot back to its component |
| `desktop/src/ui/PaneChrome.tsx` | new — the tab that names a session, and the group's controls |
| `desktop/src/store/index.ts` | `usePaneBinding`; `useSelectedSession` resolves through it; `togglePaneHold` |
| `desktop/src/store/prefs.ts` | `ScopedPrefs.paneHold: Record<string, SessionId>` |
| `desktop/src/lib/panes.ts` | `nextAgentSlot`, `splitAgent`, `closeAgentPane`, `dropOrphanHolds`, `resetLayout`, `getDock` |
| `desktop/src/lib/keymap.ts` | `pane.agent.split`, `pane.agent.hold`, `layout.reset` |
| `desktop/src/App.tsx` | panes wrapped in `PaneScope`; `rightHeaderActionsComponent`; orphan holds dropped on load |
| `desktop/src/panes/AgentPane.tsx` | own header removed; the ended state; pty keyed by pane and session |
| `desktop/src/index.css` | `.dv-tab` uppercase off; 30px → 26px; a ring on the active group |
| `desktop/src/panes/CodePane.tsx` | the path row folds into the file strip — one header row instead of three |
| `desktop/src/ui/QueuePanel.tag.test.tsx` | its hand-built `ScopedPrefs` builds on `emptyScoped()` |
| `desktop/src/lib/panes.ts` | `R-J31` — `agentSlots`, `revealSession`; `splitAgent` returns the slot it made or `null` |
| `desktop/src/ui/QueuePanel.tsx` | `R-J31` — both click sites call `revealSession` rather than `select` |
| `desktop/src/lib/revealSession.test.tsx` | `R-J31` — new, the four cases below |

### Risks and unknowns

- **A pin is invisible state, and invisible state looks like a bug.** A pane
  left pinned and forgotten makes clicking the queue appear to do nothing —
  the worst failure mode this design has, because the app looks broken rather
  than configured. The glyph on the tab is not decoration; it is the mitigation,
  which is why it is an acceptance item rather than a nicety.
- **Focus across two live terminals.** `R-B20` called this genuinely ambiguous
  with several panes visible, and two *terminals* sharpens it: xterm swallows
  keys aggressively, and `focusOwns` already defers bare keys to a focused
  terminal. Chords are unaffected, but "which pane does `j` mean" needs the
  visible focus ring to be right before the keyboard work is believable.
- **Two ptys and two `tmux attach`es.** Bounded and fine at two; this is the
  standing argument against ever making a grid of live terminals the default,
  and therefore an argument that belongs to `R-B50` rather than here.
- **A saved layout can outlive its pins.** Three slots restore; a pin naming a
  session that is gone must resolve to *nothing* and say so, not fall through
  to the selection. Falling through is the tempting default and it is wrong: it
  turns a stale pin into a pane that silently shows the wrong agent.

### Test strategy

What is ours and would fail today:

- a pinned pane resolves its own session while `selected` changes underneath it,
  and the unpinned panes follow
- a pin naming a session that no longer exists resolves to `null` and renders
  the ended state, rather than falling back to `selected`
- the next free slot id skips occupied ones rather than reusing a live pane's
- the serialised layout contains no session id, for any arrangement
- a tab's title tracks the session's label when the label changes

dockview's tree is not ours and is not re-tested, the same rule `R-B20` set.

Added by `R-J31`, and three of the four fail against the `select` this replaced:

- one pane, held on somebody else, and a queue click adds a second one bound to
  the session that was clicked — the report, verbatim
- a click on a session some pane already holds raises that pane and adds nothing
- a click while any pane is unheld adds nothing at all, which is the regression
  guard rather than the fix: it is the case that passed before and must keep
  passing, or the layout grows by one on every click
- four held panes have nowhere to put a fifth session, and say so

## Notes

**"Pin" was already taken, and the collision was in the same window.**
`ScopedPrefs.pinned` is the queue's pin — *keep this session at the top of the
list* (`R-B13`) — so a pane pin would have been a second, unrelated pin one
panel away from the first, both hand-applied, both persisted, neither able to
explain itself in a tooltip. The verb is **hold** and the glyph is an anchor.
Worth the unfamiliarity: the alternative was a window where "pinned" answers two
questions, and the spec above was written before anyone looked at the field
names.

**The pty id had to grow a pane in it.** `TerminalView` was keyed
`agent:${session.id}`, which is unique only while one pane can show a session.
Two panes on the *same* session — hold one, leave the other following, which is
a perfectly reasonable thing to do while comparing — would have opened one pty
twice, and the first unmount would have closed it under the second. It is
`${paneId}:${session.id}` now. tmux is happy to hand one session to two clients;
this is what asks it to. Slot one still produces `agent:<sid>`, so nothing that
existed before was renamed.

**`resetLayout` was missing, and only this feature made that matter.** The egui
client shipped one with the tile tree ([0006](0006-dockable-panes.md)) and the
port dropped it — survivable while the centre held one unsplittable pane, and
not survivable now that a chord can put four Agent panes on screen. It came back
on its old `Alt+0`, and it clears holds as well as tiles: a reset that left three
panes moored to sessions you can no longer see would not be a reset.

**A hold has to be dropped in two places, not one.** `closeAgentPane` clears its
own, but a layout that lost a pane some other way — a reset, a hand-edited
`localStorage`, a version that did not have that slot — leaves an entry behind,
and splitting into that number again produces a pane that arrives *already held*
on a session chosen last week. That reads as the split ignoring your selection,
which is a bug report about the wrong feature. `dropOrphanHolds` runs once at
startup against the restored tree.

**The test suite found a real hazard the app is immune to.** Five QueuePanel
tests started failing on `Object.entries(undefined)` the moment anything read
`scoped().paneHold`: that file hand-lists a `ScopedPrefs` rather than building
one, so every field added to the interface afterwards is `undefined` there while
being complete everywhere in the app. The tempting fix — `?? {}` at the reader —
is exactly what `loadPrefs`' own comment refuses, and for good reason: it makes
the next such field silently half-present instead of loudly absent. The fix is
`...emptyScoped()` in the test. Note the shape of the failure: the crash landed
in the *new* feature and the fault was three months old.

**The header merge is not a size change.** Shrinking both rows was the obvious
read of the complaint and would have left the duplication in place at a smaller
size. What was actually wrong is that two headers said `AGENT`, so one of them
had to stop existing — and once the tab carries the session name, the pane's own
header has nothing left to say. The tab dropped `text-transform: uppercase` in
the same pass, which is a small thing with a rule behind it: shouting suits a
fixed vocabulary the app chose, and reads badly over data the user named.

**`sectionLabel` was left alone on purpose.** It is the obvious lever — one
constant, every panel heading in the window — and pulling it would have shrunk
the queue, the dock and the rail to fix the Agent pane. The change is local to
the pane that had the problem.

**Four panes is a stated ceiling, not arithmetic shyness.** Every *visible*
Agent pane is a live `tmux attach` and a pty, and unlike a hidden tab it cannot
be free. The number is in `MAX_AGENT_PANES` with the reason beside it, and the
split button says so when it is disabled rather than going quiet.

**The Code tab followed the same day, and the rule is the general one.** Asked
for as soon as the Agent tab landed — *"can we do that to the Code panel as
well?"* — and it took the same answer because it is the same argument: a tab
names the thing in it, not the kind of thing it is. `Agent` and `Code` were only
ever identifying while the centre held one of each. Two details the Agent case
did not raise. The name comes from **the focused half** of the pane's internal
split (`R-B25`), because a tab pointing at the file you are not reading is worse
than one that says nothing; and it is the basename alone, since the row under
the file strip already carries the full path and a tab wide enough for
`desktop/src/ui/PaneChrome.tsx` leaves no room for the pane beside it. The name
now appears twice — outer tab and inner strip — and that is allowed to stand:
the strip lists *every* open file where the tab names the active one, so they
coincide only when a single file is open. Hover text is where the kind survived.

**And then the Code tab was hidden, one commit after it was built.** The next
ask was to take the outer tab away entirely, and it was the right call even
though it partly undid the previous hour: the pane carried *three* rows of
naming — dockview's tab, its own file strip, its path row — for a surface whose
whole job is showing you a file, and the strip is the only one of the three that
lists more than one thing. So the path row folded into the strip and the group's
header is hidden, 78px down to 28px.

The cost was stated before it was chosen and is worth repeating here, because
nothing in the code will say it later: **dockview's tab is the drag handle**, so
the Code pane can no longer be dragged, split off, or tabbed beside the Agent.
`Alt+C` and `Alt+0` are what keep that a hidden tab rather than a trap. The
tab-naming work from the commit before is *not* deleted — it is the fallback if
a group ever shows its header, and it cost nothing to leave correct.

Two traps in the hiding. A hidden header hides it for **every** panel in that
group, so a Code pane tabbed with the Agent would take the Agent's tab down with
it — hence `ensureCodeAlone`, which moves it out before hiding. And the place to
position it turned out not to be `syncCodePane` at all: `openFile` raises the
pane through `focusPane` from the tree, `Ctrl+P`, a diff row and a search hit,
so by the time the effect ran the panel had already been added *into* the
Agent's group. Positioning it in the effect looked right and did nothing.

**The selection follows the pane you click.** Asked for with three panes open,
and it is the gap the whole feature opened: everything *else* in the window —
file tabs, dock, Info — describes the selection, so a held pane you are working
in leaves all of them pointed somewhere else. Activating a held Agent pane now
writes its session to `selected`. One-way, deliberately: `select` does not touch
holds, so the pane you clicked stays moored. The interaction to know is that a
*mix* of held and unheld panes will pull the unheld ones onto the clicked
session — which is what "unheld" means, and does not arise in the arrangement
this was asked for, where every pane is held.

**The risk in the list above happened, and the glyph was not enough** —
`R-J31`, 2026-08-09. *"A pane left pinned and forgotten makes clicking the queue
appear to do nothing"* was written here as the design's worst failure mode, with
the anchor on the tab as its mitigation. The report came back three days later
in the sharpest possible form: **one** pane, held, and every click on the queue
answered by a window that changed everywhere except the place being looked at.
The glyph did its job — it says *why* nothing happened — but explaining a dead
end is not the same as not having one, and the fix has to be in the gesture.

So the queue stopped calling `select`. *Which session is current* and *put this
session on screen* had been the same call since before a pane could be held, and
holding is exactly what split them: a selection is what the **unheld** panes
follow, so once none of them is unheld it steers nothing. `revealSession` answers
the second question, in three cases and in this order — raise the pane already
holding it; else select, because some pane is following; else split, and let the
new pane arrive unheld so the selection it was just given is what points it
there. Nothing new binds a pane to a session, which is the point: a pane created
this way is ordinary, and unheld, so the person who never anchored anything has
nothing to release afterwards.

Two boundaries worth stating. The order is not arbitrary — a held pane wins over
splitting, or clicking a session you deliberately moored would open a *second*
view of the one agent that already has a home, which is the arrangement this
feature exists not to make. And this is **clicks only**: `j`/`k` and the arrows
still call `select`, because cursoring down the queue is not opening anything,
and a split per keystroke is four panes nobody asked for.

**Still unproven, and the honest list.** The focus ring is CSS on
`.dv-active-group` and has no test — jsdom will assert a class but not that the
thing is *visible*, which is the whole claim. Whether two live terminals fight
over the keyboard in practice is a question `A11` was never asked at this scale.
And `A30` itself is untested by construction: the row is built, the verdict
needs a week.
