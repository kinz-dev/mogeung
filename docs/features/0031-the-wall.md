---
title: The wall
status: shipped
updated: 2026-08-07
roadmap: [R-B50]
depends_on: [A1, A6]
---

# 0031 — The wall

Every session as a tile, on a chord, in positions that never move.

## Spec

### Problem

The Attention queue answers *which session needs me* as a **ranked list**, and
the ranking is its whole value. But a ranked list reorders. The row for
`dotfiles` is in a different place every time you look, so spatial memory never
forms, and noticing a change means reading rather than glancing.

The failure that follows is specific: an agent sits on a permission prompt for
four minutes while you read the top of the list. Nothing is broken — the queue
is doing exactly what it says — but the thing you needed to notice was three
rows down and looked like every other row.

Designed on 2026-08-06 alongside [feature 0030](0030-two-agents-at-once.md) and
deliberately deferred; built on 2026-08-07 at an explicit ask to clear the
backlog in one pass.

### Assumptions

- **A1** — a cross-session attention queue changes where the user looks.
  `SUPPORTED`. This row is the uncomfortable one for it: the queue's claim is
  that it **tells** you who needs you, and someone scanning six tiles has gone
  back to looking. That tension is the reason the wall is a chord rather than a
  view you can leave up.
- **A6** — 3–4 concurrent sessions in normal work. `SUPPORTED`. Below about
  three tiles the wall has nothing to say that the queue does not.

Neither is `UNTESTED`, so this is buildable rather than an experiment to run
first — but see *Still unproven* below, because it is a bet against a settled
assumption rather than a bet on an open one.

### Acceptance

- [x] A chord opens every session as a tile, and the same chord closes it
- [x] A tile's position does not change when its session changes state
- [x] A tile shows what its session is doing, without fetching anything
- [x] The tiles that want you are visibly different from the ones that do not,
      using the queue's own verdict rather than a second opinion
- [x] Clicking a tile goes to that session and leaves
- [x] Sessions hidden from the queue are hidden here
- [x] Only sessions that are still running get a tile
- [x] `Esc` leaves without changing the selection

### Explicitly out of scope

- **Live terminals in the tiles.** Designed and rejected in the same
  conversation: six `tmux attach`es is six ptys, and an 80-column TUI in a
  260px tile is illegible mush. Noticing needs three lines, not eighty columns.
- **Arranging it.** It is not a pane and not chrome — see the Notes.
- **Leaving it open.** There is no preference for that, on purpose.

## Plan

### Approach

An overlay over the whole window, gated on one boolean in the store, rendering
a tile per queue entry from data the snapshot already carries.

**Nothing is fetched.** `last_activity`, `recent_tools`, `error`, `git_branch`
and the attention reason are all in the session the window already holds, which
makes the wall free to open and makes it work for sessions with no
`tmux_target` — the ones the Agent pane has to refuse.

**Tiles are keyed by session id, not by score.** This is the entire claim over
the queue and it is one line of code, which is exactly why it is the first
thing a test pins.

### Files touched

| Path | Change |
|---|---|
| `desktop/src/ui/WallOverlay.tsx` | new — the overlay, the tiles, the ordering |
| `desktop/src/store/index.ts` | `showWall` |
| `desktop/src/lib/keymap.ts` | `wall`, on `Alt+W` |
| `desktop/src/App.tsx` | mounts it beside the other overlays |

### Risks and unknowns

- **It competes with the queue**, which is the product's thesis. Stated in A1's
  terms above rather than treated as a UI detail.
- **A chord is a weaker gesture than intended.** Hold-to-peek was the design;
  toggle is what shipped, and the reason is in the Notes.

### Test strategy

The ordering, first and hardest: a session that outranks another in the queue
must still sit where its id puts it. Then the wiring — the queue's own
`needsHuman` decides the ring, a hidden session is hidden, a queue entry with
no session is skipped, clicking selects and closes, `Esc` closes without
selecting.

## Notes

**Hold-to-peek lost to xterm.** The design was Mission Control: hold `Alt+W`,
the wall spreads out, release on a tile. What shipped is a toggle with `Esc`,
because the centre of this window is usually a terminal that handles keys
aggressively, and a peek whose `keyup` never arrives is a wall stuck open with
no way to say so. Toggle degrades to "press it again"; hold degrades to a
broken window. Flagged as a risk when the wall was first sketched on
2026-08-06, and it turned out to be the real constraint rather than a worry.

**The tiles are a contact sheet, not a wall of terminals**, and the difference
is what makes the feature cheap. The expensive version was designed first and
its cost was the argument against it: `tmux attach` per tile, a pty per tile,
and text too small to read. What the snapshot already streams —
*what is it doing*, *what has it been reaching for* — turns out to be the
signal, and it arrives for free.

**Fixed-height tails, deliberately.** A tile whose height rides on its content
is a tile that moves when its session gets busier, which is the exact failure
the sort order exists to prevent. Three lines, clipped, always.

**It is neither a pane nor chrome, and that is not a hole in
[ADR-0017](../decisions/0017-the-rail-is-chrome.md).** That rule decides where a
thing *lives* — the tile tree for views of a session, the edge panels for tools
that outlive the selection. The wall does not live anywhere: it cannot be
arranged, cannot sit beside anything, and holds no state but a boolean. A third
docking idea would have needed the rule; an overlay on a key does not.

**Live only, decided on the first look at it.** The wall shipped showing
everything the queue held, which was the wrong default and took one viewing to
find out. The queue carries dead sessions deliberately — an ended session can
still be `needs_review` and still want you — but a *tile* earns its square by
being something that might change while you are watching, and a grid where most
squares are inert is a grid you stop scanning. Reading the queue's own `live`
scope instead was the alternative and lost on a rule this surface has to obey:
the wall would then mean different things depending on a filter set somewhere
else, which is exactly the kind of "why is this empty" that a glanceable thing
must never have.

**Still unproven, and this is the honest part.** Every other row this week could
be judged by whether it works. This one can only be judged by whether it
*replaces* something: if the wall becomes where you live, the finding is about
the queue's ranking rather than about the wall, and the right response is to fix
the ranking rather than to grow the wall. **Removal condition agreed in
advance:** a dogfooding week in which the wall is opened and closed without
changing what you do next means it is a nicer way to see the same answer, and
it comes out.
