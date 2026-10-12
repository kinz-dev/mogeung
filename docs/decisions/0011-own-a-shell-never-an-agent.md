---
title: mogeung may own a shell, and runs it under tmux so nothing started inside it is trapped
status: active
updated: 2026-10-09
decided: 2026-07-29
---

# ADR-0011 — mogeung may own a shell, never an agent

## Context

[ADR-0010](0010-attach-a-terminal-never-own-one.md) is titled "attach a
terminal through tmux, never own one". It was written about the *agent's* pty:
a `claude` started in iTerm2 belongs to iTerm2, cannot be handed over, and the
only honest way to render it elsewhere is to attach as a second tmux client.

But the sentence is wider than the situation that produced it. Read literally,
"never own one" forbids any pty at all — including the plain shell every editor
has had for a decade, the one you use to run the tests for the diff you are
looking at. That reading is not what was decided; it is what the wording
happens to cover. ADRs are immutable, so this one narrows it rather than
editing it.

The narrowing is not free, because there is a real trapdoor between the two
cases. A shell pane in a window full of agent sessions is a shell people will
type `claude` into. It is the obvious move. And the moment they do, a
directly-owned pty would make that session **trapped in mogeung**: closing the
window, or a crash, or the widget failing, kills an agent mid-turn. That is
precisely the property ADR-0010 leans on, defeated by the back door — and
"never trapped" is what makes hosting a terminal additive instead of a repeat
of v0.1 ([ADR-0003](0003-observe-do-not-spawn.md)).

So the question is not "shell or no shell". It is: **what has to be true of a
shell mogeung owns for the trapdoor to stay shut?**

## Decision

mogeung may own a shell, and runs it inside tmux — `tmux new-session -A -s
mogeung-shell-<worktree>` — rather than on a pty of its own.

`-A` means attach-if-exists, create-otherwise, so the pane is the same shell
across restarts. The property that matters is transitive: because tmux owns the
pty, **anything started in this shell is reachable from any terminal and
outlives mogeung**, `claude` included. The trapdoor is shut by the mechanism,
not by asking people not to walk through it.

Where tmux is not installed, the pane spawns the user's `$SHELL` on a pty
mogeung owns, and **says so in its header**. That mode is the one place mogeung
can trap something, and it is labelled rather than silent.

What is *not* decided here: mogeung still never writes to a pty. This is a
terminal a human drives. Nothing types into it, nothing answers a prompt in it,
nothing runs in it on a timer — [ADR-0003](0003-observe-do-not-spawn.md) and
[ADR-0008](0008-build-the-prompt-never-send-it.md) are untouched.

## Alternatives

**A directly-owned pty, as VS Code and IntelliJ do.** The obvious build, and
the one the reference implementations chose. Lost on the trapdoor above: those
editors have no agent sessions to trap, so the property they are giving up
costs them nothing and costs us the thing ADR-0010 protects. It also throws
away a long `cargo build` on every window close, which is the same defect in a
cheaper form.

**tmux required, no fallback.** Rejected as a hard dependency out of all
proportion: needing a multiplexer installed before `ls` will run makes the pane
dead on every minimal Linux box, and dead for a reason the user cannot guess
from what they typed.

**No shell pane; keep pointing at the terminal app.** `FocusTerminalApp`
already leaves for iTerm2, and that round trip is exactly what `R-B2` exists to
shorten. It also lands you wherever that terminal happened to be, when the
shell you want is almost always rooted in the worktree already on screen.

**One shell for the whole app rather than one per worktree.** Cheaper, and
wrong every time you switch sessions: a shell whose cwd has nothing to do with
the diff in the next pane is a shell you `cd` in before every command.

## Consequences

**Easy.** The shell survives a restart with its scrollback and its running
processes. `tmux ls` shows it. You can attach to it from a real terminal and
keep the same history. The widget is `term.rs` again — the `TERM` handling, the
alternate-scroll interception, the focus lock that keeps Escape away from egui
were all paid for once.

**Hard.** Two panes now look like terminals and mean different things. That is
what the 2026-07-29 rename was for (feature
[0003](../archive/features/0003-attached-terminal.md)) — Agent is the session's,
Terminal is yours — and it stays a permanent source of confusion that the
labelling has to keep answering.

**Accepted cost.** mogeung now creates tmux sessions on your machine and does
not clean them up. That is the persistence, not a leak: killing them on exit
would remove the reason for running under tmux at all. They are all named
`mogeung-shell-…`, so `tmux kill-session -t mogeung-shell-…` finds them and a
`tmux ls` explains them.

**Accepted cost.** In the no-tmux fallback the never-trapped property is
genuinely absent, and a `claude` started there dies with the window. Labelled,
not fixed. Fixing it would mean requiring tmux.

**Ruled out.** A write path from mogeung into this pty. No "run this command",
no "paste the prompt and press Enter", no scheduled check. The moment something
other than a human's keystrokes reaches the shell, this is a tool that steers
agents and ADR-0003 is gone — and it would arrive as a convenience, which is
how that line gets crossed.

## Revisit if

The shell pane turns out to be used mostly to *start sessions*. That would mean
the honest feature is "start a session in this worktree", which already exists
as `LaunchTerminal` and should be a button rather than something people type —
and it would put the pane's cwd-per-session design under real scrutiny.

Or if a second client ever needs the shell. This is client-local, spawned by
the UI process; a web client asking the daemon to open a pty is a different
decision with a much worse security story, and this ADR does not license it.


## Amendment — 2026-10-08

**One write path into the shell pane exists, on a click, and it is not the one this ADR ruled out.** `R-O12` (2026-08-29) pastes a command the human asked a model to write into the human's own shell, *unsubmitted*, through `R-B54`'s paste-buffer path; it runs only on `Alt+Enter`, a keystroke of the human's. "Nothing types into it" is narrowed to: nothing types into it unasked, nothing presses Enter for you, nothing runs on a timer — and the ruled-out list under Consequences stands word for word for those three. ADR-0008, named above as untouched, is superseded by [ADR-0034](0034-the-draft-is-a-chat-ask.md). Two details have also moved since the port: the shells are named per panel (`mog-<n>`, `desktop/src/lib/tmux.ts`) rather than per worktree, and the widget is xterm.js in the Tauri shell since [ADR-0020](0020-the-egui-client-is-retired.md), not `term.rs`.
## Amendment — 2026-10-09

**A seat's pane has a write path. The shell pane still has none.**
[ADR-0003's amendment of this date](0003-observe-do-not-spawn.md#amendment--2026-10-09-mogeung-may-own-a-seat-it-started)
lets the daemon own a **seat** — an agent CLI it started itself, in Gau Geung
mode — and owning it means delivering text and pressing Enter under that
amendment's fences: declared text, visible, budgeted, stoppable. The mechanism
is `R-B54`'s paste-buffer path addressed to the pane by id, the same one the
2026-10-08 amendment above describes for the human's `Alt+Enter`.

What the Decision rules out — *"a write path from mogeung into this pty: no
'run this command', no 'paste the prompt and press Enter', no scheduled
check"* — **stands word for word for the shell pane**, and for the Agent pane
of every session mogeung did not start. The shell is yours; nothing types into
it unasked, nothing presses Enter for you, nothing runs on a timer. A seat
pane is the one kind of pane those three sentences no longer cover, and the
row says it is a seat so that nobody has to guess which kind they are looking
at.

The trapdoor this ADR exists to keep shut is now load-bearing twice. A seat
runs under tmux, so it outlives mogeung and is reachable from any terminal —
the property this page bought for the shell, now bought for the agent too.
And a seat is never on a pty mogeung owns: the no-tmux fallback, the one
place this page admits it can trap something, **cannot host a seat**. Without
tmux there are no seats, and the window says so rather than offering a trapped
one.

The first *Revisit if* above — the shell pane used mostly to start sessions —
has its answer: starting a seat is a button, under
[ADR-0025's amendment of this date](0025-run-a-process-you-named-never-an-agent.md#amendment--2026-10-09),
and the shell stays what it was.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
