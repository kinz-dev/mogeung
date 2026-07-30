---
title: Roadmap
status: active
updated: 2026-07-30
---

# Roadmap

The ranked backlog.

Effort: **S** = hours · **M** = about a day · **L** = multi-day.

Status: **✅** = shipped and proven · **⏳** = built, installed, awaiting
the dogfooding verdict ([A19](assumptions.md)) · blank = not started.
The distinction exists because a blank box on built work read as "not
done" and got R-D10 asked for twice.

Pillars `A`–`H` and `J` are shipped and verified. `E`–`I` (bar the two descopes
in `I`) were **built 2026-07-29 in one pass** at an explicit *"finish the R-\*
items in one-go"* ask — a deliberate override of the item-0 gate, recorded per
spec (features 0015–0022) and in the ledger (A20–A25). That gamble was settled
on 2026-07-30: `E`–`H` were used and passed, so those rows are ✅.

**`I` is the exception, and stays ⏳ on purpose.** Its rows reach machines and
tools this desk does not have — Codex with no real sessions on disk, a remote
daemon, a second OS — so nothing here has judged them and a ✅ would be a
claim about a machine nobody ran.

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

## C. Notifications and reach — **shipped and verified end to end; `R-C2`'s verdict landed 2026-07-30**

Delivered by [feature 0002](../features/0002-sharpen-triage-and-review.md).
`R-C2` — long left open because a fourth binary outweighed the pillar and
`R-C1` banners might cover it — was built at the one-go ask as
`mogeung-tray` ([feature 0019](../features/0019-waiting-count-tray.md),
[A25](assumptions.md)) and filed with its own removal condition: unglanced in
the week meant delete it. Used, and kept. The doubt was worth writing down and
the answer was worth waiting for.

| # | Item | Effort | |
|---|---|---|---|
| R-C1 | **macOS notification** when a session flips to `WAITING` | S | ✅ |
| R-C2 | **Menu-bar item** with the waiting count — glanceable without the window | M | ✅ |
| R-C3 | **Thin web client** — review and unblock from a phone. The daemon already supports it | L | ✅ |
| R-C4 | **Push** via ntfy/Pushover for away-from-desk | S | ✅ |
| R-C5 | **Ambient mode** — big-screen board for a second monitor | M | ✅ |

## D. Review depth — **R-D1–R-D18 shipped and verified; R-D18's verdict landed 2026-07-30**

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
2026-07-30, closing the pillar.

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

## G. Rate limits and cost — **shipped and verified 2026-07-30 (feature [0015](../features/0015-rate-limits.md))**

`R-G1`'s premise was wrong on contact with disk: no `rate_limit_event`
exists in any local transcript — limits arrive as a synthetic assistant
message, which is what shipped keys on ([A20](assumptions.md), filed
`AT RISK`). Warnings are estimates from observed hits, labelled so.

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

| # | Item | Effort | |
|---|---|---|---|
| R-I1 | **Codex adapter** — read its on-disk format. Tests whether the Session model generalises ([A23](assumptions.md)) | M | ⏳ |
| R-I2 | **Gemini CLI adapter** — descoped, see above | M |  |
| R-I3 | **Linux** — terminal focus/launch and notifications; Windows descoped, see above | M | ⏳ |
| R-I4 | **Remote daemon** — watch a dev box, run the UI locally ([A24](assumptions.md)) | M | ⏳ |

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

## K. Explicitly not

- **An editor.** Handoff to IntelliJ/VS Code, permanently. `R-B24` reads files
  and nothing more — a viewer with no write path is the line this bullet
  draws, not an exception to it.
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
