---
title: Roadmap
status: active
updated: 2026-07-28
---

# Roadmap

The ranked backlog.

Effort: **S** = hours · **M** = about a day · **L** = multi-day.

Status: **✅** = shipped and proven · **⏳** = built, installed, awaiting
the dogfooding verdict ([A19](assumptions.md)) · blank = not started.
The distinction exists because a blank box on built work read as "not
done" and got R-D10 asked for twice.

Pillars `A`, `B`, `C` (bar one item) and `D` are shipped. What remains — `E`
verification, `F` cross-session intelligence, `G` rate limits, `H` doc sprawl —
is still speculation until [item 0](#0-the-non-feature) is done.

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

## B. Sharpen the queue — **shipped through R-B26 (and R-B30); R-B27–29 is the backlog**

Delivered by [feature 0002](../features/0002-sharpen-triage-and-review.md).

| # | Item | Effort | |
|---|---|---|---|
| R-B1 | **Keyboard triage** — `j/k` move, `enter` open, `r` mark read, `o` open terminal | S | ✅ |
| R-B2 | **Jump to terminal** — focus the actual Terminal tab for a session via pid/tty. Closes `WAITING` → acting | M | ✅ |
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
| R-B24 | **File explorer pane** — the session's worktree as a tree, with a read-only syntax-highlighted viewer. Deliberately not an editor — see [pillar K](#k-explicitly-not) and [feature 0007](../features/0007-file-explorer.md) | M | ⏳ |
| R-B25 | **Explorer workbench** — remember and reveal, multi-file tabs, go-to-file, content search, in one pass ([A16](assumptions.md)). Still a viewer, never an editor. See [feature 0008](../features/0008-explorer-workbench.md) | L | ⏳ |
| R-B26 | **Session labels** — name a session yourself; colour badge on the card, `label:` in the filter. Client view-state like pins ([A17](assumptions.md)). See [feature 0009](../features/0009-session-labels.md) | S | ⏳ |
| R-B27 | **Editor git ergonomics** — a diff gutter vs HEAD with next/prev-change keys, inline blame on the current line, compare-with-revision side by side, and a gutter mark on lines *this session* changed (the mogeung-only one). Still a viewer, never an editor | L |  |
| R-B28 | **Editor navigation** — symbol outline and go-to-symbol (tree-sitter is already in the tree), go-to-line, sticky scroll, folding, highlight-other-occurrences | L |  |
| R-B29 | **Editor content comforts** — markdown preview, image preview, per-tab word wrap, copy path / `path:line`, file facts in the header, bookmarks with a jump list | M |  |
| R-B30 | **Per-pane zoom** — Ctrl+wheel over a pane scales that pane alone (Editor, Changes, Git, Transcript, Terminal), remembered per pane; the global Ctrl+=/− stays whole-window. Asked for directly, built 2026-07-28 | S | ✅ |

## C. Notifications and reach — **shipped except `R-C2`**

Delivered by [feature 0002](../features/0002-sharpen-triage-and-review.md).
`R-C2` needs a fourth binary with its own event loop to outlive the window,
which is a bigger commitment than the rest of the pillar combined — and may be
made redundant by `R-C1` banners. Left open deliberately.

| # | Item | Effort | |
|---|---|---|---|
| R-C1 | **macOS notification** when a session flips to `WAITING` | S | ✅ |
| R-C2 | **Menu-bar item** with the waiting count — glanceable without the window | M |  |
| R-C3 | **Thin web client** — review and unblock from a phone. The daemon already supports it | L | ✅ |
| R-C4 | **Push** via ntfy/Pushover for away-from-desk | S | ✅ |
| R-C5 | **Ambient mode** — big-screen board for a second monitor | M | ✅ |

## D. Review depth — **shipped through R-D17; dogfooding judges R-D10+**

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
three-way view, and read-marks on commit diffs. Acceptance boxes across
features 0010–0013 stay open until the dogfooding week rules
([A19](assumptions.md)) — day one produced six fixes, which is the week
doing its job — and an unused section is a removal candidate, not a
foundation. Deliberately descoped, waiting for want: a "commits on A
not on B" list, read-badges for unviewed log rows, rebindable hunk
keys. The rows below stay unchecked until the week is over, the same
bar `R-B24`–`R-B26` are held to.

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
| R-D10 | **Git view** — recent commits, uncommitted changes, per-commit diffs, blame in the Editor gutter. Read-only, permanently ([A18](assumptions.md)). See [feature 0010](../features/0010-git-view.md) | L | ⏳ |
| R-D11 | **Git depth** — branches, refs, stashes, submodules, commit graph, re-blame, file-at-revision, range diffs. The reading half of a commercial client; still read-only, permanently ([A19](assumptions.md)). See [feature 0011](../features/0011-git-depth.md) | L | ⏳ |
| R-D12 | **Git table stakes** — full commit details, log filtering by message/author/path, file history with rename following. See [feature 0012](../features/0012-git-table-stakes.md) | M | ⏳ |
| R-D13 | **Git forensics** — pickaxe search (`-S`: when did this string appear/vanish), copy hunk/file/commit as patch text, an attribution-only log filter, hunk-navigation keys in git diffs. All small, all aimed at auditing agent work. See [feature 0013](../features/0013-git-reach.md) | M | ⏳ |
| R-D14 | **Diff ergonomics** — expand hunk context on demand (daemon addition), a commit's files as a directory tree instead of a flat list, whitespace-ignore and side-by-side toggles in git diffs. See [feature 0013](../features/0013-git-reach.md) | M | ⏳ |
| R-D15 | **Ref reach** — branch-to-branch compare (three-dot from the merge base, plus "commits on A not on B"), remote branches in the list, a read-only reflog, and `git worktree list` linked to the sessions running in each. See [feature 0013](../features/0013-git-reach.md) | M | ⏳ |
| R-D16 | **Conflict three-way view** — ours/base/theirs read-only for a conflicted file, beyond the markers-in-a-diff we have. See [feature 0013](../features/0013-git-reach.md) | M | ⏳ |
| R-D17 | **Review-state on the log** — R-D8's read/unread marks surfaced per commit, so "which commits has no human read" is visible where commits live. A step toward `R-F2`. See [feature 0013](../features/0013-git-reach.md) | M | ⏳ |

## E. Verification

The observer pivot made this *easier*: the full transcript is on disk.

| # | Item | Effort |
|---|---|---|
| R-E1 | **"Did it actually run the tests?"** — the agent claims they pass; check whether a Bash call ran them | M |
| R-E2 | **Signal runner** — run tests/typecheck per repo, attach results to the session | L |
| R-E3 | **Claim ledger** — extract assertions from assistant text, bind each to evidence | L |
| R-E4 | **Edit-without-verify** — flag sessions that changed code and never built or tested | S |
| R-E5 | **Coverage delta** on changed lines only | L |

## F. Cross-session intelligence

Newly possible: 52 transcripts (67 MB) plus 2,084 prompts in
`~/.claude/history.jsonl`.

| # | Item | Effort |
|---|---|---|
| R-F1 | **Global search** across transcripts and prompt history | M |
| R-F2 | **Prompt-blame** — for a file or line, find the session and prompt that produced it | M |
| R-F3 | **Daily digest** — what happened across all repos, from evidence not self-reports | M |
| R-F4 | **Recurring-failure detection** — the same error across many sessions | M |
| R-F5 | **Personal analytics** — sessions/day, token burn, repos, time of day | S |
| R-F6 | **Prompt library** — most-reused prompts, mined from history | S |
| R-F7 | **Decision extraction** — pull architectural decisions out of transcripts into ADRs | L |
| R-F8 | **Subagent trees** — visualise `isSidechain` work | M |
| R-F9 | **Blame → transcript** — from a session-attributed commit, open that session's transcript at the turns that produced it. The cheap precursor to `R-F2`, riding `R-D11`'s attribution | M |

## G. Rate limits and cost

| # | Item | Effort |
|---|---|---|
| R-G1 | **Five-hour window status** — the CLI emits `rate_limit_event`; currently discarded. With overage disabled, exhausting it hard-fails sessions | S |
| R-G2 | **Warn before exhaustion** | S |
| R-G3 | **Token burn** per session/day/repo | S |

## H. The doc-sprawl thesis

The original stated pain, still entirely unbuilt after two versions. See
[A10](assumptions.md) — decide honestly whether it matters.

| # | Item | Effort |
|---|---|---|
| R-H1 | **Doc inventory** — classify every markdown artifact, assign lifecycle | M |
| R-H2 | **Staleness detection** — doc describes module X; X moved 40 commits ago | M |
| R-H3 | **Doc GC** — propose archive/merge/delete in batch, with evidence | M |
| R-H4 | **Derived progress** — plan items bound to real diffs | L |
| R-H5 | **Agent-instruction hub** — `CLAUDE.md`/`AGENTS.md`/`GEMINI.md` from one source | M |

Note: `scripts/check-docs.sh` and `scripts/gen-status.sh` are `R-H2` and `R-H4`
built for ourselves first, at toy scale. Worth reading before building the real
thing.

## I. Breadth

| # | Item | Effort |
|---|---|---|
| R-I1 | **Codex adapter** — read its on-disk format. Tests whether the Session model generalises | M |
| R-I2 | **Gemini CLI adapter** | M |
| R-I3 | **Linux/Windows** — watching and diffing are portable; terminal launch and "open in" are not | M |
| R-I4 | **Remote daemon** — watch a dev box, run the UI locally | M |

## J. Polish

Persist UI state (window size, filters, tab) · light theme · virtualised diff
rendering · better empty states · config file instead of flags · `mogeung` CLI
subcommands wrapping the REST API.

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
