---
title: In-app terminal
status: shipped
updated: 2026-07-30
roadmap: [R-B31, R-B32, R-B33, R-B34]
depends_on: []
---

# 0024 — In-app terminal

> **Revised on 2026-07-30, one day after it shipped.** The shell was a pane in
> the detail tree, rooted in the selected session's worktree. It is now a panel
> across the bottom with a tab per shell and no tie to any session (`R-B33`),
> and it draws in a font you choose (`R-B32`). The dogfooding verdict on the
> pane version is below under [What the pane got wrong](#what-the-pane-got-wrong);
> it is kept because it is the reason for the shape this has now.

## Spec

### Problem

Every pane in mogeung is about a worktree, and none of them can run anything in
it. You read a diff, decide the tests should be re-run, and leave for iTerm2 —
where you land in whatever directory that window was last in, `cd` to the repo,
and lose the diff you were reading off the screen behind you.

`FocusTerminalApp` (`o`) makes that trip one keystroke instead of a
window-hunt, which is `R-B2`'s whole point. It does not make the trip
unnecessary. Every editor for a decade has answered this with a shell pane
rooted where you already are, and mogeung had the widget for one sitting in
`term.rs` the entire time — pointed at somebody else's pty.

The word was the blocker as much as the code: "Terminal" already named the pane
that hosts the *agent's* session. That was resolved first, on 2026-07-29
(feature [0003](0003-attached-terminal.md)) — the agent's pane became **Agent**
and the name came free.

### Assumptions

None new. This rests on the same tmux availability the Agent pane already
depends on, and degrades without it rather than requiring it.

### Acceptance

- [x] `Alt+F12` — or `Ctrl+`` ` `` — shows the terminal and puts the keyboard in
      it, with no click needed, and the same key puts it away again
- [x] The first shell starts in the selected session's worktree, and in `$HOME`
      when nothing is selected
- [x] Moving the selection does not disturb, replace or close a shell
- [x] Closing mogeung and reopening it returns the *same* shells: the scrollback
      is there, and a command left running is still running
- [x] Each tab names its tmux session, and `tmux attach -t <that>` from a real
      terminal reaches the same shell
- [x] With tmux absent the panel still gives a working shell, and says in its
      header that this one dies with the window
- [x] The chord that gives the keyboard back is the same one the Agent pane
      uses, and it goes to whichever of the two has it
- [x] Ctrl+wheel zooms the terminal alone, like every pane (`R-B30`)
- [x] `+` opens a second shell in the same worktree without disturbing the
      first; the chevron beside it opens one in any worktree mogeung knows
      (`R-B33`)
- [x] Closing a tab detaches; the tmux session and whatever is running in it
      survive, and reopening a shell there lands back in it
- [x] A tab can be renamed — double-click it, or right-click it — and the name
      survives a restart; blank puts the folder name back (`R-B34`)
- [x] A renamed tab still reaches the same tmux session, and `tmux attach -t
      <session>` still finds it under the generated name (`R-B34`)
- [x] The panel's height, its tabs and whether it is up survive a restart
- [x] The terminal draws in a monospaced family picked from those installed,
      and a Powerline/Nerd Font prompt renders (`R-B32`)
- [x] A chosen font that has since been uninstalled leaves a working terminal
      in the bundled font *and says so*

### Explicitly out of scope

- **Anything writing into the pty.** No "run this command" button, no
  paste-and-send, no scheduled check. See [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md).
- **Splits inside the panel.** Tabs, and tmux is right there.
- **A font for the rest of the window.** This is a terminal setting. The diff,
  the transcript and the Editor keep the bundled monospace, which is the one
  known to carry mogeung's own icon glyphs.
- **Rescanning fonts while running.** Installed once per process; a font added
  after launch appears at the next one, as it does in every terminal emulator.
- **A key that renames the focused tab.** F2 is the convention and it cannot
  work here: while you are in the shell the pty owns the keyboard, so F2 is a
  keystroke for the program inside — which is where it belongs. The gesture is
  the mouse's, on a target the mouse is already over.
- **Renaming the tmux session to match.** See
  [Naming a tab](#naming-a-tab) — it strands the shell.

## Plan

### Approach

**tmux-backed, not a bare pty**, which was the one real decision here.

The obvious build is what VS Code and IntelliJ do: own a pty, spawn `$SHELL`,
done. It is wrong for this app, and the reason is not code reuse. A shell pane
sitting next to a diff, inside a window full of agent sessions, is a shell
people type `claude` into — and a directly-owned pty makes that session
**trapped in mogeung**, killed by a window close, which is precisely the
property [ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md) leans
on for the Agent pane and precisely how v0.1 failed.

`tmux new-session -A -s mogeung-shell-<worktree>` keeps the property
transitively: tmux owns the pty, so anything started inside is reachable from
any terminal and outlives this window. `-A` also gives persistence for free —
attach-if-exists means the pane is the *same* shell across restarts.

Where tmux is missing, the pane spawns `$SHELL -l` on a pty of its own and says
so in the header. Requiring a multiplexer before `ls` will run would be absurd;
degrading silently would be dishonest, because that mode really does lose
everything on close.

Sessions are keyed by **worktree**, not session id: it is the directory the
commands are about, it outlives the session it was opened from, and two
sessions in one checkout then share the shell they would both have `cd`-ed to
the same place anyway. `R-B33` added an **ordinal** beside it so one worktree
can hold several shells — ordinal 0 producing the name the single-shell build
used, because otherwise every shell anyone had open would be stranded under a
name the new build never asks tmux for again.

### What the pane got wrong

A shell was made a pane because every other view here is one, and that is the
whole mistake in a sentence. A pane is *about* the selected session. Three
things followed, and all three were wrong:

- it followed the selection, so a shell in one worktree vanished when you
  looked at a session in another — while the tmux session it was attached to
  went on running, reachable from everywhere except the app that started it;
- it could not exist with no session selected, which is exactly when you want a
  shell, because starting a session is what you are about to do;
- it competed for the same rectangle as the diff you opened the terminal to act
  on, so using it meant hiding the reason for using it.

A panel across the bottom fixes all three by not being about anything. It is
where VS Code, IntelliJ and every editor since has put it, and the reason is the
same one: a terminal is not a view of your work, it is the place you act on it.

**The tabs are the second half.** One shell per worktree was already one too
few — a build running and a `claude` to start is the ordinary case, not the
exotic one — and it only looked sufficient while the panel could hold one thing.

### Naming a tab

`R-B34`. The derived label is the worktree's basename, which stops answering
the question the moment three of four tabs say `mogeung` — the tabs exist
because one worktree holds several shells, so the default collides exactly
where the tabs are useful.

**The name is the label and nothing else.** The tempting version is
`tmux rename-session`, so `tmux ls` agrees with the tab. It is the same mistake
the ordinal was designed around: the session is *found* by `(worktree,
ordinal)`, so renaming it strands the shell — the next launch asks tmux for the
generated name, does not find it, and creates a second session beside the one
with your build running in it. The tooltip says which of the two names is
which, and the generated one is still what you type after `tmux attach -t`.

Blank clears the name rather than storing an empty one, so a tab is never
unlabelled; typing the derived name back clears it too, since agreeing with the
default is not overriding it. Names are cleaned on the way in from the
preferences file as well as on the way out of the field — the file is
hand-editable, and a name that could not be typed should not be loadable.

### Choosing a font

egui knows two families, Proportional and Monospace, and does no system font
lookup at all. The bundled monospace is Hack, which carries no Powerline or
Nerd Font glyphs, so a p10k prompt drawn in it is a row of empty boxes and
nothing in the app can fix that — the glyphs are in the user's font.

So `font.rs` finds the installed families itself and registers the chosen one
under a **third** family, `terminal`, which the two terminal panes ask for and
nothing else does. Three consequences worth stating:

- The rest of the window is untouched. The diff, transcript and Editor keep
  Hack, which the icon test already proves carries mogeung's own glyphs.
- The user's font goes **first in a chain** that still ends in the bundled
  fonts, so a glyph it lacks falls through instead of becoming a box. A font
  setting that silently deletes glyphs is worse than no font setting.
- The family is registered whether or not a font was chosen. An unknown
  `FontFamily::Name` is not a fallback in egui, it is a panic in the paint
  loop — and it would arrive every frame.

Read by hand rather than through `fontdb` or `font-kit`, for one reason that is
not dependency count: reading the three tables by hand means reading *only*
them. There are 385 font files and 220 MB under `/usr/share/fonts` on this
machine, and a crate that parses each face loads all of it.

### Files touched

- `crates/mogeung-ui/src/term.rs` — `Kind`, `Term::shell`, `shell_session_name`
  (now `(root, ordinal)`), `tmux_available`, the login-shell fallback; `ui`
  takes a `FontId` rather than a size
- `crates/mogeung-ui/src/font.rs` — the font scan, the family picker, and the
  `terminal` family registration (`R-B32`)
- `crates/mogeung-ui/src/shells.rs` — the panel's shells: ordinal allocation,
  open/close/select, and the stored shape (`R-B33`); tab names and the rename
  in flight (`R-B34`)
- `crates/mogeung-ui/src/app.rs` — `terminal_panel` and `terminal_body`,
  `terminal_font_menu`, `PanelAction`, `PtyPane` and `pty_focus`, `pty_body`
  shared by both terminals; the top bar's terminal toggle
- `crates/mogeung-ui/src/keymap.rs` — `Action::TabTerminal`, bound to `Alt+F12`
  and `Ctrl+Backquote`, now a toggle
- `crates/mogeung-ui/src/layout.rs` — the pane's *removal*, and `strip_retired`
  for layouts that still name it
- `crates/mogeung-ui/src/prefs.rs` — `terminal_font`, `terminal_font_px`,
  `terminal_panel`, and the `"terminal"` zoom key
- [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md)

### Risks and unknowns

**Two things that look like terminals.** The rename bought the vocabulary; the
labelling has to keep earning it. Both headers say what they are attached to,
and they no longer sit in the same strip of tabs — the Agent is a pane about
the session, the terminal is a panel about nothing, and after `R-B33` the
layout says so before either label is read.

**tmux sessions mogeung does not clean up.** Deliberate — cleaning them up
would delete the persistence that justified using tmux at all. They are all
named `mogeung-shell-…` so `tmux ls` explains them and one `kill-session`
removes one.

**The shortcut takes the keyboard.** That is what Alt+F12 does in IntelliJ, and
a shell you must click before typing wastes the shortcut. The failure mode it
risks — keyboard handed to a shell that never started, so the window answers
nothing — is why focus is deferred through `Shells::focus_wanted` and taken only
once the pty exists.

**Shells kept alive behind their tabs.** A background tab holds its pty rather
than dropping it. Under tmux that would be recoverable; on a machine without
tmux it would not be, and "switch tabs" would silently mean "kill that shell"
on exactly the machines that can least afford it.

**A font scan on the paint thread.** Once per process, lazily, reading three
tables per file — measured at 425 faces on this machine without a perceptible
hitch. Bounded at 4000 files and eight directories deep so a symlink loop in
`~/.fonts` cannot hang the window.

### Test strategy

The tests worth having are the ones where a mistake looks like success:

- `-A` present in the argv. Without it you still get a shell, in the right
  directory, every time — just never the same one, so a build left running
  vanishes and nothing explains it.
- No `=` prefix on `-s`. tmux would happily create a session literally called
  `=mogeung-shell-…` and make another next launch.
- Two checkouts with the same basename get different session names.
- A generated name is legal as a tmux name — `.` and `:` address a window or a
  pane, not a session.
- **Ordinal 0 produces exactly the old name.** A suffix there would look like
  everything working while every existing shell was quietly abandoned.
- Reopening a shell in a worktree returns to the session just closed, rather
  than minting a new one and stranding what was running.
- Closing a tab before the active one keeps you looking at the same shell —
  the index shifts underneath it.
- A layout that still names the retired pane parses, and does *not* restore it.
- Both default chords resolve to `TabTerminal`, and neither collides with the
  chord that gives the keyboard back.
- A `name` table is decoded per its platform: UTF-16BE for Windows records,
  ASCII for Macintosh. Reading one as the other yields either `M\0e\0s\0l\0o`
  or CJK gibberish, both of which are unselectable family names.
- Every truncation of a name table returns `None` rather than panicking —
  these files are written by other people's tools.
- The `terminal` family exists with no font chosen, and the bundled fonts stay
  behind the user's in the chain.
- **A renamed tab keeps its session name.** The failure it guards is invisible
  in the window and total outside it: the tab looks right, and the shell it
  points at is a new one.
- Anything that moves the tabs — a close, a select, a `+` — abandons a rename
  in flight, because the edit names a tab by index. Closing the tab to the left
  of the one being renamed is the case that would otherwise rename a shell you
  were not looking at.
- A pasted name cannot break the row: control characters go and the length is
  capped. Names come off the clipboard as often as the keyboard.
- Escape leaves the old name; blank restores the derived one.

## Notes

**The focus flag turned out to be two states, not two bools.** The first shape
was `shell_focused: bool` beside the existing `agent_focused: bool`, which is
the obvious extension and is wrong the moment both terminals are on screen —
every keystroke would be delivered twice. That was a split you had to build
yourself when the shell was a pane; with the shell in a panel it is the *normal*
arrangement, so `pty_focus: Option<PtyPane>` went from prudent to load-bearing
within a day.

The same change surfaced a second one. `release_pty` has to clear focus *only*
for its own pane: both draw every frame, so the Agent pane re-attaching would
otherwise pull the keyboard out of a shell mid-command. An unconditional
`= None` in the old code was correct only because there was one pane to be
wrong about.

**`Backquote` was reserved for this in the rename, and the reservation held.**
The test that guarded it (`the_shell_pane_key_is_still_unclaimed`) has been
replaced by its inverse, which now asserts both defaults resolve here.
`Alt+F12` was asked for directly; `Ctrl+`` ` `` was added alongside it because
the reflex splits cleanly by editor and neither camp should have to look it up.

**Nothing in the daemon changed.** The shells are client-local, spawned by the
UI process, which is the same shape the Agent pane already had — and after
`R-B33` the daemon does not even know which worktrees have one. A web client
asking the daemon to open a pty is a different feature with a much worse
security story, and [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md)
does not license it.

**The retired pane had to stay a variant.** `Tab::Terminal` serialises as
`"Shell"` and that string is in every saved `layout.json`. Deleting the variant
would make serde refuse those files, and `layout::load` degrades an unparseable
layout to the default — so the reward for upgrading would have been losing the
arrangement you built by hand. It parses, is stripped on load, and the tree is
simplified afterwards so its departure leaves no single-tab container behind.

**Escape and lost-focus arrive on the same frame.** An in-place rename commits
when the field loses focus, which is what clicking away should mean. egui also
surrenders focus on Escape — so reading focus first would save the edit you just
abandoned, and Escape would be indistinguishable from Enter. Escape is tested
first, and it is the only reason those two are separate branches.

**The `+` menu is where "not bound to a session" becomes visible.** It offers
the selected session's worktree first, then every other worktree mogeung knows,
then `$HOME` — which is the entry that could not have existed at all while a
shell needed a session to hang on.
