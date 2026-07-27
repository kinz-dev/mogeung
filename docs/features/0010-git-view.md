---
title: Git view
status: in-progress
updated: 2026-07-27
roadmap: [R-D10]
depends_on: [A13, A18]
---

# 0010 — Git view

A git presence inside mogeung, IntelliJ-changelist-shaped: recent commits,
the uncommitted change list, per-commit diffs, and per-line annotation.
Read-only from end to end — mogeung observes the repo the way it observes
the agent.

Asked for as *"create a new tasks to introduce a GIT features view"*, with
the presentation explicitly left open — *"I don't have a good idea how
should we show it"*. The spec proposed a shape, the user settled the open
questions the same day, and it was built.

## Spec

### Problem

A session's work stops being visible the moment the agent commits it. The
Changes tab diffs against the session's pinned base, which answers "what has
this session done *lately*" — but reviewing an agent that commits as it goes
is commit-shaped work: which commits happened, what each one touched, and
who (which session, which prompt) a surviving line came from. Today all of
that means leaving mogeung for a terminal or an IDE, for what is usually
thirty seconds of looking — the same hole the Editor pane closed for file
reading.

Asked for directly, features named: recent commits · the uncommitted change
list · file annotation · diffs of the files in a commit — *"reference the
intellij's changelist (git) for the concepts"*.

### Assumptions

- **A13** — the user drives by keyboard. `SUPPORTED`.
- **A18** — commit history and annotation are worth a pane of their own,
  beyond the Changes tab's session diff. `SUPPORTED` by the direct ask; the
  standing caveat applies, and the dogfooding week is the real test.

### Read-only — the observer rule extends to the repo

The daemon never mutates a worktree or a repository: no staging, no commit,
no checkout, no branch switching, no stash. This is [ADR-0003]'s "observe,
do not spawn" applied to git — mogeung telling the repo what to do is the
same trap as mogeung telling the agent what to do, one layer down. IntelliJ's
changelists are a *staging* device; what we borrow from them is the reading
layout, not the write verbs. Acting on the repo stays in the terminal and
the real editor, where it already lives.

### Proposed shape (open to challenge)

A seventh detail pane, **Git**, dockable like every pane — which answers the
"maybe a bottom panel" instinct: dock it to the bottom and it *is* a bottom
panel, and a saved layout keeps it there ([feature 0006]). No new layout
machinery.

Inside the pane, the IntelliJ reading layout:

- **Left, top — Local changes**: the uncommitted files of the session's
  repo (unstaged and staged both, marked as such), each opening its diff.
  This is `git status` for the whole repo — deliberately wider than the
  Changes tab, which shows only what *this session* is believed to have
  touched. The two answer different questions and cross-link.
- **Left, bottom — Log**: recent commits (subject, author, relative time,
  short sha), newest first, "show more" paging. Selecting one lists its
  files.
- **Right — the diff** of whatever is selected on the left, rendered by the
  same diff view the Changes tab uses (hunks, syntax, side-by-side — all of
  pillar D comes free).
- **Annotation lives in the Editor pane, not here**: a blame gutter toggle
  on the open file (`git blame` per line — short sha, author, age), the way
  IntelliJ's Annotate decorates the editor rather than the git window.
  Clicking an annotated line selects that commit in the Git pane. This is
  the natural mogeung twist: blame connects a line to a *commit*, and
  `R-F2` prompt-blame later connects the commit to a *session and prompt* —
  this pane is deliberately a stepping stone to that.

### Acceptance (provisional until the open questions settle)

- [ ] A Git pane exists, reachable by key and palette, dockable anywhere
      including the bottom
- [ ] It lists recent commits with subject, author, time and sha, pages
      further back on demand, and shows a selected commit's files and diffs
- [ ] It lists uncommitted changes (staged and unstaged, distinguished) and
      shows their diffs against HEAD
- [ ] The Editor pane can toggle per-line annotation for the open file, and
      an annotated line links to its commit in the Git pane
- [ ] Everything works over the wire (commands + REST twins); the web
      client could grow the same view unchanged
- [ ] Nothing anywhere writes to the repository

### Explicitly out of scope

- **Any write operation** — staging, committing, reverting, branching. See
  above; permanently, unless a future ADR argues otherwise.
- IntelliJ's changelist *grouping* (named changelists you assign files to).
  It is a staging concept; revisit only if reading-side grouping (by
  session attribution, say) proves wanted.
- Multi-repo aggregation; the pane shows the selected session's repo.
- Push/pull/fetch status, remotes, stashes, tags — log and status first.

### The open questions, answered (2026-07-27)

1. **Placement** — seventh tab, docking left to the user. *"OK."*
2. **Log depth** — 50 commits with paging. *"ok for now"*; since-base and
   toggles wait for evidence of want.
3. **Annotation** — Editor-gutter only, and confirmed session-scoped: the
   whole pane follows the selected session, like every detail pane. *"ok
   but that will be changes based on the claude session I click"* — yes.
4. **Local changes** — the whole repo, with a "this session" filter. *"ok."*

## Plan

The daemon already shells out to `git` (`mogeungd/src/git.rs`) and already
resolves each session's repo root; this extends that file, not a new
dependency. Five wire pairs, all in the `ListDir` fire-and-forget shape
with REST twins under `/api/sessions/{id}/git/…`:
`GitLog { session_id, skip, limit }` → `GitCommits` (one row past the limit
is fetched so "history ended" costs no second call) ·
`GitShow { session_id, sha }` → `GitCommitDiff`, parsed by the *existing*
`parse_unified` into `FileChange`/`Hunk` — so risk flags and the pillar-D
renderer come free · `GitStatus` → `GitLocalChanges` (porcelain v1, parsed
leniently) · `GitDiffFile` → `GitFileDiff` (`HEAD` for tracked files,
`/dev/null` for untracked) · `GitBlame` → `GitAnnotation` (porcelain blame,
capped at 20k lines, commit details remembered across porcelain's
repeat-elision). All run on the blocking pool. Client shas are validated as
hex before git ever sees them — an unauthenticated daemon must not accept a
"sha" that parses as a flag. `Tab::Git` joined `ALL_PANES` (now seven);
layouts saved before it simply lack it, which `layout::focus` handles. A
non-repo session gets "not a git repository" in the pane, not an error.

The pane: local changes and log stacked on the left, the selected diff on
the right, rendered by `render_unified` — the Changes tab's own line
renderer. The blame gutter rides the Editor's galley geometry (`R-B25`'s
measured rows), and clicking an annotated line selects that commit in the
Git pane. Keys: `V` shows the pane, `Alt+B` toggles annotation.

### Risks and unknowns

- **`git blame` is slow on big files** — porcelain blame of a 5k-line file
  can take real time; it must run off the event loop (the `list_tree`
  precedent) and cache per (path, HEAD).
- **The seventh pane crowds the strip** — the tab strip and `ALL_PANES`
  were sized by eye for six; worth a look before committing to an eighth.
- **Scope creep toward a git client** is the real danger; the out-of-scope
  list is the fence, and every "just add staging" impulse goes through an
  ADR or dies.

### Test strategy

Daemon: parser tests over log framing (hostile subjects survive because
only the `\x1f`/`\x1e` separators are trusted), porcelain status codes
including renames, porcelain blame's repeat-elision, and sha validation.
UI (`gitview.rs`): stray-answer drop, session-switch wipe, log paging
appends exactly once (duplicates dropped, page zero replaces), refresh
keeps the selection, coarse ages never go negative. Rendering is egui's
and not re-tested.

## Notes

**`parse_unified` paid for this feature.** A commit's patch and an
uncommitted file's diff both land in the same `FileChange`/`Hunk` shapes
the Changes tab uses, so risk flags, syntax colouring and word-diff came
along without a line of new rendering logic — the Git pane's right side is
`render_unified`, verbatim.

**Argument hygiene is part of read-only.** The daemon takes shas and paths
from an unauthenticated socket; a "sha" like `--output=…` must die before
`git` sees it. Shas are validated as pure hex, paths go through the same
containment guard as the explorer, with one honest wrinkle: a *deleted*
file cannot canonicalise, so `git_diff_file` falls back to a lexical
`..`/absolute check rather than refusing to show exactly the diff a
deletion is.

**Blame is worktree blame.** Uncommitted lines arrive as git's zero sha
and render as a quiet dot — the daemon passes git's answer through
unedited, and renaming "Not Committed Yet" is the client's editorial
decision. The gutter clicks resolve through the code galley's measured
rows, the same geometry the find bands and scroll-to-line use, so all
three agree by construction.
