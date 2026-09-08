---
title: A pane pops out into a window of its own, and that window is an ordinary client
status: active
updated: 2026-09-08
decided: 2026-09-08
---

# ADR-0037 — A pane pops out into a window of its own, and that window is an ordinary client

## Context

Asked 2026-09-08: *"is that possible to move a session panel out of the current
tauri window? like popup and let me move around outside of the containing
window?"* A session's terminal is the pane you watch while doing something else,
and a pane that cannot leave the window can only ever be watched **instead of**
the thing you are doing, never beside it. Every arrangement inside dockview is
still inside one rectangle.

Three facts about this codebase decide the shape, and all three were checked
rather than assumed.

**A second window is already the sanctioned model, not an exception.**
[ADR-0013](0013-one-window-one-daemon.md) is about merging *queues* across
daemons and refuses that; what it settles positively is that *"watching two
machines means two windows"*. `wire/client.ts`'s `defaultUrl` already reads an
explicit `?url=` — documented as *"how a second window is pointed at a second
daemon"*. Nothing here invents a multi-window product; it uses one that was
already designed and never built a gesture for.

**The ptys are held in the shell and already broadcast to every window.**
`pty_open` streams with `app.emit("pty:data", …)`, which is app-wide rather than
addressed to `main`. A second webview receives every chunk today and needs only
to filter on the id it opened. The pty also lives in the Rust process, not in
the webview, so nothing about a terminal is bound to the window that started it.

**A pane already carries its own pty id.** `AgentPane` opens `${paneId}:${sid}`
precisely so that two panes may show one session, with a comment saying *"tmux
is happy to hand the same session to two clients; this is what asks it to."*

## Decision

**Popping out creates a real OS window that loads this same client with
`?popout=<kind>&session=<id>`, and that window is an ordinary client with no
special powers.** It runs the same store, opens its own socket to the same
daemon, and attaches its own tmux client. Nothing is shared between the two
windows but the daemon and the shell's pty table.

**The shell composes the URL, not the webview.** `popout_open` takes a kind and
a session id, checks both against an allowlist, and builds `index.html?…`
itself. A command that took a URL from the webview would be a window that can be
pointed anywhere; this one can only ever load our own page. Same shape as
`scratch`'s naming rule, for the same reason.

**It moves the pane; it does not copy it.** The source pane closes as the popout
opens, and closing the popout puts a pane back. Two reasons, and the second is
the one that would have bitten:

1. The ask was to *move* a panel out, and a pop-out that leaves a duplicate
   behind is a split, not a move.
2. **tmux sizes a session to its smallest attached client.** Two attached
   clients of different sizes is the two-head resize churn that leaves stale
   rows on screen — a diagnosed, previously-reported artefact in this project.
   Popping out a pane that *stays* would create exactly that pairing every time,
   as a feature. Handing over creates no new client at all.

**The daemon address is not passed.** Both windows load from one origin, so they
share `localStorage`, and `defaultUrl` already falls back to
`localStorage["mogeung.url"]`. Threading the address through the query string
would be a second source of truth, and — since `R-I16` — a place a token could
end up in a window title.

## Alternatives

**dockview's `addPopoutGroup()`.** Already in the installed dockview (4.13.1)
and by far the least code: it moves the group's DOM into a popup and keeps one
React tree, so panes drag between the two. It loses on the thing that cannot be
worked around — **it is built on `window.open`**, whose behaviour inside a Tauri
webview is unproven here and differs by platform (WKWebView on macOS,
WebKitGTK on Linux). `R-J38` is this project's standing lesson that a wiring
check in a browser tab proves nothing about the shipped webview, and
`window.open` is the single most tab-flattered API there is: it would work
perfectly at `localhost:1420` and could do nothing at all in the app. Refused
for now rather than for ever — if a future Tauri makes `window.open` a
first-class webview, this becomes the cheaper implementation of the same
decision, and the seam is one function.

**A floating group inside the window** (`addFloatingGroup`). Real, supported,
and answers a different question: it floats *within* the window, so it cannot be
put on a second monitor or beside the editor, which is the entire ask.

**Render the popout from a second entry point** rather than branching the
existing one. Cleaner in the abstract; in practice it doubles the bootstrap —
the theme, the store, the keymap, the notification wiring — and the second copy
is the one that silently rots.

## Consequences

**Easy.** A session terminal on a second monitor. More windows cost nothing new
architecturally: each is a client, the daemon stays the only authority, and
`R-I11`'s split of client state by subject already decides what a second window
does and does not inherit.

**Hard.** The popout is a second React root with its own socket and its own
subscription, so a session watched in a popout is fetched twice from the daemon.
That is the honest price of *"every UI is a client"* and it is the same price a
second window pointed at a second daemon has always paid.

**The cost worth naming.** `app.emit` is app-wide, so **every window now
receives every pty chunk and discards most of them**. With one popout that is
one extra filter per chunk on a busy TUI; it is not free, and it scales with the
number of windows rather than with the number of terminals being watched. If
popouts become common the emit wants addressing to a window
(`emit_to`), and the pty table already knows enough to do it. Not done now
because a fix for a load nobody has is how the perf passes in `R-J53`–`R-J59`
got their work.

**Ruled out.** Dragging a tab between the two windows — they are separate React
trees with separate dockviews, and dockview cannot span them. Popping out
anything that is not a session pane, for now.

## Revisit if

`window.open` becomes dependable in the Tauri webview on both platforms, which
would make dockview's own popout the cheaper way to honour this same decision —
and would bring tab-dragging between windows with it.

Or: the per-window pty broadcast shows up in a profile. The fix is `emit_to`
and it is not a design change.

## Amendment — 2026-09-08

**The alternative this ADR refused is now the implementation, and the reason it
was refused turned out to be wrong in a specific and checkable way.**

What changed the question was the next ask, the same day: *"can the pop out
window be a dockable panel itself? such that I can move another panel and dock
it there?"* The Decision above cannot answer that at any price. Two windows
each running their own client are two React trees with two dockviews, and HTML5
drag-and-drop does not cross OS windows — which is why *"Dragging a tab between
the two windows"* is in **Ruled out**. Answering the ask means one dockview
across both documents, and that is `addPopoutGroup`.

**Why the refusal was wrong.** The Alternatives section says `window.open`'s
behaviour inside a Tauri webview *"is unproven here and differs by platform"*.
It is not unproven and it does not differ: in Tauri v2 it is **opt-in**.
`tauri-runtime-wry` installs wry's `with_new_window_req_handler` only when the
application supplies one, and `WebviewWindowBuilder::on_new_window` is the
documented public way to supply it — returning `NewWindowResponse::Create {
window }` to hand back a real Tauri window with capabilities and title syncing.
mogeung had never called it, so a `window.open` would have done precisely
nothing. The prediction — *"it would work perfectly at `localhost:1420` and
could do nothing at all in the app"* — was right about the symptom and wrong
about the cause, and the cause is a switch rather than a platform.

**What this replaces.** The clause *"that window is an ordinary client with no
special powers… its own store, its own socket"* no longer holds, and neither
does **Ruled out**'s first line. A popout is now the **same** client drawing
into a second document. With it go three things the old shape needed and the
new one does not: the second React root (`PopoutApp`), the hand-over that closed
a pane here and reopened it there (`returnAgentPane`, the `popout:closed`
event), and the `?popout=` URL contract. Net less code.

**What survives unchanged.** The window is still labelled `popout-*` and still
gets exactly `main`'s capabilities, for exactly the reason recorded above — a
window matching no capability has no commands and fails as a blank pane. And
the *decision in the title* is untouched: a pane still pops out into a window of
its own.

**What this costs, and it is not nothing.** The two windows are now coupled: one
dockview, one store, one socket, so a popout cannot outlive the main window and
closing the main window takes the panes with it. The old shape would have
survived that. It also means the theme has to be carried across by hand —
dockview copies stylesheets but not the `data-theme` attribute those stylesheets
read their colours from, so a popout without `mirrorTheme` renders in the wrong
palette rather than unstyled, which is far easier to mistake for a design.

**Still unverified in the shipped app**, and now that matters more rather than
less: the whole mechanism rests on `on_new_window` behaving in WebKitGTK and
WKWebView. `popOutPane` treats a refusal as a first-class outcome — the pane
stays where it is and the window says so — so the failure is legible rather than
silent, but it has not been seen. `R-J38` still applies, and a browser tab
proves less than usual here: a tab has a real `window.open` and will succeed
whatever the shell does.
