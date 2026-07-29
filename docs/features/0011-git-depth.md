---
title: Git depth — reading parity with a commercial client
status: shipped
updated: 2026-07-28
roadmap: [R-D11]
depends_on: [A13, A18, A19]
---

# 0011 — Git depth

The Git pane ([feature 0010]) grown to reading parity with a commercial
client's git window — branches, refs, stashes, submodules, a commit graph,
re-blame, file-at-revision, arbitrary-range diffs — while staying read-only
from end to end. The write half of every commercial client (staging,
committing, branching, fetching) remains permanently out.

Asked for as *"plan for a list of features that is on-par with a commercial
product"*, then — offered the list in priority tiers with the observer fence
restated — an explicit *"let's build P1 & P2 in one-go now"*. Like A16's
"make it a single pass", a deliberate one-go commitment, taken over the
staged default.

[feature 0010]: 0010-git-view.md

## Spec

### Problem

The Git pane answers "what commits exist and what did each touch", and
nothing else. Every question one layer deeper still needs a terminal:
*what branch is this session even on? is it ahead of its upstream? who
touched this line before that refactor commit? what did this file look like
before the agent rewrote it? what do these four commits amount to together?
is the agent stuck in a merge conflict right now?* Each is thirty seconds
of `git` in another window — the exact hole the pane was built to close,
one level down. IntelliJ's git window and GitLens answer all of them
without a write verb; the reading half of a commercial client is missing,
not the writing half.

### Assumptions

- **A13** — the user drives by keyboard. `SUPPORTED`.
- **A18** — commit history and annotation are worth a pane of their own.
  `SUPPORTED`; the dogfooding-week caveat stands.
- **A19** — the git pane earns commercial-grade reading depth beyond log
  and status. `SUPPORTED` by the explicit one-go ask; whether the depth is
  *used* is exactly what the dogfooding week tests, and unused sections are
  candidates for removal, not decoration.

### Read-only — restated, because scope grows here

Everything below reads. The daemon gains no verb that stages, commits,
switches, stashes, resolves, or fetches — `git fetch` writes `.git`, so
even "refresh the remote state" stays in the terminal and the pane shows
last-known state only. The fence of [feature 0010] is unchanged.

### Acceptance

- [x] The pane header names the current branch (or detached HEAD), its
      upstream, ahead/behind counts, the remote, and how stale the last
      fetch is
- [x] A branch list shows local branches with their tips; selecting one
      scopes the log to it, without checking anything out
- [x] Log rows carry ref decorations (branch heads, tags, HEAD) and a
      graph column showing branch/merge topology
- [x] A commit the selected session is believed to have produced is marked
      as such in the log
- [x] Two commits can be marked and diffed as a range, rendered by the
      same diff pipeline
- [x] A log row offers copy-sha, copy-subject, and open-on-host (when the
      remote URL is recognisably GitHub/GitLab-shaped)
- [x] Stashes list with their messages and show their diffs; tags list
      with their targets; submodules list with their state
- [x] Conflicted files are unmistakable in local changes, and conflict
      markers stand out in their diffs
- [x] Ignored files are dimmed in the explorer tree and absent from local
      changes
- [x] The blame gutter offers, per line: the commit's summary on hover,
      and re-blame at the parent of that line's commit — walking a line's
      history backwards without leaving the editor
- [x] Any file in a commit's diff can be opened read-only as it was at
      that commit, in the Editor pane
- [x] Everything works over the wire with REST twins; nothing anywhere
      writes to the repository

### Explicitly out of scope

- **Every write verb**, unchanged from [feature 0010] — now explicitly
  including `git fetch`.
- Commit graph *polish* — curved edges, avatar columns, drag-to-compare.
  The first graph is lanes and straight lines.
- Prompt-blame (`R-F2`) and review-state-per-commit (P3) — separate work.
- Multi-repo aggregation; remotes beyond naming them; reflog; bisect.

## Plan

### Approach

Every feature is a new read-only wire pair in the established
`ListDir`-shape (fire-and-forget command, broadcast answer, REST twin) plus
client cache in `gitview.rs` and rendering in the existing pane. The graph
is computed client-side from parent shas already fetched with the log.
Attribution is a daemon-side heuristic: a commit within the session's
lifetime whose files overlap the session's touched set. Re-blame reuses
`GitBlame` with an optional `before` sha; file-at-revision opens as an
Editor tab wearing a revision marker instead of fetching from the worktree.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | New commands/answers; `CommitInfo` gains refs/parents/attribution, `BlameLine` gains summary, `StatusEntry` gains conflicted |
| `crates/mogeungd/src/git.rs` | refs/stashes/submodules/range-diff/file-at-rev/re-blame/scoped-log plumbing + parsers |
| `crates/mogeungd/src/state.rs` | Session-scoped wrappers; attribution heuristic |
| `crates/mogeungd/src/api.rs` | Dispatch + REST twins under `/api/sessions/{id}/git/…` |
| `crates/mogeung-ui/src/gitview.rs` | Cache for refs/stashes/submodules/ranges; lane assignment for the graph |
| `crates/mogeung-ui/src/explorer.rs` | Revision tabs — a tab that shows a file at a sha, never persisted |
| `crates/mogeung-ui/src/app.rs` | Pane header, sections, graph column, context menus, blame hover/re-blame, conflict/ignored rendering |

### Risks and unknowns

- **Graph lane assignment** is the one algorithmic piece; wrong lanes are
  worse than no graph. Pure function, tested on straight lines, one merge,
  criss-cross.
- **Argument hygiene widens**: ref names join shas as client-supplied git
  arguments. Same rule — validate shape before git sees it; a ref name
  must not start with `-` or contain `..`/`@{`.
- **`--name-only` log parsing** rides the `\x1f`/`\x1e` framing next to
  freeform filenames; the parser trusts only the separators and the
  40-hex+`\x1f` line shape.
- **Attribution is a heuristic** and must dress like one — a quiet badge,
  not an author column. False positives are the failure mode that erodes
  trust in the whole pane.

### Test strategy

Daemon: parser tests per new format — for-each-ref branches (tracking
counts, detached HEAD), stash list, submodule status lines, log-with-files
framing, blame summary lines, conflicted/ignored porcelain codes, ref-name
validation (hostile names refused). UI: lane assignment on linear/merge
histories, stray-answer drop and session-switch wipe for every new cache,
range-selection state machine. Rendering is egui's and not re-tested.

## Notes

**`for-each-ref` does not speak `git log`'s escape dialect.** `%x1f` is a
log-format escape; for-each-ref wants bare `%1f`. Found by running both
against a real repo before writing the parser — the alternative was a
parser tested green against output git would never produce.

**`--name-only` framing has a twist worth writing down.** A commit's files
print *after* its format line, so they land at the head of the *next*
`\x1e` chunk. The parser attaches non-field lines to the previous commit,
trusting only the separators and the `\x1f` field-line shape. Merges list
no files under plain `--name-only`, so merge commits simply never earn the
attribution dot — acceptable for a hint.

**Re-blame became "open the file at the parent revision".** The first
sketch re-blamed in place, swapping the gutter under the worktree body —
wrong, because blame of an older era has different line geometry than the
file on screen. Opening a revision tab (body *and* gutter from the same
era) made the alignment problem impossible instead of solved, and the tab
strip became the blame history for free.

**Attribution matches on absolute paths through the repo root**, the same
join the local-changes session filter already used — commit files are
repo-relative, `touched_files` absolute. One minute of slack absorbs
clock skew. Two sessions editing one file both match; A8's limit is
inherited and the dot's hover text says "probably".

**Revision tabs are deliberately unsaved.** A sha in `explorer.json`
would refer to a repo state that may be gc'd, rebased away, or meaningless
by next launch; the save filter also re-points active-tab indices past the
dropped tabs, which a test pins.

**The graph is straight lines by decision, but the lane algorithm still
had to be honest**: lanes are found by expectation (which sha does this
column await), joins collapse duplicate expectations, freed lanes are
reused so disjoint histories do not leak columns rightward. Pinned by
tests on linear, merge and disjoint histories; capped at 8 drawn lanes.
