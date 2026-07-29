---
title: Doc sprawl — inventory, staleness, GC, derived progress
status: in-progress
updated: 2026-07-29
roadmap: [R-H1, R-H2, R-H3, R-H4, R-H5]
depends_on: [A10]
---

# 0022 — Doc sprawl

Pillar H, built at the 2026-07-29 one-go ask. The original stated pain,
untouched across two versions. `scripts/check-docs.sh` and
`scripts/gen-status.sh` are this pillar built for ourselves at toy
scale; their shapes (author-date staleness, evidence-only status) are
deliberately reused.

**A10 is `UNTESTED`, and R-H1 is its test.** An inventory that finds
few stale docs across the watched repos refutes the thesis; one that
finds many supports it. Building the measurement is doing the work the
ledger demands.

## Spec

### Problem

Agent sessions leave `PLAN.md`, `NOTES.md`, `TODO.md` droppings at repo
roots; design docs describe code forty commits gone; nobody knows which
of a repo's markdown files are alive. The complaint that started this
project has no data behind it — or against it.

### Acceptance

- [x] A Docs view per repo inventories every markdown file with a
      classified kind (readme/adr/plan/spec/note/generated/unknown) and
      lifecycle evidence: created, last touched, links in/out (R-H1)
      — the Insight pane's Docs view, per watched repo
- [x] A doc that names code paths whose git history is newer than the
      doc's last edit is flagged stale, with the commits since (R-H2)
- [x] A GC list proposes archive/merge/delete candidates with the
      evidence attached — orphaned, stale, duplicate-titled — and
      **proposes only**: mogeung never touches the files (R-H3)
- [x] Checklist items in plan-shaped docs are bound to evidence where
      possible — a checked box with no commits touching the named paths
      since the doc's creation is flagged "claimed, unevidenced" (R-H4)
- [x] `CLAUDE.md`/`AGENTS.md`/`GEMINI.md` in one repo are compared;
      drift between them is shown side by side; a canonical copy can be
      **copied out** for hand-pasting — mogeung writes nothing (R-H5)
      — copy-out reads the file client-side and says so when a remote
      daemon makes that impossible

### Explicitly out of scope

- Writing, moving, or deleting any file in a watched repo — proposals
  and copy-out only, the ADR-0003 shape applied to docs.
- Semantic staleness (a doc that is wrong but mentions no paths).

## Plan

### Approach

Daemon `docscan` module: walk a repo for `*.md` (bounded depth/size),
classify by name+frontmatter+content shape, extract path references and
checklists, ask git (existing plumbing) for author dates per referenced
path. Wire: `DocScan` request → `DocReport` response, per repo, cached
by HEAD+mtime. UI: Docs sub-view in the Insight pane (repo-scoped, not
session-scoped) with inventory table, staleness and GC lists, and the
instruction-file drift view.

### Test strategy

Classifier unit tests over synthetic files; path-extraction and
checklist tests; staleness tests against a fixture repo built in a
tempdir (the canary.rs pattern); e2e for the endpoint.

## Notes

2026-07-29 — engine built. Types in `crates/mogeung-core/src/docs.rs`
(`DocInventory` and components), scanner in
`crates/mogeungd/src/docscan.rs` (`pub fn scan(repo_root) ->
DocInventory`, no failure mode — it degrades). Later the same day the
integrator added the wire pair (`FetchDocScan` → `DocReport`, plus the
REST twin) and the Insight pane's Docs view; the daemon scans only
repos a watched session lives in — this is not a general filesystem
endpoint.

Choices worth recording:

- **Author dates on both sides of every staleness comparison**, and
  "commits since" is counted by filtering `git log --format=%at` epochs
  in-process rather than trusting `--since`, which filters on the
  committer date — the exact rebase false-alarm `check-docs.sh` already
  documents. The epoch window is capped at 500 per path, so
  `commits_since` is a floor on a pathologically churning path.
- **Doc-to-doc `.md` links do not feed staleness.** They are the link
  graph (orphan detection); counting them as covered code would mark
  half the tree stale every time the roadmap moved. Staleness compares
  a doc against the *non-markdown* repo paths it names — the `covers:`
  shape generalised.
- **A path reference only counts if the path exists in the repo**, and
  it is containment-checked (no absolutes, no `..`, nothing that could
  spell a git flag) before it ever reaches a `git` argv — the `git.rs`
  boundary rule, applied even to paths we extracted ourselves.
- **Docs on fs-mtime fallback get no staleness or plan verdicts.**
  Mixing an mtime with git author dates mixes clocks; a doc git has
  never seen is listed, dated by mtime, and judged on nothing.
- **Plan items naming no path are `verifiable: false`, never a
  verdict** — absence of evidence is not evidence of absence. Item
  continuation lines are joined before path extraction, because this
  repo's own specs wrap their checklist items.
- **GC exempts readme, ADR, instruction and generated kinds from the
  orphan rule** (an ADR is an immutable record; `check-docs.sh` only
  warns on unlinked ADRs), and READMEs from duplicate-title matching.
  Every proposal carries evidence; the engine contains zero write
  paths.
- Caps: 2000 files, 1 MB/file (scan clipped, row kept, `truncated`
  set), 50 staleness refs and 100 links per doc, 200 plan items
  reported, 20 drift samples per side.

Tested against fixture repos built in tempdirs (`git init` + commits
with pinned `GIT_AUTHOR_DATE`, the usage.rs Scratch pattern): 12 unit
tests cover classification evidence, git dates, staleness one-way
triggering, orphans, claimed-unevidenced flipping to evidenced after a
real commit, drift, gitignore awareness, and the no-git fallback.

