---
title: Attach a terminal through tmux, never own one
status: active
updated: 2026-07-26
decided: 2026-07-26
supersedes: ADR-0002
---

# ADR-0010 — Attach a terminal through tmux, never own one

## Context

[ADR-0002](0002-structured-transcript-not-a-terminal.md) ruled out an embedded
terminal, and its reasoning still holds for what it was arguing about: typed
events can be searched, linked to a hunk and diffed; a character stream cannot.
The structured transcript stays.

What it missed is that **Claude Code is a TUI**, and its interactive chrome
never reaches the transcript at all. Permission prompts, multiple-choice
questions, plan-mode approval, `/` autocomplete — all of it is ephemeral
terminal rendering. The `.jsonl` records what was *said*, never what is being
*asked*. So a session sitting on a multiple-choice prompt is, in mogeung, a
session you can see and cannot answer.

That is the queue's whole purpose failing at the last inch. `R-B2` gets you to
"which session needs you"; answering it still means leaving.

The obvious fixes were tried and are dead. They are recorded because each looks
plausible enough to be re-proposed:

- **A text box that types into the session.** Cannot answer a menu. This is what
  made the shortcoming visible in the first place.
- **An IPC channel into a running `claude`.** There is none. `lsof` against live
  sessions shows no listening TCP socket and no unix socket. `--remote-control`
  relays through Anthropic's infrastructure, not a local endpoint.
- **`TIOCSTI`** — inject keystrokes into another tty. Present on macOS, but
  returns `EPERM` from a process the tty is not the controlling terminal of;
  verified locally, not inferred. Disabled by default on Linux since 6.2 and
  gated behind `CAP_SYS_ADMIN`. Requiring root for a desktop observer is not a
  trade worth making.
- **Stealing the pty** (`reptyr`, ptrace). Linux-only, blocked by macOS hardened
  runtime, and it *takes* the terminal from the original owner rather than
  sharing it.

Underneath all four is one fact: **a pty has exactly one master**, held by
whichever terminal created the process. A `claude` started in iTerm2 is owned by
iTerm2, permanently. Nothing can attach to it. This is not a gap to engineer
around; it is what a pty is.

## Decision

**mogeung attaches to a tmux session. It never spawns `claude` into a pty of its
own.**

The Terminal tab runs `tmux attach-session -t <target>`. tmux owns the pty and
is built for several clients at once, so the same live session renders in a
mogeung pane and in your terminal simultaneously.

The daemon resolves which pane belongs to which session, by walking the
process ancestry of the session's pid against `tmux list-panes`. A client is
told the target and never works it out for itself
([ADR-0001](0001-rust-core-with-egui-ui.md)).

`scripts/yolomo` starts sessions under tmux. A session started any other way
reports no target, and the tab says so and offers `R-B2` instead.

## Why this is not the thing ADR-0003 forbids

[ADR-0003](0003-observe-do-not-spawn.md) says mogeung never starts, steers or
stops an agent, and the lesson it draws is that **a supervision layer must be
additive** — anything inserting itself between you and the agent has to earn
back what it takes.

An attached view takes nothing, for one specific reason: **the session is never
trapped.** Detach and it keeps running. Attach from any terminal. Kill mogeung
and nothing happens to it. v0.1 failed because each session became *worse* than
just running `claude`; a session here is byte-for-byte the same session, with
one more window onto it.

mogeung also still types nothing. The keystrokes are yours, going into a pane
you focused deliberately. That is the same boundary
[ADR-0008](0008-build-the-prompt-never-send-it.md) draws — it rejected
*keystroke injection*, mogeung composing input and delivering it on your behalf.
Nothing here composes anything. ADR-0008 stands unchanged, clipboard boundary
and all.

The distinction that matters: **owning the loop is forbidden; rendering it is
not.**

## Alternatives

**Keep pointing at the terminal (`R-B2` only).** The status quo. Rejected
because it cannot answer a prompt, which is the moment the queue exists to
serve. It remains the fallback for non-tmux sessions.

**Spawn `claude` in an embedded pty.** Simplest to build, and wrong: mogeung
would own the session, it would die with the window, and — decisively — it
*still* cannot attach to the sessions you already have running. It buys the
v0.1 failure mode and does not even solve the problem.

**Make mogeung the multiplexer** — pty in the daemon, UI as a thin client.
Architecturally clean and fits the daemon/client split exactly. Rejected on
cost and outcome: it is reimplementing tmux, and it arrives at the same wall,
because a session started in iTerm2 is still unattachable. If we are going to
depend on a multiplexer, depend on the one that exists.

**A read-only attach (`tmux attach -r`) only.** Fits "observe" with no argument
needed. Rejected as a final state — it stops exactly where the problem starts —
but it is what the widget was spiked against, and `-r` remains available.

## Consequences

- Answering a permission prompt or a multiple-choice question no longer means
  leaving mogeung.
- **A workflow change is required**, and it is the real cost: sessions must be
  started with `yolomo`. tmux cannot be retrofitted onto a running `claude`,
  so sessions started otherwise are permanently point-only. This degrades
  cleanly and is stated in the tab rather than failing quietly.
- A hard dependency on tmux for the feature, and a vendored terminal widget to
  maintain — 2k lines, MIT, see `crates/egui-term/VENDORED_FROM`.
- The keymap must yield while the terminal has focus, so `LeaveTerminal` needs a
  chord Claude Code will never want. Escape belongs to the agent.
- Terminal emulation quality is now a thing mogeung is judged on, in a widget at
  v0.1.0 that does not claim full coverage. Survivable **only** because the
  session is not trapped: if the pane renders badly, your terminal is still
  right there with the same session in it. That property is load-bearing — a
  design that removed it would make this ADR wrong.
- ADR-0002's structured transcript is unaffected and remains the default view.
  This adds a tab; it does not replace one.

## Revisit if

Claude Code grows a real local control channel — a socket, or a documented way
to attach to a running session. That would remove the tmux dependency and the
workflow change with it, which are the only two things anyone actually pays for
here.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
