---
title: A pane in its own window
status: shipped
updated: 2026-09-08
roadmap: [R-B55]
depends_on: [A30]
---

# 0041 — A pane in its own window

## Spec

### Problem

Every arrangement mogeung offers is inside one rectangle. A session's terminal
is the thing you watch *while doing something else*, and a pane that cannot
leave the window can only be watched instead of that something else — never
beside it, never on the second monitor, never next to the editor it is talking
about. Asked 2026-09-08:

> is that possible to move a session panel out of the current tauri window? like
> popup and let me move around outside of the containing window?

`R-B49` answered the neighbouring question — two Agent panes at once — and its
answer stops at the window edge.

### Assumptions

[A30](../product/assumptions.md) — the user will keep two agent sessions on
screen at once, and arrange them, rather than returning to one — is
`SUPPORTED`, and this is the same bet one step further out. Its recorded
weakness applies here unchanged and is worth repeating: it was filed
`SUPPORTED` on a day of use, not a week, and the uncomfortable direction is
toward [A1](../product/assumptions.md) — the queue's claim is that it *tells*
you which session needs you, and a user arranging windows has started watching
instead of being told.

This row does not settle that either way and must not be read as evidence for
it. If anything it raises the stakes: a detached window is a session you have
decided to watch continuously, which is the behaviour A1 exists to make
unnecessary.

### Acceptance

- [x] A control in the Agent pane's header moves that pane into a window of its
      own, which can be moved anywhere on the desktop.
- [x] The popped-out window shows the same session, with a live terminal.
- [x] The pane **closes** in the main window when it pops out — it moves rather
      than duplicating.
- [x] Closing the popped-out window puts the pane back, anchored to the same
      session.
- [x] Asking twice for the same session focuses the window that is already open
      rather than opening a second.
- [x] A session id that would not survive a query string is refused rather than
      escaped, on both sides.
- [x] The popped-out window has exactly the shell permissions the main one has —
      no more, and not fewer, or its terminal cannot open a pty.
- [x] In a browser tab the control reports that it cannot do this, rather than
      doing nothing.

### Explicitly out of scope

- **Dragging a tab between the two windows.** They are separate React trees with
  separate dockviews, and dockview cannot span them.
- Popping out anything that is not an Agent pane. The allowlist has one entry
  and grows by someone deciding it should.
- Remembering popped-out windows across a restart. A popout is a thing you did
  just now, and a window that reopens itself is one you have to close twice.
- A second popout of the same session. It is refused by focusing the first.

## Plan

### Approach

[ADR-0037](../decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md) is the
shape. The popped-out window loads the **same client** at
`index.html?popout=agent&session=<id>` and is an ordinary client: its own store,
its own socket, its own tmux attach.

The shell composes that URL from a checked kind and a checked session id, so the
webview never names a page. The window is labelled `popout-<kind>-<session>`,
which is what `capabilities/default.json` matches with `popout-*`.

`main.tsx` branches on `readPopout()` before rendering anything, because the
answer decides what a root even is.

### Files touched

| Path | Change |
| ---- | ------ |
| `desktop/src-tauri/src/popout.rs` | new — the command, the allowlist, the return event |
| `desktop/src-tauri/capabilities/default.json` | `popout-*` gets what `main` gets |
| `desktop/src/lib/popout.ts` | new — read the URL, ask the shell, hear the close |
| `desktop/src/PopoutApp.tsx` | new — what a detached pane renders |
| `desktop/src/main.tsx` | the branch |
| `desktop/src/lib/panes.ts` | `returnAgentPane` |
| `desktop/src/ui/PaneChrome.tsx` | the control |
| `desktop/src/App.tsx` | take the pane back when the window goes |

### Risks and unknowns

- **It moves rather than copying, and that is a tmux decision as much as a
  design one.** tmux sizes a session to its *smallest* attached client, so two
  attached clients of different sizes produce the leftover-rows artefact already
  diagnosed in this project. A pop-out that left the pane behind would build
  that pairing in as a feature.
- **Every window now receives every pty chunk.** `app.emit` is app-wide, so a
  popout doubles the number of listeners discarding most of what they are sent.
  Named in the ADR; the fix is `emit_to` and it is not a design change.
- **The pane comes back on `Destroyed`, not `CloseRequested`.** The second can
  be cancelled, and a pane handed back while its window is still on screen is
  the pane in two places.
- **A popout whose session has already ended** shows a dead window with an
  explanation rather than an empty terminal — but only after the socket is up,
  because "gone" said during connection would be a lie that corrects itself.

### Test strategy

Rust: the session-id check refuses everything a query string would have to
escape; the label is matched by the capability glob; the allowlist holds only
what it should.

TypeScript: `readPopout` re-checks what the shell checked, because a URL is
hand-editable from the dev tools; `returnAgentPane` anchors to the session that
left rather than to the selection, and does not add a second pane for a session
already anchored.

## Notes

**Built 2026-09-08.**

- **The architecture did most of the work, and that was checked rather than
  hoped.** Three things were already true: `pty_open` emits app-wide rather than
  to `main`, so a second webview receives its chunks with no change; the ptys
  live in the Rust process, so no terminal is bound to the window that started
  it; and `AgentPane` already opens `${paneId}:${sid}` with a comment saying
  *"tmux is happy to hand the same session to two clients"*.
- **ADR-0013 turned out to be an argument *for* this, not against it.** It reads
  as though it might forbid a second window; it is about merging queues across
  daemons, and what it settles positively is that *"watching two machines means
  two windows"*. `defaultUrl` already read `?url=` for exactly that. Worth
  knowing before anyone writes an amendment it does not need.
- **The `capabilities` file is the trap.** A new window matches no capability by
  default, and a window with no commands fails as a **black pane** rather than
  as an error — the terminal simply never opens a pty. `popout-*` had to be
  added to `windows`, and the label format is now load-bearing, which is why
  there is a test asserting the glob matches it.
- **`styles.test.ts` caught a real omission** — the second close button, in the
  session-has-ended branch, had no `focus-visible` state. A repo-wide guard
  earning its keep on new code written past it.
- **Not seen in the running window.** Every box above is ticked by a test and
  none by an eye: this session did not launch the desktop build, and this is the
  one feature in the project where a browser tab proves *least*, since a tab has
  no shell and therefore no second window at all. `openPopout` returns `false`
  there by design.
