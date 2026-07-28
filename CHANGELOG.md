# Changelog

## Unreleased — git table stakes (2026-07-28)

Roadmap `R-D12`; see
[docs/features/0012-git-table-stakes.md](docs/features/0012-git-table-stakes.md).

**Added — commits tell their whole story.** Selecting a commit now shows
its full message — subject and the body agents actually write — plus
author, committer when different, absolute dates, clickable parent shas,
ref decorations, and a files/±lines diffstat, above the diff it already
showed.

**Fixed — the handoff buttons speak Linux.** "IntelliJ", "Terminal",
"VS Code" and the file manager all launched through macOS's `open -a`,
which does not exist elsewhere — the app began life on a Mac and the
buttons quietly did nothing on Ubuntu. Each target now walks a native
launcher list (`xdg-open`, `x-terminal-emulator` and friends, the
JetBrains Toolbox scripts directory, snap names), the button says
"Files" instead of "Finder", a failed launch names everything it tried,
and spawned launchers are reaped instead of left as zombies.

**Fixed — the log's right-click menu answers the whole row.** The menu
(copy sha/subject, open on remote, mark + diff a commit range) only
listened on the commit's text, so right-clicking the graph, the ref
chips, or the space after the subject did nothing — which read as there
being no menu at all. The entire row is now the target, left-click
included, and the hover card says the menu exists.

**Fixed — labels and pins survive `/clear`.** Claude Code's `/clear`
keeps the process but mints a fresh session id, so a hand-applied label
died with the old id and the "same" session came back nameless. The live
registry is per-pid, which makes the succession a fact: when a dead
session and a live one share a pid *and* working directory, the label
and pin follow the work. A label never overwrites one you gave the
successor by hand. (Took two attempts: the daemon used to wipe a
session's pid the moment it died — the same scan that discovers the
successor — destroying the evidence the migration matches on. Dead
sessions now keep their last pid; everything needing a live one already
gates on `alive`.)

**Fixed — narrow panes scroll sideways instead of folding rows.** The
earlier worktree-tree fix added a horizontal scroll area but egui still
hands content the visible width, so rows kept wrapping at the pane edge
and the scrollbar never engaged. Text is now laid out at its natural
width (`Extend`) inside the tree, local changes, and the branch/log
lists — resize small and they scroll, not fold.

**Fixed — clicking a queue card works while "follow" is on.** Follow mode
re-selected the top of the queue every frame, so a hand-picked session
held for one frame and snapped back — a click that read as doing nothing.
Picking a session by click or `j`/`k` now switches follow off, visibly in
its checkbox, the way tailing a log stops when you scroll up.

**Added — the log is searchable.** One filter box over the log: plain
text matches commit messages, `author:` and `path:` pull their own
filters, all literal and case-insensitive — never regex. Filtering by a
path follows renames, which makes the filtered log double as **file
history**; the Editor grows a `history` button that lands there
pre-filled. Paging keeps working under any filter, and a page answered
under an old filter is dropped, not shown.

## Unreleased — git depth (2026-07-28)

Roadmap `R-D11`; see
[docs/features/0011-git-depth.md](docs/features/0011-git-depth.md).

**Added — the Git pane grows the reading half of a commercial client.** A
header names the current branch (or detached HEAD), its upstream with
ahead/behind counts, the remote, and how stale the last fetch is — display
only, mogeung never fetches. Collapsible lists for branches (click one to
scope the log to it, nothing is checked out), tags (annotated ones
dereferenced to their commits), stashes with their diffs, and submodules
with their state. The log gains a lane graph of branch/merge topology, ref
decorations, and a green dot on commits that look like the selected
session's work — files plus timing, marked as the heuristic it is. A
right-click menu offers copy sha/subject, open-on-remote for
GitHub/GitLab/Bitbucket-shaped URLs, and mark-two-commits to diff an
arbitrary range. Renames and copies are detected in every diff.

**Added — blame that investigates.** Hovering the annotate gutter shows the
line's commit — sha, author, age, subject; right-clicking offers show
commit, copy sha, open the file at that commit, and *re-blame before this
commit*: the file opens as of the parent revision, blamed at that era, so a
line's history walks backwards one tab at a time. Revision tabs are
read-only, marked `@sha`, and never persisted.

**Added — conflict and ignore awareness.** Conflicted files sort first in
local changes wearing a red `⚠ conflict`; conflict markers get their own
band in any diff. Gitignored subtrees are dimmed in the explorer tree and
kept out of local changes.

**Unchanged, on purpose — read-only, permanently.** Every new wire pair
reads; argument hygiene widens with the surface (ref names, stash indices
and `sha^` are shape-checked before git ever sees them). Staging, commit,
checkout, stash-pop and fetch stay in the terminal, per
[feature 0011](docs/features/0011-git-depth.md).

**Fixed — switching sessions in the Terminal pane no longer eats a CPU
core each time.** The vendored terminal widget's event-forwarder thread
looped `if let Ok = recv()`, which busy-spins forever once its channel
closes — and dropping the old terminal on a session switch is exactly
that. Two hours of dogfooding left 38 threads pinning every core. A
closed channel now ends the thread; the change is marked `LOCAL CHANGE
(mogeung)` in `crates/egui-term` and recorded in its `VENDORED_FROM`.

## Unreleased — sharpen triage, reach and review (2026-07-25)

Roadmap pillars `B`, `C` (bar `R-C2`) and `D`; see
[docs/features/0002-sharpen-triage-and-review.md](docs/features/0002-sharpen-triage-and-review.md).

**Changed — `mogeung` detaches from the terminal.** Launching the window now
gives the prompt straight back and survives the terminal closing, nohup-style.
Console output is discarded unless `--log PATH` names a file to append it to;
`--foreground` keeps the old attached behaviour, and is what `start.sh` and
mprocs use so their supervision keeps working.

**Added — an app icon on Linux.** The window embeds an icon (a session queue
with one row flagged amber), which X11 honours directly. Wayland does not — it
resolves icons from a desktop entry matching the window's app id — so
`scripts/install.sh` now also installs `mogeung.desktop` and the icon into the
hicolor theme on Linux, and `--uninstall` removes them.

**Added — queue.** `APPROVE`, a tier above `WAITING` for sessions blocked on a
permission prompt rather than waiting for a new instruction, told apart by an
unanswered tool call. Keyboard triage (`j/k`, `enter`, `r`, `s`, `g`, `/`).
Filter, group-by-repo, follow-the-top. Snooze, which beats even `FAILED`.
Collision warning when two live sessions edit one file. Loop detection for an
agent repeating itself. Jump to the session's Terminal tab.

**Fixed.** Page Up and Page Down did nothing in the transcript and the diff.
egui's `ScrollArea` has no keyboard handling at all — it responds to the wheel
and to dragging, and nothing else — so the keys are now handled explicitly, with
`Home`/`End` for the ends. Rebindable like everything else.

**Added — tab shortcuts.** `c`/`t`/`i`/`d` switch to Changes, Transcript, Info
and Debt; `Ctrl+Tab` cycles. Rebindable like everything else, and each tab shows
its key on hover.

**Changed — transcript.** Agent replies render as Markdown via `egui_commonmark`
— headings, lists, tables, inline and fenced code (`R-B16`). Tool *output* stays
monospace on purpose: Markdown mangles logs and stack traces. Toggles for
markdown and thinking blocks, a copy button per message, and only the last 150
events are drawn with a "show earlier" control.

**Changed — one executable.** `mogeung` now starts a daemon if none is watching
and attaches to one if there is. A daemon it started stops with the window; one
that was already running is left alone. The bind is the test, so two windows
opened together cannot both start one, and the hosted daemon is a thread rather
than a child process — no pid file, no cleanup to skip, no orphan on the port.
`mogeungd` remains the way to get a daemon that outlives every window
([ADR-0009](docs/decisions/0009-the-window-may-host-a-daemon.md)).

**Added — finding a session.** Hide and pin sessions, both persisted across
restarts (`R-B13`). A scope selector — needs-you / live / all — replacing the
"quiet" checkbox (`R-B14`). Field filters `repo:` `branch:` `file:`, where
`file:` matches what a session actually touched (`R-B15`). Clicking a repo name
filters to it, and the panel says how many sessions the current filter or scope
is excluding. View preferences now persist in `~/.mogeung/prefs.json`.

**Changed — top bar.** The actions are icons with tooltips carrying their name
and shortcut.

**Fixed.** Four glyphs already in the UI were rendering as empty boxes because
they are outside egui's bundled font coverage — including the file list's
read-marker. Icons now come from one list that a test checks against the real
font files.

**Added — keyboard.** Pane-aware navigation: `Alt+1`/`Alt+2`/`Alt+3` focus the
queue, file list and diff, and `j`/`k` act on whichever has focus (`R-B11`).
Bindings are now data and fully editable — rebind, reset, import, export, stored
at `~/.mogeung/keymap.json` (`R-B12`). Moving through the file list previews
each file, which can be turned off.

**Added — reach.** A system-wide `Ctrl+Cmd+M` that raises the mogeung window,
the return half of jump-to-terminal (`R-B10`, `--hotkey` / `--no-hotkey`).

**Added — reach.** Desktop notifications and push-to-URL, both fired only on the
transition into needing you. A self-contained web client at `/`. Ambient mode
for a second monitor.

**Added — review.** Whitespace-insensitive anchors, so reformatting no longer
resurrects read hunks. Approximate syntax highlighting, intra-line word diff,
side-by-side view. Flag hunks and build a follow-up prompt — which mogeung
copies to your clipboard and never sends
([ADR-0008](docs/decisions/0008-build-the-prompt-never-send-it.md)). Review debt
per repo. Blast radius via `git grep`.

**Fixed.** The diff base is now the last commit *before* the session started, so
work an agent committed before mogeung noticed it no longer vanishes.

**Fixed.** Jump-to-terminal assumed Terminal.app and failed for iTerm2 users.
The owning application is now found by walking the process ancestry
(`claude → zsh → login → iTermServer → iTerm2` — four levels, so checking the
parent is not enough), and iTerm2's tty lives on the session inside a tab rather
than on the tab.

**Not built.** `R-C2` (menu-bar item) needs a separate binary to outlive the
window; left open deliberately.

63 tests → 102.

## Unreleased — trust the tool (2026-07-25)

Instrumentation so that "the board looks quiet" becomes a checkable claim rather
than a guess. Roadmap `R-A1`–`R-A5`; see
[docs/features/0001-trust-the-tool.md](docs/features/0001-trust-the-tool.md).

**Added** — every transcript line is classified (read / ignored / yielded
nothing / unknown type / unreadable), and anything unclassified raises a named
alert; a health panel and `ServerMsg::Health` pushed after every scan; an
enriched, curl-able `GET /api/health`; a Claude Code version watch; a golden
corpus of anonymised line shapes.

**Fixed** — three event types (`queue-operation`, `pr-link`, `frame-link`) were
being discarded silently and are now classified. The oversized-transcript guard
was unreachable, so multi-megabyte transcripts were parsed in full inside the
scan loop; files over 4 MiB are now followed from a line boundary near their end
and the skipped span is reported.

**Changed** — `adapter::parse_line` returns `LineOutcome` instead of
`Option<Parsed>`, so "deliberately skipped" and "never seen" can no longer be
confused.

36 tests → 63, all free.

## v0.2 — the observer pivot (2026-07-25)

mogeung stopped spawning agents and started watching the ones you run yourself.

v0.1's verdict in use was *"a handicapped Claude Code with a single session"*.
The attention queue is worth zero at N=1, and to feed it v0.1 had removed the
interactive loop — so every session was worse than just running `claude`. See
[ADR-0003](docs/decisions/0003-observe-do-not-spawn.md).

**Added** — session watcher over `~/.claude/sessions` and `~/.claude/projects`;
first-party `WAITING` detection from the live registry; per-session diff
attribution; terminal launch; a synthetic-home test suite.

**Changed** — `Run` → `Session`; attention reasons rebuilt around observed
state; transcript parser reads on-disk `.jsonl` instead of `stream-json`; tokens
replace dollars ([ADR-0005](docs/decisions/0005-tokens-not-dollars.md)); the
watch root is injected rather than read from the environment
([ADR-0006](docs/decisions/0006-inject-the-watch-root.md)).

**Removed** — the run supervisor, permission modes, model selection,
follow-ups, cancel, the New Run dialog.

**Kept** — the git diff engine, risk scoring, hunk anchoring, review
checkpointing, the daemon/client split.

36 tests, all free.

## v0.1 — initial build (2026-07-25)

Spawning model: intent in, `claude -p` in a worktree, diff out. Attention
router, review checkpointing, risk-ordered diffs, worktree-per-run, structured
transcripts. 21 tests.

Superseded the same day. Its build log is preserved at
[docs/archive/2026-07-25-v0.1-v0.2-build-log.md](docs/archive/2026-07-25-v0.1-v0.2-build-log.md).
