---
title: Cross-session signals
status: active
updated: 2026-07-25
covers:
  - crates/mogeungd/src/state.rs
  - crates/mogeungd/src/notify.rs
---

# Cross-session signals

Things no single agent can know about itself, because knowing them requires a
view across sessions that none of them has. Roadmap `R-B3`, `R-B4`, `R-B5`,
`R-B7`, and the notification discipline behind `R-C1`/`R-C4`.

This is the strongest argument for the observer model beyond "it takes nothing
away". A wrapper around one agent could never produce any of it.

## Collision warning (`R-B3`)

**Two live sessions editing the same file inside a 10-minute window.**

Both sides are warned, because either might be the one you want to stop. The
cheap version — comparing cumulative `touched_files` — is useless: any two
sessions in a repo eventually overlap, and a warning that is always on is off.
So each touch is timestamped (`Session::recent_touches`, capped at 200) and only
recent ones count.

Recomputed **every scan**, not only when files move, because a collision also
*ends* — one side exits, or the window lapses — and a stale collision warning is
worse than none.

### What it cannot see

Attribution comes from `Edit`/`Write` tool calls, so an agent that changes a
file through a shell command is invisible to it ([A8](../product/assumptions.md)).
It reports overlap, not conflict: two sessions editing different functions in
one file is flagged, and is usually fine.

## Permission vs. instruction (`R-B4`)

See [attention-ranking.md](attention-ranking.md) — an unmatched `tool_use` plus
an idle registry means the session is sitting on a permission prompt rather than
waiting for a new task.

`Session::open_tools` is maintained incrementally: `tool_use` pushes, the
matching `tool_result` removes, and a new human turn clears the list. Sidechain
(subagent) tool calls are excluded — a subagent's pending tool is not something
you can approve.

## Loop detection (`R-B7`)

**The same `tool:target` four times in the last twelve calls.**

Deliberately crude. It catches the common real failure — an agent retrying an
edit that keeps not applying, or re-reading a file it has already read — without
pretending to understand intent.

It cannot distinguish "stuck" from "legitimately doing the same thing to many
similar inputs", which is exactly why it produces an **advisory string** rather
than a queue tier of its own. A heuristic this rough must not be able to
reorder the board.

## Snooze (`R-B5`)

Suppresses a session from ranking until a deadline, checked before every other
rule including `Failed`. Persisted with the session, so it survives rescans and
daemon restarts.

The rule that makes it usable: **snooze beats everything.** A snooze that failure
could override would be a snooze you could not rely on, and an unreliable mute
button is one nobody presses.

## Notification discipline (`R-C1`, `R-C4`)

Delivery is the easy half. The hard half is not being annoying, and the failure
mode is identical to the one the format canary had to learn
([health-and-canary.md](health-and-canary.md)): **a notifier that cries wolf
trains you to dismiss it, and then the one that mattered gets dismissed too.**

The rule: notify on the *transition into* needing you, once, per session. Never
on a state that is merely continuing. Without it, every 1.5-second scan would
re-announce every waiting session.

`Notifier::diff` is pure — it returns what to say and updates its own memory,
but sends nothing. That keeps the interesting question (*when do we speak?*)
testable without a desktop or a network. Delivery is `osascript` for banners and
`curl` for push: one process on a rare event, and no HTTP-client dependency that
could poison the async runtime.

Off unless asked for (`--notify`, `--push-url`). A tool that starts posting
banners the first time you run it has overstepped.

## Jump to terminal (`R-B2`)

Resolves a session's pid to its controlling tty (`ps -o tty=`), works out which
terminal application owns the process, and asks that application to focus the
matching tab.

This closes the loop the queue opens: `WAITING` tells you which session needs
you, and this puts you in front of it. It moves **your** window and types
nothing — the agent is untouched.

### Detecting the terminal

The first implementation assumed Terminal.app and told an iTerm2 user *"no tab
is attached to /dev/ttys003"* while their tab sat in plain view. Assuming one
terminal was simply wrong.

The owner is now found by **walking the process ancestry** until something
recognisable appears. The real shape is deeper than it looks:

```
claude → zsh → login → iTermServer → iTerm2
```

Four levels, so checking the immediate parent would also have failed. The walk
stops at pid 1 or after 12 hops.

Applications are addressed by **bundle id**, not name: iTerm2 has answered to
both `iTerm` and `iTerm2` across versions, while
`com.googlecode.iterm2` has not moved.

### The two dialects

| | tty lives on | Focus |
|---|---|---|
| Terminal.app | the **tab** | `set frontmost` + `set selected` |
| iTerm2 | the **session** inside a tab | `select` window, tab, then session |

iTerm2's extra level is split panes. Iterating only over tabs finds nothing,
which is its own way to fail silently.

Each script `activate`s **only after a match**. That matters because when
ancestry detection fails, mogeung falls back to asking every terminal it knows —
and a script that activated first would shuffle the user's windows on every
miss. Pinned by `a_miss_does_not_raise_the_application`.

### Getting back (`R-B10`)

Jump-to-terminal solves half a round trip. A system-wide shortcut —
`Ctrl+Cmd+M` by default — raises the mogeung window from wherever you are, so
the return leg is one key rather than a hunt through whatever is on screen.

Registered with Carbon's `RegisterEventHotKey` via the `global-hotkey` crate,
on the main thread, before the event loop starts. Failure is reported into the
window and onto stderr but is **never fatal**: a shortcut another application
already owns is an ordinary thing to hit, and it must not stop mogeung opening.

A dedicated thread blocks on the event channel and pokes egui. Polling from the
frame loop alone is not enough — a backgrounded window repaints roughly once a
second, and a second of lag on *get me back here now* reads as broken.

Only `Pressed` is acted on; every hotkey also reports `Released`, which would
otherwise fire twice per press. The pending flag is a boolean rather than a
count, so holding the key down cannot build a backlog that keeps re-raising the
window after you let go.

**Caveat that cannot be detected:** registering a shortcut macOS reserves for
itself (`Cmd+Space`, `Cmd+Tab`) *succeeds* and then never fires, because the
system consumes the key first. Verified live — `Cmd+Space` registers happily.
`--help` says so; there is nothing to check at runtime.

### Why not an in-app terminal

Asked and answered: embedding a *running* session is impossible, because its
PTY master belongs to the terminal that created it and there is no way to hand
that off. Spawning our own sessions into an embedded emulator is possible but
means writing a worse iTerm2 inside mogeung — the same trade that made v0.1 a
worse Claude Code. The hotkey costs two keys and nothing else.

### What still cannot work

Terminals without AppleScript support — Alacritty, Ghostty, kitty — cannot be
driven at all. Neither can a pane inside `tmux` or `screen`: the multiplexer
owns the tty, so mogeung can focus the window but not the pane. The error names
the terminal it detected rather than blaming the user's setup.
