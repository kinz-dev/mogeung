---
title: Roadmap
status: active
updated: 2026-07-31
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

| # | Item | Effort | |
|---|---|---|---|
| R-A1 | **Format canary** — classify every line; alert on any unclassified type | S | ✅ |
| R-A2 | **CLI version watch** — record versions seen; warn on Claude Code updates, which is when formats move | S | ✅ |
| R-A3 | **Golden-corpus test** — snapshot-test the parser against anonymised real transcripts | M | ✅ |
| R-A4 | **Health panel** — sessions found, lines parsed/skipped, last scan, and what mogeung *cannot* see | S | ✅ |
| R-A5 | **Huge-transcript handling** — cap and tail rather than reading whole files | S | ✅ |

## B. Sharpen the queue — **shipped and verified end to end; the last verdicts (R-B27–29, R-B31–34) landed 2026-07-30**

Delivered by [feature 0002](../features/0002-sharpen-triage-and-review.md).

The terminal is the moving part here. `R-B31` shipped it as a pane on
2026-07-29 and the next day's use said the pane was wrong — it followed the
selection and could not exist before a session did — so `R-B32`–`R-B34` are one
run: a panel with a tab per shell, a font you choose, and a name per tab. See
[feature 0024](../features/0024-in-app-terminal.md) and
[ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md). The four were
judged together, as they were built, and passed on 2026-07-30 — which is also
the pane's obituary: one day between shipping a shape and being told it was the
wrong one is [item 0](#0-the-non-feature) doing exactly what it is for.

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
| R-B10 | **Global hotkey** to raise the window — the return half of `R-B2` | S | ✅ |
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
| R-B26 | **Session labels** — name a session yourself; colour badge on the card, `label:` in the filter. Client view-state like pins ([A17](assumptions.md)). See [feature 0009](../features/0009-session-labels.md) | S | ✅ |
| R-B27 | **Editor git ergonomics** — a diff gutter vs HEAD with next/prev-change keys, inline blame on the current line, compare-with-revision side by side, and a gutter mark on lines *this session* changed (the mogeung-only one). Still a viewer, never an editor | L | ✅ |
| R-B28 | **Editor navigation** — symbol outline and go-to-symbol (tree-sitter is already in the tree), go-to-line, sticky scroll, folding, highlight-other-occurrences | L | ✅ |
| R-B29 | **Editor content comforts** — markdown preview, image preview, per-tab word wrap, copy path / `path:line`, file facts in the header, bookmarks with a jump list | M | ✅ |
| R-B30 | **Per-pane zoom** — Ctrl+wheel over a pane scales that pane alone (Editor, Changes, Git, Transcript, Agent, Terminal), remembered per pane; the global Ctrl+=/− stays whole-window. Asked for directly, built 2026-07-28 | S | ✅ |
| R-B31 | **In-app terminal** — a shell of your own, `Alt+F12` or ``Ctrl+` ``. Under tmux, so a build or a `claude` started in it outlives the window and stays reachable from a real terminal; a bare pty when tmux is absent, labelled as such. The pane the Agent tab was renamed to make room for — and no longer a pane at all: `R-B33` moved it. See [ADR-0011](../decisions/0011-own-a-shell-never-an-agent.md) and [feature 0024](../features/0024-in-app-terminal.md) | M | ✅ |
| R-B32 | **Configurable terminal font** — pick the family the terminal panes draw in, from the monospaced fonts installed on this machine. The bundled Hack carries no Powerline or Nerd Font glyphs, so an oh-my-zsh prompt is a row of boxes until you can say otherwise. Asked for directly, built 2026-07-30. See [feature 0024](../features/0024-in-app-terminal.md) | S | ✅ |
| R-B33 | **Terminal as a workspace panel** — the shell leaves the pane tree for a panel across the bottom, on demand, with a tab per shell and no tie to any session: a terminal is where you *start* an agent, so it must outlast the selection and exist before there is one. Asked for directly, built 2026-07-30. See [feature 0024](../features/0024-in-app-terminal.md) | M | ✅ |
| R-B34 | **Name a terminal tab** — double-click a tab, or right-click it, to call it what it is doing; blank puts the folder name back. The label only: the tmux session stays keyed by worktree and ordinal, so a rename cannot strand a shell. Asked for directly, built 2026-07-30. See [feature 0024](../features/0024-in-app-terminal.md) | S | ✅ |

| R-B35 | **Bookmarks and notes in the Transcript** — toggle a mark on a turn, a view listing them, and free text against one. Asked for directly 2026-08-02. `prefs` already carries a bookmark shape from `R-B29` (`(session, path, line)`), and this is the same idea keyed by turn rather than by line — worth reusing rather than inventing a second one. A note is the first thing in this product that is *the user's own writing* rather than a view of something the agent did, which is what makes it the small end of `R-L1` and worth building first | M | |
| R-B36 | **Search inside the Transcript panel** — find within the conversation you are reading, rather than across every session (`R-F1` already does that). Asked for 2026-08-02 with a specific shape: run substring, regex, `rg` and fuzzy in **parallel** and show whichever answers best. That shape is the interesting part and the risky part — "best" needs defining before code, and it shares the external-tool question with `R-F10` | M | |
| R-B37 | **Resize the Editor's tree and content independently** — the file tree and the file body currently move together. Asked for 2026-08-02. Small, and the kind of thing that is only noticed by someone actually reading in it | S | |
| R-B38 | **Search a rendered Markdown preview** — find in the preview, not only in the source. `R-B29` shipped the preview; searching it means searching rendered text and mapping a hit back to a source line, which is the whole of the work | S | |

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

## D. Review depth — **R-D1–R-D18 shipped and verified; the write half (R-D19–R-D23) is planned and not started**

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
| R-D19 | **Working-tree writes** — stage, unstage, discard from Local changes, and the loopback-or-token guard every later write verb passes through. Carries the cost of the whole write half: a fail-loudly posture for git, and temp-repo test fixtures ([ADR-0012](../decisions/0012-write-locally-never-publish.md)). See [feature 0025](../features/0025-git-write-local.md). **Built 2026-07-31** — the first thing in this project that changes a repository. `run_git_write` is a sibling to `run_git` rather than a flag on it, because a read that fails should degrade and a write that fails must not; failures carry git's stderr verbatim. Writing the temp-repo fixtures found **two defects in the read path**: porcelain C-quotes unusual paths (`café.txt` arrived octal-escaped), and it collapses an untracked directory to one row, so a file inside a folder an agent had just created was classified as tracked by `discard`. Both were harmless while these strings were only displayed and fatal once they became pathspecs | M | ⏳ |
| R-D20 | **Commit from the pane** — message, amend, and a trailer naming the session whose diff it came from. The trailer is why this is worth building rather than shelling out: it is the concrete step toward `R-F2` prompt-blame. See [feature 0025](../features/0025-git-write-local.md). **Built 2026-07-31**, immediately after `R-D19`, because staging without committing is the least useful half of the pair — it moves you *closer* to the terminal you were trying not to visit. Commits only what is staged, never `-a`. Hooks run (skipping them would mean a repo that rejects bad commits everywhere except from this window) with `stdin` on `/dev/null`, so a hook that prompts fails loudly instead of blocking a daemon thread for ever. Writing it found that `git commit` puts "nothing to commit" on **stdout**, so the fail-loudly path now reads both streams | M | ⏳ |
| R-D21 | **Branch and stash writes** — create, switch, stash push/pop/drop. Small, except that a switch must invalidate the session's pinned diff base ([A9](assumptions.md)) rather than let the Changes tab compare against a branch nobody has checked out. **Built 2026-07-31.** The base is cleared for *every* session in that worktree, not just the one that asked, and cleared rather than recomputed so the scan loop stays the only thing that knows how to resolve one. The open question — what to do when an agent is running in the worktree — was put to the user first and answered: **warn, name the live sessions, proceed on confirm**, and only when something is actually live, because a confirmation that always appears is always dismissed. Git refuses a switch that would *lose* work; what it cannot see is an agent reading files that have silently become different content | M | ⏳ |
| R-D22 | **Conflict resolution** — take ours, take theirs, mark resolved, on top of `R-D16`'s existing three-way read. Small because the reading half is done. **Built 2026-07-31**, completing every write verb [feature 0025](../features/0025-git-write-local.md) proposes. Whole-file only: anything finer is editing, which stays out permanently — so "mark resolved" exists to make *resolving in a real editor and coming back* a first-class path rather than a gap. Every side ends in `git add`, because in git a conflict is resolved by staging the result, and a verb that wrote the file but left the index unmerged would show a conflict that looks fixed and is not. The content is deliberately not inspected | S | ⏳ |
| R-D23 | **Honest remote staleness** — ahead/behind never rendered as a bare number. They must carry the age of the last fetch or read as unknown. The counterweight to `R-D20`, since committing from the pane means visiting a terminal less. **Built 2026-08-01.** The qualifier goes on the row and not only the hover: a hover is where you look once you already doubt a number, and the point is to be doubted in time. `RefsInfo.fetch_epoch` had been on the wire since `R-D11` and the client simply ignored it | S | ⏳ |
| R-D25 | **Fetch** — update remote-tracking refs, `Ctrl+T`, with a report saying what moved. Admitted by [ADR-0014](../decisions/0014-fetch-is-not-publishing.md), which supersedes ADR-0012 and moves the line from *the network* to *publishing and merging*: `fetch` reads a remote and changes nothing there, `push` publishes, `pull` merges under a possibly-running agent. **Built 2026-08-01**, prompted by this repository sitting six commits behind its origin while the pane would have said 0 — a shipped feature that lies, made likelier by `R-D20`. Never on a timer, never interactive (`GIT_TERMINAL_PROMPT=0`), and always reports — including "nothing moved", since a silent success cannot be told from a silent no-op. **The first outbound network call this process makes** | S | ⏳ |
| R-D24 | **Publish** — `pull` and `push`. `fetch` was split out and admitted as `R-D25` by [ADR-0014](../decisions/0014-fetch-is-not-publishing.md); this row keeps its number and now means the other two. **Not started and not decided.** ADR-0012 draws the line at the network deliberately; this needs its own ADR and needs [A24](assumptions.md) resolved first. Recorded here because it is the half of the 2026-07-30 ask that went unanswered, and that should be visible where the rows live | M | |

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

## F. Cross-session intelligence — **shipped and verified 2026-07-30 (feature [0017](../features/0017-cross-session.md))**

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

| R-F10 | **Fuzzy and parallel search across the Insight views** — substring, regex, `rg` and fuzzy run together, best answer wins. Asked for 2026-08-02, and the largest single ask in that list. **Two things need deciding before code.** *What "best" means*: four rankings over one corpus do not compose by themselves, and a search box that silently prefers one engine is worse than one that says which it used. *Whether mogeung may shell out to `rg` and `fzf` at all* — they may not be installed, and every other external dependency here (`git`, `tmux`) is either required up front or degrades to a named fallback. Speed is the stated motive, so a measurement comes first: `R-J2` was gated the same way and closed by finding the slow thing was already fast enough | L | |
| R-F11 | **Charts in the Insight views** — the prompt and analytics tables want shape, not rows. Asked for 2026-08-02. The honest constraint is [ADR-0005](../decisions/0005-tokens-not-dollars.md): tokens and counts, never money, however tempting an axis label | M | |
| R-F12 | **Resizable Insight panes** — the content is fixed where every other pane in the window can be dragged. Asked for 2026-08-02; small, and the same complaint as `R-B37` in a different tab | S | |

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
`R-I8` and the direct-bind half of `R-I10` remain untried, because both need a
daemon listening beyond loopback and the tunnel route never does.

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
| R-I1 | **Codex adapter** — read its on-disk format. Tests whether the Session model generalises ([A23](assumptions.md)) | M | ⏳ |
| R-I2 | **Gemini CLI adapter** — descoped, see above | M |  |
| R-I3 | **Linux** — terminal focus/launch and notifications; Windows descoped, see above. **Three defects found 2026-08-02**, each hidden behind the last: the alternatives symlink got xterm's flags; terminator then handed the launch to its own running instance over DBus and dropped the command (exit 0, so the liveness check called it success — the same shape as gnome-terminal succeeding); and finally the window opened and died in a second because `claude` lives in `~/.local/bin`, which reaches `PATH` through `.zshrc`, and nothing in a spawned chain is a login shell. That last one is `R-I6`'s remote-tmux lesson arriving locally, in code that had been sitting there the whole time. `+` also starts sessions in **yolo mode** now (`--dangerously-skip-permissions`), asked for directly, with the dialog saying so. | M | ⏳ |
| R-I4 | **Remote daemon** — watch a dev box, run the UI locally ([A24](assumptions.md)). **Verified 2026-07-31** over the ssh-tunnel route: the queue, sessions and diffs of a Mac, in a window on another machine. The direct-bind and token paths are still unexercised, so [A24](assumptions.md) itself is untouched by this. Guide at [guide/remote.md](../guide/remote.md) | M | ✅ |
| R-I5 | **Daemon identity** — **verified in use 2026-07-31.** Both halves: `R-I6`'s terminals reached the Mac *through a `127.0.0.1` tunnel*, which only happens when identity rather than the address decides, and jump-to-terminal and open-in both refused and named the machine. That tunnel is precisely where the old address heuristic said "local" and acted on the wrong box. `DaemonIdentity` on the snapshot and on `/api/health`: a stable `machine_id` (`~/.mogeung/machine-id`), hostname, watched `~/.claude`, pid, version, optional ssh target. The window compares ids instead of guessing from the address string, so an `ssh -L` tunnel no longer reads as local. Repo roots were not included — nothing needed them, and the identity comparison did not | S | ✅ |
| R-I6 | **Remote terminal** — **verified in use 2026-07-31**, an egui window on Linux driving tmux on an Apple-silicon Mac over an ssh tunnel: both panes, a worktree path containing a space, two concurrent tabs, and detach-not-kill across a window restart. Both terminal panes drive tmux over ssh when the daemon is elsewhere (`Reach::Ssh`), using the `ssh_target` from `R-I5`'s identity; without one they refuse rather than guess a hostname ssh may not want. ADR-0010 and ADR-0011 hold unchanged, one layer further out. No bare-pty fallback remotely: it would trade the right machine for a shell on the wrong one | M | ✅ |
| R-I7 | **Connections in the window** — **verified in use 2026-07-31**, switching a running window between two daemons on different machines and back. Add, name, switch and forget daemons from the connection dot or `Alt+D`; saved in `~/.mogeung/connections.json`, written `0600` because it holds tokens. **Reopening the active one next launch was reverted 2026-07-31** on a dogfooding report: it was a sticky default that survived leaving the machine, and applying it ahead of the local-port check silently disabled ADR-0009, so no local daemon was hosted and the board was empty with no explanation. Every launch now starts on a synthetic `LOCAL` row that cannot be edited or forgotten; a remote is chosen per session. Switching drops everything the old daemon said and keeps what the window owns; terminal tabs detach rather than die. The `Net` teardown it needed turned out to be a real leak — the reconnect loop ignored a dropped receiver and would have spun for ever per switch. **Redrawn 2026-07-31** at a *"design a better view"* ask: a header saying which daemon you are on and what it watches, three labelled sections, and one card per daemon instead of a run of one-line rows carrying a name, a URL, three suffixes and three buttons at equal weight. That pass found a leak of a different kind — the window rendered `Net::url`, which is the *dialled* URL and carries `?token=`, in the connection tooltip and the old footer. `connections::redacted` now blanks it wherever it is shown, with a test that the secret cannot survive the round trip | M | ✅ |
| R-I8 | **LAN discovery** — **built 2026-07-31.** `--advertise` publishes `_mogeung._tcp` (off by default: the broadcast announces *"this machine is watching Claude Code sessions"* to the segment); the window's Scan button browses for 2s on a thread and lists what it finds. **Finding is never connecting** — a result fills the form and waits for a hand. A loopback bind refuses to advertise, which is also the interlock that makes everything discoverable token-gated by construction (`R-I10`). **First contact with a real network, 2026-07-31, found it invisible:** `--listen 0.0.0.0` was published verbatim, and `0.0.0.0` is not an address anyone can dial, so the browse side dropped its own record as unusable. Wildcard binds now publish live interface addresses. **The client half was worse:** a 2s one-shot browse fought mdns-sd's continuous model, and the address came out of a `HashSet` via `.find()`, so repeated scans returned IPv4, then IPv6, then nothing. Now a subscription held while the panel is open, accumulating rows and merging addresses — a wifi picker, not a search box | M | ⏳ |
| R-I9 | ~~**Multi-daemon mix mode** — one window, several daemons, one merged queue.~~ **Refused 2026-07-31 by [ADR-0013](../decisions/0013-one-window-one-daemon.md)**, and kept here with its reasoning rather than deleted. The ADR was the gate this row was always behind, and writing it settled the row: the queue is the cheapest thing to merge and the least valuable, because every pane behind a click is single-origin; the intelligence (collisions, all of `F`) is computed in the daemon and cannot be merged by a client at all; and a window that ranks across daemons is a second implementation of the ranking, against the rule that a UI has no local authority. The routing alone is 30 of 45 `ClientMsg` variants with no compiler backstop, since `SessionId` is a bare `String`. `R-I11` replaces it. If merging is ever wanted, the ADR says the shape to reconsider is **federation in the daemon**, not an aggregating window | L | |
| R-I10 | **Remote security** — the ladder past A24's bet. **Rung (b) landed 2026-07-31:** a non-loopback bind with no token now refuses to start (`server::admit`, before the database opens), with no `--insecure` override, and the window applies the same rule to the daemon it hosts. **Rung (c) landed the same day:** both clients are built with `rustls` and dial `wss://`, so TLS is available through a reverse proxy without the daemon owning certificates or renewals — Route C in [guide/remote.md](../guide/remote.md). It was *not* the one Cargo flag it looked like: the flag alone leaves rustls with no crypto provider selected, which does not fail the build — it panics on the first TLS connection — so both binaries name `ring` explicitly and a test asserts a real ClientHello reaches the wire. Remaining: whether the daemon should ever terminate TLS itself (the answer looks like no), and A24's own verdict | M | ⏳ |
| R-I11 | **Make one window per daemon honest** — the alternative [ADR-0013](../decisions/0013-one-window-one-daemon.md) chose, which currently has a bug in it: `prefs.json` is one fixed path written whole, so two windows on one machine fight over it and the last writer wins. Scope the client state two windows contend for, put the machine into terminal tab keys and the derived tmux session name (`shell_session_name` has no machine in it, and the same checkout path on two boxes is the normal case), and make the tray say *which* daemon a waiting count belongs to. A fraction of `R-I9`'s cost, fixes something broken now, and is the experiment that would justify reopening it. **Built 2026-07-31**, and the split is not the one this row first described: scoping the *whole* file per daemon would have meant choosing a theme once per machine, so `prefs.json` keeps what describes the window and `~/.mogeung/state/<machine_id>.json` keeps what is keyed by a session id or a path on the watched machine — including the terminal tab list, which swaps with the daemon. **The tmux session name was left alone deliberately**: this row asked for a machine in it, and that would have stranded every running shell for no gain, since each machine has its own tmux server and the names cannot collide across them. An old `prefs.json` migrates whole into the first machine adopted, which `R-I7` guarantees is LOCAL. **Verified in use 2026-07-31**, same day it was built | M | ✅ |

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

| # | Item | Effort | |
|---|---|---|---|
| R-J1 | **Window geometry** — remember size and position across launches, the one piece of UI state `prefs.rs` does not already hold. In our own store, not eframe's, so app state has one home | S | ✅ |
| R-J2 | **Virtualised diff rendering** — draw the visible lines, not every line of every hunk. Gated on a measurement first: if a real diff is already fast enough, this row closes unbuilt | M | ✅ |
| R-J3 | **Config file** — `~/.mogeung/config.toml` for both binaries, flags still winning. A malformed file degrades to defaults rather than refusing to start | S | ✅ |
| R-J4 | **`mogeung` CLI subcommands** — `queue`, `sessions`, `health`, `rescan`, `diff`, `search`, each with `--json`. Six of the forty endpoints, chosen; wrapping all of them would be a worse tool, not a more complete one | M | ✅ |
| R-J5 | **Empty states** — seventeen sites where "nothing here" cannot be told apart from a failed fetch | S | ✅ |
| R-J6 | **Light theme** — two hand-written palettes behind one lookup, a `dark`/`light`/`system` preference, and contrast tests over every pair that has to hold. Built last, deliberately: the only row that touches every pane | L | ✅ |

| R-J7 | **A loading state at start-up** — with progress, while the first scan builds the queue. Asked for 2026-08-02. Today an empty board during the first scan is indistinguishable from an empty board because nothing is running, which is the exact confusion `R-J5`'s empty states were built to remove and this is the one place they do not reach. The daemon already counts what it is reading (`health.rs`), so this is mostly carrying a number that exists | S | |

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
| R-L1 | **Design session: tasks and scratchpad** — what a task is, where documents live, what they attach to, and what happens to them when a session ends or a repo moves. Ends in a feature spec and probably an ADR; no code until it does. The one row here that must come first | M | |
| R-L2 | **Notes and documents** — markdown documents you write, stored under `~/.mogeung`, edited in the window. The scratchpad half of `R-L1`, and never the worktree's files | L | |
| R-L3 | **Tasks** — the checklist half. Whether it is a real task model or a markdown convention is exactly what `R-L1` decides; building it as a model first is how it becomes a project manager nobody asked for | M | |
| R-L4 | **Question: may the editor edit?** — pillar K says handoff to IntelliJ/VS Code, permanently, and `R-B24` has been a viewer with no write path since it shipped. Raised 2026-08-02: once `R-L2` exists, allowing *simple* edits to worktree files may be worth reconsidering. **Nothing is decided.** It needs its own ADR arguing against a line that has held from the beginning, and the honest first question is whether the want survives having a scratchpad — a good deal of "let me just fix this typo" may turn out to have been "let me write this down somewhere" | S | |

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
