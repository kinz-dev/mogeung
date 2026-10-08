---
title: The Git tool window
status: active
updated: 2026-10-08
---

# The Git tool window

**Git** in the bottom dock (`Alt+9`) is the repository the selected session
is in — IntelliJ's tool window, in mogeung's colours. It reads everything and
writes locally; it never publishes
([ADR-0014](../decisions/0014-fetch-is-not-publishing.md)).

## Log

Three panes. On the left, the branches: HEAD, Local, one node per remote,
Tags, grouped on `/`, with a search box and a star for favourites. A click
**scopes the log to that ref and checks nothing out**. In the middle, one row
per commit — the graph, the subject with its refs as chips, author, date — and
at the end of a row the two marks that are mogeung's: a blue dot for a commit
that probably came from this session, and *read* once every hunk of it has
been read. The columns have a header and share the pane's width, so they scale
with it; drag a divider to move the line between two columns, and the shares
are kept. Above the rows, the filter bar: *text or hash*, then Branch, User,
Date and Paths. Enter runs the query; a hash selects that commit; `author:`,
`path:` and `find:` in the box reach the same fields the dropdowns do. What is
in force is named beneath the bar, with *clear* beside it. The default is every
branch, which is what the graph needs; any text filter hides the graph, because
lanes drawn over a subset would join dots that are not adjacent.

On the right, the selected commit: its files as a tree with counts, and the
details beneath — message, hash, author and email, date, the signature state,
and the branches that contain it. **Click a file and its diff replaces the
details**, read marks and all; *all files* is the root row; *details* brings
the message back. `n` and `p` step through the hunks and across files. The
strip's ⧉, `Ctrl+D`, or `Enter` on the file you already have selected opens
that diff as a pane in the centre, where a long read has the window's height
and can pop out into a window of its own; it closes like a file. `Ctrl+D` in
the log opens the selected commit's whole diff the same way. A **double-click
on a file** opens the file itself, as it stood at that commit, in a file pane.

## Local changes, Stash, Console, More

**Local changes** is the working tree, grouped Conflicted · Staged · Unstaged
· Untracked, with a *this session* toggle and the selected file's diff — or
its three sides when it is conflicted. **Stash** lists what is shelved and
shows one through the same inspector. **Console** is every git question this
window asked, in order, with what came back — git's refusals in its own
words, and the fetch report in full. **More** holds the reflog, the worktrees
and the submodules.

Three panes at the dock's usual height is a cramped IntelliJ, and IntelliJ
knows it: **maximise** the dock with the button on its strip, `Alt+Shift+9`,
or a double-click on the open tool's tab, and the centre folds away until
you restore it.

Right-click a commit for copy sha, copy subject, copy as patch, a link to the
commit on GitHub or GitLab, and *mark* — two marked ends make a range diff.
Right-click a branch for *compare with the current branch*, which shows what
merging it would bring, from the merge base.

The keyboard: `/` is the search box; `↑` `↓` or `j` `k` move the log; `Enter`
steps into the file tree and `Esc` back; `n` `p` walk hunks; `Ctrl+T`
fetches, and the header says when the last fetch was, because ahead/behind is
only as true as that.

## And it acts

In Local changes the checkbox is the index: tick a file to stage it, untick to
take it out of the next commit; *stage all* and *unstage all* sit on each
group's header. The commit box on the right takes a message, *amend*, and
*name this session in a trailer* — a `Mogeung-Session:` line that prompt-blame
reads back — and commits only what is staged, never `-a`. Right-click a file
for *discard*, which asks first and names every file, because git keeps no
copy. A conflicted file's three-way view offers *take ours*, *take theirs* and
*mark resolved*. Right-click a branch to *check it out*; if an agent is running
in that worktree the window names it and asks, because git cannot see an agent
reading files that silently change. The branch pane's `+` makes a new branch
from HEAD. Stash has *Stash all*, and *pop* and *drop* on each entry. Every
refusal comes back in git's own words.

Nothing here reaches a remote but fetch. Pull and push are not in this window,
by decision; `R-D24` on the roadmap holds the question and
[ADR-0022](../decisions/0022-a-fast-forward-is-not-a-merge.md) the draft of an
answer.

## More than one repository

The window follows the selected session's repository by default. The
repository picker in its header offers every checkout under the session's
workspace roots — the session's own, and any folder you added to its workspace
— so a monorepo's sibling or the second checkout a session touched is one
click away. Every git message the window sends names the repository it is
about; the diff base and the review marks stay with the session.

## The git popups

Everything above is a tool window you open, find a tab in, and click. `Alt+` `
is the same verbs on one key: IntelliJ's **Git Operations** popup, numbered.

| | |
|---|---|
| `1` | Commit… — the Local changes tab, cursor in the message box |
| `2` | Commit File… — stages the selected file first |
| `3` | Rollback… — names the file and asks, as discard always does |
| `4` | Show History — the log, scoped to the selected file |
| `5` | Annotate with Git Blame — the blame gutter, for that file |
| `6` | Show Diff |
| `7` | Branches… — the second popup |
| `8` | Push… — **refuses**, and says why |
| `9` `0` | Stash and unstash |

Below the numbers: *Update Project (fetch)*, *Worktrees…*, *Copy Branch Name*
and *Show Local History*, which here means the reflog — mogeung keeps no record
of your edits, and git's own is the nearest true answer.

A digit runs its entry; `↑` `↓` and `Enter` do the same; `Escape` closes. An
entry with nothing to act on — *Rollback* with no file selected — **says so**
rather than doing nothing, and the list stays up, because the answer is usually
the entry below.

`7`, or `Ctrl+Shift+` ` on its own, opens **Branches**: one box that searches
branches *and* actions, the repository and the branch it is on, the branches you
were last looking at, then Local, Remote and Tags grouped on `/`. `Enter` checks
one out — behind the same warning the branch pane gives when an agent is live in
that worktree. Right-click a branch for compare, its log, copy name and
favourite. *New Branch…* makes one from HEAD; *Checkout Tag or Revision…* takes
a tag or a sha and leaves HEAD detached, which is the one place in the window
that does.

Merge, rebase and pull-into are not there, for the reason push is not: this
window writes locally and never publishes.

## On a followed machine

Against a session mirrored from another machine, every read here answers about
that machine's checkout. The writes — staging, commits, branches, stashes,
fetch — cross only when *allow changes* is on for that follow, and that
machine's own token gate still decides ([remote.md](remote.md)).
