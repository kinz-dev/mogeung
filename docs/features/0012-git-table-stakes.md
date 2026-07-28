---
title: Git table stakes — commit details, log filtering, file history
status: in-progress
updated: 2026-07-28
roadmap: [R-D12]
depends_on: [A13, A18, A19]
---

# 0012 — Git table stakes

The tier that was deliberately skipped when [feature 0011] was built
"P1 & P2 in one go": full commit details, log filtering, and per-file
history. The features a commercial client's user misses in the first
minute, landed after the deeper tiers only because the ask sequenced them
that way.

[feature 0011]: 0011-git-depth.md

## Spec

### Problem

Three gaps, each a trip to the terminal:

- **A commit is only its subject line.** Agents write long commit bodies —
  often the only honest record of *why* — and the pane cannot show them.
  Neither committer, absolute date, parents, nor a diffstat.
- **The log is only scrollable, not searchable.** "When did anything touch
  the parser?" or "which commits mention R-D10?" means paging 50 at a
  time and reading every subject.
- **A file has no history.** The single most-used git action in an IDE —
  show this file's commits, follow renames — does not exist.

### Assumptions

- **A13** — keyboard-driven. `SUPPORTED`.
- **A18**, **A19** — the pane and its depth are wanted. `SUPPORTED`, both
  still carrying the dogfooding-week caveat; this tier is the part of the
  original ask most likely to survive that week, being table stakes.

### Read-only

Unchanged from [feature 0011]. Everything here reads.

### Acceptance

- [ ] Selecting a commit shows its full message (subject and body),
      author, committer when different, absolute date, clickable parent
      shas, ref decorations, and a files/±lines diffstat
- [ ] A filter over the log narrows it by message text, `author:` and
      `path:` — combined freely, cleared in one gesture, paging still
      works while filtered
- [ ] Filtering by a single path follows renames, so a file's history
      survives the agent moving it
- [ ] The Editor offers "history" on the open file, which lands in the
      Git pane already filtered to that path
- [ ] Everything works over the wire with REST twins; nothing writes

### Explicitly out of scope

- Date-range filtering; `--grep` regex syntax (literal text only —
  regex is a footgun in a filter box).
- A dedicated file-history view. History *is* the filtered log; a
  separate surface waits for evidence the shared one fails.
- Diffing one file across two arbitrary revisions from the history list
  (range diff already covers commit-to-commit).

## Plan

### Approach

Commit details ride the existing `GitShow` answer: one extra `git show
-s --format=…` in the same daemon call, parsed into a `CommitDetail`
carried as an optional field — old clients ignore it, failure degrades to
"no details" rather than no diff. Filtering extends `GitLog` with three
optional fields (`grep`, `author`, `path`), attached to git as
`--grep=…`/`--author=…` with `-i --fixed-strings`, the path after `--`
with the explorer's lexical containment; `--follow` exactly when a path
is set. `GitCommits` echoes the filter so a superseded page is dropped —
the stray rule, third verse. The filter bar parses `author:` and `path:`
prefixes out of one text field; "history" on an Editor tab is that field
pre-filled.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | `CommitDetail`; `GitLog`/`GitCommits` filter fields; `GitCommitDiff.detail` |
| `crates/mogeungd/src/git.rs` | Detail fetch + parse; filtered/followed log; filter-string validation |
| `crates/mogeungd/src/state.rs`, `api.rs` | Plumb-through; REST query params |
| `crates/mogeung-ui/src/gitview.rs` | Detail cache; filter state + restart/stray rules; filter-syntax parser |
| `crates/mogeung-ui/src/app.rs` | Details block above the commit diff; filter bar; Editor "history" button |

### Risks and unknowns

- **`--follow` is only defined for one path** — the daemon enforces one,
  and the UI never offers more.
- **Filter strings are client text handed to git.** Attached with `=` so
  they cannot open a new argument; length-capped and control-characters
  refused anyway.
- **`%B` is multiline** — the detail parser must split fields with
  `splitn` so the body keeps its newlines, and a body containing the
  field separator would corrupt nothing past it (the body is last).

### Test strategy

Daemon: detail parsing (multiline body, empty body, missing committer),
filter validation (flag shapes refused, length cap), follow-arg
construction. UI: filter-syntax parsing (`author:`/`path:`/free text in
any order), filter change restarts the log and drops stray pages, detail
ingest keyed by sha. E2e: the hostile-argument sweep gains filtered-log
calls.

## Notes

**The detail header rides `GitCommitDiff` instead of owning a wire pair.**
One selection already triggers one `GitShow`; a second round-trip for the
header would race it for no benefit. The daemon makes the `-s` call in the
same trip and the header is `Option` — a failure there costs the header,
never the diff. `%B` last plus `splitn(7)` keeps the body's newlines.

**File history fell out of the filter for free.** The plan's separate
history feature reduced to `path:` + `--follow` on the existing log —
same paging, same stray-drop rule, same UI. The only new surface is the
`history` button pre-filling the filter box.

**`--fixed-strings -i` covers `--author` too**, so both filters are
literal with one pair of flags. Filter values are joined with `=` and so
cannot become arguments; validation still refuses control characters and
novels, because "harmless to git" is not the same as "sane".

**The stray rule grew a third verse.** `GitCommits` now echoes rev *and*
all three filters; the client drops a page when any of the four disagree
with its current scope — pinned by a test, like the branch-switch case
before it.
