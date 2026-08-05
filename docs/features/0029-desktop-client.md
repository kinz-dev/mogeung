---
title: The React desktop client
status: shipped
updated: 2026-08-05
roadmap: [R-M1, R-M2, R-M3, R-M4]
depends_on: [A1, A6, A13, A14, A15, A16]
---

# 0029 — The React desktop client

Decided 2026-08-04, after a week of real use settled the question item 0 was
waiting on. The reasoning is in
[ADR-0018](../decisions/0018-a-second-client-in-typescript.md); this is what
gets built and in what order.

## Spec

### Problem

Two things arrived at once and pointed the same way.

**The week happened.** mogeung carried 70–80% of the interaction with agents.
[A1 and A6](../product/assumptions.md) — a cross-session attention queue changes
where you look, and 3–4 concurrent sessions is normal work — are the product's
own premises, and they have been `UNTESTED` since v0.1 died before the question
could be asked. They are no longer speculation.

**What is left to build is UI-shaped.** A better Insight (`R-F11`'s charts), a
scratchpad (`R-L2`), a bookmark panel that summarises, a richer global search.
Every one of those is a place where the web ecosystem is a decade ahead of what
can be hand-rolled on an immediate-mode canvas, and where the egui client's
`explorer_viewer` — 600 lines of painting text runs and hit-testing galley
geometry by hand — is the cost being paid.

The concrete moment that started it: *"the current editor looks very immature"*.
It is not an immature editor. It is a hand-written text renderer, because Rust
has no CodeMirror and no Monaco, and there is no crate to swap in.

### Assumptions

- **A1 / A6** — no longer `UNTESTED` by the only test that was ever going to
  settle them. A week of use at 70–80% adoption is exactly what item 0 asked
  for. The ledger is updated to `SUPPORTED` in the same pass as this spec.
- **A13** (`SUPPORTED`) — keyboard-first. The bindings port unchanged.
- **A14** (`SUPPORTED`) — dockable panes are wanted and kept. dockview replaces
  `egui_tiles`; this is the requirement that chose React over Vue.
- **A15 / A16** (`SUPPORTED`) — the explorer earned a pane and then a
  workbench. Nothing here re-litigates that.

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is to
> test the assumption, not to build the feature.

Nothing below rests on an untested one, which is the point. This is the first
feature in the repository that can say so.

**What is deliberately *not* assumed:** that the viewer should become an editor.
That question was raised, considered, and answered *no* on 2026-08-04 —
[ADR-0019](../decisions/0019-a-viewer-not-an-editor.md). It matters here because
answering it that way removed the riskiest part of this port: no write verb, no
ADR superseding pillar K, no new daemon work at all.

### Acceptance

- [x] A second client speaks the existing protocol with **no daemon change**
- [x] Both clients can run at once against one daemon
- [x] Queue, Transcript, Code, Changes, Git, Insight, and the rail's four tools
- [x] The Code pane is read-only, and says so when you type into it
- [x] Charts where the data is a shape (`R-F11`)
- [x] A window that mounts before the daemon has said anything (`R-J7`)
- [x] The two pty panes — written; the Rust half needs webkit's headers to compile
- [ ] Health, keymap and connections windows
- [ ] Blame gutter, symbol outline, markdown preview
- [ ] A week of use at parity, and the egui client retired

### Explicitly out of scope

- **Editing worktree files.** ADR-0019. The daemon has no verb for it.
- **Git write verbs.** They exist on the wire (`R-D19`–`R-D22`) and this client
  sends none. Adding them is a decision, not a port.
- **Changing the wire protocol.** If the port needs a protocol change, that is a
  signal logic is leaking into the client — see the note below.
- **Retiring the egui client early.** It is what works today.

## Plan

### Approach

A strangler, not a rewrite. Multiple clients on one daemon is already the design
— the tray is one, and `R-C3`'s phone client was one at zero daemon cost. So the
new client runs *beside* the old one and grows until it is better.

**Stack**, with the one decision that was not obvious: React rather than Vue,
because docking is the single place Vue's ecosystem is genuinely thinner and
`A14` says docking is wanted. Everything else — Monaco, xterm, TanStack — is
framework-agnostic.

| layer | choice | why |
|---|---|---|
| shell | Tauri 2 | its Rust side holds the pty, so ADR-0010/0011 survive |
| ui | React 19 + TS + Vite | docking, and depth for everything else |
| layout | dockview | `egui_tiles` + `layout.json`, serialisable |
| code | Monaco (`readOnly`) | a read-only editor is a better viewer than a hand-rolled one |
| state | zustand | one store fed by the socket; no request/response layer |
| lists | TanStack Virtual | deletes the transcript's `show earlier` cap |
| charts | Recharts | `R-F11` |
| keys | tinykeys + cmdk | the existing bindings, and the palette |

**`wire-protocol.md` is the porting spec.** The protocol is the contract; the
client is written against it and does not get to change it.

### Files touched

- `desktop/` (new) — the client. `src-tauri/` is its own cargo workspace, so a
  machine without webkit's headers can still run `cargo test --workspace`
- `docs/design/architecture.md` — a second client, and what is native about it
- Nothing under `crates/`. That is the claim this feature is making.

### Risks and unknowns

- **The two pty panes are the hard part and are last.** `R-I6`'s
  tmux-over-ssh target building is real logic living in the client today, and
  `egui-term` is a vendored crate that goes away. Scoped, not built.
- **Tauri needs `libwebkit2gtk-4.1-dev`**, absent here. Until it is installed
  the client runs in a browser — which is a real client, not a fallback, and is
  how everything but the terminals was exercised.
- **Two clients means two sets of view state.** `prefs.json`/`layout.json` on
  one side, `localStorage` on the other. They will disagree, and that is
  tolerable while both run — but `R-I12`'s argument (that the machine-scoped
  half belongs to the daemon) gets stronger the longer this lasts, and should be
  settled before the egui client is retired rather than after.
- **Nothing here has been looked at.** The agent that wrote it cannot render a
  window. Typecheck, tests and a jsdom mount all pass; whether it *looks* right
  is unmeasured, and the first session with it will be a list of corrections.

### Test strategy

- `tsc --noEmit` and `vitest` in `npm run build`.
- A **smoke test that mounts the whole window** with the socket stubbed. It
  cannot judge appearance; it catches the failure that shows a blank page. It
  earned its keep immediately — see the notes.
- The keyboard contract (`searchMove`) is tested at both boundaries, as its
  Rust twin is.

## Notes

Built 2026-08-04 in one pass, at an explicit *"be ambitious, port the whole UI
in one go"*.

**The smoke test found an infinite render loop before any human saw the app.**
`scoped()` returned `prefs.scoped[key] ?? emptyScoped()` — a fresh object on
every call, read through a zustand selector, so it was never equal to itself,
so every subscriber re-rendered for ever. A blank frozen window. The fix is a
module-level constant; the lesson is that a lazily-defaulted getter must not
mint its default when it is read through a selector, and two more getters had
the same shape.

**A second, quieter one:** the explorer's fetch ran in the render (correctly —
that is the "one door" rule that makes a docked pane work unswitched) but had no
in-flight guard for file *bodies*, only for directory listings. It would have
sent a `fetch_file` every frame for any tab whose content had not arrived. The
Rust original has `pending_files` for exactly this and it did not survive the
port until the loop above sent me looking.

**The import cycle was caught by reasoning rather than by a test**, which is
worth noting because it is the kind that tests often miss: `App.tsx` imported
`useKeymap` and `keymap.ts` imported `focusPane` from `App.tsx`. Vite tolerates
it until it does not, and the failure is `undefined is not a function` at module
init. `focusPane` moved to `lib/panes.ts`.

**The terminals landed in a second pass, on 2026-08-04.** The argv-building was
the part worth care and it ported almost verbatim: tmux's exact-match `=` on
attach (without it, `mogeung-api` also matches `mogeung-api-v2` and you end up
in front of the wrong agent), `new-session -A` so a shell tab is the *same*
shell across restarts, and the ssh wrapper — a login shell *and* an appended
PATH fallback, because `ssh host cmd` is non-login and a login shell run with
`-c` is still non-interactive. Twelve tests cover it, including the one that
matters most: `reachFor` must not read a tunnelled daemon as local.

Two smaller decisions. Shell tabs reuse the egui client's session names
(`mog-0`, `mog-1`) on purpose — while both clients run they are two views of one
set of tmux sessions, not two sets. And closing a tab removes the tab, not the
session: `tmux attach -t mog-0` still finds it, which is the whole of ADR-0011.

**The Rust was written twice-unverified and then simply built.** For most of a
day it was recorded as uncompilable because `pkg-config --exists webkit2gtk-4.1`
came back false — so the work was checked indirectly instead: the dependency
graph resolves, and every `portable-pty` call was read against the crate's own
source, including that `PtyPair` has no `Drop` impl, which is what makes the
partial moves in `pty_open` legal. All of that held. But the premise did not:
the headers were installed the whole time, and one bad reading was carried
forward into three documents as fact rather than re-checked. **Building it took
under a minute and found two real errors that no amount of reading would have:**
Tauri rejects an icon that is not RGBA (ours was RGB), and an import was unused.

The lesson is not about icons. A negative capability check is a claim, and a
claim that shapes a plan deserves to be re-tested before it is written down —
especially a cheap one.

**Two defects found by the first real use of the Tauri build**, and neither was
visible from the browser.

*The shortcuts were dead.* The guard asked "is a `<textarea>` focused?" to
decide whether a bare letter belonged to a text box — and Monaco keeps a hidden
textarea for selection and clipboard, so the **read-only** Code pane read as
typing and swallowed every bare-letter binding in the window for as long as it
had focus. Two changes: the guard now asks *which surface* has focus (a
terminal owns everything, a read-only viewer owns only navigation keys, a real
text box owns everything bare), and the listener moved to the **capture** phase,
because Monaco and xterm both call `stopPropagation` on keys they handle and a
listener on the way up never sees them. Eight tests now cover it, including the
three-surface policy — the jsdom wiring test passed before the fix, which is
exactly why it needed a test that models focus rather than one that models the
map.

*Monaco was being fetched from a CDN.* `@monaco-editor/react` loads it at
runtime by default, at a version pinned by the loader rather than by our
lockfile — 0.55.1, where `package.json` says 0.52.2. So the app ran a
dependency we had never installed, and would show no Code pane on a train.
Worse in principle: `architecture.md` states that exactly **one** outbound
network call exists in this product, `git fetch` on a keystroke (ADR-0014). This
was a second one, in the client, on every launch, silently. Monaco is now
bundled; the loader short-circuits before it can inject a script tag.

**A design-system pass followed on 2026-08-04**, asked for as "a coherent
design system and material UI styling". The interpretation is worth recording
because it was a judgement rather than a transcription: Material's *system* —
a type ramp, an elevation scale, state layers, motion tokens — but not its
**metrics**. 48dp targets and 16dp gutters are right for a thumb on a phone and
would roughly halve the rows on screen in a board read beside a terminal; MUI
the library would have spent the pass fighting Tailwind and `R-J6`'s two tested
palettes.

The audit that started it is the useful part, because it measured rather than
guessed: **six** ad-hoc font sizes (including `text-[11.5px]`, a half-pixel
nobody chose), four corner radii, one `transition` in the whole client, and
**zero** `focus-visible` styles. That last one is not a polish item. `A13` —
keyboard-first — is `SUPPORTED`, and a keyboard-driven interface where you
cannot see what has focus is the product not working.

What landed: tokens in `@theme` so Tailwind emits real utilities; a
`ui/styles.ts` holding the shared vocabulary (**this** is the system, not the
tokens — the property that "a thing you can press" has one spelling); 199
ad-hoc values replaced; a focus ring on all 51 interactive controls; state
layers as translucent washes of the *content* colour, which is what lets one
hover rule work on the queue, the tree and a tab strip without any of them
knowing what surface they sit on; `prefers-reduced-motion` honoured; and a
`Segmented` control replacing three hand-rolled spellings of the same thing.

`ui/styles.test.ts` is the guard: it fails on a reintroduced ad-hoc size,
radius, raw hex, or a `<button>` with no focus ring. A design system decays one
hurried component at a time, and by the time that is visible it costs another
sweep. Writing it caught a bug in **itself** first — `<button onClick={() =>`
contains a `>`, so the obvious non-greedy tag regex truncated before reaching
`className` and reported seven failures of its own making.

**Four things the first real session found**, all reported within an hour of
running the Tauri build — which is the argument for shipping it before parity
rather than after.

*The queue did not fill its panel.* Dragging it wider grew the container and
left the rows at their old width. `ZoomPane` had been slipped between the panel
and its content the day before, and a plain `div` in a flex row sizes to its
content — so the wrapper needed `flex-1 w-full min-w-0` it had never been given.
A wrapper added for one purpose silently changed the layout of everything it
wrapped.

*Ctrl+wheel did nothing, and the handler was never the problem.* React
registers `onWheel` at the root as a **passive** listener, so `preventDefault()`
inside a synthetic wheel handler is ignored: the code ran perfectly and the
browser declined the one thing it asked for. A native listener with
`{ passive: false }` fixes it. Worth remembering as a class — a synthetic event
that cannot prevent its default looks exactly like a handler that never fired.

*The icon was a placeholder.* `crates/mogeung-ui/assets/mogeung.png` has existed
since the egui window shipped; the Tauri set is now generated from it at every
size, because two clients wearing different faces would read as two products.

*Tabs could be closed.* Removed — these are fixed views of a session, not
documents, and there is no such thing as the Git tab you are finished with.
Done with a custom tab component rather than CSS that hides the button, because
the CSS would have left dockview's middle-click-to-close armed and invisible.
Every pane is also re-added on load if a layout saved before the change is
missing one: a pane reachable only by remembering its shortcut is a pane you
have lost.

**The queue lost its right-click, and the two symptoms had one cause.**

Reported as two things — labelling a session was gone, and the Attention list
"looks different" from the Rust one, with titles truncated to `M.` and `<..`.
The port had replaced the egui client's context menu with hover-revealed icon
buttons, and a hover-revealed button **still occupies its space at
`opacity: 0`**. Three of them held ~70px of every row hostage, so the title was
squeezed to two characters — while the label editor, which never had a home in
this client at all, was simply missing.

Both fixed by going back to what the original did: a right-click menu with
Label / Pin / Snooze / Hide, plus `R-B26`'s label editor. One detail carried
over deliberately — the hide entry is **absent** for a live session rather than
greyed out, because an item you can see but cannot press invites the question
"why not", every time.

Two smaller bugs surfaced on the way. `truncate` needs `min-w-0 flex-1` on a
flex child or the ellipsis never engages, which is why the title collapsed
rather than clipping. And `Row` named its props and dropped the rest — Radix's
`asChild` clones the trigger with merged props, `onContextMenu` among them, so
a component that swallows unknown props means the menu never opens with nothing
to debug.

**Two React lifecycle bugs, both invisible to every cheaper test.**

*The label field lost the caret on every keystroke.* `Dialog` listed `onClose`
in its focus effect's dependencies — and `onClose` is an arrow function defined
in the caller's body, so it has a new identity on every render. The effect
therefore re-ran after every character and called `panel.focus()`: the dialog
was stealing focus from itself. Held in a ref now, with the mount effect
depending on nothing, and it declines to grab focus at all if something inside
already has it. The regression test asserts **focus after a re-render**, which
is the only assertion that fails — the value updates, the handler fires and the
dialog is on screen either way. Verified by restoring the old dependency array
and watching it fail.

*Every character typed into the Agent pane appeared twice.* Not an echo: two
listeners. The pty setup is a chain of `await`s, and React's StrictMode mounts,
unmounts and remounts an effect in development — so cleanup ran while those
promises were still in flight, `unlistenData` was still `null`, and the listener
registered *after* teardown stayed attached for ever. The remount added a
second, and every byte from the pty was written twice. Each `await` now
re-checks the disposal flag and undoes itself; `pty_open` on the Rust side
explicitly drops any existing pty under the same id, because a reader thread
that outlives its pane is the same bug one layer down.

Worth naming as a pattern: **an async effect must re-check its own teardown
after every `await`**, not only before the first one.

**One executable is enough again.** The Tauri client started life as a pure
client, so launching it from the app menu with nothing on 7717 sat at
"connecting" for ever and looked broken. ADR-0009 had already settled this for
the egui window and the port simply had not carried it, so `daemon.rs` came
across largely verbatim — the reasoning was paid for once and two clients
behaving differently in the same situation would read as two products.

The parts worth keeping intact: the test is the **bind**, not a probe, because
two windows opened together would both see an empty port and both try to start
one; the daemon runs on a **thread in this process**, so it cannot outlive it
and there is no pid file to go stale or orphan to reason about; and anything
already holding the port is confirmed to be mogeungd via `/api/health` before
it is trusted, or an unrelated service on 7717 leaves the window on a socket
that never connects with nothing on screen explaining why.

Two rules are now tested rather than assumed. A bare JSON server answering
`{"ok":true}` must **not** pass for a daemon — `headline` is the shape only
mogeungd produces. And a non-loopback URL must never trigger hosting: being
told to watch another machine is a decision already made. Writing the first
test found a flaw in the test itself, which is worth recording because the
failure was indistinguishable from the bug: serving a fixed number of
connections dropped the listener partway through, so `acquire` found the port
free and hosted — exactly the symptom being checked for.

**Rebinding, `R-B12`'s other half.** The window listed the bindings and could
not change them, which is half a feature in a tool whose keyboard-first premise
is `SUPPORTED`: a binding you cannot discover is one you do not use, and one you
cannot change is one you work around.

Two decisions worth recording. **Recording captures every key**, with
`stopImmediatePropagation` — the whole point is to rebind a chord that already
means something, so pressing `Ctrl+K` to rebind it must record `Ctrl+K` rather
than open the palette. Anything less makes exactly the keys worth rebinding the
ones you cannot. And a conflict **steals and says so**, leaving the displaced
action *unbound* rather than shuffled onto a free key nobody chose: an unbound
action is visible in this window, whereas one silently moved to `Alt+J` is a
booby trap. That is the egui client's behaviour too.

**It does not share `~/.mogeung/keymap.json`**, and that is not laziness. The
file is keyed by the egui client's own action names, and its loader fails the
*whole file* on a key it does not recognise — so writing this client's ids into
it would silently reset the Rust keymap to defaults. Two clients, two files,
until there is one client. Checked in the source rather than assumed.

**The title bar folded into the top strip.** The OS bar spent a whole row on
the word "mogeung" and three buttons — about 35px, permanently, in a tool whose
job is fitting a board and a diff on one screen. `decorations: false`, and the
top strip carries `data-tauri-drag-region` instead.

The trade is not free and is worth writing down rather than discovering: with
decorations off the window manager stops providing **dragging, maximise on
double-click, and the resize border**, so the application owns all three. Two
follow from the drag region; the third does not, so there is an explicit grip in
the bottom-right corner. Whether GTK leaves any invisible resize border behind
on an undecorated window is not something to find out after shipping a window
nobody can resize, and the grip costs nine pixels.

The capability file grew by five permissions — minimise, maximise, is-maximised,
close, resize-drag. That file is the list of what the window may ask the shell
for and it is deliberately small, so each one is a line someone can read.

**Bookmarks learned to point, and to carry a remark.**

Clicking one scrolled the Transcript to the right place and then said nothing
about *which* line — and a conversation is a column of turns that all look
alike, so "somewhere near here" is not an answer. The jump now sets two things:
the timestamp the pane scrolls to, and the `seq` it rings on arrival. The ring
fades after a couple of seconds, because a highlight that never leaves becomes
part of the furniture and stops meaning "here". Search hits use the same
mechanism — one way of pointing, not two.

The remark is written **in the Transcript**, on the turn, because the moment you
want to write one is the moment you are reading it. It leads in the bookmarks
list with the turn's own words underneath, which is the order that matters:
"egress vs ingress" is what you scan for, and the turn is how you confirm you
found it.

**No new wire field**, and that was the constraint worth respecting rather than
routing around. A `title` on `Note` would have meant changing `mogeungd`, and
the no-daemon-work property is what makes this port safe (ADR-0018). ADR-0015
already settled it anyway: markdown is the truth, and a note's own text is its
remark. A bookmark stays a note with an empty body — marking a turn and writing
about it remain one gesture with two depths.

**The keymap moved to `Alt`+letter, at request.** Panes and rail tools now take
a modifier chord each — `Alt+T` transcript, `Alt+C` code, `Alt+F` files, and so
on — rather than the bare letters inherited from the egui client. Two knock-ons
worth recording: cycling the theme lost `Alt+T` and moved to `Alt+Shift+T`,
because a pane is a constant gesture and a theme is a once-in-a-while one, so
the constant gets the cheaper chord; and `Alt+S` went to the search rail so its
three siblings were not letters with one number among them.

`Alt+1` focuses the Attention list, which needed more than a binding. Focusing
something collapsed to a strip is not focusing it, so it expands first; with
nothing selected it takes the top of the queue, so the first arrow moves *from*
somewhere rather than jumping; the list carries a visible focus ring, because
"which list are my arrows driving" has to be answerable by looking or a focus
model is only a hidden mode; and the viewport now follows the selection, since
arrowing down a long queue that does not scroll walks the highlight off the
bottom and looks like it stopped responding.

Two guards came with it, both ported from the egui client's own keymap tests: no
chord may be bound to two actions — a collision means one silently never fires
and *which* one depends on map iteration order — and no action may ship with no
binding, since one you cannot discover is one you do not use. The first caught
the `Alt+T` clash while the remap was being written.

**Global search gained an overlay**, on `Ctrl+Shift+F` — the chord every editor
taught for find-in-files, and the one this client had pointing at the rail.

Two ways in, because they are two jobs. The overlay is **ask and go**: it takes
the window, gives the results room, and closes when you act on one. The rail is
**keep it open**, beside the thing you are reading. That is the same argument
the palette and the rail already settled between them (ADR-0017), applied once
more rather than re-litigated.

They are the *same component and the same store state*, which is the part worth
recording: a query typed in one is still there in the other, and neither re-runs
what the other already asked. The only difference is a callback — the overlay
passes one that closes it when a result is opened, the rail passes none.
Duplicating the panel would have meant two searches that drift.

**The duplicated characters, third time and finally the right layer.**

Reported precisely enough to solve: switching away from a session and back
added *one more* copy of every keystroke, so `a` came out as `aaaaaaa` after
about seven switches. That count is the diagnosis — one orphan per switch.

Both earlier fixes were real bugs and neither was this one. The cause is that
`try_clone_reader` hands the reader thread its **own duplicated descriptor**, so
dropping the writer and the master does not close the pty: the `tmux attach`
client keeps running and the thread keeps emitting `pty:data` under an id the
window reuses the moment you come back to that session. N visits, N emitters,
one live listener — and the listener was never the problem.

Closing now *says so* rather than letting go of a handle: a `Drop` that raises a
stop flag and kills the child. For `tmux attach` the child is the **client**, so
this detaches and leaves the session running — ADR-0010 intact, which is the
thing that made killing feel wrong to reach for and is worth being explicit
about.

The test asserts on the **child process**, not the flag. A flag that is set
while the process keeps running is exactly the state that produced the bug, so a
test of the flag would have passed throughout. Verified by restoring the old
`Drop` and watching it fail.

Worth naming as a pattern, because it caught me twice: **a resource with a
cloned descriptor is not closed by dropping the original.** The types say
"owned"; the kernel disagrees.

**Warnings stopped blocking, and one of them stopped happening.**

Selecting a session outside a git repository put a red banner across the window
that stayed until dismissed by hand. Two separate faults, and fixing only the
visible one would have been the wrong lesson.

*It should not have been asked.* The Git pane refuses to render without a
`repo_root`, but its fetch effect ran regardless — so choosing a non-repo
session fired `git_log`, `git_status` and `git_refs` at a daemon that could only
refuse them. The guard was on the render and not on the request.

*And the answer should not have blocked.* A banner that waits for a click is
right for something you must act on and wrong for almost everything the daemon
refuses: "not in a git repository" is a **fact**, not a task, and making you
clear it is the window nagging you about your own choice. Now it toasts for
fifteen seconds, leaves on its own, and lands in a log behind a count in the
status bar. Nothing waits for anyone.

Two details worth keeping. The toast uses **one timer for the next expiry**
rather than one per toast or a poll, so an idle window schedules nothing at all.
And the log is capped at fifty — a daemon failing in a loop must not grow it
without bound, and the fiftieth copy of a message says nothing the first did
not.

**A bottom dock, and the centre got smaller on purpose.**

Insight, Git, Info and Debt left the centre for a dock above the status bar,
collapsing to a strip of names. The distinction is what you are *doing* versus
what you *consult*: the diff, the conversation, the file and the agent are the
work; the other four are reference you open, read, and leave. As tabs in one
strip, every consultation cost you the thing you were reading.

It is the right rail's construction turned on its side, and chrome by the same
rule (ADR-0017) — collapsed it is a strip, never nothing, because a dock you can
lose entirely is one you have to rediscover. Their chords moved with them
unchanged: a binding belongs to the thing, not to where it is docked.

Two details a move like this hides. A layout saved before it still names the
four panes, and dockview will happily restore tabs whose component no longer
exists — so they are closed explicitly on load, or the first launch after
upgrading shows four dead tabs. And the strip sits **above** the status bar
rather than in it: the status bar describes the selection, and a row of buttons
among that makes both harder to read.

The dock spans the centre column rather than the full width, so the queue stays
whole. That is a deliberate departure from IntelliJ's shape, on the grounds that
the queue is the reason this application exists and should not be shortened to
make room for reference material.

**Info moved again, out of the bottom dock and under the queue**, and the
second placement is the better-argued one. The other three reference views
answer a question about the repository or about every session at once; Info
answers "what is the row I just clicked". Sitting directly under the list you
clicked in makes selecting a session and reading about it one glance — nothing
comes forward, nothing goes away.

Its chord came with it unchanged, which is the same rule the dock move
followed: a binding belongs to the thing, not to where it is docked.

One detail in the drag: the divider is bounded against the **window**, not
against the column it is in, so pulling Info upwards can never leave the queue
with no rows. The queue is the reason this application exists and must not be
squeezable to nothing by furniture sitting under it.

**Notes gained a draggable editor and a scoped find.**

`Ctrl+F` searches within Notes, and **only while Notes has the keyboard** —
scoped by asking whether focus is inside the tool rather than by a window-wide
binding, because `Ctrl+F` belongs to whatever you are looking at and Monaco
wants it for its own find the moment the Code pane has focus. Pressing it twice
selects the box rather than merely focusing it, so the second press means
"search for something else" instead of "now delete what you typed".

The design-system guard earned its keep here: it failed the build on a
`rounded-[2px]` written into the match highlight. Chasing that down surfaced the
larger problem — the filter reads the **whole body** while the list previewed
only the first line, so a note matching on its fourth line looked like it had
matched at random. The preview now shows the line that matched, with the match
marked, which is the difference between a filter and a search.

**Bookmarks: a jump that needed a `seq`, and a row you could not remove.**

Clicking a bookmark "lost track of the session", and the cause was the jump
being expressed as a **timestamp**. A bookmark knows the turn it marks, but the
timestamp was read out of that session's *loaded events* — and a bookmark on a
session this window had never opened has none, so the jump resolved to `null`
and silently did nothing. It now jumps by `seq`, which stays pending until the
events arrive and then lands exactly on the marked turn.

A bookmark whose session is no longer watched now **says so**. It outlives the
session it marks — that is the point of it — so swallowing the click reads as
the panel having lost track, which is how it was reported.

And bookmarks can be removed from the panel they live in, rather than only by
finding the turn again in a transcript that may not be loaded.

**The notes delete: two hours, and the bug was in my probe.**

Deleting a note appeared to do nothing. A wire probe against a throwaway daemon
said `DELETE DID NOT TAKE` — and that was wrong. The daemon sends **more than
one snapshot per subscribe**, the probe saved a note on each, and it then read
the *second save's* broadcast as the delete's answer. Re-run with the save
guarded, delete works exactly as written.

So client and daemon are both correct, verified separately: a jsdom test
asserts the button sends `note_delete` with the open note's id, and the wire
probe asserts the daemon removes it. What was left was a gap between them, so
note mutations now re-ask for the list. That costs one small message and makes a
missed broadcast self-correcting **without the client taking local authority**
over notes, which is the line ADR-0015 and the client contract both draw.

Worth recording as a method note: a probe that races itself produces a failure
indistinguishable from the bug it is looking for, and I nearly filed a daemon
defect on the strength of one. Instrumenting the *sequence* rather than the
outcome is what separated them.

**What went easier than expected.** The wire types are a mechanical mirror and
the daemon needed no change whatsoever — every pane is fed by an endpoint that
already existed. The three-corpora search, the group-state distinctions and
ADR-0017's chrome-versus-panes rule all ported as *designs* even though none of
the code did, which is the argument for having written them down.

**The window had no face, and the branch had no colour.**

Two things from the same sitting. `decorations: false` makes `TopBar` the title
bar, which is also how the window stopped showing the mascot anywhere you could
see it while using it — the bundle icons were right the whole time, so it was
only ever the taskbar and the launcher that wore it. A 16px icon at the head of
the strip puts it back, and `index.html` now names a favicon so the browser
build (the one the README tells you to run first) stops wearing a blank page
icon. The wordmark stays out: the icon says it in less room.

In the Attention card the repo and the branch were the same grey and ran
together as one phrase — `immix-trading-v2 main` reads as a four-word name until
you stop and parse it. The branch is now blue in **both** clients, which is the
tint the egui status bar already gives git identity, so the two agree rather
than each inventing a convention.

**The tree's filter box searched the wrong thing.**

It matched a literal substring against the rows the tree had already
materialised — and the tree only walks a directory that is expanded, so a file
two collapsed folders down could not be found however exactly you typed its
name. That is the worst kind of empty result: "no such file" and "you have not
opened that folder" looked identical.

Two changes, and the second is the one that matters. The box is now a **regex**,
tried against the file name *and* the whole path, because anchors mean different
things on each — `^use` asks for names, `^src/` asks for a subtree, `\.tsx$`
wants the name — and a filter that tested only one of them is wrong half the
time. And a filter now searches the **whole worktree**: typing asks for
`list_tree`, the same walk the palette's go-to-file already uses, and the
results are a flat list carrying their directory rather than a tree indented by
parents that are not on screen.

Smart-cased, matching the daemon's content search, so the two boxes do not
disagree about what `Foo` means. An uncompilable pattern is **not** an error —
`explorer(` on the way to `explorer(1)` falls back to a literal substring and
says so under the box, because a red "nothing matches" mid-keystroke is
indistinguishable from a query with no hits. Results are capped at 300 rows with
the cap stated: these are plain divs, and `.*` over a monorepo must not be what
discovers that.

The egui client's rail has no filter box at all, so nothing there to port back
yet.

**The queue's last two gaps, and a stylesheet that never existed.**

`R-B6`'s grouping and forget-a-session were the two things the queue still could
not do. Both are ports, and grouping is the one with a rule worth restating: it
preserves rank *within* a repo and orders the repos by their most urgent
session — which falls out of taking first appearance in an already-ranked list —
so the top of the panel is still the top of the queue. Which repos you have
folded away is per-sitting state and deliberately not persisted, the same as the
egui client: a queue that opened with its urgent half collapsed from last week
would hide the thing it exists to show. Forget sits last behind a separator and
does not ask twice; it drops the session and its review marks from the daemon's
record and touches nothing under `~/.claude`, so a session whose transcript is
still on disk returns on the next scan as one nobody has read.

**The transcript's markdown was never styled.** `prose-mogeung` was applied from
the day markdown was turned on and defined nowhere, so `react-markdown` produced
correct HTML that Tailwind's preflight had already flattened — no bullets, no
heading sizes, no rules. Nothing errored; it simply looked like the markdown
toggle did nothing, which is why it survived so long. Written by hand rather
than by adding `@tailwindcss/typography`, whose scale is built for documents and
would have to be argued back down to the size of a dense pane, and the headings
are deliberately modest steps: a transcript is a conversation, and an `# H1`
typed by an agent must not shout over the message beside it.

The guard is in `styles.test.ts`, with the rest of the design-system rules: a
class matching this codebase's own naming must have a rule in `index.css`. It is
narrow on purpose — checking every class against Tailwind's real utility set
needs Tailwind's resolver, and a guard that cries wolf gets deleted. It fails on
the tree without the stylesheet, which is the only proof that matters.

**Ctrl+wheel over the Code pane, and the same bug's second half.**

The first half was written up above: React's `onWheel` is passive, so the
handler ran and the browser ignored its `preventDefault`. A native listener
fixed every pane except one — the editor. Reported as "I can't Ctrl+wheel to
resize the code", and the cause is the mirror image of the first: **Monaco's
scrollable element consumes the wheel and calls `stopPropagation`**, so the
event never reached the wrapper at all. A bubble-phase listener cannot see an
event that never bubbles.

Registered in the **capture** phase instead, which runs root-first, so the
modifier case is claimed before the editor sees it. Worth generalising: the
pattern is *any* embedded component with its own scrolling — xterm has the same
shape — and capture is what makes a wrapper's gesture win over a child that
handles its own input.

The fix also had to avoid a second bug it would otherwise have created. The
wrapper scales its pane with CSS `zoom`, and the Code pane *already* hands the
factor to Monaco as `fontSize: 12 * zoom` — so the first working Ctrl+wheel
would have applied it twice. `ZoomPane` grew a `scale` prop for that: the Code
pane keeps the wheel handling and the remembered factor, and scales itself.
Monaco also measures in device pixels to map a click to a character, which a
CSS scaling context throws off, so this is the right way round rather than a
tidier one.

Two tests, both of which fail on a bubble-phase listener: a child that
`stopPropagation`s still lets the wrapper zoom, and a bare wheel is left alone
so the content scrolls. There is deliberately no test for the CSS half — jsdom
does not implement the non-standard `zoom` property, so the assertion could not
tell the two behaviours apart.

**Navigation that did not navigate.**

Clicking a bookmark selected its session and set the turn and did nothing else,
so with the Agent or Git tab forward the click had no visible effect at all. The
state was right the whole time — the destination was simply behind another tab,
which from the outside is indistinguishable from a broken row.

The fix is `jumpToTurn`, and its two ordering rules are the interesting part.
`select` **clears** `focusSeq`/`highlightSeq` when the session changes, so
publishing the seq first would wipe the very jump it is setting up; and the pane
is raised *before* the seq is published, because the Transcript's scroll effect
asks a virtualised list to scroll to an index, and a list that is not being laid
out cannot answer.

Two more places had the same hole, found by looking rather than by being
reported. The global search jumps to a turn the same way — now raises the
Transcript. And **every** caller of `openFile` promised a file it did not show:
the diff row's button says "open this file in the Code pane" in its own tooltip.
The raise went into `openFile` rather than into its six call sites, which is the
same one-door rule `explorerFetch` follows. A comment in the Files tool claiming
"the Code pane is raised by the pane itself" was simply wrong, and is corrected.

Four tests, with a fake `DockviewApi`; the first fails without the raise,
verified by removing it.

**Banners, and the question of who speaks.**

`R-C1` was the gap worth closing first, because it was the only one where the
missing piece was the product's own promise rather than a view: mogeung exists
to tell you which session needs you *while you are looking elsewhere*, and in
this client it could not. The plumbing had been there all along — the Tauri
shell registers `tauri-plugin-notification` and the capability is granted — and
nothing imported it.

The interesting part is not delivery, it is that **two processes can post the
same banner**. `mogeungd --notify` announces its queue; a daemon hosted by this
window is started with notifications off, on the reasoning already written into
`daemon.rs` — the window is right there in front of you. That leaves exactly one
uncovered case, and it is the one that matters: this window hosting a daemon
while you work in another application. So the window speaks only when it hosts,
and only while it is unfocused. A daemon you merely attached to is someone
else's to announce.

Everything else is a port of `notify.rs`, deliberately faithful: the transition
*into* needing you, once, per session, with the seen-state rebuilt from the
queue so a session that leaves and comes back is heard again. The seen-state
advances **even while silent**, which is the difference between a notifier and a
backlog — without it, turning banners on, or coming back after an hour, would
fire one for every session that had been waiting all along.

Off until asked for, which is the rule the daemon states and the reason
`--notify` is a flag rather than a default. The OS permission is requested when
you turn it on, not at startup: prompting for a feature nobody has asked for is
the same overstep in a different costume. Six tests over the pure half.

**The diff grew its three reading aids, and they are one port.**

`wordDiff`, `sideBySide` and `syntax` had been sitting in `prefs.ts` since the
port began with nothing reading them — settings for features that did not exist.
All three live in `diff.rs` in the egui client as pure functions, so this is that
file translated: `highlight`, `wordDiff`, `sideBySide`, and the same eleven
properties asserted against the port. Two clients drawing one diff differently
would be worse than either drawing it badly, and the tests are what say so.

Not Shiki, which is already in the tree for Monaco. The highlighter is a
tokenizer with one flat keyword set and no language detection; it will
mis-colour things, and that is the trade a reading aid earns. Shiki would mean
an async highlighter and a theme pipeline for the sake of colour on a five-line
hunk.

One thing the port had to decide that the original did not spell out: a changed
line can carry a word diff *or* syntax colour, not both. They answer different
questions — what moved, versus what this is — and drawing both on one line puts
two colour systems on the same characters, where the emphasis that matters
loses. So a paired change wears the word diff and everything else takes syntax.

**Blast radius** (`R-D9`) needed no daemon work either: the command was in the
wire types and the store already handled the reply. What was missing was a
button. It sits on the file header, and its first version folded the file it was
asking about — `IconButton` swallowed the event, so a button inside a clickable
row had no way to stop it. The signature now hands the event on.

**Pillar E's other half.**

The Info pane already showed `verify_runs` and `claims` — what the agent
happened to run, scraped from its own transcript. What was missing is the half
where *you* ask: one command per repository, held by the daemon, `R-E2`, with
`R-E5`'s coverage on changed lines underneath it.

The rule that shapes the UI is that it **never runs on its own** — not on a
scan, not when a session goes quiet, not when the pane opens. mogeung observes;
a tool that ran your suite on its own initiative would be spending your machine
and your tokens on a decision you did not make, which is ADR-0003's line drawn
around a different verb. The button is the entire consent model, and the caption
above it says so rather than leaving it to be inferred.

Two details carried over deliberately. A daemon broadcast does not overwrite the
box while you are typing in it — the same rule the label editor follows — and
`exit: null` is rendered as **did not run** rather than folded in with a
non-zero exit, because "never started" and "ran and failed" are opposite
answers. Coverage says *no data* when there is none: an invented zero reads as
"nothing is covered", which is a much louder claim than the truth.

`unverified` came across as a helper on the wire types with four tests, since it
is the one that decides whether a session is marked as having edited files
nobody checked — and it means **no completed check**, not no passing check. A
session whose tests failed has been verified, badly.

**Switching daemons, from the window.**

`R-I7`'s hard part had been done for months: `store.setUrl` clears the board and
re-points the socket, and nothing called it. So the address came from `?url=` or
from whatever localStorage happened to hold, and moving between a laptop's
daemon and a dev box's meant editing a query string. The window is a list you
can add to, rename in place, switch by clicking, and forget from — written on
change, because a dialog dismissed with Escape must not lose the daemon you just
added.

The list is client-side and local, like the keymap and the layout: which daemons
*you* watch is not daemon state, and a remote daemon has no business holding a
list of its peers. A URL is the identity, deliberately — `ws://localhost:7717/ws`
through an `ssh -L` tunnel and `ws://devbox:7717/ws` are the same process by two
routes, and which route works is exactly what you are switching between. Whether
it is the same *machine* stays a separate question, answered by the daemon's own
identity (`R-I5`).

One thing the egui window has and this does not: browsing the LAN. That is
multicast, which a webview cannot do at all — it would have to move into the
Tauri half. The window says so rather than leaving an empty list to be
interpreted.

**The Code pane's four.**

Outline, markdown preview, blame and in-file bookmarks, and each one had to
answer a question about *where* it belongs in a viewer rather than an IDE.

*The outline* is `symbols.rs`'s posture rather than its code: line scanners, not
parsers, per language family, with an unknown extension getting an empty outline
and never a guess. Bounded the same way — 4096 characters of a line, 2000
symbols, 256 characters of a name — so a machine-generated file clips instead of
hanging. The first version called `if (ready) {` a method, which is exactly what
the ported test was written to catch; the fix is a keyword blocklist, and the
comment says that the blocklist *is* the rule's precision.

*Markdown is read, not previewed beside its source.* A viewer's job is the
rendered thing, and a split would spend half a narrow pane on syntax you did not
open the file for. It reuses the same `prose-mogeung` the transcript needed.

*Blame is injected text*, not a gutter. Monaco's glyph margin holds an icon and
nothing else, and a second synchronised scroll pane beside the editor is a lot
of machinery to keep aligned; injected text moves the code right, which is the
honest cost of putting words in a margin that has none. Repeats are blank, the
way `git blame` reads on a terminal. It is asked for only while the toggle is on
— a `git blame` per opened tab would be real work on the daemon for something
nobody asked to see.

*Bookmarks* claim the glyph margin, which is now drawn for the purpose: a click
in it marks the line. Nothing else lives there, so the gesture collides with
nothing, and the marks ride in the machine-scoped preferences beside the session
labels.

**Git depth, which was the largest gap and the shallowest work.**

Every command was already on the wire and typed; the store already had slots for
`reflog`, `worktrees`, `submodules`, `author`, `path` and `pickaxe`. What was
missing was the asking. So `R-D11`–`R-D16` is mostly wiring, and the two things
that were not are worth recording.

`git_conflict_stages` arrived at a `case` whose entire body was `break`. The
daemon answered, the client dropped the answer, and the pane sat on its empty
state — the failure that looks exactly like the daemon not implementing the
feature. It has state now, and a test that fails without it.

And the log's filters had to become **one** query. Five things narrow it —
message, author, path, pickaxe and the branch scope — and the second call site
that forgets one of them looks precisely like a filter being ignored, so there
is a single `askLog` and both the box and *load more* go through it.

The three-way conflict view shows ours, base and theirs read-only and says in
the header that resolving is git's job in your terminal. `git_resolve` is on the
wire and this client does not send it: what a conflict needs first is to be
read, and the markers in the worktree file are the one view that shows neither
original. The write family stays unsent for the reason it always has — ADR-0012
anticipated exactly this pressure, and taking it up is a decision rather than a
port.

**The last three windows, and the one that is a refusal.**

*Launch a session* (`R-B2`) is the only place mogeung causes an agent to exist,
and the shape is what keeps it inside ADR-0003: the daemon opens a **real
terminal window** in a directory and stops there. Nothing is wrapped, nothing is
proxied, and the session is read afterwards exactly like one you started
yourself. The `--dangerously-skip-permissions` warning is stated in the window
rather than left to be discovered — the flag is the agent's own and the daemon
only passes it, but a permission prompt that never comes is not something to
meet by surprise. The worktree option is offered for what it prevents: two
agents in one checkout is the collision the queue can only report after the
fact.

*The follow-up prompt builder* is the feature whose whole design is a refusal.
mogeung assembles what you flagged while reading a diff into text — and there is
no send button, and no wire message behind one, because writing into a session
is steering and steering is what made v0.1 worse than the terminal it wrapped.
The clipboard is deliberately the widest part of the pipe: a human is on the
other end of it deciding whether the text is right. Flagging is a button on each
hunk, and the quoted body is the changed lines only, since context is what the
reader can look up.

*The ambient board* (`R-C5`) is the queue at a size you can read across a room:
only the sessions that need you, only the reason and the label, nothing to
click. When there is nothing it says **All clear** in green rather than showing
an empty list — an ambient display you have to walk over and inspect is not one.

That closes the gap list. Everything still typed on the wire and never sent is
now the deliberate set: `git_stage`, `git_commit`, `git_discard`, `git_switch`,
`git_branch_create`, the three stash verbs and `git_resolve` — the write family
ADR-0012 anticipated, which is a decision rather than a port.

**A colour you choose, in a list full of colour that means something.**

Asked for as Finder's tags, and the reason given was cognitive load: knowing
which session is which without reading a word. That is the whole design
constraint, and it is also why the tag could not tint anything the row already
has. The badge is *why this row is where it is*, the live text is what the CLI
says it is doing, the label chip is a hash of your own name for it — three
colours that are all computed. A tag means whatever you decided this morning and
nothing computes it, so it takes the one strip of a dense row carrying no other
signal: a bar down the leading edge, absolutely positioned so it cannot push the
text along.

Seven, like Finder, and named after the colour rather than after a meaning. A
palette with meanings attached ("review", "mine") would be a guess about a
workflow you would then have to work around.

In the **collapsed strip** the tag beats the hashed label colour, which is the
one place the two compete. The hash exists so a chip means *something* before
you have decided anything; once you have decided, that is the better signal —
and the strip is the view with no text at all, so it is where a colour is worth
most.

`tag:` joins the field filters, and `tag:none` asks the opposite question. That
last one is the case worth a test: an untagged session has to *match* `none`
rather than match nothing, or there is no way to find what you have not sorted
yet.

**The colour tags shipped a blank window, and the smoke test did not catch it.**

Adding `tags` to the scoped preferences broke the app for anyone whose
preferences file predated it. `scoped()` returns the stored object **itself** —
it has to, because a selector that mints a fresh object every call re-renders
for ever, which is a bug this codebase has already had — so a field added after
a file was written is `undefined` at every read site. The first
`scoped.tags[id]` threw, React unwound the tree, and the window came up blank.

The fix is at the boundary rather than at the readers: `loadPrefs` completes
every stored scoped entry against the current shape. That is the TypeScript of
`#[serde(default)]`, and it belongs in the same place for the same reason — one
line that cannot be forgotten, instead of a `?.` at each of a dozen call sites
that can. It repairs `bookmarks` and `editorWrap` retroactively too; both were
added the same way and were the same accident waiting for a reader.

The instructive part is the test. The smoke test exists for exactly this class —
"a store selector that throws before the first message arrives" is in its own
header — and the first regression test written for the bug **passed against the
bug**, because a window with an empty queue never renders a row and the throw is
in the row. Seeding one session is the whole difference. A test that cannot fail
is worse than no test, because it is also a claim that the case is covered.

**The capture-phase fix broke the terminal's selection, which is the same bug
from the other end.**

Reported as text in the Agent pane selecting at the wrong position when dragged.
The cause is the fix two entries above: making `ZoomPane`'s wheel listener
**capture** meant it now runs *before* content that handles its own Ctrl+wheel.
The terminal is exactly that — xterm measures a character cell in device pixels,
so it changes its **font** rather than letting anything CSS-scale it, and the
comment saying so has been beside its handler since it was written. Capture took
the event first, CSS-zoomed the pane, and the grid drifted out of alignment with
the mouse.

So the boundary needed the other half. Stopping propagation from the inside used
to be enough to opt out; once the outer handler runs first, opting out has to be
**declared** — the terminal's host carries `data-owns-zoom`, and `ZoomPane`
leaves any event from that subtree entirely alone, unprevented as well as
unclaimed. The Agent pane also takes `scale={false}`, so a zoom factor stored
before this rule cannot still apply to it.

Worth stating as a general lesson, because it is the second time in two days
this exact trade appeared: a wrapper that captures gets to win over content that
would otherwise swallow the gesture, and pays for it by having to know which
content is *supposed* to win. The declaration is the price, and it is cheaper
than the alternative — an outer handler quietly overriding an inner one is
invisible until someone drags across a terminal.

**A port can lose a bug fix without losing a feature.**

Reported on 2026-08-05: a label applied by hand disappears on `/clear`. The
label code was ported from `prefs.rs` faithfully — the map, the editor, the
`label:` filter, the hashed chip colour — and every one of those works. What
did not come across was `migrate_succession`, the function that exists only
because `/clear` mints a new session id, added to the Rust client a day after
labels shipped and in response to exactly this report.

That is the failure mode of porting by feature: you port what the feature *is*,
and the fixes that are invisible unless you know the history stay behind. The
detail that makes it costly is that nothing in this client looked wrong. There
was no missing button and no failing test — the gap is a gesture that happens
outside the window, in the CLI, and it takes a day of real use to meet it.

`migrateSuccession` in `store/prefs.ts` now runs wherever sessions arrive, and
carries the **colour tag** as well: tags are keyed by session id the same way,
shipped the same morning, and would have been the next report. Its tests mirror
the Rust ones case for case rather than being written fresh, because the risk
worth designing against is the two clients disagreeing about what a successor
is — one window moving a label the other left behind is worse than neither
moving it.

The reason it can be a pure function at all is that the evidence is on the
wire: `Session` already carries `pid`, `alive`, `cwd` and `started_at`, so
succession is a fact the client can read rather than something either client
has to be told. The daemon had the matching hole one layer down and it was the
July bug repeating — `AppState::load` wiped `pid` for every session read back
from the database, so a `/clear` spanning a daemon restart lost its evidence
before any client saw it. The death path had been fixed in July and says why in
place; the load path had never been read in that light.

## Retirement, 2026-08-05

The egui client is deleted —
[ADR-0020](../decisions/0020-the-egui-client-is-retired.md) has the decision and
the alternatives. What is worth keeping here is what the removal *found*, since
that is the part a plan cannot predict.

**A global hotkey that was ✅ in the roadmap and registered nowhere.** The Tauri
shell loads `tauri-plugin-global-shortcut` and its capability file grants
`global-shortcut:default`, and nothing ever called `register`. Both halves of
the evidence said the feature was there — a dependency, a permission — and the
feature was not there. Deleting the old client would have deleted `R-B10` in
the same commit, quietly, with the roadmap still claiming it worked.

The lesson generalises past this row: **a port is checked against the code, not
against the dependency list.** Anything ported by feature has this shape
available to it, because the scaffolding is what gets copied first and the call
site is what gets forgotten.

**Documentation that described a CLI nobody could run.** `mogeung --url`,
`--hotkey`, `--addr` and `--foreground` are in four guide pages, and the window
that took them is gone; the window that replaced it takes no arguments at all,
because where it dials is a setting. Those pages were rewritten rather than
annotated. A command in a guide is an instruction, and an instruction that
cannot be followed is worse than a missing page — it costs the reader the time
to discover it is wrong, and some of that credibility does not come back.

**Two things stayed put, deliberately.** The `Files touched` tables in features
0001–0028 still name `crates/mogeung-ui/...`, because they record what was done
at the time and rewriting them to point at TypeScript would be a lie about the
past. And `~/.mogeung/prefs.json` and `state/<machine_id>.json` are untouched on
disk: nothing reads them now, nothing deletes them, and an importer into the
window's storage stays possible for as long as they exist.

## Two reports from use, 2026-08-05

Both arrived from the same session and neither is a port defect in the sense
`R-M4` looked for — nothing was left behind. One is a gesture the library
deliberately does not implement, and the other is a design that was right about
where colour goes and wrong about how much.

### Copy and paste in the Agent pane

**xterm.js implements neither, on purpose.** It draws its own selection rather
than making a DOM one, so the browser has nothing to copy; and it hands
`Ctrl+V` to the pty as `^V`, which is what a terminal is supposed to do. The
app is expected to supply the gesture, and this one never did. It read as
"copy and paste is broken" because every other pane in the window copies with
`Ctrl+C` and this one did not.

`Ctrl+C` is why the chords look the way they do. It has to stay `SIGINT` **with
a selection on screen**, or the pane loses the one thing you cannot do without:
stopping something. So copy is `Ctrl+Shift+C` — what every Linux terminal
already settled on, for this reason — and `Cmd+C`, which collides with nothing.

Paste is split in two, and the split is the load-bearing part:

- `Ctrl+V`, `Cmd+V` and `Shift+Insert` are **the webview's own editing
  bindings**. Declining to handle them hands the key to WebKit, which pastes
  into the hidden textarea xterm keeps focused; xterm's `paste` listener then
  forwards it with bracketed-paste applied. No clipboard API is involved, so
  there is nothing to be gated, prompted for, or missing.
- `Ctrl+Shift+V` is bound to nothing, so that one has to read the clipboard
  itself, and it is the only path that can fail. It says so when it does,
  naming the chord that does not depend on it.

Preferring the native route where one exists is the whole defence against a
webview whose clipboard *read* is gated, absent or asynchronous — three
different behaviours across the platforms this ships to, and the paste chord
people actually press now depends on none of them.

The price is `^V`: quoted-insert can no longer be typed at a pane. It is
reachable through tmux's own prefix, and it is much rarer to want than paste in
a pane whose job is answering a prompt.

**The half that is not a keyboard problem** is documented in the Keyboard
window rather than fixed, because it cannot be fixed from here: Claude Code
turns on mouse reporting, so a drag is the program's to interpret and xterm
never sees a selection to copy. Holding `Shift` while dragging is what takes it
back, and someone who does not know that reads a working copy as a broken one.

### The selection itself: a CSS zoom on the document root

The report above was the *keyboard* half. The one that mattered came next —
**"I can't use the mouse to select the text correctly"** — and it is not a
clipboard problem at all.

`applyAppZoom` set `zoom` on `document.documentElement`. The stored factor on
this desk was `1.4641`, four notches of `Ctrl+=`. A CSS `zoom` scales layout,
but `MouseEvent.clientX` and `getBoundingClientRect()` are then reported in
different spaces — so anything mapping a pointer to a cell drifts, and drifts
further the further you drag. That is the symptom, exactly.

**This is the third fix for one bug, and the first at a layer that can reach
it.** `R-B30` gave the terminal its own font-size zoom so a pane factor would
not scale it. The Ctrl+wheel fix earlier the same day let the terminal declare
`data-owns-zoom` so `ZoomPane` would keep its hands off. Both are about a
wrapper the terminal sits inside — and this factor is applied to the document,
above every pane. A pane cannot opt out of an ancestor it does not have.

The fix is to stop scaling with CSS: `getCurrentWebview().setZoom()` is applied
by the compositor, so hit-testing scales with the pixels and the two spaces
stay one. It needs `core:webview:allow-set-webview-zoom` in the capability
file, and the CSS path stays for the browser dev tab — which has no webview to
ask, and no pty either, so it renders no terminal to skew.

Two things guard it. `zoom.test.ts` asserts the negative — in the desktop
shell, the document root is never given a `zoom` — because that is the whole
of the bug and it is invisible in a screenshot. And `Terminal.tsx` reads the
*computed* zoom above itself and counter-scales when it finds one, so the
fallback path degrades to a smaller terminal rather than to an unselectable
one.

The prior claim in `zoom.ts` that a Tauri webview has no zoom of its own was
about the *keyboard*: `Ctrl+=` does nothing there, which is true and is why the
window has to provide the binding. It does not follow that there is no zoom to
drive, and that inference is what cost three fixes.

### The colour tags were not glanceable

`R-B42` shipped a bar down the leading edge, and the argument for it still
holds: the badge, the live text and the label chip are all computed, and a tag
means whatever you decided this morning, so it should not tint any of them.
What was wrong was the *amount*. Four pixels answers "which colour is this row"
only once you are already looking at the row, and the row is what you were
trying to find.

A tagged row now carries the colour across its whole width, at the weight
`--selection-bg` already uses — the request's own words were a background that
matches the selected highlight. Three things that decision needed:

- **Hand-picked per theme, not mixed at runtime.** The same translucency over
  `#141518` and over `#e8eaee` lands in two different places and only one of
  them would ever have been looked at. `--tag-*-bg` is fourteen values, and a
  test fails if `tags.ts` names one either palette does not define — a missing
  variable renders as *no background*, which looks exactly like the complaint
  it was meant to answer and would have said nothing while failing.
- **Both signals, always.** The first attempt withheld the tint while a row was
  selected, reasoning that one surface can only say one thing. The report back
  settled it: *"I need to be able to see that it is being coloured, and also
  that it is currently being SELECTED."* A tagged row that stops looking tagged
  when you click it is wrong precisely when you are checking you are on the
  right session. So the colour stays and selection is said **differently** — a
  `--state-selected` wash over the tint, keeping the hue, plus an inset ring in
  `--selection-stroke` and a wider leading bar. An *untagged* row keeps the
  plain `--selection-bg` surface, which is the case where a surface is free.
  The wash hangs off `aria-selected`, which makes that attribute load-bearing
  rather than merely assistive — worth knowing before anyone "tidies" it.
- **Blue is not the selection's hue.** `--selection-bg` is a navy of exactly
  this weight, so a blue-tagged row would have been the selected row; the tint
  sits cooler and darker, and the ring is what says selected in any case.

One implementation note worth keeping: the tint arrives as a `--tag-bg`
property on the element and the class reads it, rather than seven classes, so
the palette stays the only place a tag's colour is written down. The hover wash
is a `background-image`, because `--state-hover` is a translucent layer applied
as a `background-color` everywhere else — a tint set the same way would replace
it, and a tagged row would be the one row in the list that does not answer the
pointer.

### Files touched

| Path | Why |
|---|---|
| `desktop/src/lib/clipboard.ts` | the chords, and which paste route each takes |
| `desktop/src/lib/clipboard.test.ts` | that `Ctrl+C` is never copy, and one press is never two pastes |
| `desktop/src/ui/Terminal.tsx` | the handler, and `term.paste` so bracketed paste survives |
| `desktop/src/ui/KeymapWindow.tsx` | the terminal's chords, listed and marked not rebindable |
| `desktop/src/index.css` | `--tag-*-bg` in both palettes, and `.mogeung-tag-row` |
| `desktop/src/lib/tags.ts` | a surface beside each tag's colour |
| `desktop/src/ui/QueuePanel.tsx` | the tint, withheld while selected |
| `desktop/src/ui/QueuePanel.tag.test.tsx` | that the tint survives Radix's clone, and survives selection |
| `desktop/src/lib/zoom.ts` | the webview's zoom, not the document's |
| `desktop/src/lib/zoom.test.ts` | the negative: the document root is never zoomed in the shell |
| `desktop/src/lib/tauri.ts` | `setWebviewZoom` |
| `desktop/src-tauri/capabilities/default.json` | `core:webview:allow-set-webview-zoom` |
| `desktop/src/panes/AgentPane.tsx` | why the header keeps the panel surface |

### A darker queue, and which "background" that meant

Asked for immediately after the tints landed on it, and the two belong
together: a row's colour is read *against* what is behind it, so the darker
that is, the more a tint is worth.

**The first attempt darkened the wrong surface**, and the report — *"I don't
see the difference"* — is worth keeping because the mistake is easy to repeat.
The queue had no surface of its own; it showed `--bg` through. Giving it
`--queue-bg` darkened the frame, while the thing actually being looked at, the
box carrying a session's badge, title, branch and detail, was **transparent** —
so it stayed the same colour as the frame and nothing appeared to change. The
list is mostly rows; darkening everything except the rows is close to
darkening nothing.

So the queue is now two surfaces. `--queue-bg` is the frame — the header, the
gutter, the space under a short list. `--row-bg` is the card, and it is the
darker of the two, so a row reads as a thing sitting *in* the list rather than
as a slice of it. The collapsed strip takes the frame colour, because a panel
that changes colour when you collapse it looks like a different thing instead
of the same thing narrower.

**One row, three things wanting its background**, and they are not settled the
same way. `cn` is `twMerge`, so the last Tailwind `bg-*` wins — the card
arrives through `className` and would have silently dropped `Row`'s selection
surface. `.mogeung-tag-row` is not a Tailwind class at all, so twMerge cannot
see it and stylesheet order would have decided that one. Both are stated as
conditions instead, and the test that only ever one is present is the reason
the first version of this did not ship broken: removing the guard breaks
*selection*, three components away from the line you edited.

The other half of the report was *"or your changes actually have no effect"*,
and that deserves an answer too: `./scripts/start.sh` runs `npm run tauri dev`,
so CSS and TSX hot-reload and there is nothing to rebuild. A change you cannot
see in that setup is a change that is too small, not a change that has not
arrived — which is precisely what this one was.

### A darker terminal

Asked for in the same breath, and it is not only taste: the Agent pane drew on
`--bg-panel`, the same surface as the chrome around it, so the terminal and the
window read as one thing. A terminal is the one region here that is *not* this
app talking — it is another machine's output arriving — and the depth is what
says so. `--terminal-bg` is a shade under the panel in both themes, on
`TerminalView` itself so the shell panel gets it too. The pane header stays on
`--bg-panel`, because `PaneHeader` paints no surface of its own and the
boundary between the two is the point.

### A test that was proving less than it said

Writing the render test above turned one up. **The queue draws no rows in
jsdom at all.** It is virtualised, every element there is zero by zero, and a
list that draws only what fits draws nothing — so `App.smoke.test.tsx`, whose
comment reads *"a row on screen is the whole point: the throw is in
`QueueRow`, so a window with an empty queue proves nothing"*, has an empty
queue. Its own note says the first version of it passed against the bug it was
written for; it is in that state again, for a different reason.

Three stubs fix it and all three are needed — a `ResizeObserver` that actually
fires, and `offsetHeight`/`offsetWidth`, because `@tanstack/react-virtual`
takes its first rect from those before any observer runs. They are in the new
test rather than in `test-setup.ts` on purpose for now: giving every suite a
non-zero layout changes what several other tests are measuring, and that is a
sweep to make deliberately rather than as a side effect of a colour fix.
