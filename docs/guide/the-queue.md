---
title: The attention queue
status: active
updated: 2026-10-08
---

# The attention queue

The Attention list ranks every session by who needs you. The badge is the
category; the dim line beneath it is the evidence.

| Badge | Meaning | Detail shows |
|---|---|---|
| `APPROVE` | Blocked on a permission prompt — it asked to do something and is waiting on your answer | `needs approval for Bash: rm -rf build/` |
| `WAITING` | Alive and idle — it has finished its turn and wants you to type | `waiting for you — 4m12s` |
| `FAILED` | An API error was recorded | the error, e.g. `server_error` |
| `LIMIT` | A rate limit was hit; several sessions usually go dark together | the reset time, when the CLI said one |
| `REVIEW` | Exited, changed files, not all read | `3 file(s), +47 -12 unread` |
| `STALLED` | Alive and busy, silent 5+ minutes | `busy but silent for 8m30s` |
| `running` | Alive and busy, producing output | its current tool call |
| `idle` | Nothing to do | `reviewed` / `ended with no changes` / `snoozed` |

Uppercase wants a human. Lowercase is informational.

`APPROVE` and `WAITING` both look like "idle" to Claude Code. mogeung tells them
apart by whether a tool call is still unanswered, and ranks `APPROVE` higher:
that session has work in flight it cannot finish, while a `WAITING` one has
already done what you asked. Codex and opencode write a blocked approval to
disk and rank `APPROVE` the same way; Qwen Code does not, so a Qwen session
waiting on a prompt reads as `running` — which is why `qwenmo` starts it in
yolo mode.

A row for a **tmux pane that is running an agent which has written nothing
yet** — a trust prompt, a login, a first-run migration — appears marked
*provisional*, with zero counts because there is nothing to count, and is
replaced by the real session the moment the agent writes a line
([ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md)).

## Keyboard

| Key | Does |
|---|---|
| `↓` / `↑` | move down / up the queue |
| `[` | collapse or show the queue |
| `Alt+1` | put the keyboard in the queue |
| `Alt+Shift+o` | switch to the terminal app that session runs in |
| `Ctrl+Cmd+M` | bring mogeung back, from anywhere |

There are no bare-letter shortcuts: a bare letter is a character in a scratch
file or a terminal, so every chord carries a modifier. Every binding is listed,
and rebindable, in the shortcuts window (`Alt+K`), which is generated from the
keymap. Keys are ignored while a text box has focus, so typing in the filter
does not trigger them.

`Ctrl+Cmd+M` is fixed. If another application owns the combination, the window
says so on stderr and opens anyway; a shortcut macOS reserves (`Cmd+Space`,
`Cmd+Tab`) will appear to register and then never fire.

**Jump to terminal** drives Terminal.app and iTerm2 on macOS, and X11 window
managers through `wmctrl`/`xdotool` on Linux. Terminals without AppleScript
support (Alacritty, Ghostty, kitty) and Wayland sessions cannot be focused,
and a pane inside `tmux` or `screen` cannot be picked out — the multiplexer
owns the tty. When it cannot work, it says which terminal it found rather than
failing silently. For a session under tmux this rarely matters: the Agent pane
attaches instead.

## Scope

Three buttons above the filter decide what the queue is *for*:

| | |
|---|---|
| **needs you** | waiting, blocked, failed, rate-limited, stalled, unreviewed. **The default** |
| **live** | every session still running, busy or not |
| **all** | everything, including finished and reviewed |

The queue exists to answer *where do I look*, not *what exists*, so it starts
narrow. If it looks emptier than expected the panel says how many sessions are
outside the current scope.

### Under tmux only

**needs you** and **live** show only sessions mogeung can attach to — ones
running in a tmux pane, which is what `yolomo`, `codexmo`, `qwenmo` and
`opencodemo` give you. A session started in a plain terminal or in iTerm2 is
owned by that terminal and has no pane to host, so there is no Agent pane to
open and nothing to type into: it is a row you can only read past.

**all** ignores the rule entirely, and the terminal button in the panel header
turns it off without leaving the scope you are in. Whenever rows are being held
back the panel says so — `3 hidden — not under tmux` — and pressing that line
shows them.

The cost is worth knowing: a session that has **ended** has no pane either, so
by default **needs you** no longer lists ended-but-unreviewed work. Turn the
rule off, or use **all**, when review is what you are doing.

## Hiding, pinning, labels and tags

The row's menu (right-click, or the `⋯` on the card) hides a session or pins it
to the top; `L` on the selected row names it; the menu's colour tag is one of
seven. All four survive a restart, filed under the daemon's machine id so two
windows watching two machines do not overwrite each other's; *export* in the
palette writes them to a file.

**Hiding is not forgetting.** It is a view filter and nothing else — the daemon
never hears about it, review marks are untouched, and it is reversible from the
`N hidden` button at the top of the panel. *Forget session*, in the same menu,
is the destructive one: the daemon drops the session and its review marks.

A pinned session ignores scope, because a pin that a scope could override would
not be worth setting. Pinning something hidden reveals it; hiding something
pinned drops the pin. Pins, then colour, then label sort above the daemon's
rank — a hand key beats a computed one, within the tier it is in.

## Filter, group, follow

**filter** matches the label, repo, branch, cwd and current activity. Every
word must match, so typing more always narrows.

It also understands fields, which is how you find a session free text cannot
describe:

| | |
|---|---|
| `repo:mogeung` | or `r:` |
| `branch:main` | or `b:` |
| `file:state.rs` | or `f:` / `path:` — matches files the session **touched** |
| `label:risky` | or `l:` |
| `tag:red` | or `t:` — the colour tag |

They combine: `repo:mogeung branch:main retry`. Clicking a repo name, a label
badge or a tag in any card filters to it. An unknown prefix (`todo:`) stays
plain text rather than matching nothing.

**group** collapses the queue by repository — and by machine first, when you
follow other daemons ([remote.md](remote.md)). Repos are ordered by their most
urgent session, so the top of the panel is still the top of the queue. Click a
repo header to fold it.

**follow** keeps the top of the queue selected as it changes — useful on a
second monitor, disorienting while you are reading something.

## Snooze

The *Snooze 30m* button on the selected row. The session ranks `idle` with the
detail *snoozed*, drops to the bottom, stops counting as needing you, and shows
a small clock; the same button reads *Wake*.

**Snooze beats everything, including `FAILED`.** A mute button that failure could
override is one you would never trust.

## Collisions and loops

`⚠ COLLISION` means **another live session is editing the same file right now**
(within 10 minutes). Both sides are warned. This is the one thing only a
cross-session observer can tell you — neither agent can see the other. It is
based on `Edit`/`Write` tool calls, so an agent changing files through a shell
command is invisible to it, and two sessions on two machines are never a
collision.

`↻` means the session has repeated the same tool on the same target four times
in its last twelve calls — usually retrying something that is not working. It is
advisory and never changes the ranking.

*unverified* means the session changed code and ran no build, test or
typecheck that mogeung could see
([reviewing.md](reviewing.md#verification)).

## Ordering is strict

Scores are 1100 / 1000 / 900 / 850 / 800 / 700 / 100 / 0, and the tiebreaker
(longest wait first) is capped at 99. A session can never jump into a more
urgent tier, so a brand-new `FAILED` always outranks a three-day-old `REVIEW`.
The daemon ranks; the window computes nothing — one ranking, even across
followed machines. See [attention-ranking.md](../design/attention-ranking.md).

## `WAITING` is a fact

Claude Code publishes `busy`/`idle` in its own live registry. mogeung is not
guessing. `FAILED` is checked before liveness, so a live session that hit an API
error still shows failed; it clears when you send a new prompt.

The small badge — `live·busy`, `live·idle` or `live` — is raw liveness from the
registry, independent of the ranking. Ended sessions show none.

## The panel when it is folded

Collapsed to a strip (`[`), the panel shows one status dot per session that
needs you, and its tooltip says *N session(s) need you*. A repository group's
header carries *N need you* for that repo. The health button beside it is the
complement — *reading everything it recognises*, or amber *N thing(s) it cannot
see* ([troubleshooting.md](troubleshooting.md)).

## Tuning

`stall_secs` defaults to 300 in `crates/mogeung-core/src/attention.rs`. Five
minutes is generous on purpose — a long build is normal silence, not a stuck
agent. Raise it if `STALLED` fires on healthy work.
