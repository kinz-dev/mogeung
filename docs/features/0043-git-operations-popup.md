---
title: The Git Operations popup — IntelliJ's two popups, on a keystroke
status: active
updated: 2026-09-16
roadmap: [R-D31, R-D32]
depends_on: [A13, A19, A26]
---

# 0043 — The Git Operations popup

Asked 2026-09-16, with two screenshots of IntelliJ:

> I want to build a popup git ops operation with keymap : Alt + `
> When that popup is open, user can press 7 (or other shortcut number) and go
> to the next operation or popup. For 7, it should show this screen and allow
> equivalent function.

The first screenshot is IntelliJ's **VCS Operations Popup** (`Alt+` `): a
numbered list of fourteen entries. The second is the **Git Branches popup**
(`Ctrl+Shift+` `, entry 7): a searchable list of branch actions over the
repository's refs.

Three things were asked for — a gap analysis, a plan, and the build — and this
file is the first two. [The notes](#notes) are the third.

## Spec

### Problem

[Feature 0042](0042-git-tool-window.md) built the Git tool window, and every
verb in these two popups that mogeung has at all is **in that window**. The
window is a dock tool: reaching a verb means opening the dock (`Alt+9`),
finding the tab, finding the row, and clicking. That is the right cost for
*reading* — a log, a diff, a branch tree — and the wrong one for *doing*, which
is why IntelliJ ships a popup over the same commands and binds it to one key.

`A13` is the assumption underneath, and it is `SUPPORTED`: this user drives by
keyboard and reaches for a palette before a menu. The palette (`Ctrl+K`) already
exists and lists every action — but it has **one git action in it**, `git fetch`,
because that is the only git verb the keymap has ever had. Everything the write
half shipped under `R-D28` is mouse-only.

So the gap is not that mogeung cannot do these things. It is that it has no way
to *say* them.

### Gap analysis — screen 1, the VCS Operations popup

Fourteen entries. What mogeung has today, verb by verb:

| # | IntelliJ | mogeung today | Gap |
|---|---|---|---|
| 1 | Commit… `Ctrl+K` | `git_commit` + the commit box in **Local changes** (`R-D28`) | Route only — open the tool window on that tab and put the cursor in the box |
| 2 | Commit File… | `git_stage(paths)` then the same box | Route only, and it needs a **target**: `git[id].selectedPath` is the file the Local-changes tab has selected |
| 3 | Rollback… `Ctrl+Alt+Z` | `git_discard(paths)` behind a dialog naming every file | Route only, same target |
| 4 | Show History | `git_log` with a `path` filter (`R-D12`) | Route only — the Log tab, scoped to the selected file |
| 5 | Annotate with Git Blame | `git_blame` → the blame gutter in the Code pane | **Half a gap**: the gutter's toggle is `useState` *inside* `FilePane`, so nothing outside it can turn blame on |
| 6 | Show Diff `Ctrl+D` | `git_diff_file`, the diff pane (`R-D30`), the Changes dock | Route only |
| 7 | Branches… `Ctrl+Shift+` ` | `BranchTree` inside the Log tab (`R-D26`) | **Screen 2** — the shape is the gap, not the data |
| 8 | Push… `Ctrl+Shift+K` | **nothing, by decision** | [ADR-0012](../decisions/0012-write-locally-never-publish.md) and [ADR-0014](../decisions/0014-fetch-is-not-publishing.md) draw the line at *publishing and merging*. `R-D24` is open and undecided |
| 9 | Stash Changes… | `git_stash_push` + the Stash tab | Route only |
| 0 | Unstash Changes… | `git_stash_pop` / `git_stash_drop` | Route only |
| — | Worktrees… | `git_worktrees` + the More tab | Route only |
| — | Copy Branch Name | `refs.head` + `writeClipboard` | Route only |
| — | Show Local History… | **nothing** — IntelliJ's local history is an IDE-side recording of every edit, which mogeung does not keep | Nearest honest thing is the **reflog** (`git_reflog`, More tab), which answers the same question — *where did that work go* — from git's own record |

Two rows are not routing:

- **Push (8)** is the one entry the product has decided against. It is drawn
  **disabled, with the reason**, rather than omitted: an IntelliJ user will
  press `8`, and a row that is missing teaches nothing while a row that says
  *mogeung never publishes — ADR-0012* teaches the rule once. Same choice
  `R-J92` made for the IntelliJ button.
- **Annotate (5)** needs `blameOn` lifted out of `FilePane`'s local state.
  `scoped.editorWrap` is the precedent — a per-path editor toggle kept in
  scoped prefs — so blame joins it as `editorBlame`.

### Gap analysis — screen 2, the Branches popup

JetBrains' own description of this popup: four nodes (**recent branches**,
**local**, **remote**, **tags**), branches grouped on `/`, a search field over
branches *and* actions, and per-branch actions — checkout, new branch from,
compare with current, show diff with working tree, rebase, merge, pull into,
rename, delete, update, favourite, copy name.

| Screen 2 row | mogeung today | Gap |
|---|---|---|
| Search for branches, actions, repositories | `cmdk` (the palette is built on it); `refTree(refs, query, favs)` already filters | Route only |
| Update Project… `Ctrl+T` | `git_fetch`, **already bound to `Ctrl+T`** as *Update remote-tracking refs* | None — mogeung took IntelliJ's key for the honest half of it. It fetches; it does not merge |
| Commit… | as screen 1 | Route only |
| Push… | — | Disabled, as above |
| Show Pull Request / Submit Review / Review Mode | — | **Out of scope.** These are forge and Space integrations; mogeung has no forge account and ADR-0012 keeps it off the network but for `fetch` |
| New Branch… `Ctrl+Alt+N` | `git_branch_create(name, switch_to)` | Route only — from HEAD, which is what the verb does |
| Checkout Tag or Revision… | `git_switch` runs `git switch <name>`, which **refuses a tag or a sha** | **The one daemon gap.** `git switch --detach <rev>` is the same verb with a flag |
| Repositories (two rows, coloured) | one session is one worktree; `liveSessionsIn(root)` knows the others | Drawn as **one** row — this repository, its branch, its ↑↓ — rather than a list mogeung does not have |
| Recent Branches in … | `prefs.gitRecents[root]`, written by `scopeTo` | Route only |
| Local / Remote / Tags, grouped on `/` | `refTree` / `visible` (`lib/gitTree.ts`) | Route only |
| Per-branch: checkout, compare with current, copy name, favourite | `switchTo`, `compareWith`, `copyText`, `toggleFavourite` | Route only |
| Per-branch: rebase, merge, pull into, update | — | **Out of scope by ADR-0014**: merging is the line |
| Per-branch: rename, delete, new branch from | — | **Not built.** Local-only writes that no ADR forbids and no wire verb exists for. Filed, not built — see [out of scope](#explicitly-out-of-scope) |

### Assumptions

- `A13` — *drives by keyboard, reaches for a palette before a menu* —
  `SUPPORTED`. This feature is a second test of it: the palette is the general
  answer, and this is the argument that one domain deserves its own.
- `A19` — *the git pane earns commercial-grade reading depth* — `SUPPORTED`.
- `A26` — *the user will commit from mogeung rather than from the terminal* —
  **`UNTESTED`**, and the rule says the work is then to test it rather than to
  build on it. This feature **adds no write verb** (bar a flag on one that
  exists): every write it reaches was built under `R-D28` as `A26`'s own test,
  and what this changes is the *distance* to them. That is the honest reading —
  a popup that makes an untested verb one key away is part of testing it, not a
  second bet on top of it. Nothing here is worth keeping if `A26` fails.

### Acceptance

- [x] `Alt+` ` opens a popup listing the operations, numbered `1`–`9`, `0`
- [x] Pressing a number runs that operation and closes the popup; `↑`/`↓` and
      `Enter` do the same; `Escape` closes it
- [x] `7` opens the Branches popup, from which `Escape` returns to the window
- [x] An operation with no target is **disabled and says why**, and pressing its
      number says the same thing rather than doing nothing
- [x] `8` (Push) is disabled with ADR-0012's reason, always
- [x] The Branches popup searches branches *and* actions in one box
- [x] Checkout from the Branches popup goes behind `R-D21`'s live-session
      warning, exactly as the branch tree's does
- [x] Checkout Tag or Revision reaches a tag or a sha
- [x] Both popups say what to do when the session is not in a git repository

### Explicitly out of scope

- **Push, pull, merge, rebase, pull-into, update-branch.** ADR-0012 and
  ADR-0014. `R-D24` is where that decision would be revisited, not here.
- **Forge integration** — pull requests, reviews, review mode.
- **Branch rename and delete, and new-branch-from-a-ref.** Local-only writes
  that would need three new daemon verbs; a row of their own if they are
  wanted (`R-D33`), not a rider on this one.
- **A configurable popup.** IntelliJ's list is editable in *Menus and
  Toolbars*; mogeung's is a fixed list until someone asks.

## Plan

### Approach

**The list is data, in `lib/`, and the popups are two renderers of it.** Each
entry is `{ id, digit, label, hint, group, enabled(ctx), run(ctx) }` over a
context of `{ session, repoRoot, selectedPath, refs }`. That is what makes the
rules testable without a DOM: *which digit runs what*, *what is disabled and
why*, and *what each one does to the store* are assertions over a pure module,
which is where every bug in this area will be.

**Routing is a store signal, never a reach into a component.** Two pieces of
component state have to be liftable for the router to exist: the Git tool
window's tab (`useState` in `GitPane`) and the blame gutter (`useState` in
`FilePane`). The tab becomes `gitTab` in the store — view state, not a
preference, like `activePane`; blame becomes `scoped.editorBlame`, a per-path
list beside `editorWrap`.

**One daemon change, and it is a flag.** `GitSwitch` gains `detach: bool`, and
`git::switch` adds `--detach` when it is set. `check_ref` already admits a sha
or a tag, so nothing widens: the same names that could be switched to before
are the ones that can be detached onto now.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | `GitSwitch { detach }`, `#[serde(default)]` |
| `crates/mogeungd/src/git.rs` | `switch(root, name, detach)` → `--detach` |
| `crates/mogeungd/src/state.rs`, `api.rs` | the flag passed through |
| `desktop/src/wire/types.ts` | `git_switch` gains `detach?: boolean` |
| `desktop/src/lib/gitOps.ts` | **new** — the operations, as data |
| `desktop/src/lib/gitActions.ts` | `switchTo(id, name, detach)`; `stashPush` unchanged |
| `desktop/src/ui/GitOpsPopup.tsx` | **new** — the numbered popup |
| `desktop/src/ui/BranchesPopup.tsx` | **new** — the searchable branches popup |
| `desktop/src/store/index.ts` | `gitPopup`, `gitTab`, `commitFocus` |
| `desktop/src/store/prefs.ts` | `scoped.editorBlame` |
| `desktop/src/lib/keymap.ts` | `git.ops` (`Alt+Backquote`), `git.branches` (`$mod+Shift+Backquote`) |
| `desktop/src/panes/GitPane.tsx` | the tab comes from the store |
| `desktop/src/panes/git/LocalChanges.tsx` | the commit box takes the keyboard when asked |
| `desktop/src/panes/FilePane.tsx` | blame reads `scoped.editorBlame` |
| `desktop/src/App.tsx` | mounts both popups |

### Test strategy

- `gitOps.test.ts` — the numbering, the disabled reasons, and what each entry
  does to the store. **Would fail today**: none of it exists.
- `GitOpsPopup.test.tsx` — `Alt+` ` opens; a digit runs its entry; `7` swaps to
  the branches popup; a disabled digit pushes the reason rather than closing.
- `BranchesPopup.test.tsx` — the search matches a branch and an action; checkout
  raises `R-D21`'s warning when a session is live in the worktree.
- Rust — `switch` builds `--detach` only when asked; the wire round-trips a
  message without the field (`serde(default)`), which is what an older client
  sends.

### Risks and unknowns

- **`Alt+` ` on macOS composes a dead key.** `⌥`` is a grave accent, so the
  chord must be spelled by **physical key** (`Alt+Backquote`) — the rule `R-L6`
  already wrote down for `⌥P`. One spelling then works on both platforms.
- **The popup steals a key from a focused terminal.** It is a chord, and
  `focusOwns` defers only bare keys, so an Agent pane keeps everything it needs.
- **Two popups, one Escape.** The branches popup is reached *from* the
  operations popup, so Escape has to mean *back to the window* rather than
  *back one step* — chosen because the second popup is also reachable directly.

## Notes

Built 2026-09-16, the day it was asked for.

**The plan held, and the two lifts were the whole cost.** Everything the popups
do was already a function call; what was missing was a way to *say* it from
outside the component that owned the state. Three pieces moved:

| Was | Is | Why it had to move |
|---|---|---|
| `GitPane`'s `useState<Tab>` | `gitTab` in the store | *Stash Changes…* means the Stash tab |
| `MoreTab`'s `useState<List>` | `gitMore` in the store | *Worktrees…* is a list inside a tab |
| `FilePane`'s `useState(blameOn)` | `scoped.editorBlame`, by path | The popup has a path and no handle on a pane |

The blame lift is the one with a behaviour change attached, and it is an
improvement rather than a cost: annotation now survives closing the file, which
is what `editorWrap` has always done beside it.

**One existing test had to be told where the tab lives now.** `GitWrite`'s
fixture opened a tab and the next test inherited it, because store state
outlives `cleanup()` where `useState` does not — so the fixture sets `gitTab`
the way it already sets `selected`. That is the honest cost of lifting state,
and it surfaced immediately rather than as a flake.

**`git switch --detach` was the only daemon change, and `check_ref` needed
nothing.** Its rule — letters, digits and `. _ - /`, not starting with `-`, `.`
or `/` — already admits a tag and a hex sha, so the flag widened the *verb*
without widening what may be named. The test asserts both halves: that plain
`switch` **refuses** a tag (the reason the flag exists at all), and that the
same evil names are refused with and without it.

**What the screenshots asked for and did not get, with the reason:**

- *Push*, *Show Pull Request*, *Submit Review*, *Review Mode* — ADR-0012, and
  no forge. Push is drawn and refuses; the other three are absent, because a
  row that could never work in any configuration teaches nothing.
- *Show Local History* — renamed on the row to *Show Local History (git
  reflog)*. IntelliJ records every edit; mogeung records none; git's own memory
  is the nearest true answer and the row says which one it is giving you.
- *Repositories*, plural — one mogeung session is one worktree, so the list is
  a line: the repository, its branch, and its ↑↓ against the upstream.

**`Update Project` kept IntelliJ's name.** It was drawn as *Update remote-
tracking refs (fetch)* first, which is what the keymap calls it — and it
truncated at the popup's width and read as a different command to someone
arriving from IntelliJ. The row is *Update Project (fetch)* with *reads the
remote; merges nothing* beside it, which says the same thing in the space
available.

**Verified in the running window**, not only in jsdom: `Alt+` ` opens the list
over the real app, `7` with no session selected answers *Branches… — no session
is selected*, and `Ctrl+Shift+` ` opens the branches popup straight to its own
empty state. The populated tree is covered by tests rather than by eye — the
daemon on this machine reported no sessions at the time.
