---
title: The attention queue
status: active
updated: 2026-09-16
---

# The attention queue

The left panel ranks every session by who needs you. The badge is the category;
the dim line beneath it is the evidence.

| Badge | Meaning | Detail shows |
|---|---|---|
| `APPROVE` | Blocked on a permission prompt — it asked to do something and is waiting on your answer | `needs approval for Bash: rm -rf build/` |
| `WAITING` | Alive and idle — it has finished its turn and wants you to type | `waiting for you — 4m12s` |
| `FAILED` | An API error was recorded | the error, e.g. `server_error` |
| `REVIEW` | Exited, changed files, not all read | `3 file(s), +47 -12 unread` |
| `STALLED` | Alive and busy, silent 5+ minutes | `busy but silent for 8m30s` |
| `running` | Alive and busy, producing output | its current tool call |
| `idle` | Nothing to do | `reviewed` / `ended with no changes` |

Uppercase wants a human. Lowercase is informational. **quiet** controls whether
`idle` sessions are listed at all.

`APPROVE` and `WAITING` both look like "idle" to Claude Code. mogeung tells them
apart by whether a tool call is still unanswered, and ranks `APPROVE` higher:
that session has work in flight it cannot finish, while a `WAITING` one has
already done what you asked.

## Keyboard

| Key | Does |
|---|---|
| `↓` / `↑` | move down / up the queue |
| `[` | collapse or show the queue |
| `Alt+1` | put the keyboard in the queue |
| `Alt+Shift+o` | switch to the terminal app that session runs in |

**This table was wrong until 2026-09-08 and is worth a note.** It described the
egui client and listed five bindings — `o`, `r`, `s`, `g`, `/` — that the
TypeScript window has never had. `j` and `k` *did* exist and were **removed** on
2026-09-08, on a report that they could not be typed into a scratch file: a bare
letter is given to whatever has focus, so a bare letter can be a shortcut or it
can be a character, and `R-L5` made the second one matter. The arrows do the
same job and are owned by an editor when one has focus, which is why they never
had the problem. Every binding is listed, and rebindable, in the shortcuts
window (`R-B12`) — which is generated from the keymap and was therefore right
all along.

Keys are ignored while a text box has focus, so typing in the filter does not
trigger them.

**`Ctrl+Cmd+M` brings mogeung back** from anywhere — the return half of the
round trip. It is fixed: the flags that changed it belonged to the retired egui
window, and the setting has not been rebuilt. If another application owns the
combination, the window says so on stderr and opens anyway.

A shortcut macOS reserves (`Cmd+Space`, `Cmd+Tab`) will appear to register and
then never fire; pick another rather than assuming it is broken.

**Terminal.app and iTerm2** are supported for jump-to-terminal. Terminals without
AppleScript support (Alacritty, Ghostty, kitty) cannot be focused, and a pane
inside `tmux` or `screen` cannot be picked out — the multiplexer owns the tty.
When it cannot work, it says which terminal it found rather than failing
silently.

## Scope

Three buttons above the filter decide what the queue is *for*:

| | |
|---|---|
| **needs you** | waiting, blocked, failed, stalled, unreviewed. **The default** |
| **live** | every session still running, busy or not |
| **all** | everything, including finished and reviewed |

The queue exists to answer *where do I look*, not *what exists*, so it starts
narrow. If it looks emptier than expected the panel says how many sessions are
outside the current scope.

### Under tmux only

**needs you** and **live** show only sessions mogeung can attach to — ones
running in a tmux pane, which is what `yolomo`, `codexmo` and `qwenmo` give
you. A session started in a plain terminal or in iTerm2 is owned by that
terminal and has no pane to host, so there is no Agent pane to open and nothing
to type into: it is a row you can only read past.

**all** ignores the rule entirely, and the terminal button in the panel header
turns it off without leaving the scope you are in. Whenever rows are being held
back the panel says so — `3 hidden — not under tmux` — and pressing that line
shows them.

The cost is worth knowing: a session that has **ended** has no pane either, so
by default **needs you** no longer lists ended-but-unreviewed work. Turn the
rule off, or use **all**, when review is what you are doing.

## Hiding and pinning

`h` hides the selected session; `p` pins it to the top. Both survive a restart
(`~/.mogeung/prefs.json`).

**Hiding is not forgetting.** It is a view filter and nothing else — the daemon
never hears about it, review marks are untouched, and it is reversible from the
`N hidden` button at the top of the panel. "Forget session" in the Info tab is
the destructive one.

A pinned session ignores scope, because a pin that a scope could override would
not be worth setting. Pinning something hidden reveals it; hiding something
pinned drops the pin.

## Filter, group, follow

**filter** (`/`) matches the label, repo, branch, cwd and current activity.
Every word must match, so typing more always narrows.

It also understands fields, which is how you find a session free text cannot
describe:

| | |
|---|---|
| `repo:mogeung` | or `r:` |
| `branch:main` | or `b:` |
| `file:state.rs` | or `f:` / `path:` — matches files the session **touched** |

They combine: `repo:mogeung branch:main retry`. Clicking a repo name in any card
filters to it. An unknown prefix (`todo:`) stays plain text rather than matching
nothing.

**group** collapses the queue by repository. Repos are ordered by their most
urgent session, so the top of the panel is still the top of the queue. Click a
repo header to fold it.

**follow** keeps the top of the queue selected as it changes — useful on a
second monitor, disorienting while you are reading something.

## Snooze

`s`, or the button on the selected row. The session stays visible with a `ZZZ`
badge but drops to the bottom and stops counting as needing you.

**Snooze beats everything, including `FAILED`.** A mute button that failure could
override is one you would never trust.

## Collisions and loops

`⚠ COLLISION` means **another live session is editing the same file right now**
(within 10 minutes). Both sides are warned. This is the one thing only a
cross-session observer can tell you — neither agent can see the other.

It is based on `Edit`/`Write` tool calls, so an agent changing files through a
shell command is invisible to it.

`↻` means the session has repeated the same tool on the same target four times
in its last twelve calls — usually retrying something that is not working. It is
advisory and never changes the ranking, because repetition is suggestive rather
than conclusive.

## Ordering is strict

Scores are 1100 / 1000 / 900 / 800 / 700 / 100 / 0, and the tiebreaker (longest
wait first) is capped at 99. A session can never jump into a more urgent tier, so a
brand-new `FAILED` always outranks a three-day-old `REVIEW`.

## `WAITING` is a fact

Claude Code publishes `busy`/`idle` in its own live registry. mogeung is not
guessing.

`FAILED` is checked before liveness, so a live session that hit an API error
still shows failed. It clears when you send a new prompt.

## The small badge

`live·busy`, `live·idle` or `live` is raw liveness from the registry,
independent of the ranking. Ended sessions show none.

## Top bar

`N waiting for you` (red) · `N need you` (amber, all uppercase categories) ·
`queue clear`.

## Tuning

`stall_secs` defaults to 300 in `crates/mogeung-core/src/attention.rs`. Five
minutes is generous on purpose — a long build is normal silence, not a stuck
agent. Raise it if `STALLED` fires on healthy work.
