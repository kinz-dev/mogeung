---
title: Cross-session signals
status: active
updated: 2026-08-28
covers:
  - crates/mogeungd/src/state.rs
  - crates/mogeungd/src/notify.rs
---

# Cross-session signals

> `state.rs` also holds the run state `R-N4` added, which this document
> deliberately does not describe — that belongs to
> [run-and-debug.md](run-and-debug.md). Noted here because both docs cover the
> same file, so a staleness warning on one can be raised by a change that has
> nothing to do with it.
>
> What *does* belong here, and is easy to mistake for the same coincidence, is
> anything in `state.rs` that reaches out to the machine on the daemon's behalf
> — launching a terminal, focusing one, and since `R-J34` showing a folder in
> the file manager. They are the same family and they teach each other, so
> they are described together below.
>
> The **file surface** — which roots a session may be read through, and the
> guard around them — is a third resident of `state.rs` and belongs to
> [architecture.md](architecture.md), where `R-J40`'s workspaces are described.
> Checked when that landed rather than assumed: none of the signals here reads
> a root. `R-J39`'s discovery pass, which suggests folders from what a session
> has already written, reads `touched_files` — the same field collision
> detection reads — and changes nothing about it: it only *offers* a folder,
> and the two never meet. A collision is computed from what two transcripts *say* was touched,
> so widening what the explorer may open changes nothing about who is editing
> the same file as whom.

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

**A touched path and a diff path are matched after resolution** (`R-J27`): the
transcript writes the session's own `cwd` prefix and git answers with the
resolved root, so a checkout reached through a symlink — or any macOS temp
directory — spells the same file two ways. Collision detection compares paths
from two transcripts and is therefore consistent either way; the attribution
filter in `state.rs` compares a transcript's path against git's and is not,
which is where the reconciliation lives.

Recomputed **every scan**, not only when files move, because a collision also
*ends* — one side exits, or the window lapses — and a stale collision warning is
worse than none. Cheaply, since `R-J57`: each live session's touch window is
computed **once** per pass (it used to be recomputed per *pair*), detection
runs over those precomputed sides rather than a clone of the whole board, and
the paths sit in a `BTreeSet` so identical collisions compare equal between
ticks instead of re-broadcasting in a shuffled order.

### What it cannot see

Attribution comes from `Edit`/`Write` tool calls, so an agent that changes a
file through a shell command is invisible to it ([A8](../product/assumptions.md)).
It reports overlap, not conflict: two sessions editing different functions in
one file is flagged, and is usually fine.

**Git writes do not clear it.** `state.rs` gained the write verbs in
`R-D19`–`R-D22`, and none of them participates in this signal: a collision is
computed from what sessions *touched*, recorded from their transcripts, not
from what the working tree currently holds. So discarding a file, or committing
it, leaves the warning standing until the window lapses or a side exits — which
is right. Both agents did edit that file, and undoing the edit on disk does not
unmake the fact that two of them were working on it at once.

### What a repair does to them

Every input these signals read — `recent_touches`, `open_tools`, `recent_tools`,
`loop_signal` — is derived from the transcript, so the `R-A6` repair pass
(`repair_reingested_history`, and
[data-model.md](data-model.md#repair)) zeroes them and folds the file in again
rather than patching them. A signal computed from a doubled history is wrong in
a way no correction factor fixes: the same touch counted twice is not a
collision, and a tool call counted twice is four repeats where there were two.
The rebuild is why the loop threshold below can stay a bare count.

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

## Starting one, and knowing whether you did (`R-B2`, `R-I3`)

Launching is the other half of jumping, and on Linux it is a table of
candidate terminals tried in order. Two things learned from it on 2026-08-02,
both the kind that only a second machine teaches:

**`spawn()` succeeding is not a launch.** It means the process started. A
terminal given flags it does not understand starts, prints a usage error and
exits — by which time `spawn` has long since returned `Ok`. So a launch waits,
asks whether the child is still alive, and only then calls it a success.
Exiting **zero** immediately is also success and deliberately distinguished:
`gnome-terminal` hands the window to its own daemon and returns at once.

**`x-terminal-emulator` is not a terminal.** It is a Debian alternatives
symlink to whichever one the user chose, and the flag that carries a command
differs between them — `-e` takes argv for xterm and a single string for
terminator. It is resolved to its real target so the row matches the program.

### Which CLI it starts (`R-J51`)

The caller's choice since 2026-08-25, and the shape of that choice is
[ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)'s rule
applied to the one place mogeung starts anything: `agent_command` is an
exhaustive `match` on `SessionSource`, so the next CLI added is a compile error
here rather than a session quietly started as Claude. There is no default arm.

Three consequences worth stating, because each is a mistake the obvious version
makes:

**A source with no recipe is an error, not a fallback.** There is no way to
start Codex from here, and the answer says so. Starting a *different* agent
than the one asked for is the worst answer available — worse than refusing,
because the refusal is visible and the substitution is not.

**It is refused before the worktree is cut.** `worktree: true` creates a branch
and a checkout; doing that first and discovering the recipe afterwards would
leave both behind for a session that was never going to exist.

**The flag belongs to the CLI, not to mogeung.** `--dangerously-skip-permissions`
is Claude's and `--approval-mode yolo` is Qwen's; the daemon passes whichever
the source names, which is still not wrapping the conversation
([ADR-0003](../decisions/0003-observe-do-not-spawn.md)). Both are the ones
`yolomo` and `qwenmo` use, so the three move together. Qwen's mode is `yolo`
rather than `auto` for a reason particular to it: Qwen writes nothing to disk
when a tool blocks on approval, so under `auto` a session waiting for a human
reads as one busily working — the blind spot feature 0036 records, avoided by
not creating the prompt in the first place.

The tmux session name carries the CLI for everything that is not Claude
(`mogeung-qwen-<place>-<stamp>`), which is `qwenmo`'s convention: two agents
started in one directory are then tellable apart in `tmux ls`. Claude keeps the
bare `mogeung-<place>-<stamp>` it has always had — a name already written into
`tmux attach` lines should not move for a feature that did not touch it.
Nothing in mogeung parses either; panes are matched by process ancestry.

### Handing a folder to the desktop (`R-J34`)

`open_folder` is the third thing in this file that reaches for another
application — `open` on macOS, `xdg-open` elsewhere — and it is here rather
than in the window for the reason the other two are: **this is the machine the
folder is on.** A client dialled into another machine's daemon that opened the
path locally would show whatever happens to sit there, which is `R-J27`'s
lesson one layer up — one path is two answers on two machines.

`open` is macOS-only and that is not a house style. Linux has a program called
`open` too: `openvt`, from util-linux, which switches virtual terminals. The
familiar name on the wrong platform would not fail — it would do something
else, quietly and successfully.

**It deliberately does not apply the lesson above.** A launch waits to see
whether the child is still alive, because a terminal handed flags it does not
understand exits before you can tell. A file manager is not worth that wait:
the answer to the client is *asked*, and a handler that sat on the connection
until a desktop had finished starting Nautilus would trade a reporting nicety
for a stalled socket. What it does instead is **reap** — `xdg-open` hands the
directory over and exits at once, and a child nobody waits on is a zombie, one
per click on a button meant to be clicked often. The wait happens on a thread
of its own, where a non-zero exit becomes a log line rather than an error the
user has already been told did not happen.

Reading a worktree and never writing it is what makes this a handoff rather
than a step towards an editor: the moment you want to *do* something to a file,
the answer is an application that can, which is what
[pillar K](../product/roadmap.md#k-explicitly-not) asks for.

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

Registered in the Tauri shell, through `tauri-plugin-global-shortcut` — the
same `global-hotkey` crate the egui client used directly, so the accelerator
string and its `Cmd`-means-SUPER mapping are unchanged. Failure is reported on
stderr but is **never fatal**: a shortcut another application already owns is an
ordinary thing to hit, and it must not stop mogeung opening.

Raising unminimises before it focuses. `set_focus` on a minimised window is a
no-op on every platform, and the failure reads as a broken shortcut rather than
as an iconified window.

Only `Pressed` is acted on; every shortcut also reports `Released`, which would
otherwise raise the window twice per press.

**This nearly died with the egui client.** The Tauri shell had the plugin loaded
and the capability granted, and registered nothing — so the retirement in
[ADR-0020](../decisions/0020-the-egui-client-is-retired.md) would have deleted a
shipped feature while the roadmap still called it `✅`. A plugin being present is
not a feature being wired; the roadmap row is the claim, and it was checked
against the code rather than against the dependency list.

**Caveat that cannot be detected:** registering a shortcut macOS reserves for
itself (`Cmd+Space`, `Cmd+Tab`) *succeeds* and then never fires, because the
system consumes the key first. Verified live — `Cmd+Space` registers happily.
`--help` says so; there is nothing to check at runtime.

### Bindings as data (`R-B11`, `R-B12`)

Rebinding, pane-aware navigation and import/export all needed the same thing
first: actions had to stop being a `match` arm per key and become data. They are
an action table plus a map to text bindings, and the event handler resolves a
chord to an action and dispatches.

Navigation actions are **pane-agnostic** — `Next` means "next thing in whatever
has focus" — so one binding does the obvious thing in three panes instead of
needing three bindings and a rule for which applies.

Stored **client-side**. Not a breach of "every UI is a client with no local
authority" ([ADR-0001](../decisions/0001-rust-core-with-egui-ui.md)): a keymap
is not daemon state, and a second client would rightly have its own — which is
exactly what happened, and why the window's keymap deliberately never shared a
file with the retired egui client's
([ADR-0020](../decisions/0020-the-egui-client-is-retired.md)). Two clients, two
action vocabularies; one loader failing on the other's names would have silently
reset a keymap rather than reporting anything.

Only what you changed is stored, so a default that improves later reaches you;
loading merges over the defaults so an action added later appears with its
default binding rather than silently unbound.

**Binding parsing rejects anything it does not fully understand.** The first
version ignored unrecognised tokens, so `Ctl+J` — the obvious hand-edit typo —
parsed as a bare `J`: it fired on the wrong key, and validation called it fine.
That is the worst failure available to a keymap, because "this shortcut does
nothing" is indistinguishable from "this action is broken". Caught by a test
written to check the validator, which then failed on the validator itself.

### Icons must be proven to render

Kept as a lesson, though the code it describes went with the egui client
([ADR-0020](../decisions/0020-the-egui-client-is-retired.md)).

egui bundled four fonts (Ubuntu-Light, Hack, NotoEmoji, emoji-icon-font), and a
glyph outside their combined coverage drew as an **empty box, silently**: layout
unaffected, clicks still working, and nothing but a human looking at the window
able to tell. Four shipped that way before anyone noticed — `✎` on the flag
button, `⌁` on blast radius, `⑂` beside the branch name, and `✓`, the
read-marker in the file list added the same day. The fix was to funnel every
icon through one function and have a test parse the cmap tables of the actual
vendored `.ttf` files, so the check survived an upgrade changing what was
bundled.

The web view has the reverse problem and it is worth stating, because "we use a
browser now" is not the same as "solved": the system font stack renders almost
any glyph *somewhere*, so nothing shows an empty box on this machine — and an
icon that resolves here can still fall back to something else-shaped on a
machine with a different font set. The invariant that transferred is the first
half: icons come from one place, so what is used is enumerable rather than
scattered through the components.

### Why not an in-app terminal

The premise here was right and the conclusion was too broad — `R-B18` now ships
one. Worth reading the correction, because the mistake is instructive.

Still true: embedding a *running* session is impossible, because its pty master
belongs to the terminal that created it and there is no way to hand that off.
Still true: spawning our own sessions into an embedded emulator means writing a
worse iTerm2 inside mogeung, the same trade that made v0.1 a worse Claude Code.

What was missed is one line further down this page. "The multiplexer owns the
tty" was filed as a *limitation* — the reason a tmux pane could not be focused.
It is the whole solution. Because tmux owns the pty, and is built for several
clients at once, mogeung can attach as one more client and the session stays
reachable from every terminal it was already reachable from. The property listed
as the blocker was the mechanism.

The trade that made v0.1 bad is avoided for a specific, checkable reason: an
attached session is **never trapped in mogeung**. See
[ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md), and
`desktop/src/lib/tmux.ts` for the argv, which was ported from the Rust
faithfully enough to keep its tests.

And then a second correction, 2026-07-29: `R-B31` ships a terminal mogeung
*does* own the pty of — a plain shell, in a worktree, moved out of the pane
tree and into a panel of its own a day later by `R-B33`. The surviving argument
is the one that had to be answered, not dodged: a shell is where someone types
`claude`, and a pty we owned outright would trap that session exactly as
predicted. It runs under tmux for that reason, which keeps the never-trapped
property for anything started inside it. So the paragraph above stands as
written; what it rules out is owning the *agent's* loop, not owning a process.

A third turn, 2026-07-31 (`R-I6`): against a remote daemon both panes run
`ssh -t <target> tmux …` rather than `tmux …`. The never-trapped property is
unchanged but its *scope* now needs saying out loud — the session is reachable
from any terminal **on the daemon's machine**, which is where it was always
going to be, since that is where the files and the agent are. What would have
broken the property is the thing this replaced: running tmux locally against a
path that only exists elsewhere, which produced a shell on the wrong machine and
called it the session's.
See [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md).

### What still cannot work

Jump-to-terminal (`R-B2`) drives terminals over AppleScript, so terminals
without it — Alacritty, Ghostty, kitty — cannot be driven at all, and neither
can an individual pane inside `tmux` or `screen`. The error names the terminal
it detected rather than blaming the user's setup.

For a tmux session that limitation no longer bites, because attaching (`R-B18`)
replaces focusing rather than depending on it. A session started with a bare
`claude` in an unscriptable terminal remains genuinely unreachable, and mogeung
says so.

## What the signals do not read (2026-08-28)

`state.rs` gained a `model` field with `R-O1`, and none of the signals above is
an input to it or an output of it: collision warning, permission-vs-instruction,
loop detection, snooze and the notification rules are all computed from
transcripts and the registry exactly as before, and a configured model changes
none of them.

Recorded rather than left implicit, because `state.rs` is covered by this
document — so every change to that file asks whether the signals moved, and the
answer here is no. The same note in `attention-ranking.md` exists for the same
reason. When a model *does* start feeding a signal, it will be
[pillar `O`](../product/roadmap.md)'s own rows saying so, and ADR-0030 clause 5
is the standing rule that its output gets a column of its own rather than being
merged into one of these.

## The insight layer (2026-07-29)

Collision detection and notifications were the first cross-session
signals; pillar F generalised the idea into `insight.rs`: literal search
across every transcript and `history.jsonl`, per-day digests counted
from evidence (never assistant self-reports), recurring-failure grouping
with an auditable normalised key, prompt reuse clusters, hour-of-day
analytics, subagent trees, decision-candidate extraction (pattern named
on every row), and prompt-blame for a file from `touched_files` — each
answer stating how it matched, because A8 says attribution cannot be
certain and the UI must not pretend otherwise.
