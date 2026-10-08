---
title: Cross-session signals
status: active
updated: 2026-10-08
covers:
  - crates/mogeungd/src/state.rs
  - crates/mogeungd/src/notify.rs
  - crates/mogeungd/src/send.rs
  - crates/mogeungd/src/env.rs
---

# Cross-session signals

Things no single agent can know about itself, because knowing them requires a
view across sessions that none of them has — and, in the same file, the few
places the daemon reaches out to the machine on your behalf. This is the
strongest argument for the observer model beyond "it takes nothing away": a
wrapper around one agent could never produce any of it.

## Collision warning (`R-B3`)

**Two live sessions editing the same file inside a 10-minute window.** Both
sides are warned, because either might be the one you want to stop. Comparing
cumulative `touched_files` would be useless — any two sessions in a repo
eventually overlap, and a warning that is always on is off — so each touch is
timestamped (`Session::recent_touches`, capped at 200) and only recent ones
count. Paths are compared as two transcripts spelled them, so a checkout
reached through a symlink is consistent on both sides
([read-boundary.md](read-boundary.md#two-spellings-of-one-path) holds the
reconciliation the attribution filter needs instead).

Recomputed **every scan**, not only when files move, because a collision also
*ends* and a stale warning is worse than none; cheaply, since each live
session's touch window is computed once per pass and the paths sit in a
`BTreeSet` so identical collisions compare equal between ticks.

What it cannot see: attribution comes from `Edit`/`Write` tool calls, so an
agent that changes a file through a shell command is invisible
([A8](../product/assumptions.md)). It reports overlap, not conflict. Git
writes do not clear it — discarding or committing a file does not unmake the
fact that two agents were working on it at once. Two machines are never a
collision: mirrored sessions live outside `AppState.sessions`, so
`detect_collisions` never sees them, and a mirrored session carries its own
daemon's collisions across with the other id namespaced by the same machine.

## Permission vs. instruction (`R-B4`)

An unmatched `tool_use` plus an idle registry means the session is sitting on
a permission prompt rather than waiting for a new task
([attention-ranking.md](attention-ranking.md)). `Session::open_tools` is
maintained incrementally: `tool_use` pushes, the matching `tool_result`
removes, a new human turn clears the list. Sidechain (subagent) tool calls are
excluded — a subagent's pending tool is not something you can approve.

## Loop detection (`R-B7`)

**The same `tool:target` four times in the last twelve calls.** Deliberately
crude: it catches an agent retrying an edit that keeps not applying, or
re-reading a file it has already read, without pretending to understand
intent. It cannot distinguish "stuck" from "legitimately doing the same thing
to many inputs", which is why it is an advisory string rather than a tier. A
heuristic this rough must not be able to reorder the board.

## Snooze (`R-B5`)

Suppresses a session from ranking until a deadline, checked before every other
rule including `Failed`, persisted with the session. **Snooze beats
everything** — a snooze that failure could override is one you could not rely
on, and an unreliable mute button is one nobody presses.

## Notification discipline (`R-C1`, `R-C4`)

Delivery is the easy half. The hard half is not being annoying: a notifier
that cries wolf trains you to dismiss it, and then the one that mattered gets
dismissed too. The rule is to notify on the *transition into* needing you,
once, per session, never on a state that is merely continuing. `Notifier::diff`
is pure — it returns what to say and updates its own memory, sends nothing —
so *when do we speak?* is testable without a desktop or a network. Delivery
is `osascript` on macOS, `notify-send` on Linux, `curl` for push: one process
on a rare event, and no HTTP client that could poison the runtime. Off unless
asked for (`--notify`, `--push-url`, or the config keys). The notifier reads
the merged queue, so a followed machine's waiting session notifies here,
labelled *on HOST*, and names a session by `label()`, which a `/rename` now
wins.

## What a repair does to them

Every input above — `recent_touches`, `open_tools`, `recent_tools`,
`loop_signal` — is derived from the transcript, so the re-ingestion repair
([data-model.md](data-model.md#repair)) zeroes them and folds the file in
again rather than patching them: the same touch counted twice is not a
collision, and a tool call counted twice is four repeats where there were two.
The rebuild is why the loop threshold can stay a bare count.

## Starting one, and knowing whether you did (`R-B2`, `R-I3`)

The one thing mogeung starts is a real interactive CLI, unmodified, in a
terminal or headless under tmux, and the rules are the ones a second machine
taught:

- **`spawn()` succeeding is not a launch.** A terminal given flags it does not
  understand starts, prints usage and exits after `spawn` has returned `Ok`.
  So a launch waits, asks whether the child is alive, and only then calls it
  a success; exiting zero at once is also success, because `gnome-terminal`
  hands the window to its own daemon. On Linux it is a table of candidate
  terminals tried in order, and `x-terminal-emulator` is resolved to its real
  target so it gets that program's flags rather than xterm's.
- **Which CLI is the caller's choice** (`R-J51`), and `agent_command` is an
  exhaustive `match` on `SessionSource` with no default arm
  ([ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)):
  a source with no recipe is an error in words, never a different agent
  started quietly, and it is refused before a worktree is cut. The flag
  belongs to the CLI — `--dangerously-skip-permissions` is Claude's,
  `--approval-mode yolo` Qwen's (because Qwen writes nothing to disk when a
  tool blocks, so an approval prompt would read as *working*),
  `--ask-for-approval never --sandbox workspace-write` Codex's (short of the
  flag that also drops its sandbox), `--auto` opencode's — the same ones the
  `*mo` launchers use, so the two move together. A headless Codex launch into
  a directory Codex has not been trusted in is refused (`R-J74`): the prompt
  would have no window to appear in, and the answer is never given for you.
- **The tmux session name carries the CLI** for everything that is not
  Claude (`mogeung-qwen-<place>-<stamp>`); Claude keeps the bare
  `mogeung-<place>-<stamp>` already written into `tmux attach` lines. Nothing
  parses either — panes are matched by process ancestry.
- **Every spawn goes through `env::command`**, never `Command::new`, so where
  the program is and what the child's `PATH` will be are answered together
  ([architecture.md](architecture.md#the-path-a-launcher-does-not-give-r-j87)).

### A pane running an agent that has not written (`R-J75`)

Every session is discovered from the agent's own bookkeeping, and that has one
failure no parsing fixes: an agent that is running and blocked *before* it
writes — Codex asking whether it may trust a directory, a login, a first-run
migration — is invisible. So a tmux pane whose process tree contains an agent
CLI is published as a session, marked `provisional`, identified by tmux's own
`pane_id` rather than by the attach target (a target renumbers when a window
closes, and an identity that is a location is not one), recomputed every scan
and never persisted. `is_agent` reuses ADR-0025's list of programs mogeung
must never *start* — the same list rather than a second that could drift, and
therefore the same limit: a wrapper script calling `claude` is not `claude`.
[ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md)
carries the reasoning; an unclaimed opencode pane is *waiting for your first
prompt*, because opencode writes a session the moment it is spoken to.

### Handing a folder to the desktop (`R-J34`)

`open_folder` — `open` on macOS, `xdg-open` elsewhere — is in the daemon
rather than the window because **this is the machine the folder is on**; a
client dialled into another machine's daemon that opened the path locally
would show whatever happens to sit there. `open` is macOS-only and that is not
a house style: Linux has an `open` too, `openvt`, which switches virtual
terminals, so the familiar name on the wrong platform would do something else
quietly and successfully. It does not wait for the child the way a launch
does — the answer to the client is *asked*, and a handler that sat on the
connection until a desktop had started Nautilus would stall the socket — but
it reaps, on a thread of its own, so a click meant to be clicked often leaves
no zombie. `open_in_intellij` and the probe beside it (`R-J92`) are the same
shape.

## Jump to terminal (`R-B2`)

Resolves a session's pid to its controlling tty, works out which terminal
application owns the process, and asks that application to focus the matching
tab. It moves **your** window and types nothing — the agent is untouched.

The owner is found by **walking the process ancestry** until something
recognisable appears — `claude → zsh → login → iTermServer → iTerm2` is four
levels, so checking the immediate parent fails — stopping at pid 1 or after
12 hops. Applications are addressed by bundle id, not name, because iTerm2
has answered to both `iTerm` and `iTerm2` across versions and
`com.googlecode.iterm2` has not moved. Two dialects: Terminal.app's tty lives
on the *tab* (`set frontmost` + `set selected`); iTerm2's on the *session*
inside a tab, so the script selects window, tab, then session, and iterating
only over tabs finds nothing. Each script activates **only after a match**,
because when detection fails mogeung asks every terminal it knows, and a
script that activated first would shuffle the user's windows on every miss
(`a_miss_does_not_raise_the_application`). On Linux the X11 window is found
through `wmctrl` and `xdotool` from the tmux clients' pids; Wayland refuses
honestly.

**What still cannot work.** Terminals without AppleScript — Alacritty,
Ghostty, kitty — cannot be driven, and neither can an individual pane inside
`tmux` or `screen`; the error names the terminal it detected. For a tmux
session that no longer bites, because attaching (`R-B18`) replaces focusing
([ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md),
[ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md)). A session
started with a bare `claude` in an unscriptable terminal remains genuinely
unreachable, and mogeung says so. The return leg — `Ctrl+Cmd+M` raising the
window — is the shell's ([the-window.md](the-window.md#chrome-and-panes)).
Jump never crosses a federation hop: raising a window on another machine's
screen is an act nobody here would see.

## Sending text into a session (`R-B54`)

The one thing in this file that reaches an agent rather than the machine
around it, inside the fence
[ADR-0003's 2026-08-29 amendment](../decisions/0003-observe-do-not-spawn.md#amendment--2026-08-29-a-human-may-press-send)
drew: a human clicks, confirms, and the text they are looking at goes into
that session's own tmux pane, once. `R-B2` above still types nothing; the two
are opposite kinds of act and both end at a terminal.

- **The target** is `tmux_target`, already resolved for the Agent tab by
  walking process ancestry against `tmux list-panes`. Nothing new is computed
  for sending — a second way of working out where a session lives would be a
  second way of being wrong about it.
- **The gate** is `send_allowed`, set once at start-up from
  `send::allowed(&addr)`, or the asking socket being on this machine
  (`R-J104`). Its own flag rather than `writes_allowed` or `config_editable`:
  those accept a token, and this door takes none. A shared secret on a LAN is
  not a person at this machine.
- **The delivery** is `send.rs`: `load-buffer` on stdin, `paste-buffer -p -t
  <pane>`, `send-keys Enter`. Running it corrected two things its module doc
  carries: tmux returns before the receiver has read, and `-p` inserts
  bracketed-paste markers only when the program in the pane has asked for
  that mode. The command box (`R-O12`) reuses the paste half into your own
  shell, unsubmitted.
- **Across a hop** (`R-I23`), a send to a mirrored session is forwarded only
  with the follow's *allow changes* consent, and the owning daemon then
  applies the gate to the relay as to any client — over a LAN it is refused
  there, in its words.

## What the signals do not read

`state.rs` holds more than the signals: the run state (`R-N4`,
[run-and-debug.md](run-and-debug.md)); a live session's branch read from its
repository's `HEAD` each pass (`R-J110`, [claude-code-formats.md](claude-code-formats.md));
notes and their tasks (`save_note`, `set_task`, the mirror —
[data-model.md](data-model.md)); the file surface and `session_roots`
([read-boundary.md](read-boundary.md)); the git wrappers
([review-checkpointing.md](review-checkpointing.md),
[wire-protocol.md](wire-protocol.md)); the model (`R-O1`), the pricing record
and its kick (`R-J124`), the scratch directory (`R-L5`), artifacts (`R-J115`)
and the pinned title (`R-J120`). None of them is an input to a signal:
collision, permission-vs-instruction, loop, snooze and the notification rules
read transcripts and the registry exactly as they always have, and when a
model *does* feed a signal it will be pillar O's own rows saying so, with
[ADR-0031](../decisions/0031-consent-to-a-named-host.md) clause 5 the standing
rule that its output gets a column of its own. Recorded because every commit
to `state.rs` raises the staleness check on this document, and the answer has
been *no signal moved* each time — most recently for `R-J115`, `R-J120` and
`R-J124` on 2026-10-08.

## The insight layer

Collision detection and notifications were the first cross-session signals;
pillar F generalised the idea into `insight.rs`: literal search across every
transcript and `history.jsonl`, per-day digests counted from evidence (never
assistant self-reports), recurring-failure grouping with an auditable
normalised key, prompt reuse clusters, hour-of-day analytics, subagent trees,
decision-candidate extraction with the pattern named on every row, and
prompt-blame for a file from `touched_files` — each answer stating how it
matched, because A8 says attribution cannot be certain and the UI must not
pretend otherwise ([wire-protocol.md](wire-protocol.md#insight-usage-and-docs)).
