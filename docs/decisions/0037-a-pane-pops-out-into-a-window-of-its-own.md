---
title: A pane pops out into a window of its own, and that window is an ordinary client
status: active
updated: 2026-10-11
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

## Amendment — 2026-09-25

**The Decision's promise that "closing the popout puts a pane back" was false in
the shell, and is true again.** Nothing is reversed; this records how the
mechanism the 2026-09-08 amendment left *"still unverified in the shipped app"*
actually behaves there, and what `R-J108` does about it.

Reported the same day: *"After popout, there is no easy way to pop back in. I
have to drag and drop the pane and then manually close the empty pop-up
window."* dockview puts a popout group back when the popout document raises
`beforeunload`, and closes the window with `window.close()`. Neither holds in
a Tauri window:

- **The OS close button never raises `beforeunload`.** wry destroys the webview
  and the page's unload does not run, so dockview is never told and the group
  is not returned.
- **Script's `window.close()` does not close the OS window.** On WebKitGTK wry
  answers it by destroying the webview alone, which leaves the window standing
  and empty — the window in the report. On WKWebView it does nothing.

**What changes.** The popout's header offers the way home where the way out was,
and it raises `beforeunload` itself, so dockview's own return runs rather than
a move of our making. `popout.rs` holds the first close of a `popout-*` window
and emits `popout:close`, and the client sends that window's group home first;
a second press passes through, so a client that never answers cannot trap a
window. And the client closes a popout's window through the shell whenever its
group goes, however it went — the button, the close button, or its last tab
dragged back by hand.

**What it replaces.** Nothing in the Decision or the 2026-09-08 amendment.
Closing a popout returns its panes, which is what the Decision said and what
dockview does in a browser tab; the shell now agrees with both. The
`popout:closed` event the 2026-09-08 amendment retired stays retired —
`popout:close` travels the other way, shell to client, and carries no pane.

**Still not seen in the shipped app**: the close routing and the window's
destruction are shell behaviour, which a browser tab cannot show. The client's
half is pinned against a real dockview.

## Amendment — 2026-10-08

**One fact under Context is wrong, and the decision does not depend on it.** "tmux sizes a session to its smallest attached client" describes tmux's default; `yolomo` sets `window-size latest` ([ADR-0021](0021-the-pane-sizes-the-window.md)), so a session follows whichever client last sent input. Two heads still churn — the resize ping-pongs with focus rather than clamping to the smaller — so the consequence drawn from it holds.

## Amendment — 2026-10-11

**The door serves dialogs too.** Asked 2026-10-11: *"make the new 'New
Session' or 'Teams' a real popup dialog that can move around out of the
current mogeung application. (But make it a modal popup, only allow one copy)
because the font is too small and I want to make that popup screen resizeable
independently"* (`R-J128`).

The New session and Teams dialogs now open in OS windows of their own,
through the same `window.open` the 2026-09-08 amendment opted the shell into:
`popout.rs` makes each a `popout-*` window with the system's title bar, the
main window's `Dialog` portals its panel into that document, and the React
tree — the store, the socket, every `send` — stays where it was. The arcade's
page-of-its-own was weighed and refused for these: a page with no store would
have to be told the teams, the sends and the sessions over a channel, where a
portal reads them. What this adds beyond the pop-outs: a dialog window is
**claimed** in `popout.ts` under its label, so the close the shell routes as
`popout:close` closes the dialog rather than looking for a group; the main
window keeps a backdrop that names the window and raises it, so the dialog is
modal from either side; and the window has a zoom of its own, stepped from
inside it and applied to its webview by label.

**What it replaces.** Nothing. The Decision's window is still an ordinary
client — this one is not even that, being a view of the main window's tree —
and dockview still owns the pop-outs it opens. The 2026-09-25 amendment's close
routing is reused as it stands; a label with a tenant simply answers it first.

**Not yet seen in the shipped app**: the sizing `window_features` gives a
dialog window, the zoom by label, and the close routing for a window that
holds no group. The client's half is pinned by `Dialog.detached.test.tsx`.

## Amendment — 2026-10-11, later the same day: the opener is told the label

**What changed.** `popout.rs` now announces every window it opens to the
main window, as `popout:opened` with the label and the URL that was asked
for. The amendment above left the label with the *page* — an initialization
script, read by the opener once the page had loaded — and a dialog closed
before that moment had no label to close by, so it fell back to the page's
own `window.close()`, which the 2026-09-25 amendment already records as
leaving an OS window standing, empty. `tauri dev` did it on every open,
because StrictMode mounts, unmounts and mounts a dialog once. With the
label announced at creation, `lib/dialogWindow.ts` closes through
`popout_close` in every case, including a page that never loads. The
dialogs' zoom is a CSS `zoom` on their page's root rather than the webview's
by label: nothing of the shell is needed for it, and the reason the main
window refuses a CSS zoom — the terminal's hit-testing — is not in a dialog.

**What it replaces.** Nothing of the Decision, and nothing of the
amendments: the page is still told its label, dockview's pop-outs still
close as the 2026-09-25 amendment says, and the announcement is one more
sentence the shell speaks, which a pop-out's opener may ignore.

**Still not seen in the shipped app**: the announcement's arrival before a
close, and `window_features` sizing. The client's half is pinned by
`Dialog.detached.test.tsx`.
