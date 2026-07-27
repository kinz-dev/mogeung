---
title: Attached terminal
status: shipped
updated: 2026-07-27
roadmap: [R-B18]
depends_on: [A11, A12]
---

# 0003 — Attached terminal

Host a session's own terminal in a mogeung pane, by attaching to tmux rather
than owning a pty. Decision and the four rejected mechanisms are in
[ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md).

## Spec

### Problem

The queue tells you a session is `WAITING`. You open it, and the transcript
shows the conversation up to that point — but not the thing it is waiting on.
Claude Code's permission prompts, multiple-choice questions, plan-mode approval
and `/` autocomplete are TUI rendering; they never reach the `.jsonl`. So the
one moment the queue exists to serve is the one moment mogeung cannot help
with, and you leave for the terminal anyway.

`R-B2` (jump to terminal) shortens that trip. It does not remove it.

### Assumptions

- **A11** — an egui terminal widget can render Claude Code's TUI well enough to
  answer a prompt. `UNTESTED`.
- **A12** — starting sessions with `yolomo` is a change that will actually
  stick. `UNTESTED`.

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is to
> test the assumption. That is what this is. The mechanism underneath A11 was
> verified first and separately — `tmux attach` through a pty created by a
> non-terminal process returns real ANSI output — so what remains untested is
> the *widget's* fidelity, not whether the approach can work at all. The
> acceptance list below is the A11 experiment; it is deliberately made of things
> that fail loudly.

### Acceptance

- [x] A session started with `yolomo` shows a live Terminal tab
- [x] A session started with bare `claude` says why it cannot be hosted, and
      offers `R-B2` instead
- [x] Detaching leaves the session running and reachable from any terminal
- [x] Text entry works, including macOS **dictation** — which arrives as an
      ordinary `Event::Text` and so exercises the same path an IME uses. Korean
      composition specifically is still unconfirmed
- [ ] **A multiple-choice question can be answered from the pane** — arrows
      move, Enter selects
- [ ] Plan mode (`Shift+Tab`) and `/` autocomplete behave
- [ ] `Ctrl+C` interrupts a running turn
- [ ] Resizing the pane does not corrupt the display

The three unticked keyboard items are the ones that decide A11; failing any of
them means withdrawing the feature rather than patching around it, since the
fallback (`R-B2`) is intact and costs nothing.

This originally read *a permission prompt can be answered*, which `yolomo`
cannot produce — it passes `--dangerously-skip-permissions`, so the prompt never
appears. Any menu exercises the same arrows-and-Enter path, and plan-mode
approval is the easiest one to reach deliberately.

### Explicitly out of scope

- Retrofitting tmux onto an already-running session. Impossible; see ADR-0010.
- mogeung composing or sending input. Unchanged by this feature —
  [ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md) stands.
- Replacing the structured transcript. This adds a tab.

## Plan

### Approach

Daemon resolves, client renders. The daemon walks each live session's process
ancestry against `tmux list-panes` and publishes a `tmux_target` on the session;
the client attaches a pty running `tmux attach-session -t =<target>`. The client
never derives the mapping itself
([ADR-0001](../decisions/0001-rust-core-with-egui-ui.md)).

Lookup cost is one `tmux` call and one `ps` call per scan, not per session — the
ancestry walk runs against an in-memory pid table.

### Files touched

- `crates/egui-term/` — vendored widget, MIT, see `VENDORED_FROM`
- `crates/mogeung-core/src/session.rs` — `tmux_target`
- `crates/mogeungd/src/state.rs` — pane resolution, wired into the scan
- `crates/mogeung-ui/src/term.rs` — the pane
- `crates/mogeung-ui/src/app.rs` — Terminal tab, focus model
- `crates/mogeung-ui/src/keymap.rs` — `TabTerminal`, `LeaveTerminal`
- `scripts/yolomo`

### Risks and unknowns

- **Widget fidelity (A11).** v0.1.0, no claim of full terminal coverage. The
  mitigation is structural rather than technical: because the session is never
  trapped, a bad render costs a glance at your real terminal and nothing else.
- **Adoption (A12).** The feature is worth nothing for sessions not started
  under tmux, and that cannot be fixed after the fact.
- **Keyboard contention.** Claude Code wants Escape, arrows, `/` and single
  letters; so does mogeung's keymap. Resolved by yielding entirely while the
  terminal has focus, with a release chord Claude Code will never want.
- **Vendoring drift.** Upstream moves; we carry 2k lines.

### Test strategy

Unit tests for pane parsing, including a session name containing spaces. One
integration test that stands up a real tmux server, nests a process under a
pane, and resolves it — the flat-pid case passes trivially and would have hidden
the bug that matters. It skips where tmux is absent and tears down its server
even on failure. Nothing spawns an agent.

## Notes

**The wheel used to page the agent's prompt history** (reported on Ubuntu,
2026-07-27, but tmux-config-specific rather than platform-specific): with
tmux's `mouse` option off, a fullscreen app leaves the wheel in "alternate
scroll", which the emulator converts to arrow keys — and Claude Code reads
Up/Down as history navigation. Fixed twice over. The vendored widget now
speaks the wheel half of the mouse protocol, so `mouse on` setups scroll
natively (see `egui-term/VENDORED_FROM`); and for `mouse off`, the pane
consumes wheel events itself and drives `tmux copy-mode` / `send-keys -X
scroll-up` — tmux's *view* changes and the agent's stdin sees nothing, which
keeps it inside ADR-0010's line.

**The pty wall was the whole design.** Four plausible mechanisms were tried
before tmux: a text box (cannot answer a menu), IPC into `claude` (no listening
socket exists), `TIOCSTI` (`EPERM`, verified locally rather than assumed), and
pty stealing (destructive, macOS-blocked). Each was cheap to test and each
failure narrowed the design. Recorded in ADR-0010 because every one of them
looks reasonable enough to be proposed again.

**The widget bumped to egui 0.35 with zero source changes.** The main perceived
risk of the whole feature evaporated in one build, which is a good argument for
spiking the scary dependency before designing around it.

**`sh -c "cmd"` exec-optimises itself away.** The first shell probe of the
ancestry walk showed no child process at all and looked like a bug in the walk.
It was the shell being clever. The integration test now uses two commands
specifically to defeat that, and says so — otherwise a future reader "simplifies"
it back and silently stops testing the nesting.

**The pane shipped unfocusable.** `allocate_ui`'s response senses hover, not
click, so the `clicked()` that took the keyboard was never true and the hint
said *click the terminal to type into this session* about a click that did
nothing. The widget's own response is the clickable one. Nothing in the build or
the test suite could have caught it — the first person to open the tab did.

Two more defects were sitting behind it, both invisible until the click worked:

- **egui's focus navigation eats arrows, Tab and Escape.** Unfiltered, they move
  or drop focus instead of reaching the pty — which is precisely the acceptance
  list. Fixed with `set_focus_lock_filter`, the same mechanism `TextEdit` uses.
- **Upstream gates keyboard input on `contains_pointer()`**, so typing stops
  when the mouse leaves the pane and nothing says why. First local change to the
  vendored crate; see `VENDORED_FROM`.

**Shift+Enter needed the terminal's own configuration, read not guessed.** A pty
cannot express it — Return is one byte and the modifier is gone before the
process sees it — so Claude Code's `/terminal-setup` makes the *terminal* send
something else. Upstream binds Shift+Enter to `\r`, identical to plain Enter, so
multi-line input was impossible. Rather than guess between `\n`, `\x1b\r` and
`\x1b[13;2u`, the answer came out of the machine's own
`com.googlecode.iterm2.plist`, where `/terminal-setup` had already written `\n`.
Overridden from `term.rs` via the widget's public `add_bindings`, so this is not
a fifth local patch to the vendored crate.

**Two existing tab-cycling tests broke** on the fifth tab because they hardcoded
`Debt` as last. Rewritten against the ends of `TAB_ORDER`, so the next tab does
not produce failures in tests that are not about it.
