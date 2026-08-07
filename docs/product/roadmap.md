---
title: Roadmap
status: active
updated: 2026-08-07
---

# Roadmap

The ranked backlog.

Effort: **S** = hours · **M** = about a day · **L** = multi-day.

Status: **✅** = shipped and proven · **⏳** = built, installed, awaiting
the dogfooding verdict ([A19](assumptions.md)) · **🗑** = shipped, then
removed · blank = not started.

The distinction exists because a blank box on built work read as "not
done" and got R-D10 asked for twice.

~~Struck through~~ is a fourth state and deliberately not a glyph: the idea
was considered and **refused**, so it was never built and there is nothing
to mark shipped or removed. The row keeps its number and its box stays
blank; only the proposal is struck, and the reason beside it is left
readable. A glyph would have implied it had a lifecycle. It did not — it
had an argument.

A removed row keeps its number and stays where it was. Deleting it would
leave the ledger claiming the idea was never had, and the next person to
want a web client deserves to find the reason it went rather than the
silence.

**Item 0 is done.** A week of real use, reported 2026-08-04: mogeung carries
70–80% of the interaction with agents. A1 and A6 — the two assumptions the whole
product rests on — are `SUPPORTED` rather than speculation for the first time
since the project began. Pillar `M` is what that unlocked.

Pillars `A`–`H` and `J` are shipped and verified. `E`–`I` (bar the two descopes
in `I`) were **built 2026-07-29 in one pass** at an explicit *"finish the R-\*
items in one-go"* ask — a deliberate override of the item-0 gate, recorded per
spec (features 0015–0022) and in the ledger (A20–A25). That gamble was settled
on 2026-07-30: `E`–`H` were used and passed, so those rows are ✅.

**`I` was the exception**, because its rows reach machines and tools this desk
does not have — Codex with no real sessions on disk, a remote daemon, a second
OS — so a ✅ would have been a claim about a machine nobody ran. That changed
on 2026-07-31 for the remote half: a Mac watched from a Linux window settled
`R-I4`, `R-I5`, `R-I6`, `R-I7` and `R-I11`. What is still unjudged is `R-I1`
(Codex, which needs real Codex use rather than a second machine), `R-I3`
(Linux), and the two rows that need a daemon listening *beyond* loopback —
`R-I8` discovery and `R-I10`'s direct-bind rungs — because the ssh-tunnel route
that settled the rest never binds one.

## Identifiers

Roadmap items are `R-` plus a group letter and number: `R-A1`, `R-B3`, `R-H4`.
Assumptions in [assumptions.md](assumptions.md) are bare: `A1`, `A6`.

The prefix exists because the two collided. Roadmap `A1` (format canary) and
assumption `A1` (a queue changes where you look) are unrelated, and "A1 is
untested" meant different things depending on which file you had open. Feature
specs carry both a `roadmap:` and a `depends_on:` field, so the ambiguity was
going to land in every spec we ever wrote.

ADRs written before 2026-07-25 use bare roadmap ids — they are immutable and
were left alone. None of them reference a group-`A` item, so nothing there is
ambiguous.

---

## 0. The non-feature

**Use mogeung for a week with 3–4 terminals open.**

Assumptions [A1 and A6](assumptions.md) are the product, and both are
`UNTESTED`. v0.1 died before the question could even be asked. Every item below
is speculation until this is done, and some will look obviously wrong
afterwards.

The only work that should precede it is whatever makes the tool trustworthy
enough to judge — that was pillar `A`, and it is now **done**. Nothing else
should go ahead of the week of use.

---

## A. Trust the tool — **shipped**

Everything rests on two undocumented file formats ([A4](assumptions.md)). If
mogeung silently stops seeing things, nobody would know.

Delivered by [feature 0001](../features/0001-trust-the-tool.md) and
[ADR-0007](../decisions/0007-classify-every-transcript-line.md). It found three
event types that had been discarded silently, an unreachable size guard, and one
alert of its own that was confidently wrong.

`R-A6` was added 2026-08-02, from a bug found by using the thing: the canary
watches what gets *parsed*, and nothing watched what got parsed **twice**.

| # | Item | Effort | |
|---|---|---|---|
| R-A1 | **Format canary** — classify every line; alert on any unclassified type | S | ✅ |
| R-A2 | **CLI version watch** — record versions seen; warn on Claude Code updates, which is when formats move | S | ✅ |
| R-A3 | **Golden-corpus test** — snapshot-test the parser against anonymised real transcripts | M | ✅ |
| R-A4 | **Health panel** — sessions found, lines parsed/skipped, last scan, and what mogeung *cannot* see | S | ✅ |
| R-A5 | **Huge-transcript handling** — cap and tail rather than reading whole files | S | ✅ |
| R-A6 | **Durable tail offsets** — a restart resumes where it stopped reading, and a database written before that is repaired once. Reported 2026-08-02: every restart re-read every transcript whole, so a transcript appeared again in full and every counter grew by a copy. See [ADR-0016](../decisions/0016-rebuild-derived-state.md) | S | ✅ |

## B. Sharpen the queue — **shipped and verified end to end; `R-B39`–`R-B42`'s verdicts landed 2026-08-05, after `R-B35`–`R-B37` on 2026-08-03 and R-B27–29 and R-B31–34 on 2026-07-30**

Delivered by [feature 0002](../features/0002-sharpen-triage-and-review.md).

**The last four verdicts landed together on 2026-08-05**, and the signature is
weaker than the earlier pillars' — recorded here rather than smoothed over. It
is one report of a week's use, self-described as having exercised *about 90%*
of what these rows added, rather than a row-by-row walk. The rule that follows
from that: a defect found in them later gets **its own row**, and these are not
reopened. `R-B39`–`R-B41` carry a second caveat — their build notes below
describe the egui client, which
[ADR-0020](../decisions/0020-the-egui-client-is-retired.md) retired two days
after they were written. What was judged is the React window they were ported
to; the prose is kept as the record of where they came from, not as a
description of the code that now runs.

The terminal is the moving part here. `R-B31` shipped it as a pane on
2026-07-29 and the next day's use said the pane was wrong — it followed the
selection and could not exist before a session did — so `R-B32`–`R-B34` are one
run: a panel with a tab per shell, a font you choose, and a name per tab. See
[feature 0024](../features/0024-in-app-terminal.md) and
[ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md). The four were
judged together, as they were built, and passed on 2026-07-30 — which is also
the pane's obituary: one day between shipping a shape and being told it was the
wrong one is [item 0](#0-the-non-feature) doing exactly what it is for.

**Reopened once more on 2026-08-07 by `R-B53`**, which is the same thread
finishing rather than a new one: the Code pane had been pretending to be a tab
strip since `R-B25`, and two separate header fixes in one week were the symptom.
It awaits a verdict.

**The pillar was closed the day before, and the window's shape is what reopened
it.**
`R-B38` and `R-B43`–`R-B52` were built across 2026-08-06 and 2026-08-07 and
**all took a `kept` verdict on 2026-08-07**, at an explicit ask to close the
dogfooding backlog in one pass. The signature is weaker than the earlier
pillars' and is recorded here rather than smoothed over: this is one owner's
report at the end of the second day, not a week per row, and two of them are
bets only a week can settle — see `R-B49` and `R-B50`, whose caveats are
written into the rows. The rule that follows is the one this pillar already set
for `R-B39`–`R-B42`: **a defect found in these later gets its own row, and
these are not reopened.** They are one thread rather than a scattering:
`R-B45` moved Changes and Transcript into the dock, which left the centre
holding the file and the agent, and the first thing asked of a centre that
narrow was to put *two agents* in it. `R-B50` is the same request taken to
every session at once. It was held back on purpose until `R-B49` had been lived
with, and that gate came off on 2026-08-07 at an explicit ask to clear the
backlog in one pass — recorded here rather than smoothed over, because
[A30](assumptions.md) has had one evening rather than the week it asked for, and
the wall is a bet against the very queue A1 rests on. Both now want the same
dogfooding week.

| # | Item | Effort | |
|---|---|---|---|
| R-B1 | **Keyboard triage** — `j/k` move, `enter` open, `r` mark read, `o` open terminal | S | ✅ |
| R-B2 | **Jump to terminal** — focus the terminal *app's* window or tab for a session via pid/tty. Closes `WAITING` → acting | M | ✅ |
| R-B3 | **Collision warning** — two *live* sessions editing the same file right now | M | ✅ |
| R-B4 | **Permission vs. instruction** — distinguish "waiting for approval" from "waiting for next task", via a pending `tool_use` with no result | M | ✅ |
| R-B5 | **Snooze** a session for N minutes | S | ✅ |
| R-B6 | **Group by repo**, collapsible | S | ✅ |
| R-B7 | **Loop detection** — same tool + same path repeatedly is thrashing, not progress | M | ✅ |
| R-B8 | **Auto-select top item** and a "next" key | S | ✅ |
| R-B9 | **Search/filter** the session list | S | ✅ |
| R-B10 | **Global hotkey** to raise the window — the return half of `R-B2`. Re-registered in the Tauri shell on 2026-08-05: the port had the plugin loaded and the capability granted but registered nothing, so this row was ✅ against code that no longer existed anywhere. `Ctrl+Cmd+M` still, and no longer configurable — the flags that changed it belonged to the retired window | S | ✅ |
| R-B11 | **Pane-aware navigation** — `Alt+1`/`Alt+2`/`Alt+3`, one set of keys per focused pane | S | ✅ |
| R-B12 | **Editable keymap** — rebind, reset, import, export | M | ✅ |
| R-B13 | **Hide and pin sessions**, persisted across restarts | S | ✅ |
| R-B14 | **Scope** — needs-you / live / all | S | ✅ |
| R-B15 | **Field filters** — `repo:` `branch:` `file:` | S | ✅ |
| R-B16 | **Markdown transcript** — render replies as prose, not one long string | S | ✅ |
| R-B17 | **Tab shortcuts** — `c`/`t`/`i`/`d`, and cycling | S | ✅ |
| R-B18 | **Attached terminal** — host a tmux-backed session in a pane, so a TUI prompt can be answered without leaving mogeung. See [ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md) | L | ✅ |
| R-B19 | **Dismiss a session from the card** — corner `✕` and a right-click menu, never for a live one | S | ✅ |
| R-B20 | **Dockable panes** — arrange the detail tabs freely, two side by side | L | ✅ |
| R-B21 | **Command palette** — every action by name, with its binding shown | M | ✅ |
| R-B22 | **Keyboard-driven keyboard settings** — cursor, search and rebind without a mouse | S | ✅ |
| R-B23 | **Status bar** — session reference facts out of the header and along the bottom | S | ✅ |
| R-B24 | **File explorer pane** — the session's worktree as a tree, with a read-only syntax-highlighted viewer. Deliberately not an editor — see [pillar K](#k-explicitly-not) and [feature 0007](../features/0007-file-explorer.md) | M | ✅ |
| R-B25 | **Explorer workbench** — remember and reveal, multi-file tabs, go-to-file, content search, in one pass ([A16](assumptions.md)). Still a viewer, never an editor. See [feature 0008](../features/0008-explorer-workbench.md) | L | ✅ |
| R-B26 | **Session labels** — name a session yourself; colour badge on the card, `label:` in the filter. Client view-state like pins ([A17](assumptions.md)) — a claim now in question, see `R-I12`: a label is text you wrote, which is the one thing here that is authored rather than observed. See [feature 0009](../features/0009-session-labels.md) | S | ✅ |
| R-B27 | **Editor git ergonomics** — a diff gutter vs HEAD with next/prev-change keys, inline blame on the current line, compare-with-revision side by side, and a gutter mark on lines *this session* changed (the mogeung-only one). Still a viewer, never an editor | L | ✅ |
| R-B28 | **Editor navigation** — symbol outline and go-to-symbol (tree-sitter is already in the tree), go-to-line, sticky scroll, folding, highlight-other-occurrences | L | ✅ |
| R-B29 | **Editor content comforts** — markdown preview, image preview, per-tab word wrap, copy path / `path:line`, file facts in the header, bookmarks with a jump list | M | ✅ |
| R-B30 | **Per-pane zoom** — Ctrl+wheel over a pane scales that pane alone (Editor, Changes, Git, Transcript, Agent, Terminal), remembered per pane; the global Ctrl+=/− stays whole-window. Asked for directly, built 2026-07-28. **The global half was the cause of a defect reported three times, and fixed at the right layer on 2026-08-05**: it was a CSS `zoom` on the document root, which leaves mouse coordinates and element rectangles in different spaces, so a terminal — which measures a character cell in device pixels — selected text further off the further you dragged. Two earlier fixes could not reach it because it is applied *above* every pane. It now asks the webview to zoom itself, which the compositor applies to hit-testing as well as to pixels. See [feature 0029](../features/0029-desktop-client.md) | S | ✅ |
| R-B31 | **In-app terminal** — a shell of your own, `Alt+F12` or ``Ctrl+` ``. Under tmux, so a build or a `claude` started in it outlives the window and stays reachable from a real terminal; a bare pty when tmux is absent, labelled as such. The pane the Agent tab was renamed to make room for — and no longer a pane at all: `R-B33` moved it. See [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md) and [feature 0024](../features/0024-in-app-terminal.md) | M | ✅ |
| R-B32 | **Configurable terminal font** — pick the family the terminal panes draw in, from the monospaced fonts installed on this machine. The bundled Hack carries no Powerline or Nerd Font glyphs, so an oh-my-zsh prompt is a row of boxes until you can say otherwise. Asked for directly, built 2026-07-30. See [feature 0024](../features/0024-in-app-terminal.md) | S | ✅ |
| R-B33 | **Terminal as a workspace panel** — the shell leaves the pane tree for a panel across the bottom, on demand, with a tab per shell and no tie to any session: a terminal is where you *start* an agent, so it must outlast the selection and exist before there is one. Asked for directly, built 2026-07-30. See [feature 0024](../features/0024-in-app-terminal.md) | M | ✅ |
| R-B34 | **Name a terminal tab** — double-click a tab, or right-click it, to call it what it is doing; blank puts the folder name back. The label only: the tmux session stays keyed by worktree and ordinal, so a rename cannot strand a shell. Asked for directly, built 2026-07-30. See [feature 0024](../features/0024-in-app-terminal.md) | S | ✅ |

| R-B35 | **Bookmarks and notes in the Transcript** — toggle a mark on a turn, a view listing them, and free text against one. Asked for directly 2026-08-02, and promoted by `R-L1` to the **first slice of pillar L**: it is the smallest honest test of [A27](assumptions.md), because it is the one place a note has an obvious home and no editor is closer to hand. **Built 2026-08-02.** A mark and a note are one object at two depths — an empty body *is* a bookmark — so marking a turn and writing about it are the same gesture and you never choose which before you start. Daemon-owned per ADR-0015, mirrored one way to `~/.mogeung/notes/*.md`, and every change answers with the whole set so two windows cannot drift. `prefs` already carries a bookmark shape from `R-B29` (`(session, path, line)`), and this is the same idea keyed by turn rather than by line — worth reusing rather than inventing a second one. A note is the first thing in this product that is *the user's own writing* rather than a view of something the agent did, which is what makes it the small end of `R-L1` and worth building first. **Dogfooded 2026-08-03.** | M | ✅ |
| R-B36 | **Search inside the Transcript panel** — find within the conversation you are reading, rather than across every session (`R-F1` already does that). Asked for 2026-08-02 with a specific shape: run substring, regex, `rg` and fuzzy in **parallel** and show whichever answers best. That shape is the interesting part and the risky part — "best" needs defining before code, and it shares the external-tool question with `R-F10`. **Built 2026-08-02, with two of the four engines and the reasons recorded.** `rg` and `fzf` are **refused**: external binaries that may not be installed, where every other external dependency here is either required up front (`git`) or degrades to a named fallback (`tmux`), and where the corpus is one transcript — small enough that the process boundary would cost more than the matching. Regex is **deferred**, not refused: it needs a new crate, which is a dependency decision worth making deliberately. "Best" is answered narrowly: every engine scores on one scale, highest wins, and **the winner names itself**, so a wrong ranking is a bug report rather than a shrug. **Dogfooded 2026-08-03.** | M | ✅ |
| R-B37 | **Resize the Editor's tree and content independently** — the file tree and the file body currently move together. Asked for 2026-08-02. Small, and the kind of thing that is only noticed by someone actually reading in it. **Built 2026-08-02**: both the tree and the side-by-side split are draggable, because how much room a tree wants depends on how deep the project nests and no default can know that. **Dogfooded 2026-08-03 and half-right**: the drag is useful and stays, but "resize" meant Ctrl+wheel, not the divider — see `R-B39`. **Dogfooded 2026-08-03.** | S | ✅ |
| R-B38 | **Search a rendered Markdown preview** — find in the preview, not only in the source. `R-B29` shipped the preview; searching it means searching rendered text and mapping a hit back to a source line, which is the whole of the work. **Built 2026-08-07.** `renderedLines()` strips the syntax line by line and keeps each line's origin — a stripper, not a parser, the same posture (and the same warning) as `outline()`. Two decisions worth the tests behind them: a **fenced block is left verbatim**, because the preview shows it verbatim and stripping its asterisks would break the search on the text people paste most; and hits are ordered **down the page**, not by score, because a "next" that jumped to a better match further up is a next that lost you. What it does not do is highlight in the rendering — `react-markdown` owns that DOM — so the payoff is the source line, which is what the row asked for **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B39 | **Zoom the Editor's tree and content independently** — `R-B37` read the same complaint as *width* and made the divider draggable; the ask was **Ctrl+wheel**, which `R-B30` scoped to one factor per pane, so growing the code also fattened every row of the tree. Re-asked 2026-08-03 after dogfooding `R-B37`. **Built 2026-08-03**: the Editor is the first pane with two zoom regions, `editor-tree` and `editor`, picked by where the pointer is. A side-by-side split keeps *one* `editor` factor on purpose — a split is one document read two ways, and two halves of the same file at different sizes is a bug. The trap worth naming: an `egui::Panel` moves its parent's *cursor*, not its `max_rect`, so the leftover region must test `available_rect_before_wrap()` or it swallows a wheel that happened over the tree. **Verdict 2026-08-05: kept**, in the React window it was ported to | S | ✅ |
| R-B40 | **A right tool-window rail** — an always-present strip on the right edge that expands into a panel, IntelliJ-fashion. Asked for 2026-08-03 with four screenshots of RustRover. The construction already exists on the other edge: the Attention panel is a docked panel that collapses to a 30px strip and has never been in the tile tree, and this is that, mirrored, holding more than one tool. [ADR-0017](../decisions/0017-the-rail-is-chrome.md) sets the rule the window needed before a third docking idea arrived — the tile tree holds views of a session, the edge panels hold tools that outlive the selection. **Built 2026-08-03.** A `RailTool` enum, a `rail_panel` declared beside `queue_panel`, and two preferences. The strip is declared *before* the open tool so it keeps the outermost edge — the other way round, opening a tool slides the strip inboard and moves the very button you are about to press to close it. Width rides in `prefs.json`, not egui's `PanelState`, which dies with the process because eframe is built here without `persistence`. `]` collapses it, the mirror of `[`. See [feature 0027](../features/0027-right-rail.md). **Verdict 2026-08-05: kept**, in the React window it was ported to — the rail was not left collapsed, which is what [A28](assumptions.md) asked | S | ✅ |
| R-B41 | **Files as a tool window** — the worktree tree moves out of the Editor tab into the rail, so it is visible with the Transcript, the Git pane or the terminal forward rather than only with the Editor. Mostly a move: `explorer_dir`, the expansion state and the open-pinned-and-revealed bridge all exist already. `R-B37`'s drag and `R-B39`'s zoom travel with the tree rather than being deleted, and the `editor-tree` zoom key is kept so a preference written the week before survives. The cost is stated in ADR-0017 and is real: with the rail collapsed the Editor has no tree at all. Tests **A28** — that a tree beside every tab is worth permanent chrome — and comes out again if the rail sits collapsed for a week. **Built 2026-08-03**, and the move found something the plan missed: the tree does not fetch its own listings, `explorer_tab` did, in a block whose own comment explains it lives in the paint so a *docked* pane works unswitched. Moving the tree without it would have shipped a rail stuck on `listing…` unless you also opened the Editor. It is now `explorer_fetch`, called by both — safe because every branch already guarded on `pending`, so two callers in one frame still send once. **Verdict 2026-08-05: kept**, in the React window it was ported to | S | ✅ |
| R-B42 | **Colour tags on a session** — seven colours, Finder-fashion, set from a row of swatches in the row's context menu, with `tag:` and `tag:none` in the filter. Asked for 2026-08-05 with the reason attached — knowing which session is which *without reading a word* — and that constraint decided the design: a tag means whatever you decided this morning, where the badge, the live text and the label chip are all computed, so it tints none of them. **Built 2026-08-05** as a bar down the leading edge, the one strip of a dense row carrying no other signal. **Amended the same day on the first report**: a 4px bar was not glanceable, which is the only thing this row was for, so a tagged row now carries a tint across its whole width at the weight `--selection-bg` already uses — hand-picked per theme, with blue kept off the selection's own hue. **Amended twice**: the first pass withheld the tint while a row was selected, on the theory that one surface says one thing, and the report was that a tagged row must stay visibly tagged *and* visibly selected — a row you click to check you are on the right session is the worst moment to take its colour away. Selection is now said over the top of the colour rather than instead of it: a wash, an inset ring, a wider bar. Recorded here late: this shipped with no row at all, and the 2026-08-05 perf pass had the same gap until `R-J8` closed it. **Verdict 2026-08-05: kept**, tint and all | S | ✅ |
| R-B44 | **Order the queue by colour, then label** — a tagged session sorts above an untagged one, a labelled one above an unlabelled one, and the daemon's rank decides everything underneath. Asked for 2026-08-06. **Built the same day, and the cost is stated rather than discovered**: this puts two hand-made keys above the computed one, so an untagged `APPROVE` now sits below a tagged `running` — the panel's own claim is that it is *ranked by who needs you*, and that claim is now true only within a colour. It is a layered comparison rather than a replacement: equal on pin, colour and label returns exactly `0`, and `Array.prototype.sort` has been stable since ES2019, so the rank survives as the tiebreak and still orders every group. A test asserts that `0` on purpose — if it ever returns non-zero the ranking stops being the ranking, quietly. Colours sort in **palette** order (red, orange, amber, green, blue, purple, grey) rather than alphabetically, so the strip down the list reads the same way every time. Pins still float above colour: a pin is the stronger hand-made statement **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B45 | **Changes and Transcript become dock tools** — they leave the centre for the bottom dock beside Git, Insight and Debt. Asked for 2026-08-06. The dock's own rule said it held what you *consult* against a centre holding what you *do*; this redraws that line, and the doc comments that stated the old one were rewritten rather than left to rot. `Alt+X` and `Alt+T` keep working and now toggle, because the dock shows one tool at a time. **The cost, stated where it will be felt:** the diff and the conversation can no longer sit side by side, which the tile tree allowed. `showPane` learned the difference between a pane and a dock tool at the same time — before this it would happily add a tab for a component that no longer exists, which is how a bookmark click did nothing **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B46 | **The Code pane appears with a file and leaves with the last one** — no empty Code tab. Asked for 2026-08-06 alongside `R-B45`. It is the one pane with nothing to say by itself: every other pane describes the selected session, where an empty Code tab is a promise of a file that is not there. Added by any route that opens one — the tree, `Ctrl+P`, a diff row, all of which already went through `openFile` — and closed when the last tab goes. `Alt+C` still opens it deliberately: this hides a pane with nothing in it rather than forbidding one **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B47 | **Numbered dock chords, and Git at the right-hand end** — `Alt+2` Changes, `Alt+3` Transcript, `Alt+4` Insight, `Alt+5` Debt, `Alt+9` Git, in the order the strip is drawn; `Alt+1` still focuses the queue. Asked for 2026-08-06, and the reason is a real defect rather than taste: **`Alt+T` is Claude Code's own *toggle thinking*.** A chord always fires in the window — `focusOwns` defers only *bare* keys to a focused terminal — so pressing it over the Agent pane opened the Transcript instead of reaching the agent. A binding that takes a key from the program the pane exists to show is in the wrong place whatever it spells, and `R-B18`'s whole claim is that the pane is a view of a session you can still drive. The numbers cannot collide that way and carry an order the initials never did. A test presses `Alt+T`, `Alt+X`, `Alt+I` and `Alt+D` and asserts the dock does **not** move, so nothing quietly reclaims them **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B48 | **Open a transcript at its newest turn, and read it that way if you like** — asked for 2026-08-06 as *"sort in reverse order… or by default scroll to the bottom"*, and both shipped because they answer different halves. **Landing on the newest turn is the default and needs no toggle**: a conversation is read from where it got to, and arriving at the top of a four-hundred-turn session to scroll for the thing that just happened is the wrong end of the work. It fires once per session rather than per tick — following live output would yank the viewport mid-read — and a pending `focusSeq` wins, so a bookmark still lands where it aimed. **`newest first` is a checkbox** rather than the default, because the cost is real: a tool call and its result swap places, and a long answer split across turns reads bottom-up. The trap it set is worth naming — the displayed array is also the one a copied conversation and an exported file read, so an in-place `reverse()` would have written every export backwards, and only when the toggle was on. Two arrays now, one chronological and one for the screen, with a test asserting the reversal never touches the first **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B43 | **Export a transcript to a file** — a download button in the Transcript header that writes the whole conversation as Markdown. Asked for 2026-08-06. The window already had two ways to carry a conversation out — the clipboard and `R-L2`'s copy-into-a-note — and neither is a file you can attach to a ticket or keep after mogeung is uninstalled. **Built 2026-08-06.** Two decisions worth their tests. *Every turn goes in, `thinking` checkbox notwithstanding*: that box is a reading preference, and a file that quietly omitted the agent's reasoning would be wrong in the one direction an archive must not be. *Where it lands*: it shipped without a picker, on the argument that a dialog **and** a filesystem plugin would hand the webview a general write verb. Asked for the picker the same day, and the argument turned out to be about the wrong half — only `dialog:allow-save` was added, so the window may *ask* for a path and still cannot write to one, because the write stayed in a command the shell owns. With no picker to ask — plugin absent, or a desktop with no portal — it falls back to `$XDG_DOWNLOAD_DIR`, then `~/Downloads`, then `~/.mogeung/exports`. The two routes differ on overwriting on purpose: a path you chose replaces what is there because the dialog already asked, and a path nobody chose may destroy nothing, so it sanitises the name (a session title is **the agent's text**, and `../../etc/passwd` is a title) and suffixes rather than overwrites. Either way the pane shows the path it actually wrote, rather than flashing *done* **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B49 | **Two agents at once** — an Agent pane can be pinned to a session that is not the selected one, so two sessions are on screen and live together. Asked for 2026-08-06. The splitting half already exists (`R-B20`); what does not is a pane whose session is not `selected`, so two Agent panes today are two views of one session. Pinning is chosen over a pane-per-session because dockview persists the layout and a panel id naming a session restores a tab pointing at something that ended days ago — numbered slots plus a pin keeps the arrangement durable and lets the binding be dropped. **The pane's own header goes at the same time**: the dockview tab and `PaneHeader` both say `AGENT`, 58px of chrome per pane for one word twice, and the split makes each half pay all of it. Tests [A30](assumptions.md), which is deliberately *not* A14 — that one is two views of one session, this is one view of two. **Built 2026-08-06**, and the verb changed on contact with the code: `pinned` is already the queue's pin, so a pane is **held**, with an anchor on its tab. Three things the plan did not have. The pty was keyed by session alone, which is unique only while one pane can show a session — two panes on the same one would have opened a pty twice and closed it once. `resetLayout` had been dropped in the port from egui and had to come back on its old `Alt+0`, because a split you cannot undo is reachable again. And a hold outliving its pane makes the *next* split arrive pre-held on last week's session, so orphans are dropped at startup as well as on close. **Ran the same day and reported working**, which closes the two things the tests could not reach — two live attaches rendering together, and the focus ring being visible — and settles nothing about [A30](assumptions.md), which is a question about the second week rather than the first hour. **The Code pane took two follow-up asks the same day.** First its tab named the file rather than the pane, on the general rule the Agent tab established — a tab names the thing in it, not the kind of thing it is. Then the tab went entirely: the pane carried *three* rows of naming for a surface whose whole job is showing you a file, so the path row folded into the file strip and the group's header is hidden, 78px down to 28px. The cost was chosen with the trade in front of it and is stated because nothing in the code will say it later — dockview's tab **is** the drag handle, so the Code pane can no longer be dragged, split off or tabbed beside the Agent; `Alt+C` and `Alt+0` are what keep that a hidden tab rather than a trap. **And the selection follows the pane you click**: everything else in the window describes `selected`, so a held pane you are working in left the file tabs, the dock and Info pointed at a different session. Activating a held Agent pane now writes its session to the selection, one-way — the hold is not disturbed. See [feature 0030](../features/0030-two-agents-at-once.md) **Verdict 2026-08-07: kept** — the arrangement was kept through a day of real use rather than the week [A30](assumptions.md) asked for. | M | ✅ |
| R-B51 | **Give the keyboard back to the window** — `Alt+Escape` releases a pane that is holding it. Asked for 2026-08-07, and the report contained its own design: *"if some of the keymap is actually a key for the claude session, tmux should capture all the key input… but it would be great to add a keymap to change the focus back to the app"*. The capture **is** correct and stays — `focusOwns` gives every **bare** key to whatever has focus, which is what lets an agent receive `j`, and chords already fire from a focused pane because the keymap listens in capture. What was missing was a way out that is not the mouse. Caught inside xterm's own key handler rather than left to the keymap, because this is the one binding that has to work when the terminal is winning, and because `Alt+Escape` reaches a TUI as `ESC ESC` — which Claude Code reads as a cancel, so a release that also forwarded the keystroke would interrupt the agent on its way past. **The chord took three attempts, and the two failures were the same failure**: claimed by the desktop, so the keystroke never arrived and nothing happened — the hardest kind to diagnose from inside the app, because nothing is wrong except that nothing works. `Alt+Escape` is GNOME's *switch windows directly*; `Alt+Shift+Escape` is also claimed on Ubuntu, and three keys is a bad ask for something pressed mid-flow anyway. **`Shift+Escape`** is two keys under one hand, unbound in GNOME and on macOS, and not something a TUI asks for — **plain `Escape` still reaches Claude Code untouched**, which is the one that had to keep working, and a test says so. Chrome binds `Shift+Escape` to its task manager and that is not a hazard here: this window is WebKitGTK on Linux and WKWebView on macOS, neither of which has one. The chord is a single constant shared by the keymap and the terminal's own handler, because two literals would drift silently — the shortcuts window would keep advertising one while the terminal answered another. Rebindable like everything else (`R-B12`), so a desktop that disagrees costs a preference rather than a patch. Focus parks on a `[data-focus-host]` **ancestor** of the terminal: `focusOwns` asks `closest(".xterm")`, so an ancestor reads as outside. Throwing focus at the queue instead was rejected — it would silently rebind `j`/`k` to moving between sessions the moment you escaped a pane **Verdict 2026-08-07: kept**. | S | ✅ |
| R-J12 | **A fourteenth event type appeared** — `bridge-session`, in 47 of the 60 newest transcripts and in neither `HANDLED` nor `KNOWN_IGNORED`, so the adapter's canary would have called it unknown. Found 2026-08-07 by re-running the classification sweep **by hand** over a corpus that had grown from 235 to 315 transcripts since July. Classified `Ignored`: it carries a session id, the bridge's own id for it, and a sequence number — no content, no tokens, no tool use, the same category as `pr-link` and `frame-link`, which were themselves found this way. This is [A4](assumptions.md) working rather than failing: the format drifted inside two weeks and the drift was *findable*. **The repo's own invariant caught the incomplete fix**: adding a name to `KNOWN_IGNORED` without a matching shape in `tests/fixtures/corpus.jsonl` fails `the_corpus_covers_every_type_we_claim_to_know`, so the golden corpus grew a synthetic line with it. Worth repeating by hand after any CLI upgrade — the canary only speaks from a running daemon, and none had been up long enough to say it | S | ⏳ |
| R-J11 | **Export your labels, tags and pins** — [ADR-0023](../decisions/0023-judgements-stay-in-the-client.md) kept the judgements in the client and named the exposure it was accepting in the same breath: a label is text you wrote, living in a webview's `localStorage`, so clearing it loses every label, tag and pin on that machine silently and with nothing to restore from. **Built 2026-08-07**, the cheap half of that ADR's own Revisit-if. No new capability — the shell's save path already exists for `R-B43` — and no change of ownership. **The whole preferences object, not a hand-picked subset**: a backup that restores some settings and quietly not others is a worse promise than either whole answer. Carries `kind` and `version` so an importer can refuse a file that is not this; the importer is the other half and is **not** built, because reading a file needs a capability this window does not have and that is a decision rather than a chore. It also needed a notice channel that is not red — `pushNotice` beside `pushError` — because the only way to say a thing had *worked* was to say it in an alert, which is why two earlier features went without saying anything at all | S | ⏳ |
| R-J10 | **The bell looked switched on while unable to speak** — asked 2026-08-07: *"I tried to enable it, but I didn't see any behaviour changes"*. Nothing was broken, and that is the problem. Banners are split between two processes on purpose (`notify.ts`): the **daemon** announces when run with `--notify`, which `scripts/start.sh` passes by default, and the **window** announces only when it *hosts* the daemon and is unfocused. Run the usual way, the window is attached, so its half is permanently silent — the toggle genuinely cannot do anything. The tooltip said so; the icon did not, and a tooltip is no answer to *"I turned it on and nothing happened"* because you have to already suspect something to go looking. The bell now has **three** states rather than two, with `elsewhere` in amber — the colour this window already uses for *on, with a caveat* **Verdict 2026-08-07: kept**. | S | ✅ |
| R-J9 | **The Token burn chart ran backwards** — reported 2026-08-07: *"the other graphs with a timeline move forward on the X-axis, that one looks reversed"*. It did. `usage.rs` reversed its day list to newest-first so that truncating to `DAY_RETENTION` kept *this* week rather than last month, and never reversed back — while `insight.rs` emits `sessions_per_day` and `prompts_per_day` straight out of a `BTreeMap`, which is ascending. Two day-series on one dashboard, in opposite directions. **Fixed in the daemon rather than in the chart**, because the alternative leaves a wire that hands the same screen two conventions and will keep producing this bug whichever end patches it next. The regression test pins both properties at once — oldest first *and* it is the recent window that survived — since they are the two that pull against each other and the reason the second `reverse` was easy to leave out **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B53 | **A pane per file** — every open file becomes its own dockable pane, so the window runs one tab system instead of two. Asked 2026-08-06 (*"can each of the editor open as its own docker tab?"*) and the binding question was settled before any code: offered a pane that **stays put** against one that closes and reopens as the selection moves, *stays put* won, and everything follows from it — a file pane reads its file out of its own id, so a selection change has nothing to act on. **Built 2026-08-07.** Deletes the Code pane's own strip, its two-way split, and `active`/`focus`/`FileTab.group`, all of which existed only to model a split dockview already does. The cost of the old shape had been paid twice that week: `R-B49` merged the Agent pane's duplicate header, then the Code pane needed its own fix for the same complaint and paid for it by losing the ability to be dragged. Both were one pane pretending to be a tab strip. Two rows of chrome come back and the pane is draggable again, which is the trade. `closeTab(id, index)` became `closeFile(id, path, rev)` — an index is fine while one component owns the list and the click, and stale the moment a pane knows only its own file. See [feature 0032](../features/0032-a-pane-per-file.md) | L | ⏳ |
| R-B52 | **Move between panes with the arrows** — `Alt+Shift+←↑↓→` focuses the pane that way. Asked for 2026-08-07 with two agents up: *"move the focus to the left claude session on screen"*. `R-B49` made "the one on the left" a thing you can mean and left the mouse as the only way to say it. Spatial rather than a cycle, and **no wrapping**: a cursor that jumps from the rightmost pane back to the leftmost is right for a list and wrong for a floorplan — the point of moving spatially is that the layout is something you can picture, and a move that teleports across the screen breaks the picture it trades on. The scoring prefers a pane that **shares an edge** over a nearer one on the diagonal, which is the case a naive nearest-centre pick gets wrong and the one a test draws in ASCII. Landing focuses the **terminal**, not the tab: activating the group alone would leave the keyboard behind, so the next thing you typed would go to the pane you just left. And the terminal swallows the chord, for a reason the release chord did not have — an arrow reaches a TUI as a real escape sequence, so a move that also forwarded it would walk the agent's menu on the way out. **Fixed the same day on the first report**: the swallow was written as *handle it here too*, so a chord pressed over a terminal ran the move **twice** and stepped straight over the middle pane — reported as focus flipping between the outer two and never landing in between. The terminal now swallows on `e.defaultPrevented` alone, which is the window keymap's own signal (it listens on `window` in capture and prevents what it handles) rather than a second list of chords to keep in sync. That reading also survives a rebind, and closes a bug nobody had reported: **every** chord — `Alt+2`, `Alt+A` — was being handled by the window *and* forwarded to the agent as an `ESC`-prefixed sequence **Verdict 2026-08-07: kept**. | S | ✅ |
| R-B50 | **The wall** — every live session as a tile in a grid, held open on a chord and collapsing when you pick one. Designed alongside `R-B49` on 2026-08-06 and deferred deliberately, on two grounds. It **depends** on `R-B49`: a tile click has to open a session in a pane without moving the selection, which is what pinning introduces. And it is a **bet against the ranked queue** — a list reorders, so the row for a given session is somewhere new every time you look and spatial memory never forms, where a wall does not move and a change in the bottom-left corner registers before you have read a word. That is a genuinely different mechanism from ranking, and it is also [A1](assumptions.md) being quietly re-litigated: if eyeballing six tiles beats being told, the queue is not doing what the product claims. Worth a verdict on `R-B49` first. The buildable version is the cheap one — tiles are the last few lines of each transcript, re-rendered from what the daemon already streams, not live ptys: six `tmux attach`es is six ptys, an 80-column TUI in a 260px tile is illegible, and noticing needs three lines rather than eighty columns. It would also cover sessions with no `tmux_target`, which the Agent pane has to refuse. **Built 2026-08-07, and the cheap version is the one that shipped**: every tile comes from the snapshot the window already holds — `last_activity`, `recent_tools`, the attention reason — so opening the wall costs no fetches and needs no tmux. `Alt+W`, and a **toggle** rather than the hold-to-peek the design preferred: the centre is usually an xterm, which handles keys aggressively, and a peek whose keyup never arrives is a wall stuck open. The claim that justifies it over the queue is a sort order — **tiles are keyed by session id, never by score** — so a tile does not move when a session changes state, and that is the first thing a test pins, because it is exactly what a later "improvement" would revert. **Amended 2026-08-07 on the first look at it**: live sessions only. The queue holds dead sessions on purpose — an ended one can still be `needs_review` — but a tile earns its square by being something that might change while you watch it, and a grid where most squares are inert is a grid you stop scanning. Reading the queue's own `live` scope instead was the alternative and lost: the wall would then mean different things depending on a filter set somewhere else, which is exactly the "why is this empty" a glanceable surface must not have. **Amended again the same day, and the two changes are one thought**: live-only means fewer tiles, so each can afford to say more — repo, turns, the uncommitted diff, tokens out, and *how long a waiting session has been waiting*, which is the number that changes what you do next and which nothing else on the card carried. Only non-zero facts are drawn; a tile reading `0 turns · 0f +0 −0` looks broken rather than quiet. The type scale went up with it — the reason chip, the name and the tail all a step larger, and three columns only past `2xl` — because a surface read from across the room set in the same 10px as a dense list is a surface you lean into, which is the opposite of what it is for. And the colour tag became **the whole card** rather than an 8px dot, through the same `--tag-bg` route the queue rows use — `R-B42` had already learnt this about its own 4px bar, that a colour you have to look for has not done the one job a colour has. Still [A1](assumptions.md) being re-litigated, and deliberately so — with a removal condition agreed in advance in [feature 0031](../features/0031-the-wall.md): a week in which the wall is opened and closed without changing what you do next means it is a nicer way to see the same answer, and it comes out **Verdict 2026-08-07: kept** — opened and used on the day it shipped, which is short of the week its removal condition names. | L | ✅ |

## C. Notifications and reach — **shipped and verified end to end; `R-C2`'s verdict landed 2026-07-30**

Delivered by [feature 0002](../features/0002-sharpen-triage-and-review.md).
`R-C2` — long left open because a fourth binary outweighed the pillar and
`R-C1` banners might cover it — was built at the one-go ask as
`mogeung-tray` ([feature 0019](../features/0019-waiting-count-tray.md),
[A25](assumptions.md)) and filed with its own removal condition: unglanced in
the week meant delete it. Used, and kept. The doubt was worth writing down and
the answer was worth waiting for.

**`R-C3` got the opposite verdict, 2026-07-30, and it is the more useful
result.** The thin web client shipped and was never once opened — *"I won't
open it from my phone, no one is using this ui page."* It came up while
auditing the daemon's exposure, and the honest finding was that deleting it
buys **no** security: the port must stay open for the desktop window, which is
a WebSocket client on it, and the REST API on that same port serves every
transcript regardless. So it was removed on maintenance grounds instead — it
could not carry `R-I4`'s token, it hard-coded `ws://` so it could never sit
behind a TLS proxy, and it was a second UI to keep in step with every wire
change. `R-C2` shipped with a written removal condition and survived it;
`R-C3` never got one, and needed it. **The REST API stays**: a second client
remains buildable without touching the daemon, which was the architectural
claim `R-C3` was proving, and that claim is now proven and does not need a
standing demonstration.

| # | Item | Effort | |
|---|---|---|---|
| R-C1 | **macOS notification** when a session flips to `WAITING` | S | ✅ |
| R-C2 | **Menu-bar item** with the waiting count — glanceable without the window | M | ✅ |
| R-C3 | **Thin web client** — review and unblock from a phone. **Removed 2026-07-30, unused.** See the pillar note above | L | 🗑 |
| R-C4 | **Push** via ntfy/Pushover for away-from-desk | S | ✅ |
| R-C5 | **Ambient mode** — big-screen board for a second monitor | M | ✅ |

## D. Review depth — **shipped and verified end to end; the write half (R-D19–R-D23, R-D25) was built and its verdicts landed 2026-08-03**

`R-D1`–`R-D9` delivered by
[feature 0002](../features/0002-sharpen-triage-and-review.md); `R-D1` is
the observer-safe shape: mogeung writes the prompt, you paste it
([ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md)).

**Git-integration status (2026-07-28):** the whole pillar is built,
committed and installed — `R-D10`–`R-D12` (the pane, its depth, its
table stakes; features [0010](../features/0010-git-view.md)–
[0012](../features/0012-git-table-stakes.md)) and, later the same day at
an explicit "do R-D10 to R-D17" ask, `R-D13`–`R-D17` in one pass
([feature 0013](../features/0013-git-reach.md)): pickaxe search,
copy-as-patch, the attribution filter, hunk keys, diff context/
whitespace/side-by-side controls, the file index, merge-base branch
compare, remote branches, reflog, worktrees-with-sessions, the conflict
three-way view, and read-marks on commit diffs. Dogfooding has since
cleared the first tranche: `R-D10`–`R-D12` verified in use 2026-07-28,
then `R-D13` (forensics) and `R-D14` (diff ergonomics) the same day.
Feature 0013's acceptance boxes were held open until the week ruled on
them ([A19](assumptions.md)) — day one produced six fixes, which is the
week doing its job — because an unused section is a removal candidate,
not a foundation. Deliberately descoped, waiting for want: a "commits
on A not on B" list, read-badges for unviewed log rows, rebindable
hunk keys. `R-D15`–`R-D17` were verified later the same day, closing
the pillar as built. `R-D18` — asked for directly during dogfooding,
then made concrete with an IntelliJ screenshot — was built 2026-07-29
([feature 0014](../features/0014-intellij-commit-view.md)): the file
tree with details beneath it, one file's diff at a time, `n`/`p`
crossing file edges, branches-containing on the header. Verified in use
2026-07-30, closing the pillar as a **reading** pillar.

**The fence moved (2026-07-30).** Every row above is read-only, restated in
each spec because [A19](assumptions.md) predicted that *"just add staging"*
pressure would arrive here. It arrived, asked directly, and was answered by
[ADR-0012](../decisions/0012-write-locally-never-publish.md): mogeung may
write the working tree and the local repository, and may never talk to a
remote. **The line is the network, not the repo.** `R-D19`–`R-D22` are the
write verbs that follow, `R-D23` is the counterweight they need, and
`R-D24` — `push`, the half of the original question this does *not* answer —
stays unstarted behind a second ADR and behind [A24](assumptions.md) being
resolved rather than assumed. All of it is
[feature 0025](../features/0025-git-write-local.md), planned and not built.
[A26](assumptions.md) is the bet: an unused write verb is liability, not
decoration, and the removal condition is written down in advance.

| # | Item | Effort | |
|---|---|---|---|
| R-D1 | **Copy as prompt** — build a follow-up from flagged hunks + notes, ready to paste into the terminal. The observer-safe version of note→instruction | M | ✅ |
| R-D2 | **Whitespace-insensitive anchors** — stop reformatting from making hunks unread | S | ✅ |
| R-D3 | **Keyboard review flow** — `space` = mark read and advance | S | ✅ |
| R-D4 | **Syntax highlighting** in diffs (tree-sitter) | M | ✅ |
| R-D5 | **Intra-line word diff** | M | ✅ |
| R-D6 | **Side-by-side view** | M | ✅ |
| R-D7 | **Commit-aware diffing** — committed work currently vanishes as the base moves with HEAD ([A9](assumptions.md)) | M | ✅ |
| R-D8 | **Review debt across HEAD** — what fraction of the repo no human has read | L | ✅ |
| R-D9 | **Blast radius** — callers and tests affected by a changed symbol | L | ✅ |
| R-D10 | **Git view** — recent commits, uncommitted changes, per-commit diffs, blame in the Editor gutter. Read-only, permanently ([A18](assumptions.md)). See [feature 0010](../features/0010-git-view.md) | L | ✅ |
| R-D11 | **Git depth** — branches, refs, stashes, submodules, commit graph, re-blame, file-at-revision, range diffs. The reading half of a commercial client; still read-only, permanently ([A19](assumptions.md)). See [feature 0011](../features/0011-git-depth.md) | L | ✅ |
| R-D12 | **Git table stakes** — full commit details, log filtering by message/author/path, file history with rename following. See [feature 0012](../features/0012-git-table-stakes.md) | M | ✅ |
| R-D13 | **Git forensics** — pickaxe search (`-S`: when did this string appear/vanish), copy hunk/file/commit as patch text, an attribution-only log filter, hunk-navigation keys in git diffs. All small, all aimed at auditing agent work. See [feature 0013](../features/0013-git-reach.md) | M | ✅ |
| R-D14 | **Diff ergonomics** — expand hunk context on demand (daemon addition), a commit's files as a directory tree instead of a flat list, whitespace-ignore and side-by-side toggles in git diffs. See [feature 0013](../features/0013-git-reach.md) | M | ✅ |
| R-D15 | **Ref reach** — branch-to-branch compare (three-dot from the merge base; the "commits on A not on B" list is descoped per the pillar note above, and the code has ahead/behind counts rather than the list), remote branches in the list, a read-only reflog, and `git worktree list` linked to the sessions running in each. See [feature 0013](../features/0013-git-reach.md) | M | ✅ |
| R-D16 | **Conflict three-way view** — ours/base/theirs read-only for a conflicted file, beyond the markers-in-a-diff we have. See [feature 0013](../features/0013-git-reach.md) | M | ✅ |
| R-D17 | **Review-state on the log** — R-D8's read/unread marks surfaced per commit, so "which commits has no human read" is visible where commits live. A step toward `R-F2`. See [feature 0013](../features/0013-git-reach.md) | M | ✅ |
| R-D18 | **Commit diff, file-at-a-time** — IntelliJ-style: a commit's changed files as a selectable directory tree beside the diff, the pane showing only the chosen file's diff instead of every file in one scroll. R-D14's index jumps within the scroll; this replaces it with selection. Asked for directly 2026-07-28 | M | ✅ |
| R-D19 | **Working-tree writes** — stage, unstage, discard from Local changes, and the loopback-or-token guard every later write verb passes through. Carries the cost of the whole write half: a fail-loudly posture for git, and temp-repo test fixtures ([ADR-0012](../decisions/0012-write-locally-never-publish.md)). See [feature 0025](../features/0025-git-write-local.md). **Built 2026-07-31** — the first thing in this project that changes a repository. `run_git_write` is a sibling to `run_git` rather than a flag on it, because a read that fails should degrade and a write that fails must not; failures carry git's stderr verbatim. Writing the temp-repo fixtures found **two defects in the read path**: porcelain C-quotes unusual paths (`café.txt` arrived octal-escaped), and it collapses an untracked directory to one row, so a file inside a folder an agent had just created was classified as tracked by `discard`. Both were harmless while these strings were only displayed and fatal once they became pathspecs. **Dogfooded 2026-08-03.** | M | ✅ |
| R-D20 | **Commit from the pane** — message, amend, and a trailer naming the session whose diff it came from. The trailer is why this is worth building rather than shelling out: it is the concrete step toward `R-F2` prompt-blame. See [feature 0025](../features/0025-git-write-local.md). **Built 2026-07-31**, immediately after `R-D19`, because staging without committing is the least useful half of the pair — it moves you *closer* to the terminal you were trying not to visit. Commits only what is staged, never `-a`. Hooks run (skipping them would mean a repo that rejects bad commits everywhere except from this window) with `stdin` on `/dev/null`, so a hook that prompts fails loudly instead of blocking a daemon thread for ever. Writing it found that `git commit` puts "nothing to commit" on **stdout**, so the fail-loudly path now reads both streams. **Dogfooded 2026-08-03.** | M | ✅ |
| R-D21 | **Branch and stash writes** — create, switch, stash push/pop/drop. Small, except that a switch must invalidate the session's pinned diff base ([A9](assumptions.md)) rather than let the Changes tab compare against a branch nobody has checked out. **Built 2026-07-31.** The base is cleared for *every* session in that worktree, not just the one that asked, and cleared rather than recomputed so the scan loop stays the only thing that knows how to resolve one. The open question — what to do when an agent is running in the worktree — was put to the user first and answered: **warn, name the live sessions, proceed on confirm**, and only when something is actually live, because a confirmation that always appears is always dismissed. Git refuses a switch that would *lose* work; what it cannot see is an agent reading files that have silently become different content. **Dogfooded 2026-08-03.** | M | ✅ |
| R-D22 | **Conflict resolution** — take ours, take theirs, mark resolved, on top of `R-D16`'s existing three-way read. Small because the reading half is done. **Built 2026-07-31**, completing every write verb [feature 0025](../features/0025-git-write-local.md) proposes. Whole-file only: anything finer is editing, which stays out permanently — so "mark resolved" exists to make *resolving in a real editor and coming back* a first-class path rather than a gap. Every side ends in `git add`, because in git a conflict is resolved by staging the result, and a verb that wrote the file but left the index unmerged would show a conflict that looks fixed and is not. The content is deliberately not inspected. **Dogfooded 2026-08-03.** | S | ✅ |
| R-D23 | **Honest remote staleness** — ahead/behind never rendered as a bare number. They must carry the age of the last fetch or read as unknown. The counterweight to `R-D20`, since committing from the pane means visiting a terminal less. **Built 2026-08-01.** The qualifier goes on the row and not only the hover: a hover is where you look once you already doubt a number, and the point is to be doubted in time. `RefsInfo.fetch_epoch` had been on the wire since `R-D11` and the client simply ignored it. **Dogfooded 2026-08-03.** | S | ✅ |
| R-D25 | **Fetch** — update remote-tracking refs, `Ctrl+T`, with a report saying what moved. Admitted by [ADR-0014](../decisions/0014-fetch-is-not-publishing.md), which supersedes ADR-0012 and moves the line from *the network* to *publishing and merging*: `fetch` reads a remote and changes nothing there, `push` publishes, `pull` merges under a possibly-running agent. **Built 2026-08-01**, prompted by this repository sitting six commits behind its origin while the pane would have said 0 — a shipped feature that lies, made likelier by `R-D20`. Never on a timer, never interactive (`GIT_TERMINAL_PROMPT=0`), and always reports — including "nothing moved", since a silent success cannot be told from a silent no-op. **The first outbound network call this process makes**. **Dogfooded 2026-08-03.** | S | ✅ |
| R-D24 | **Publish** — `pull` and `push`. `fetch` was split out and admitted as `R-D25` by [ADR-0014](../decisions/0014-fetch-is-not-publishing.md); this row keeps its number and now means the other two. **Not started and not decided.** ADR-0012 draws the line at the network deliberately; this needs its own ADR and needs [A24](assumptions.md) resolved first. Recorded here because it is the half of the 2026-07-30 ask that went unanswered, and that should be visible where the rows live. **The ADR was written 2026-08-07 and the verbs were not built** — [ADR-0022](../decisions/0022-a-fast-forward-is-not-a-merge.md), `status: proposed`, because ADR-0014 refused `push` "permanently as far as this ADR reaches" and overturning a sentence like that is not a side effect of a backlog sweep. It splits the row rather than answering it as a pair: `pull --ff-only` is admissible on the argument that ADR-0014 objected to *merging* and a fast-forward does not merge, while `push` stays refused until [A24](assumptions.md) is resolved — the word doing the work in A24 is *read-only*, and a push admitted over the same socket changes a **shared** remote where a local commit changes a file you can reset | M | |

## E. Verification — **shipped and verified 2026-07-30 (feature [0016](../features/0016-verification.md))**

The observer pivot made this *easier*: the full transcript is on disk.
Claims bind to evidence and say how; the signal runner fires on an
explicit click only ([A21](assumptions.md), A7 still the open question).

| # | Item | Effort | |
|---|---|---|---|
| R-E1 | **"Did it actually run the tests?"** — the agent claims they pass; check whether a Bash call ran them | M | ✅ |
| R-E2 | **Signal runner** — run tests/typecheck per repo, attach results to the session | L | ✅ |
| R-E3 | **Claim ledger** — extract assertions from assistant text, bind each to evidence | L | ✅ |
| R-E4 | **Edit-without-verify** — flag sessions that changed code and never built or tested | S | ✅ |
| R-E5 | **Coverage delta** on changed lines only | L | ✅ |

## F. Cross-session intelligence — **shipped and verified 2026-07-30 (feature [0017](../features/0017-cross-session.md)); reopened 2026-08-03 by `R-F13`, which puts `R-F1`'s corpus in front of you rather than waiting to be asked, and closed again 2026-08-05 when `R-F12` and `R-F13` took the same verdict as the `B` backlog**

The material measured at build time: 235 transcripts (149 top-level + 86
nested subagent files) and 2,076 prompts in `~/.claude/history.jsonl`.
Lives in the Insight pane. Each view was filed to be judged separately, an
unused one a removal candidate ([A22](assumptions.md)); the verdict kept all
nine.

| # | Item | Effort | |
|---|---|---|---|
| R-F1 | **Global search** across transcripts and prompt history | M | ✅ |
| R-F2 | **Prompt-blame** — for a file or line, find the session and prompt that produced it | M | ✅ |
| R-F3 | **Daily digest** — what happened across all repos, from evidence not self-reports | M | ✅ |
| R-F4 | **Recurring-failure detection** — the same error across many sessions | M | ✅ |
| R-F5 | **Personal analytics** — sessions/day, token burn, repos, time of day | S | ✅ |
| R-F6 | **Prompt library** — most-reused prompts, mined from history | S | ✅ |
| R-F7 | **Decision extraction** — pull architectural decisions out of transcripts into ADRs | L | ✅ |
| R-F8 | **Subagent trees** — visualise `isSidechain` work | M | ✅ |
| R-F9 | **Blame → transcript** — from a session-attributed commit, open that session's transcript at the turns that produced it. The cheap precursor to `R-F2`, riding `R-D11`'s attribution | M | ✅ |

| R-F10 | **Fuzzy and parallel search across the Insight views** — substring, regex, `rg` and fuzzy run together, best answer wins. Asked for 2026-08-02, and the largest single ask in that list. **Two things need deciding before code.** *What "best" means*: four rankings over one corpus do not compose by themselves, and a search box that silently prefers one engine is worse than one that says which it used. *Whether mogeung may shell out to `rg` and `fzf` at all* — they may not be installed, and every other external dependency here (`git`, `tmux`) is either required up front or degrades to a named fallback. Speed is the stated motive, so a measurement comes first: `R-J2` was gated the same way and closed by finding the slow thing was already fast enough. **Both questions were answered on 2026-08-02 while building `R-B36`**, which now owns the shared `search` module: no external binaries, and one scale with the winning engine named. What remains here is applying it to the Insight views and is smaller than this row's `L` suggests. **Built 2026-08-07** as `rank()` and `winner()` over the existing engines, wired into Prompts, Failures, Decisions and the Memory/Skills lists — the last of which had been three `.includes()` calls and could not find `qcommit` from `q-commit`. One deliberate non-behaviour, pinned by a test: **a blank query returns everything in its original order** rather than ranking it, because the daemon has already ordered these lists by a count or a recency and scoring an unsearched list at zero would throw that away **Verdict 2026-08-07: kept**. | L | ✅ |
| R-F11 | **Charts in the Insight views** — the prompt and analytics tables want shape, not rows. Asked for 2026-08-02. The honest constraint is [ADR-0005](../decisions/0005-tokens-not-dollars.md): tokens and counts, never money, however tempting an axis label. **Half of this had already shipped without the row being ticked** — the Insight redesign gave Analytics four charts and said so in its own header comment, which is a reminder that a row is only a ledger if it is written to. **Finished 2026-08-07**: Prompts and Failures were still bare lists, and they are the two where the shape carries the finding — one prompt asked forty times against a tail asked twice is a fact about your week that a column of `×40` makes you reconstruct by reading. Horizontal bars, because the labels are sentences. ADR-0005 is asserted by a test rather than remembered **Verdict 2026-08-07: kept**. | M | ✅ |
| R-F12 | **Resizable Insight panes** — the content is fixed where every other pane in the window can be dragged. Asked for 2026-08-02; small, and the same complaint as `R-B37` in a different tab. **Built 2026-08-02, and not where the row expected**: the Insight tab has no split to drag, so what shipped is every other fixed side panel becoming draggable — the git sidebar, the commit tree, the Changes file list and the symbol outline. The complaint was "I cannot resize this", and those are the panels it actually lands on. If the Insight *content* still wants proportions, that is a different row and needs to say what is being split. **Verdict 2026-08-05: kept** | S | ✅ |
| R-F14 | **Memory, read at a glance** — every file an agent has saved under `~/.claude/projects/<project>/memory`, with its description, its project and its text. Asked for 2026-08-06. Sixty-three of them on this machine across eighteen projects, written by agents over weeks, and until now the only way to review one was to know it existed and open it. Insight is the right home because this is the other half of what it already does: every other view reads what agents *did*, and this reads what they were *told* — the half you edit. **Read only**, and more pointedly than elsewhere: these files change what an agent does next, so a panel that could edit one could change every session on the machine from a port that has no token on loopback. It shows the path instead, because what it owes you is where to go. **First report was that the panel showed nothing**, and the panel was right to show nothing and wrong about how: the window was a `tauri dev` process from four hours earlier, so its frontend had hot-reloaded and its *daemon* had not, and `fetch_kit` reached something that had never heard of it. [ADR-0009](../decisions/0009-the-window-may-host-a-daemon.md) makes a window-older-than-daemon pairing an ordinary thing to be sitting in front of, so the empty state now distinguishes three cases — waiting, answered-with-nothing, and nobody answered — and names the last one **Verdict 2026-08-07: kept**. | S | ✅ |
| R-F15 | **Skills, with their contents** — every `SKILL.md` on the machine, yours and each plugin's, with the body rendered. Asked for 2026-08-06 with the reason attached: *"I can easily see what skills are there in Claude Code, but I need to be able to see the skills content to update it."* Names are the one thing the CLI already gives you. **The scan found six and missed forty-four on the first run** — plugin skills sit seven levels below `~/.claude/plugins` and the walk capped at six, so the list looked complete and was not, which is the exact failure `A4` and the health panel exist for. Depth is 8 now and a test pins a skill at 7. Both roots are recorded in [claude-code-formats](../design/claude-code-formats.md), because they are undocumented like everything else here **Verdict 2026-08-07: kept**. | S | ✅ |
| R-F13 | **One search box over three corpora** — the Transcript's find (`R-B36`), the palette's file-content search and `R-F1`'s cross-session search are three good boxes you must choose between *before* you know where the answer is. This is one box, three labelled groups and a preview pane, living in the rail (`R-B40`) rather than as a tab, because results you act on have to stay open while you act on them. Asked for 2026-08-03. No new engine: all three searches exist and already echo their query back, so the work is federation, rendering each group as it lands, and one honest gap — an Insight hit's preview is the ~200-char clip the daemon returns and nothing more, and closing that needs a wire message this row deliberately does not build yet. Tests **A29**. The palette stays; IntelliJ ships both for a reason. **Built 2026-08-03.** The `SearchState` collision the spec worried about needed neither a second slot nor a shared one: both the palette and the panel are fed from the `ContentMatches` arm and each drops what is not theirs by the echoed query, so no shared mutable slot exists to fight over. Enter means two things by rule — run the search while the arrows are in the box, open the row once they have walked into the results, and Up off the top leaves the list rather than wrapping, or there is no keyboard way back to a query you are refining. The Insight preview gap is still open on purpose: a cross-session hit shows the ~200-char clip the daemon returns and says so. See [feature 0028](../features/0028-global-search.md). **Verdict 2026-08-05: kept** — the panel was reached for, which is what [A29](assumptions.md) asked; the Insight preview gap stays open and is still not this row's | M | ✅ |

## G. Rate limits and cost — **shipped and verified 2026-07-30 (feature [0015](../features/0015-rate-limits.md))**

`R-G1`'s premise was wrong on contact with disk: no structured limit
event exists in any local transcript — limits arrive as a synthetic
assistant message, which is what shipped keys on
([A20](assumptions.md), filed `AT RISK`). Warnings are estimates from
observed hits, labelled so.

| # | Item | Effort | |
|---|---|---|---|
| R-G1 | **Five-hour window status** — where you are in the window, since with overage disabled exhausting it hard-fails sessions. Built on the synthetic limit message, not the structured event this row first assumed ([A20](assumptions.md)) | S | ✅ |
| R-G2 | **Warn before exhaustion** | S | ✅ |
| R-G3 | **Token burn** per session/day/repo | S | ✅ |

## H. The doc-sprawl thesis — **shipped and verified 2026-07-30 (feature [0022](../features/0022-doc-inventory.md)); the inventory it produces is A10's test**

The original stated pain, finally measured rather than asserted: the Docs
view inventories a repo's markdown with evidence attached, and what it
finds across the watched repos is what decides
[A10](assumptions.md) — honestly, either way. The rows being ✅ says the
view works and gets used; **it does not say A10 is answered.** That verdict
belongs to [assumptions.md](assumptions.md), on the evidence this view
produces, and is a separate judgement from whether the tool is any good.

| # | Item | Effort | |
|---|---|---|---|
| R-H1 | **Doc inventory** — classify every markdown artifact, assign lifecycle | M | ✅ |
| R-H2 | **Staleness detection** — doc describes module X; X moved 40 commits ago | M | ✅ |
| R-H3 | **Doc GC** — propose archive/merge/delete in batch, with evidence | M | ✅ |
| R-H4 | **Derived progress** — plan items bound to real diffs | L | ✅ |
| R-H5 | **Agent-instruction hub** — `CLAUDE.md`/`AGENTS.md`/`GEMINI.md` from one source | M | ✅ |

Note: `scripts/check-docs.sh` and `scripts/gen-status.sh` are `R-H2` and `R-H4`
built for ourselves first, at toy scale. Worth reading before building the real
thing.

## I. Breadth

`R-I1`, `R-I3` (Linux) and `R-I4` were built 2026-07-29 (features
[0020](../features/0020-codex-adapter.md) and
[0021](../features/0021-linux-and-remote.md)). `R-I1` carries an honest
caveat: the local `~/.codex` is a fresh install, so the adapter is verified
against the real index schema and synthetic rollouts, not real Codex use
([A23](assumptions.md)). `R-I2` is **descoped, not started**: no `~/.gemini` exists on any machine we
run, so an adapter would be built blind against an undocumented format with
nothing to test it on — the A4 lesson says don't. Revisit when Gemini CLI
sees real local use. `R-I3`'s Windows half is descoped the same way (no
machine to verify on); the row covers Linux.

**Remote reach, opened 2026-07-30.** `R-I4` shipped the plumbing — a token, a
`--url` that never starts a local daemon, and five local-only actions that
refuse rather than acting on the wrong box — and a walkthrough now exists at
[guide/remote.md](../guide/remote.md). Using it surfaced a set of questions that
share one root: **the daemon publishes no identity.** It never says which host
it is, which `~/.claude` it watches, or how to reach it — so the window infers
"am I remote?" from the address string it dialled, which an ssh tunnel defeats.
`R-I5` fixes that and is the prerequisite for the five rows after it: a remote
terminal needs a target to ssh to, discovery needs a name to show, multi-daemon
needs an origin to key sessions by, and honest refusals need to know whose
filesystem is on the other end. Ordered deliberately: `R-I5` first because
everything else assumes it, then `R-I6` and `R-I7`, which are self-contained
and immediately useful; `R-I9` (mix mode) last of the build rows, because it is
the only one that changes the data model rather than adding to it — and it is
now refused rather than last, by
[ADR-0013](../decisions/0013-one-window-one-daemon.md), with `R-I11` in its
place. `R-I10`
paces the others — its first two steps are overdue already, and how far up its
ladder we climb depends on whether `R-I6` makes ssh the transport for
everything.

**Dogfooding began 2026-07-31**, and pillar `I` stopped being the pillar
nothing had judged: a second machine appeared — an Apple-silicon Mac watched
from a Linux window over `ssh -L`. `R-I4`, `R-I5` and `R-I6` are ✅ on that
evidence — including the refusals, which fired and named the machine through a
loopback tunnel, the exact case the old heuristic got wrong. `R-I7` followed the same day —
a running window moved between a Mac and a local daemon without a restart.
`R-I8` and the direct-bind half of `R-I10` were the last two untried, because
both need a daemon listening beyond loopback and the tunnel route never does;
both were **reported dogfooded 2026-08-03** in a blanket pass over every `⏳`
row. That is the provenance of their ✅, and it is weaker evidence than the
2026-07-31 entries above, which name the machine and the failure they caught.
If a LAN listener was never actually stood up, these two want re-opening —
`R-I10`'s tunnel half is separately proven and is not what is in doubt.

**It found two bugs in the first hour, both invisible locally.** The remote
tmux was launched through a non-login shell, so macOS never ran `path_helper`;
fixing that with `-l` was not enough either, because a login shell run with
`-c` is still non-interactive and Homebrew's `PATH` commonly lives in `.zshrc`.
Both produced the same line — `zsh:1: command not found: tmux` — on a machine
where tmux was installed. This is [item 0](#0-the-non-feature)'s argument
applied to a pillar rather than a feature: no amount of local testing reaches
a second machine's login shell.

**Progress, 2026-07-31**, on the `remote-reach` branch: `R-I10`'s
mandatory-token rung, then `R-I5`, then `R-I6`. That last one settled
something. ssh is now a *hard requirement* for terminals against a remote
daemon, so "adopt ssh as the transport" has stopped being hypothetical — it is
already half-adopted. The remaining `R-I10` question is therefore narrower than
it looked: not *TLS or ssh*, but whether the WebSocket should join the terminals
in the tunnel, or gain `wss://` for people who would rather run a reverse proxy.
The one-Cargo-flag experiment ran the same day and answered it: `wss://` now
works, so the reverse-proxy route is real. It also cost more than a flag —
see `R-I10`.

| # | Item | Effort | |
|---|---|---|---|
| R-I1 | **Codex adapter** — read its on-disk format. Tests whether the Session model generalises ([A23](assumptions.md)). **Dogfooded 2026-08-03.** | M | ✅ |
| R-I2 | **Gemini CLI adapter** — descoped, see above | M |  |
| R-I3 | **Linux** — terminal focus/launch and notifications; Windows descoped, see above. **Three defects found 2026-08-02**, each hidden behind the last: the alternatives symlink got xterm's flags; terminator then handed the launch to its own running instance over DBus and dropped the command (exit 0, so the liveness check called it success — the same shape as gnome-terminal succeeding); and finally the window opened and died in a second because `claude` lives in `~/.local/bin`, which reaches `PATH` through `.zshrc`, and nothing in a spawned chain is a login shell. That last one is `R-I6`'s remote-tmux lesson arriving locally, in code that had been sitting there the whole time. `+` also starts sessions in **yolo mode** now (`--dangerously-skip-permissions`), asked for directly, with the dialog saying so. **Dogfooded 2026-08-03.** | M | ✅ |
| R-I4 | **Remote daemon** — watch a dev box, run the UI locally ([A24](assumptions.md)). **Verified 2026-07-31** over the ssh-tunnel route: the queue, sessions and diffs of a Mac, in a window on another machine. The direct-bind and token paths are still unexercised, so [A24](assumptions.md) itself is untouched by this. Guide at [guide/remote.md](../guide/remote.md) | M | ✅ |
| R-I5 | **Daemon identity** — **verified in use 2026-07-31.** Both halves: `R-I6`'s terminals reached the Mac *through a `127.0.0.1` tunnel*, which only happens when identity rather than the address decides, and jump-to-terminal and open-in both refused and named the machine. That tunnel is precisely where the old address heuristic said "local" and acted on the wrong box. `DaemonIdentity` on the snapshot and on `/api/health`: a stable `machine_id` (`~/.mogeung/machine-id`), hostname, watched `~/.claude`, pid, version, optional ssh target. The window compares ids instead of guessing from the address string, so an `ssh -L` tunnel no longer reads as local. Repo roots were not included — nothing needed them, and the identity comparison did not | S | ✅ |
| R-I6 | **Remote terminal** — **verified in use 2026-07-31**, an egui window on Linux driving tmux on an Apple-silicon Mac over an ssh tunnel: both panes, a worktree path containing a space, two concurrent tabs, and detach-not-kill across a window restart. Both terminal panes drive tmux over ssh when the daemon is elsewhere (`Reach::Ssh`), using the `ssh_target` from `R-I5`'s identity; without one they refuse rather than guess a hostname ssh may not want. ADR-0010 and ADR-0011 hold unchanged, one layer further out. No bare-pty fallback remotely: it would trade the right machine for a shell on the wrong one | M | ✅ |
| R-I7 | **Connections in the window** — **verified in use 2026-07-31**, switching a running window between two daemons on different machines and back. Add, name, switch and forget daemons from the connection dot or `Alt+D`; saved in `~/.mogeung/connections.json`, written `0600` because it holds tokens. **Reopening the active one next launch was reverted 2026-07-31** on a dogfooding report: it was a sticky default that survived leaving the machine, and applying it ahead of the local-port check silently disabled ADR-0009, so no local daemon was hosted and the board was empty with no explanation. Every launch now starts on a synthetic `LOCAL` row that cannot be edited or forgotten; a remote is chosen per session. Switching drops everything the old daemon said and keeps what the window owns; terminal tabs detach rather than die. The `Net` teardown it needed turned out to be a real leak — the reconnect loop ignored a dropped receiver and would have spun for ever per switch. **Redrawn 2026-07-31** at a *"design a better view"* ask: a header saying which daemon you are on and what it watches, three labelled sections, and one card per daemon instead of a run of one-line rows carrying a name, a URL, three suffixes and three buttons at equal weight. That pass found a leak of a different kind — the window rendered `Net::url`, which is the *dialled* URL and carries `?token=`, in the connection tooltip and the old footer. `connections::redacted` now blanks it wherever it is shown, with a test that the secret cannot survive the round trip | M | ✅ |
| R-I8 | **LAN discovery** — **built 2026-07-31.** `--advertise` publishes `_mogeung._tcp` (off by default: the broadcast announces *"this machine is watching Claude Code sessions"* to the segment); the window's Scan button browses for 2s on a thread and lists what it finds. **Finding is never connecting** — a result fills the form and waits for a hand. A loopback bind refuses to advertise, which is also the interlock that makes everything discoverable token-gated by construction (`R-I10`). **First contact with a real network, 2026-07-31, found it invisible:** `--listen 0.0.0.0` was published verbatim, and `0.0.0.0` is not an address anyone can dial, so the browse side dropped its own record as unusable. Wildcard binds now publish live interface addresses. **The client half was worse:** a 2s one-shot browse fought mdns-sd's continuous model, and the address came out of a `HashSet` via `.find()`, so repeated scans returned IPv4, then IPv6, then nothing. Now a subscription held while the panel is open, accumulating rows and merging addresses — a wifi picker, not a search box. **Dogfooded 2026-08-03.** | M | ✅ |
| R-I9 | ~~**Multi-daemon mix mode** — one window, several daemons, one merged queue.~~ **Refused 2026-07-31 by [ADR-0013](../decisions/0013-one-window-one-daemon.md)**, and kept here with its reasoning rather than deleted. The ADR was the gate this row was always behind, and writing it settled the row: the queue is the cheapest thing to merge and the least valuable, because every pane behind a click is single-origin; the intelligence (collisions, all of `F`) is computed in the daemon and cannot be merged by a client at all; and a window that ranks across daemons is a second implementation of the ranking, against the rule that a UI has no local authority. The routing alone is 30 of 45 `ClientMsg` variants with no compiler backstop, since `SessionId` is a bare `String`. `R-I11` replaces it. If merging is ever wanted, the ADR says the shape to reconsider is **federation in the daemon**, not an aggregating window | L | |
| R-I10 | **Remote security** — the ladder past A24's bet. **Rung (b) landed 2026-07-31:** a non-loopback bind with no token now refuses to start (`server::admit`, before the database opens), with no `--insecure` override, and the window applies the same rule to the daemon it hosts. **Rung (c) landed the same day:** both clients are built with `rustls` and dial `wss://`, so TLS is available through a reverse proxy without the daemon owning certificates or renewals — Route C in [guide/remote.md](../guide/remote.md). It was *not* the one Cargo flag it looked like: the flag alone leaves rustls with no crypto provider selected, which does not fail the build — it panics on the first TLS connection — so both binaries name `ring` explicitly and a test asserts a real ClientHello reaches the wire. Remaining: whether the daemon should ever terminate TLS itself (the answer looks like no), and A24's own verdict. **Dogfooded 2026-08-03.** | M | ✅ |
| R-I11 | **Make one window per daemon honest** — the alternative [ADR-0013](../decisions/0013-one-window-one-daemon.md) chose, which currently has a bug in it: `prefs.json` is one fixed path written whole, so two windows on one machine fight over it and the last writer wins. Scope the client state two windows contend for, put the machine into terminal tab keys and the derived tmux session name (`shell_session_name` has no machine in it, and the same checkout path on two boxes is the normal case), and make the tray say *which* daemon a waiting count belongs to. A fraction of `R-I9`'s cost, fixes something broken now, and is the experiment that would justify reopening it. **Built 2026-07-31**, and the split is not the one this row first described: scoping the *whole* file per daemon would have meant choosing a theme once per machine, so `prefs.json` keeps what describes the window and `~/.mogeung/state/<machine_id>.json` keeps what is keyed by a session id or a path on the watched machine — including the terminal tab list, which swaps with the daemon. **The tmux session name was left alone deliberately**: this row asked for a machine in it, and that would have stranded every running shell for no gain, since each machine has its own tmux server and the names cannot collide across them. An old `prefs.json` migrates whole into the first machine adopted, which `R-I7` guarantees is LOCAL. **Verified in use 2026-07-31**, same day it was built | M | ✅ |
| R-I12 | **Question: should your judgements live on the daemon?** — raised 2026-08-02, on noticing that `R-I11` and [ADR-0015](../decisions/0015-markdown-is-the-truth.md) answer the same question two different ways one day apart. Both scope to the watched machine; they disagree about who holds the bytes, so two windows on one daemon show the same *notes* and different *pins*. **The decisive test is whether you would want a second window to agree.** Labels yes — a label is text you wrote, which makes it a very short note, and `R-B26` filing it as "client view-state like pins" now looks like the mistake. Pins, hidden and bookmarks yes: they are judgements about the work, not about the screen. Terminal tabs no — a tab is a view onto tmux, which is already the store of record on that machine, and two windows may legitimately differ. Theme, fonts, layout, geometry and zoom obviously no. **This implies `R-I11` was partly wrong**: the bug was two windows racing on one `prefs.json`, and partitioning the file per machine works, while daemon ownership would have removed the race rather than splitting it. Note that `prefs.rs`'s "hiding is not forgetting" argument does **not** defend client storage — it defends not conflating two verbs, and survives the move as a `hidden` column. Costs: a migration from `state/<machine>.json`, four wire families, and the daemon starting to hold preferences rather than only observations. **Not started on purpose.** It reverses a shipped decision that is a day old and has not been used long enough to know whether the divergence bites; the argument is filed so it does not have to be rediscovered the first time two windows disagree. **Answered *no* on 2026-08-07** by [ADR-0023](../decisions/0023-judgements-stay-in-the-client.md): they stay in the client, machine-scoped, where `R-I11` put them, and notes stay on the daemon. The asymmetry is accepted rather than resolved — the divergence is hypothetical under ADR-0013, and the cost is four wire families plus a daemon that starts holding preferences rather than only observations. Two corrections this row needed first, both in the ADR: the migration would **not** be from `state/<machine>.json` — that was the retired egui client's path, where the React client keeps one `localStorage` blob — so it is a different job from the one costed here. And the real exposure turns out not to be divergence at all: **a label is text you wrote, living somewhere with no export and no backup**, and clearing the webview's storage loses every label, tag and pin on that machine, silently | M | ✅ |

**On the struck row.** `R-I9` is the first item here refused rather than
shipped, removed or descoped, and those are four different things. A descope
(`R-I2`, Gemini) means *we cannot judge this yet* — no `~/.gemini` exists to
build against. A removal (`R-C3`, the web client) means *we shipped it and
nobody used it*. `R-I9` is neither: it was specified, gated on an ADR, argued
properly, and lost the argument. Nothing was built and nothing was wasted,
which is the whole return on having had the gate.

The strike is a claim about the proposal only. The reasoning beside it is not
struck, because it is the durable part — and the row is where someone who wants
a merged queue will look first, so it has to answer them rather than merely
say no. What it should tell them:
[ADR-0013](../decisions/0013-one-window-one-daemon.md) for the argument,
`R-I11` for what is being done instead, and *federation in the daemon* as the
shape to bring back if the case is reopened. The one thing that would reopen it
is a named failure from actually running several windows — not a preference for
one window.

## J. Polish

The last pillar to be numbered, and for a while the only one with unbuilt work
— which is why "what is left?" could not be answered from this file. Numbered
2026-07-29 at a *"move quick on J"* ask, after checking each line of the old
prose against the code rather than against its own claim: UI state turned out
half-built (`prefs.rs` persists nineteen fields; window geometry is not one),
and the rest not started.

J never waited on a `⏳` verdict — it is the one pillar that could proceed
during the dogfooding week rather than after it. Two couplings existed and
neither was a dependency: `R-J5` and `R-J6` scale with the number of panes, and
several panes are removal candidates if the week rules against them, so both
were sequenced last. See [feature 0023](../features/0023-polish.md).

`R-J1`–`R-J5` were built the same day they were numbered. `R-J2` was gated on
a measurement and the measurement said build it: 30ms per frame on the largest
commit in this repo, 0.28ms after, and flat in diff size rather than linear.
`R-J6` followed, and found a defect in the palette it was extending: white
lettering on an amber badge is 2.36:1, which had shipped, on the two badges
most often on screen. All six were exercised and signed off 2026-07-29; the
pillar is shipped end to end.

Two rows arrived after that line was written: `R-J7` on 2026-08-02 and `R-J8`
on 2026-08-05. `R-J8` is the perf pass, filed **after** it shipped rather than
before — the one thing this file is not supposed to allow, and left visible in
the row rather than backdated.

| # | Item | Effort | |
|---|---|---|---|
| R-J1 | **Window geometry** — remember size and position across launches, the one piece of UI state `prefs.rs` does not already hold. In our own store, not eframe's, so app state has one home | S | ✅ |
| R-J2 | **Virtualised diff rendering** — draw the visible lines, not every line of every hunk. Gated on a measurement first: if a real diff is already fast enough, this row closes unbuilt | M | ✅ |
| R-J3 | **Config file** — `~/.mogeung/config.toml` for both binaries, flags still winning. A malformed file degrades to defaults rather than refusing to start | S | ✅ |
| R-J4 | **`mogeung` CLI subcommands** — `queue`, `sessions`, `health`, `rescan`, `diff`, `search`, each with `--json`. Six of the forty endpoints, chosen; wrapping all of them would be a worse tool, not a more complete one | M | ✅ |
| R-J5 | **Empty states** — seventeen sites where "nothing here" cannot be told apart from a failed fetch | S | ✅ |
| R-J6 | **Light theme** — two hand-written palettes behind one lookup, a `dark`/`light`/`system` preference, and contrast tests over every pair that has to hold. Built last, deliberately: the only row that touches every pane | L | ✅ |

| R-J7 | **A loading state at start-up** — with progress, while the first scan builds the queue. Asked for 2026-08-02. Today an empty board during the first scan is indistinguishable from an empty board because nothing is running, which is the exact confusion `R-J5`'s empty states were built to remove and this is the one place they do not reach. The daemon already counts what it is reading (`health.rs`), so this is mostly carrying a number that exists. **Built 2026-08-02.** A spinner, "reading your sessions…", and the transcript count as it climbs — and it distinguishes *not connected yet* from *connected and still scanning*, which are different waits with the same blank screen. **Dogfooded 2026-08-03.** | S | ✅ |
| R-J8 | **Stop repeating work nothing consumes** — two symptoms, one review, reported 2026-08-05: short-lived processes flickering in `htop`, and a window that stuttered while agents worked. Both were work redone at the poll rate with no consumer. The daemon renders untracked files in-process instead of a `git diff --no-index` per file per tick (up to 200 forks a pass, gone, with a parity test pinning the anchors byte-for-byte against git's own output); queue and change broadcasts are gated on actual change while explicit requests are always answered; Codex rollouts are tailed through a `ScanCache` rather than re-read whole; a "not a repo" answer is remembered. The window's reducer does a binary insert instead of a linear scan and a re-sort per tick, and the diff, transcript and prefs subscriptions were narrowed so a font wheel-notch stops re-rendering three panes. The shell moved every Tauri command off the painting thread and coalesced pty output. **Recorded here late**, on 2026-08-05: the work shipped as `cefe599` with no row, which is the gap `R-B42` names — a ledger that is the answer to "what is left?" cannot have shipped work living only in git. **Verdict 2026-08-05: kept** | M | ✅ |

## L. A place to think

Asked for 2026-08-02, and unlike every pillar above it this one is not a view
of something else. Everything mogeung shows today is derived — sessions,
diffs, commits, transcripts, all of it produced by an agent or by git and
rendered here. A scratchpad is **the user's own writing**, and that is a
different kind of thing to own: nothing else can regenerate it, so losing it
is a real loss rather than a refresh.

It needs a design session before rows become work. The shape of the question:
what a task *is* (a checkbox, a note, a thing bound to a session?), where the
documents live, and whether they are per-repo, per-session or global.

**Two boundaries this pillar has to be explicit about**, because both are one
careless feature away from moving:

- **These are your notes, not the repo's files.** Editing a worktree file is
  what [pillar K](#k-explicitly-not) forbids; a document in `~/.mogeung` that
  never touches the worktree is a different thing. If that distinction is not
  written down it will erode a feature at a time.
- **`R-L4` is a question, not a plan.** Relaxing the editor handoff was raised
  on 2026-08-02 and explicitly left open — *"let's keep it for next phase,
  nothing decided yet"*. It is filed so it cannot be lost, not so it can be
  assumed.

| # | Item | Effort | |
|---|---|---|---|
| R-L1 | **Design session: tasks and scratchpad** — what a task is, where documents live, what they attach to, and what happens to them when a session ends or a repo moves. **Held 2026-08-02**, and it did end in both: [ADR-0015](../decisions/0015-markdown-is-the-truth.md) and [feature 0026](../features/0026-notes-and-tasks.md). Four answers — documents are markdown and tasks are checkboxes in them; the daemon owns them; a binding is a tag rather than a location; and the first slice is a note on a transcript turn, not a pane. The structured half is a derived cache that must be droppable, which is the whole defence against the two-representations risk the session accepted | M | ✅ |
| R-L2 | **Notes and documents** — markdown documents you write, owned by the daemon and mirrored to `~/.mogeung/notes/*.md` one way, edited in the window. The scratchpad half, and never the worktree's files. **Gated on `R-B35`**, and **the gate came off on 2026-08-05 against the count** — two real notes in the week where the condition asked for a handful. Lifted on an explicit decision rather than by the numbers, with the argument recorded in [feature 0026](../features/0026-notes-and-tasks.md) and [A27](assumptions.md) moved to `AT RISK` rather than quietly to `SUPPORTED`: one of the two notes was a `todo-list`, which is `R-L3` asking for itself, and what had been tested was a *remark on a turn* rather than the scratchpad that was asked for. **Built 2026-08-05.** Most of the storage half already existed from `R-B35` — the `notes` table, the wire family, the mirror, and a rail tool with list, filter, editor and delete — so what this row actually added is the gesture that was missing: **copy a turn, or a whole conversation, into a note of its own**. A copy takes the words rather than pointing at them, which is what makes it outlive the session; a bookmark cannot, and `session_id` on a note is a tag with no foreign key behind it so nothing cascades when a session is forgotten. **Amended 2026-08-06 on the first report of it**: the copied text was quoted with `> `, which is safe and useless — a table renders as a table *inside a quotation*, a fenced block as a quoted fence, and in a plain editor every line simply wears a `>`. What was copied has to arrive as what it was, so it is now verbatim markdown under a horizontal rule, with the provenance lines above it. The safety that bought is gone and the cost is stated instead: the copied text and the note share one structure now, so a turn opening with its own `#` reads as a heading of the note. The export (`R-B43`) was quoting for the same wrong reason and was fixed with it. **And the note itself renders**, added the same day on the next report: the rail showed source where most of a note's content now arrives as markdown — a table, a fenced block — so a `markdown` checkbox renders it, on by default, with the source one click away because you cannot type into a rendering. An empty note ignores the preference and shows the editor: rendering nothing renders as nothing, and a blank panel where you just pressed **+** reads as a broken button. **And a marked turn stopped being a document**, the same day: the remark on a bookmark is a *name for the mark* — why you stopped there — and it was appearing in the scratchpad as well as in Bookmarks. One table stays right (ADR-0015, `seq` as a tag); it was the two views that disagreed, one filtering to anchored notes and the other to nothing. The rule is now `seq`: with one it is a mark, without one it is something you wrote. A copied conversation is capped at the last 200 turns and **says in the note what it left behind** — a note that reads as the whole conversation while holding its tail is worse than no note. **And `Ctrl+S` saves it**, asked for 2026-08-06 with the scope attached — *only* inside Notes. That scope is the whole design: `Ctrl+S` means save in every editor anyone arrives from, so a window-wide binding would claim it for a panel that is usually shut, and take it from the Code pane, which is the surface most likely to want it next. A capture-phase listener gated on focus containment, the shape `Ctrl+F` in this panel already used, and deliberately not an `ACTIONS` entry — everything in that list fires window-wide by design. See [feature 0026](../features/0026-notes-and-tasks.md) **Verdict 2026-08-07: kept** — the row is kept; [A27](assumptions.md) is **not** settled by it — that assumption asks about a month of notes and stays `AT RISK`. | L | ✅ |
| R-L3 | **Tasks** — the checklist half. `R-L1` decided: a task is a `- [ ]` line in a document and nothing else, with a derived table carrying the transitions so "what did I close today" is answerable — a question a checkbox has no memory of. If the two ever disagree the document wins and the table is rebuilt from it. See [feature 0026](../features/0026-notes-and-tasks.md) | M | |
| R-L4 | **Question: may the editor edit?** — pillar K says handoff to IntelliJ/VS Code, permanently, and `R-B24` has been a viewer with no write path since it shipped. Raised 2026-08-02: once `R-L2` exists, allowing *simple* edits to worktree files may be worth reconsidering. **Nothing is decided.** It needs its own ADR arguing against a line that has held from the beginning, and the honest first question is whether the want survives having a scratchpad — a good deal of "let me just fix this typo" may turn out to have been "let me write this down somewhere" | S | |

## M. The second client — **in flight; built 2026-08-04, awaiting a verdict**

Numbered 2026-08-04, the day after the dogfooding week returned its answer.

The week is what unlocked this. [Item 0](#0-the-non-feature) had gated
everything since v0.1: use the thing for a week, because A1 and A6 are the
product's own premises and neither had ever been tested. The verdict was
70–80% of all interaction with agents. That settles the question the whole
roadmap was waiting on, and it changes what the *next* investment should be —
because everything left to build is UI-shaped, and the Rust ecosystem has no
CodeMirror and no Monaco to build it on.

Two decisions came first and both are load-bearing.
[ADR-0019](../decisions/0019-a-viewer-not-an-editor.md) closes `R-L4` by
refusing the editor — which is what makes this port need **no daemon work at
all**. [ADR-0018](../decisions/0018-a-second-client-in-typescript.md) replaces
the window and keeps the daemon, running both clients side by side against one
daemon until the new one is better. That is not v0.1 again: v0.1 died for
building on an unvalidated premise, and this replaces a presentation layer
against a validated daemon and a frozen protocol.

**Status: done, and the side-by-side is over.** The port landed on 2026-08-04
and the egui client was deleted on 2026-08-05
([ADR-0020](../decisions/0020-the-egui-client-is-retired.md)) — R-M1–M4 all ✅.
There is one window again, and it is `desktop/`. The daemon was never changed to
allow any of it, which is the claim the whole approach rested on.

See [feature 0029](../features/0029-desktop-client.md).

| # | Item | Effort | |
|---|---|---|---|
| R-M1 | **A second client, in TypeScript** — React, Monaco, dockview, packaged with Tauri. Speaks the existing protocol; the daemon is not changed and does not know. `wire-protocol.md` is the contract it is written against, and pressure to change the protocol mid-port is a signal that logic is leaking into the client. **Built 2026-08-04** in one pass: queue, transcript, code, changes, git, insight, the rail's four tools, palette and keymap. Nothing under `crates/` was touched, which is the claim the whole approach rests on. **In sole use since 2026-08-05**, when the client it was built beside was deleted | L | ✅ |
| R-M2 | **The Insight redesign** — charts where the data is a shape, which is what `R-F11` asked for and what a table of numbers cannot be. Sessions and prompts per day, an hour histogram, token burn. The digest and the decision candidates stay text on purpose: both are *evidence*, and a dashboard invites exactly the "looks authoritative" reading they were built to avoid. **Built 2026-08-04** as part of `R-M1`, and in use since | M | ✅ |
| R-M3 | **The pty panes in Tauri** — the attached terminal (`R-B18`) and the shell panel (`R-B33`), with the pty held by the Tauri process so [ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md) and [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md) stay true. The hardest pane and deliberately last: `R-I6`'s tmux-over-ssh target building is real logic living in the client today, and the vendored `egui-term` goes away with it. **Built 2026-08-04**, second pass. xterm.js against four Tauri commands (`pty_open`/`write`/`resize`/`close`), with the argv-building ported faithfully from `term.rs` — the exact-match `=` on attach, `new-session -A` for shells, and the ssh wrapper whose login-shell-plus-PATH-fallback was paid for once already by a Mac that said `command not found: tmux` with tmux installed. Twelve tests cover the argv. **Compiles and links** on 2026-08-04 after two fixes found by finally building it: the bundled icon has to be RGBA and was written as RGB, and an unused import. The earlier claim that webkit's headers were missing was wrong — they were installed, and a bad `pkg-config` reading had been taken at face value. Exercised since: the Agent pane has been driven daily since 2026-08-04, and the selection bug fixed on 2026-08-05 was found by using it rather than by reading it | M | ✅ |
| R-M4 | **Parity, then retirement** — the health, keymap and connections windows, the blame gutter, the symbol outline and the markdown preview; then a week of use; then the egui client comes out. `R-I12` should be settled before that, not after: two clients means two sets of view state, and they disagree. **Done 2026-08-05**, [ADR-0020](../decisions/0020-the-egui-client-is-retired.md): the parity list was finished on 2026-08-04/05 and `crates/mogeung-ui` and `crates/egui-term` are deleted — five crates to three, and the whole egui/wgpu tree out of `Cargo.lock`. The week of use was **waived knowingly**: two days of exclusive use with nothing to fall back on had already answered what it was there to ask. `R-I12` was *not* settled first, and that ordering was chosen rather than forgotten — its sharpest argument was two clients disagreeing, which retirement removes; what remains is the smaller question of whether judgements should travel with the sessions rather than sit in one window's storage. The removal turned up one real regression and fixed it: the global hotkey (`R-B10`) had the Tauri plugin loaded and *registered nothing*, so it would have gone out with the old window while the roadmap still called it shipped | M | ✅ |

## K. Explicitly not

- **An editor.** Handoff to IntelliJ/VS Code, permanently. `R-B24` reads files
  and nothing more — a viewer with no write path is the line this bullet
  draws, not an exception to it. *(A revisit was raised 2026-08-02 and filed
  as `R-L4`. Nothing is decided, and until an ADR says otherwise this bullet
  is what stands.)*
- **Anything that re-acquires the conversation loop.** See
  [ADR-0003](../decisions/0003-observe-do-not-spawn.md).
- **Cloud or multiplayer.**
- **Half-measures on risk scoring.** Either keep honest keyword heuristics or
  replace them wholesale with real analysis. Something in between would look
  authoritative while still being wrong.

---

## Candidate bundles

Not decisions — starting points for the priority conversation.

**Cheap trust and triage** — `R-A1 + R-A4 + R-G1 + R-B1 + R-C1`, roughly a day.
Makes the tool trustworthy and fast to triage without betting on anything
unproven. The natural companion to [item 0](#0-the-non-feature).

**Most distinctive** — `R-B3` collision warning. Two live agents editing the
same file is a real failure mode of parallel work, is invisible today, and only
the observer model can see it. Nothing else here is unique to what we built.

**Honouring the original brief** — Pillar `H`. Doc sprawl was the opening
complaint and two versions have not touched it.

**The sleeper** — `R-E1`. A few hours, and it is the smallest real slice of the
trust layer that made [concept.md](concept.md) interesting.
