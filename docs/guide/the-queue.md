---
title: The attention queue
status: active
updated: 2026-07-25
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
| `j` / `k` | move down / up the queue |
| `enter` or `o` | switch to the terminal app that session runs in |
| `r` | mark everything in its diff read |
| `s` | snooze 30 minutes, or wake it |
| `g` | jump to the top of the queue |
| `/` | jump to the filter box |
| `esc` | clear the filter, close ambient mode |

Keys are ignored while a text box has focus, so typing in the filter does not
trigger them.

**`Ctrl+Cmd+M` brings mogeung back** from anywhere — the return half of the
round trip. Change it with `mogeung --hotkey "Alt+Space"`, or turn it off with
`--no-hotkey`. Hover the "mogeung" title to see the current one.

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
