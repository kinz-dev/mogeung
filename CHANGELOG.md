# Changelog

## Unreleased — sharpen triage, reach and review (2026-07-25)

Roadmap pillars `B`, `C` (bar `R-C2`) and `D`; see
[docs/features/0002-sharpen-triage-and-review.md](docs/features/0002-sharpen-triage-and-review.md).

**Added — queue.** `APPROVE`, a tier above `WAITING` for sessions blocked on a
permission prompt rather than waiting for a new instruction, told apart by an
unanswered tool call. Keyboard triage (`j/k`, `enter`, `r`, `s`, `g`, `/`).
Filter, group-by-repo, follow-the-top. Snooze, which beats even `FAILED`.
Collision warning when two live sessions edit one file. Loop detection for an
agent repeating itself. Jump to the session's Terminal tab.

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
