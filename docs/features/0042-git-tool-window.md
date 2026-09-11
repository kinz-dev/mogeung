---
title: The Git tool window — IntelliJ's log, in the dock
status: active
updated: 2026-09-11
roadmap: [R-D26, R-D27, R-D28, R-D29, R-D30]
depends_on: [A13, A18, A19, A26]
---

# 0042 — The Git tool window

Asked 2026-09-10, with a screenshot of IntelliJ's Git tool window open on a
real repository:

> The Git panel in mogeung is still very ugly, I want a feature rich panel
> that is on par with the one provided by intellij.

Three things were asked for — a gap analysis, a design, and a plan — and this
file is all three. The mockup that goes with the design is linked from
[The design](#the-design).

## Spec

### Problem

The Git pane in the window is a **regression against the pane's own past**,
and only then a gap against IntelliJ.

Between 2026-07-27 and 2026-08-01 the egui client's Git pane reached reading
parity with a commercial client and then grew the write half —
[0010](0010-git-view.md), [0011](0011-git-depth.md),
[0012](0012-git-table-stakes.md), [0013](0013-git-reach.md),
[0014](0014-intellij-commit-view.md), [0025](0025-git-write-local.md), the
last of them built from an IntelliJ screenshot too. Every one of those
features is a daemon verb that still exists and a client that no longer does.
The React port ([0029](0029-desktop-client.md)) carried the daemon's whole
git family onto the wire types and then drew a fraction of it: the write
verbs were left out **by decision** — *"a decision rather than a port"* — and
the graph, the file tree, the details block, the context menus, the hunk
keys, the diff-context controls, the tags and remotes, copy-as-patch,
open-on-host and mark-two-commits went **by omission**, in a port whose
acceptance line for this pane was one word.

What is on screen today (`desktop/src/panes/GitPane.tsx`): one 340-px column
with five segmented sub-views — `log`, `local`, `refs`, `stashes`, `more` —
and a diff on the right. The log is a stack of three-line cards ending in a
*load more* button; there is no graph, no column, no tree, no branch list
beyond local heads, no way to act on anything, and the compare and range
tools are two text boxes at the bottom of *more*. It is honest and it works.
It is not a place anyone would choose to read a repository's history when
IntelliJ is open on the next screen, and the ask says so.

### Gap analysis

The reference is the screenshot: IntelliJ's tool window with **Local
Changes · Stash · Log · Console** tabs, and the Log tab's three panes — a
branch tree on the left, a commit table with graph, refs, author, date and
CI columns in the middle, and on the right the selected commit's changed
files as a tree above its full message, hash, author, signature and the
branches that contain it.

*Wire* says whether the daemon already answers the question; *takes* says
where the work is. Most rows are client-only. The daemon additions are
three small fields, all in `R-D27`.

**Layout and navigation**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| A tool window with four tabs: Local Changes, Stash, Log, Console | one pane, five segmented sub-views in one narrow column | — | client — tabs, three panes (`R-D26`) |
| Three resizable panes; the branch pane collapses | one drag handle between a list and a diff | — | client — `react-resizable-panels` is already a dependency |
| Maximise the tool window; float it | the dock's height is draggable; a dock tool is chrome and cannot pop out ([ADR-0017](../decisions/0017-the-rail-is-chrome.md), [ADR-0037](../decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md)) | — | `R-D29` maximise; `R-D30` for a diff that leaves the dock |
| A repository picker | the pane follows the selected session's repo | — | keep; name the repo in the header |

**Log**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| Graph column with lanes and merges | none — parents are on every row and the egui pane drew them | ✓ `parents` | client — port `lanes()` from the archive (`git show a16699d:crates/mogeung-ui/src/gitview.rs`) |
| Columns: subject · refs and tags · author · date | one card per commit, refs as chips beneath | ✓ | client — a grid row |
| Infinite scroll | 100 rows and a *load more* button | ✓ `skip`/`limit` | client — `@tanstack/react-virtual` is already a dependency |
| *Text or hash* search | message filter, Enter to run | ✓ `grep`; a hash is `git_show` | client — detect a hex prefix and jump |
| **Branch** filter: Select…, Recent, Favorites, HEAD, then Local and each remote as submenus | scope by finding a branch under `refs`, then a chip | ✓ `rev` | client — a dropdown; favourites and recents are machine-scoped preferences |
| **All branches** as the default view | HEAD's first-parent line only | ✗ | daemon — `GitLog.all` → `--all` (`R-D27`) |
| **User** filter | an author text box behind a funnel icon | ✓ `author` | client — a dropdown of authors seen in the loaded log, plus free text |
| **Date** filter | none | ✗ | daemon — `since`/`until` (`R-D27`) |
| **Paths** filter | a path text box; one path, `--follow` | ✓ `path` | client — a directory works today; one path stays the rule because `--follow` is defined for one |
| CI status column | none, and mogeung has no CI source | ✗ | **not doing** — forge integration is "not git" ([0025](0025-git-write-local.md)); see [out of scope](#explicitly-out-of-scope) |
| Sort by date or topology | date | ✗ | not doing until asked |
| — | *(mogeung's own)* a session-attribution dot on the row, and a filter to only those | ✓ `touches_session` | client — the dot exists as a chip; the filter is `R-D13`'s and was not ported |
| — | *(mogeung's own)* a *read* badge once a commit's hunks were read | ✓ `reviewed` on hunks | client — `R-D17`, not ported |
| Row menu: copy revision, copy message, create patch, open on GitHub, compare with…, show in file history | none | ✓ remotes' URLs, hunks, `git_diff_range` | client — Radix context menu is already a dependency; patch text is rebuilt from hunks as `R-D13` did |

**Commit inspector**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| Changed files as a directory tree, folders with counts, single-child chains flattened | every file's diff in one scroll | ✓ `git_show` files | client — port `file_tree()` from the archive (`R-D18`'s shape) |
| Pick a file, see that file | the whole commit or nothing | ✓ | client — filter the cached files by the selection |
| Message, author with email, date, *committed by*, hash, signature state, *In N branches · Show all* | message, author, date, branches — one line | partly: no emails, no signature | daemon — `%ae`, `%ce`, `%G?` on the detail (`R-D27`) |
| The diff opens in the editor or a dialog | in the pane | ✓ | `R-D30` — a diff pane in the centre, optional |

**Branches**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| HEAD · Local · Remote (per remote) · Tags, each a tree grouped on `/`, with a search box, favourites, and ↑↓ counts | a flat list of local heads with ↑↓; remote branches and tags are on the wire since `R-D11`/`R-D15` and are not drawn at all | ✓ `refs` | client (`R-D26`) |
| Branch menu: checkout, new branch from, compare with current, show diff with working tree, rename, delete, push | none | ✓ `git_switch`, `git_branch_create`, `git_compare` · ✗ rename, delete, push | client — reads in `R-D26`, writes in `R-D28`; rename and delete wait for want; push is `R-D24` |

**Local Changes**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| Changelists with checkboxes; unversioned files; a message box; Commit and Amend | a list of status rows; a diff on click; nothing can be done to any of it | ✓ `git_stage`, `git_unstage`, `git_discard`, `git_commit` with `amend` and the session trailer — built in the daemon by [0025](0025-git-write-local.md), sent by no client since 2026-08-05 | client (`R-D28`) — **the write half, and [A26](../product/assumptions.md)'s test** |
| Rollback | — | ✓ `git_discard` | client, with the confirmation `R-D19` specified |
| Resolve conflicts | ours, base and theirs read-only, and *"resolving is git's job, in your terminal"* | ✓ `git_resolve` | client (`R-D28`) |
| Shelve | stash, below | ✓ | — |
| — | *(mogeung's own)* only the files this session touched | none | client — the Changes pane already knows the set |

**Stash and Console**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| Stash tab: list, diff, pop, apply, drop | a list under `stashes`; a diff on click | ✓ push/pop/drop | client (`R-D28`); apply-without-drop waits for want |
| Console: every git command the IDE ran, and its output | errors arrive as a red banner; the fetch report is a 96-px box | — | client (`R-D29`) — the wire commands the window sent, and the daemon's words back, per session |

### Assumptions

- **A13** — the user drives by keyboard. `SUPPORTED`. Every pane here is
  reachable and operable without the mouse, and the log gets the keys a list
  is expected to have.
- **A18** — history and blame are worth a pane. `SUPPORTED`.
- **A19** — the pane earns commercial-grade depth. `SUPPORTED`, and this ask
  is A19's own sentence — *"on-par with a commercial product"* — said a second
  time, against a client that has not yet had the depth to use. The ledger
  records that today. The week after this ships is the test the row has been
  waiting for since the egui client was retired.
- **A26** — the user will commit from mogeung rather than from the terminal
  beside it. `UNTESTED`, and honestly so: the verbs were used for two days
  and then left with the client that carried them. **`R-D28` is the test**,
  not a build that assumes the answer — the removal condition
  [0025](0025-git-write-local.md) wrote stands unchanged, and `R-D26`,
  `R-D27`, `R-D29` and `R-D30` do not depend on it. If the terminal keeps
  winning, the Local Changes tab loses its checkboxes and its commit box and
  keeps its list.

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is
> to test it. The read-only stages rest on `SUPPORTED` rows only; the write
> stage is the test and is sequenced last so the rest is not held by it.

### The design

**Mockup:** [Git tool window — design mockup](https://claude.ai/code/artifact/15096ee1-ca94-4279-98db-97bbb4d185f0),
drawn in the window's own tokens with this repository's real history. Its
tabs, rows and tree are clickable so the selection behaviour can be felt
rather than described.

The shape is IntelliJ's, with three departures that are mogeung's and are
named below. The Git tool stays in the **bottom dock** — chrome, one tool at
a time, following the selected session — because that is where every
*about the repository* view lives ([ADR-0017](../decisions/0017-the-rail-is-chrome.md)).
What changes is that the tool becomes a window in its own right: tabs, three
panes, and a height it can claim when it needs it.

```
┌ GIT · mogeung ── Log · Local changes 4 · Stash 1 · Console ──────────── ⤢ ⇄ ⟳ fetched 2h ago · ↻ ┐
│ ⌕ branch or tag      │ ⌕ text or hash   Branch: all ▾  User ▾  Date ▾  Paths ▾   ● session  read  │ 11 files  +321 −23      │
│ HEAD → main          │ ●  feat: a tmux pane running an agent…  main origin/main  keith   17:17     │ ▾ crates/mogeungd/src  3 │
│ ▾ Local              │ │  docs: the tasks rows take their verdicts…                keith   15:53     │     notes.rs   +150 −3   │
│   ★ main             │ │  feat: the task panel draws the shape…                    keith   11:56     │     state.rs    +16 −4   │
│     git-fetch        │ ●  fix: an indented checkbox is nested…             ●        keith   11:43 ✓  │ ▾ desktop/src/ui/tools 2 │
│     git-write-local  │ ├╮ fix: the Tasks panel can make a task…                    keith   11:18     │     TasksTool.tsx        │
│ ▾ Remote             │ │● feat: tasks are checkboxes in documents…                 keith   Sep 9     │ ─────────────────────────│
│   ▾ origin           │ │  …                                                                          │ fix: an indented checkbox│
│       main           │                                                                               │ is nested when…   ⌄ more │
│ ▾ Tags               │                                                                               │ fa64b61 · keith <…>      │
│     (none here)      │                                                                               │ 2026-09-10 11:43 · signed│
│                      │                                                                               │ in 2 branches: main, …   │
└──────────────────────┴───────────────────────────────────────────────────────────────────────────────┴──────────────────────────┘
```

**The header** is the tool's tabs and its global actions. The tabs carry
counts where a count is news — changed files, stashes — and nothing where it
is not. On the right: maximise, side-by-side, fetch with the age of the last
one beside it (`R-D23`'s honesty, kept), reload.

**Log** is the tab the screenshot shows and the one that is open by default.

- *Branches*, left, 220 px, collapsible to nothing: a tree of **HEAD**,
  **Local**, **Remote** (one node per remote) and **Tags**, grouped on `/` so
  `claude/issue-303` sits under `claude/`. A search box narrows it. Stars mark
  favourites, which live in the machine-scoped preferences beside the queue's
  pins. The current branch is bold and carries ↑↓ against its upstream, and
  the age of the fetch those numbers are as of. A click **scopes the log** and
  checks nothing out — that is a menu item, and a write. The menu: *scope the
  log here* · *compare with the current branch* (merge base, `R-D15`) · *copy
  name* · *favourite* · *new branch from here…* · *check out*, the last two
  arriving with `R-D28`.
- *The log*, centre, takes the rest. One grid row per commit — graph, subject
  with its refs and tags as chips, author, date, and at the end the two marks
  that are mogeung's: the session-attribution dot and the *read* badge. Rows
  are virtualised and the next page is asked for as the end approaches, so
  there is no *load more*. Above it the filter bar: a *text or hash* box, and
  **Branch**, **User**, **Date**, **Paths** as dropdowns that show what is in
  force, plus the ● *session* toggle and a *read* toggle. The default scope is
  **all branches**, which is what makes the graph worth drawing; a branch
  picked in the tree or the dropdown narrows it, and the chip that says so is
  the way out of it. Any of grep, author, path or pickaxe hides the graph, as
  `R-D13` decided — a graph drawn over a filtered subset joins dots that are
  not adjacent.
- *The inspector*, right, 380 px. Top: the commit's files as a tree, folders
  carrying counts, files carrying ± and the read count, single-child chains
  flattened. Beneath it the details: subject in strong text, body folded
  behind *more* once it passes six lines, then hash · author and email ·
  date; *committed by* only when the committer differs; the signature state
  when there is one; and *in N branches* with the first three named and
  *show all*. **Selecting a file replaces the details with that file's diff**
  — the same `DiffList` the Changes pane uses, read marks and all, so a hunk
  read here is read everywhere — and the details shrink to a one-line strip
  with a toggle. The root row restores the whole-commit diff. That is
  [0014](0014-intellij-commit-view.md)'s shape, rebuilt.

**Local changes** is the same three panes with different contents: a tree of
the working tree's changes on the left, grouped **Staged** · **Unstaged** ·
**Untracked** · **Conflicted**, each file with a checkbox that *is* staging —
IntelliJ's checkbox means "in the commit", and in git that is the index;
the selected file's diff in the centre; and on the right a commit box:
message, *amend*, and *name this session in a trailer*, which is on by
default when a session is selected because that trailer is the reason
committing here is worth anything (`R-D20`, `R-F2`). *Discard* is a menu item
that asks first and names every file. A *this session* toggle narrows the
tree to files the session touched. Until `R-D28` lands, the checkboxes and
the box are simply absent and the tab is a list with a diff, which is
what the pane has today.

**Stash** lists stashes with their message and age; selecting one shows
its diff in the centre. *Pop* and *drop* are menu items under `R-D28`.

**Console** is a per-session log of what the window asked git for and what
git said: every command sent, in order, with its answer summarised (*42
commits* · *11 files*) or its error verbatim, and the fetch report in full.
It exists because writes fail loudly and a red banner is a poor place to keep
git's own words.

**Keyboard**, because A13: `/` focuses the search; `↑`/`↓` and `j`/`k` move
the log; `Enter` moves into the inspector's tree; `n`/`p` walk hunks and step
across files as `R-D18` did; `Esc` goes back a level; `Ctrl+T` fetches;
`Alt+9` opens the tool. Double-clicking the dock's *Git* tab maximises it,
which is IntelliJ's own gesture for a tool window.

**Three things that are mogeung's, not IntelliJ's.** The session-attribution
dot and its filter; the read marks and the *read* badge, which make the log a
review ledger and not only a history; and the session trailer on a commit.
None of them costs a column, and they are the reason this is not a worse
IntelliJ.

**Two things IntelliJ has that this does not.** A CI column — mogeung has no
CI source and forge integration is not git. And *float*: a dock tool is
chrome and stays in the window; `R-D30` is the concession, a diff that can
leave the dock as a pane and, from there, as a window under `R-B55`.

### Acceptance

`R-D26` — the window:

- [x] The Git tool has Log, Local changes, Stash and Console tabs, and the
      Log tab is three resizable panes whose widths survive a restart
      *(Console is `R-D29`; a fifth tab, More, holds the three lists the
      wire already carried — see Notes)*
- [x] The branch tree shows HEAD, local branches, every remote's branches and
      tags, grouped on `/`, searchable, with favourites that persist; the
      current branch shows ahead/behind and the age of the fetch it is as of
- [x] The log is one row per commit with a graph column drawn from parents,
      subject, refs and tags as chips, author and date; it scrolls without a
      *load more* button, and selecting a row shows that commit
- [x] The filter bar narrows by text, branch, user and path, says what is in
      force, and clears in one gesture; a hash typed into the search selects
      that commit; the session and read toggles narrow to those rows
- [x] The inspector shows the commit's files as a directory tree with counts
      and read marks, its details beneath, and one file's diff on selection
      with `n`/`p` walking across files
- [x] A commit row's menu offers copy sha, copy subject, copy as patch, *copy
      link on host* when the remote is recognisable, and mark/diff against the
      marked commit; a branch's menu offers scope, compare and copy
- [x] Every list is reachable and operable from the keyboard, and the focus
      ring is visible on every row

`R-D27` — the wire:

- [ ] The log can be asked for across all refs, within a date range, and the
      window uses both — all refs by default, a Date dropdown for the range
- [ ] The details show author and committer emails and the signature state,
      and an older daemon that lacks them still renders a header

`R-D28` — the writes, A26's test:

- [x] Local changes stages and unstages by checkbox, discards from a menu
      after a confirmation that names every file, and commits from a box
      with amend and the session trailer; the list reflects git's answer
      without a manual reload
- [x] A branch can be created and checked out from the tree, with `R-D21`'s
      warning when a session is live in that worktree; a stash can be pushed,
      popped and dropped; a conflicted file can be resolved from its
      three-way view
- [x] Every refusal arrives in git's own words, in the Console and beside the
      control that asked *(beside the commit box; elsewhere the banner and
      the Console)*
- [x] The removal condition from [0025](0025-git-write-local.md) is
      restated in the roadmap row and dated

`R-D29` — console and height:

- [x] The Console tab lists, per session, every git command the window sent
      and what came back; the fetch report lands there in full
- [x] The bottom dock can be maximised to the centre's height and restored,
      by a gesture and by a key, and the state survives switching tools

`R-D30` — the diff pane:

- [x] A file in the inspector can be opened as a pane in the centre, showing
      that file's diff with read marks; the pane pops out under `R-B55` and
      is not restored by a saved layout

### Explicitly out of scope

- **CI status.** No source exists and a forge is not git. If a `gh`-shaped
  read is ever wanted it is its own feature with its own ADR, because it is
  the second outbound call this process would make.
- **`pull` and `push`.** `R-D24`, [ADR-0022](../decisions/0022-a-fast-forward-is-not-a-merge.md)
  is `proposed` and stays so; nothing here reopens it.
- **Rename, delete, rebase, merge, cherry-pick, revert, reset.** The verbs
  where a wrong click costs an afternoon; [0025](0025-git-write-local.md)
  left them for want and none has arrived.
- **Hunk-level staging.** File-level first; `R-D28` is the test of whether
  even that is used.
- **Topological sort, a repository picker, multi-repo aggregation.** The pane
  follows the session's repo; date order is git's.
- **A float mode for the dock.** A dock tool is chrome by decision; `R-D30`
  is as far as this goes.

## Plan

*Drafted by an agent, 2026-09-10. Approved 2026-09-11 — "I have read the docs
and it looks good" — and building began the same day, `R-D27` first.*

### Approach

Five roadmap rows, each shippable, in this order. The daemon work is done
first because it is a morning and the window's default view wants it; the
write half is last because it is the only stage that rests on an `UNTESTED`
row and the other four must not wait on it.

**1. `R-D27` — the three daemon fields.** `GitLog` gains `all: bool`,
`since: Option<i64>` and `until: Option<i64>` (serde-defaulted, so the
REST twin and any older client are unchanged); `log_page` turns them into
`--all`, `--since=<iso>` and `--until=<iso>` — epochs are formatted
daemon-side so no date text from a client reaches git. `CommitDetail` gains
`author_email`, `committer_email` and `signature` (`%ae`, `%ce`, `%G?`,
serde-defaulted). `GitCommits` echoes the new scope fields so the stray rule
still holds. The REST twin and `wire-protocol.md` move with it. **A public
`mogeung-core` type changes, so `cargo check --manifest-path
desktop/src-tauri/Cargo.toml` is on this stage's checklist** — the shell is
its own workspace and the rest of the list does not compile it.

**2. `R-D26` — the window, client only.** `GitPane.tsx` becomes a folder,
`desktop/src/panes/git/`, with one component per region, and two pure
modules under `desktop/src/lib/` that carry the algorithms and their tests:

- `gitGraph.ts` — `lanes(commits)` ported from the archived egui
  `gitview.rs` (expectation-based lane assignment, joins collapsed, freed
  lanes reused, capped at eight drawn lanes), plus the SVG cell that draws
  one row. Computed over every loaded row, never per page.
- `gitTree.ts` — `fileTree(files)` for the inspector (chains flattened,
  directories before files, counts rolled up) and `refTree(refs)` for the
  branch pane (HEAD · Local · Remote per remote · Tags, grouped on `/`).
- `gitFilter.ts` — the filter bar's state as one value (the `Query` the pane
  already insists on, grown by `rev`, `all`, `since`, `until`), hash
  detection, and the chip text that says what is in force.

The log is a CSS grid row under `@tanstack/react-virtual` with a sentinel
that calls the existing `askLog(skip)` near the end — the store's stray rule
and page-append logic are untouched. The three panes are
`react-resizable-panels` with sizes in `prefs`. Menus are
`@radix-ui/react-context-menu`, which the queue already uses. Selection
state (`selected`, `selectedPath`, a new `selectedFile`, `rangeMark`) stays
in `GitState`; favourites, recents and pane sizes go to machine-scoped
preferences. `DiffList` is reused as it is; the file focus filters the
cached `FileChange[]` before it is handed over, which is how `n`/`p`, the
patch text and the read counts follow for free.

**3. `R-D29` — Console and maximise.** A ring buffer per session in the
store, fed by `send` for every `git_*` command and by `ingest` for every
`git_*` answer and every `Error`, rendered as a table. The dock gains a
`dockMax` preference and a toggle in `BottomDock`; the keymap gains
`dock.maximise`; a double-click on the active dock tab toggles it.

**4. `R-D28` — the writes.** Local changes gets its checkboxes
(`@radix-ui/react-checkbox`), its commit box and its discard dialog
(`@radix-ui/react-dialog`); the branch menu gets *new branch* and *check
out*, the latter behind a dialog that names live sessions in that worktree
(the client knows sessions by `cwd`); Stash gets *pop* and *drop*; the
three-way view gets *take ours* · *take theirs* · *mark resolved*. Every
verb is a `send` of a message that is already typed in `wire/types.ts`, and
every answer is the `git_local_changes` re-broadcast the daemon already
sends. The roadmap row carries 0025's removal condition, dated.

**5. `R-D30` — the diff pane.** A `diff:<session>:<sha>:<path>` pane kind
registered beside `file:` in `panes.ts`, rendering `DiffList` for one file
from the git store (re-asking `git_show` if the store has moved on). It is
never restored by a saved layout — a sha names a repo state that may be
gone by next launch, the rule revision tabs already follow. Pop-out comes
from `R-B55` unchanged.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | `GitLog { all, since, until }`, `GitCommits` echoes them; `CommitDetail { author_email, committer_email, signature }` |
| `crates/mogeungd/src/git.rs` | `LogFilter` grows; `log_page` args; detail format and parser; tests |
| `crates/mogeungd/src/api.rs` | plumb-through; REST query params `all`, `since`, `until` |
| `desktop/src-tauri/` | **`cargo check`** after the wire change |
| `desktop/src/wire/types.ts` | the same fields |
| `desktop/src/store/index.ts`, `store/prefs.ts` | `GitState` grows `selectedFile`, `rangeMark`, `console`; prefs grow `gitFavourites`, `gitRecents`, `gitPaneSizes`, `dockMax` |
| `desktop/src/lib/gitGraph.ts`, `gitTree.ts`, `gitFilter.ts` (+ tests) | the pure parts |
| `desktop/src/panes/git/*.tsx` | `GitPane`, `BranchTree`, `LogTable`, `LogToolbar`, `CommitInspector`, `LocalChanges`, `StashTab`, `ConsoleTab`, `menus` |
| `desktop/src/ui/BottomDock.tsx`, `lib/keymap.ts`, `lib/panes.ts` | maximise; keys; the `diff:` pane kind |
| `docs/design/wire-protocol.md`, `docs/design/architecture.md` | the three fields; the pane's new shape |
| `docs/guide/reviewing.md` | a section on the Git tool window |
| `docs/product/roadmap.md`, `docs/product/assumptions.md` | rows `R-D26`–`R-D30`; A19 and A26 notes — done with this spec |

### Risks and unknowns

- **The dock is 300 px tall on a laptop.** Three panes at that height is a
  cramped IntelliJ, and IntelliJ knows it, which is why it can maximise and
  float. Maximise is `R-D29` and is cheap; if a week says the tool is only
  ever used maximised, the honest next step is a Git *pane* in the centre,
  and that is an ADR-0017 conversation, not a bug.
- **`git_show` carries every hunk of every file.** An eighty-file agent
  commit is one large answer, and the tree needs only names and counts.
  Measure first; a `--stat`-only variant is a daemon addition only if a real
  commit proves slow. `truncated` already bounds the worst case.
- **`--all` on a paged log.** Lanes are drawn over the concatenated pages, so
  a page boundary can land mid-merge; the archive's algorithm handled it by
  carrying expectations across rows, and the port must keep that property —
  a test with a merge straddling a page boundary.
- **`--follow` and `--all` together** are legal but `--follow` is
  one-path-only; the Paths dropdown never offers more than one.
- **Lists are `role="option"` divs.** A grid row with four columns wants
  `role="row"` semantics for the screen reader and the focus tests; the
  `Row` primitive is not the right base and a sibling is.
- **A26 is still the bet** for `R-D28`, and the shell tab is one keystroke
  away. The mitigation is the same as before: the removal condition is
  written before the code, and the stage is last so nothing else waits.
- **Two ways to see one diff** (`R-D30` beside the inspector) is the kind
  of duplication `R-D18` refused. The pane exists for one reason — height —
  and if the maximise toggle makes it unwanted, it is the row to drop.

### Test strategy

Pure functions first, because they are where the port can be wrong quietly:
`lanes()` on linear, one-merge, criss-cross and disjoint histories, and on a
merge straddling a page boundary; `fileTree()` losing no file, flattening
chains, directories before files, counts summing; `refTree()` grouping
`claude/a` and `claude/b` under `claude/` and `origin/*` under its remote;
hash detection on `abc123`, `abc123 fix` and `deadbeef` the word.

Component tests in the shape `GitPane.test.tsx` already has — a fake socket,
assert the exact `ClientMsg`: the default log asks `all: true`; picking a
branch asks `rev` and drops a stray page; typing a hash sends `git_show`;
selecting a file filters the diff without a new request; each write control
sends its verb once and *discard* sends nothing until confirmed; a blank
commit message never leaves the window; the Console shows an `Error` in
git's words. The dock's maximise toggles a preference and a class.

Daemon: `log_page` argument construction for `all`, `since`, `until` (an
epoch becomes an ISO date and nothing else); the detail parser with and
without the three new fields; the e2e hostile sweep grows the new query
params. `cargo test --workspace`, `npm test`, `npm run check`,
`./scripts/check-docs.sh`, and the Tauri shell's `cargo check`.

## Notes

*The gap analysis above was taken against the window as of commit `6005012`
and the daemon's `git.rs` at 2,565 lines.*

### `R-D26` (2026-09-11)

Built the day after the spec, and three things in it are not what the spec
drew.

**A fifth tab.** IntelliJ's window has no home for the reflog, the worktrees
or the submodules, and the wire has carried all three since `R-D13`/`R-D15`.
Dropping them would have regressed the pane to make it look more like the
screenshot; they sit under *More*, with the same inspector on the right, and
compare and range moved into the menus where the spec put them.

**"Open on host" is "copy link on host".** The shell's one URL opener,
`open_local_url`, refuses anything not on this machine, and a general opener
is a Tauri capability this row does not add. The link is built the same way
and put on the clipboard, which is the widest part of the pipe here as it
is for the follow-up prompt.

**No panels library.** `react-resizable-panels` sizes in percentages of its
group; every other draggable edge in this window is pixels in the
preferences, written once on release, so the splitters use that idiom
(`useDragWidth`) rather than a second one. What the idiom did not have was
a clamp, and the first look in a browser found why one was needed: a dock
620 px wide with a 210 px branch pane and a 360 px inspector left the log
50 px, with the ref chips drawn over the graph. The outer panes are now at
most a share of the pane — 28 % and 42 % — and the log's columns give way
from the right below 700 px and 480 px. The saved widths are untouched, so
a wide window gets them back.

**The echo rule grew a clause.** The store drops a page whose echo disagrees
with the scope in force, and `R-D27` added three fields to the echo. A
daemon a build behind the window answers without them, and comparing an
absent field against `all: true` would have dropped every page it sent —
an empty log for good on the machine whose daemon lagged. A missing field
now means "the daemon did not say" and matches anything; a present one is
compared. Pinned by a test.

**`n`/`p` needed one attribute.** The hunk block gained `data-hunk` so the
keys can find the next hunk in the scroller and step into the next file past
the last one — the `R-D18` walk, without the inspector knowing how a hunk
is drawn.

### The first day of use (2026-09-11, after the install)

Two asks within the hour, both on the Log tab, both built the same day.

**"Make the grid line clearer so that I can resize the column, and make all
column resizable."** The log has a header row now — *subject · author ·
date · marks*, with the graph column unlabelled — and a divider on every
fixed column, dragged the way every other edge in this window is and written
to the preferences on release (`gitLogColumns`). Cells carry a vertical
rule. The subject is the column that flexes, so a divider resizes the fixed
column beside it; the graph sizes itself from the lanes until a hand sets it
and clips after. Floors keep a column from being dragged out of existence.
The virtualiser is told the header's height as a scroll margin so keyboard
selection still lands where it should.

**"A keymap Ctrl+D for opening the diff in the main window. Double click the
file to open that file in the main window."** Read as two different things,
deliberately: `Ctrl+D` opens the **diff** — the focused file's in the
inspector, the selected commit's every file in the log — as an `R-D30`
pane; a **double-click** on a file opens the **file itself as it stood at
that commit** in the Code pane, through the revision tabs `R-D11` built,
with a deleted file read at the parent where it still exists. Enter on an
already-selected file still opens the diff, and a double-click on a log row
opens the whole commit's diff. If the double-click was meant to be the diff
too, it is one line to change and the spec's author will say.

### `R-D30` (2026-09-11)

An afternoon rather than the medium it was sized at, because the two hard
parts already existed: `R-B53` made a pane per file with an id that carries
what the pane needs, and `R-B55` made every pane pop out. A `diff:` id
beside `file:`, the same strip-on-restore rule, the same close button, and
the pane is `DiffList` — read marks included, because the marks are content
hashes and a hunk read here is read in the dock.

**The one thing that needed inventing was where the files come from.** The
git slice keeps one diff, the selected commit's, and a pane outlives the
selection that fetched it. So the store now keeps a small cache of revision
diffs, `revDiffs`, keyed `session:rev` and bounded at eight, filled by every
commit or range diff that arrives whether or not it is still selected — a
pane opened from the inspector never asks, and a pane opened cold asks once.
Not a stash: a stash has no revision a pane could ask for again, so the
button is absent there rather than promising a re-fetch that cannot happen.

Two ways to see one diff is the duplication `R-D18` refused, and the row
said in advance which one goes if the maximise makes the pane unwanted.
That verdict is the week's.

### `R-D28` (2026-09-11)

The smallest of the client rows in code and the largest in what it changes,
which is why it went last. Every verb was already on the wire and already
guarded, so the work was controls and their tests — 934 green on the first
run — and three decisions worth writing down.

**A new branch is made from HEAD, and the button says so.** The daemon's
`git_branch_create` is `git branch <name>` with no start point, so a menu
item on a row — *new branch from here* — would promise what it cannot do.
The button is on the pane, labelled *from HEAD*, and a "create and check
out" onto a worktree with a live agent gets `R-D21`'s warning like any other
checkout: create first, then ask.

**The commit box empties on evidence, not on the click.** A commit is
answered by the status re-broadcast, and the box clears when that arrives
with nothing staged — so a refused commit keeps the message you typed and
shows git's refusal beneath the button, from the Console's row.

**The checkbox is the index, and a partially staged file is ticked.** A file
both staged and changed again since wears *+ unstaged* beside its name;
ticking it stages the rest. The alternative — an indeterminate checkbox —
is a state git does not have.

### `R-D29` (2026-09-11)

Smaller than `R-D26` by an order of magnitude, and two things are worth
writing down. **The ledger has one writer on each side.** Every `git_*`
command becomes a row inside the store's `send`, and every answer closes a
row inside `ingest` before the switch — so no region of the window, and no
future one, has to remember the console exists. **The wire's error has no
address.** `ServerMsg::Error` carries a message and nothing else, so a
refusal lands on the latest command still waiting, across sessions, ordered
by a send sequence rather than the clock — two commands in one millisecond
have a latest, and the first test found they did not when the clock was the
tie-break. The row says on hover that this is a guess. Giving the error a
`session_id` and a `cmd` on the wire would make it certain and is a small
daemon change; it was not made here because a window a build ahead of its
daemon would still need the guess, and because the guess is right whenever
one command is in flight, which is nearly always.

Maximise cost no layout: the dock asks for the column with `flex: 1 1 100%`
and the centre, already `min-h-0 flex-1`, folds to nothing. `dockHeight` is
untouched, so restoring gives back what you had.

### `R-D27` (2026-09-11)

A morning, as planned, and two things worth writing down. **`@<epoch>` is a
date git accepts, and the first check said it was not** — the range tried was
a year out, because a hand-computed epoch landed in 2025. The ISO form was
already the plan and is what shipped, but the lesson is the one the spec
gives for the filter text: do not learn git's date grammar at the argument,
fix one form and test it. **`--since` is a cutoff on the *commit* date**, not
the author date the first draft of the wire comment said, and it is applied
during traversal rather than as a filter — fine for a date dropdown, wrong
for anything that wanted exactness, and the wire comment now says which.
`log_page`'s argument list became `log_args` so the shape could be pinned
without a repository; the test that does so would have failed on the old
code by not compiling, which is the honest kind.
