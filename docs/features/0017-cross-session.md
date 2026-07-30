---
title: Cross-session intelligence
status: shipped
updated: 2026-07-30
roadmap: [R-F1, R-F2, R-F3, R-F4, R-F5, R-F6, R-F7, R-F8, R-F9]
depends_on: [A1, A4, A22]
---

# 0017 — Cross-session intelligence

Pillar F, built at the 2026-07-29 one-go ask. The raw material measured
on this machine: 149 top-level transcripts plus 86 nested subagent
transcripts across 37 project dirs, and 2,076 prompts in
`history.jsonl` (471 sessions, 54 projects, uniform 5-key schema).

## Spec

### Problem

Everything mogeung knows is per-session. "Where did I solve this
before", "what did all my agents do today", "this error again?" — the
evidence for each sits in 67 MB of transcripts nobody can grep by hand,
and prompt history is write-only.

### Assumptions

A22 (`UNTESTED`) — the mining is being built ahead of proof that its
signal earns a pane; the week judges each view separately and an unused
one is a removal candidate. A1 (`UNTESTED`) — the queue question
underneath. A4 (`AT RISK`).

### Acceptance

- [x] One search box reaches every transcript and all prompt history;
      results say which session, when, and open the transcript at the
      hit (R-F1) — Insight pane, Enter-to-search; a transcript hit
      opens the Transcript pane scrolled to the moment
- [x] Given a file (or a line via the Editor), mogeung lists the
      sessions whose edits touched it, with the prompt that drove each —
      heuristic, and labelled with how it matched (R-F2) — the Insight
      File view over the daemon's `touched_files`/`recent_touches`;
      every row names its match rule, and A8's limits are stated in the
      empty-state text
- [x] A digest view summarises a chosen day from evidence — sessions,
      repos, files touched, tokens — never from assistant self-reports
      (R-F3); per-session verify status lives one pane over in Info,
      deliberately not duplicated
- [x] Error text recurring across ≥2 sessions surfaces as a
      recurring-failure row with the sessions listed (R-F4)
- [x] An analytics view shows sessions/day, tokens/day, per-repo and
      hour-of-day distributions — tokens, never dollars (R-F5); the
      view composes prompt-history analytics with the usage scanner's
      burn tables (which also closes R-G3's open box)
- [x] Near-duplicate prompts cluster into a reuse list, most-reused
      first (R-F6) — click copies the full prompt
- [x] Decision-shaped statements ("decided", "instead of", "because")
      can be skimmed per session and copied out as an ADR skeleton;
      extraction is pattern-based and presented as candidates, never as
      authority (R-F7) — the skeleton labels its raw material
      "pattern-extracted, verify before trusting"
- [x] A session's subagents render as a tree under their parent, from
      the nested `subagents/agent-*.jsonl` files (R-F8) — a collapsible
      section atop the Transcript pane (one level deep, which is all
      the on-disk layout carries)
- [x] From a session-attributed commit in the Git pane, one action opens
      that session's transcript scrolled to the turns nearest the
      commit time (R-F9) — "Open transcript here" on the commit's
      context menu; client-side, since the events are already cached
      (`insight::turns_near` remains the REST twin's engine)

### Explicitly out of scope

- Semantic/embedding search — honest substring/token search first.
- Cross-machine aggregation (see R-I4; local only).
- Any write to `~/.claude`, as everywhere.

## Plan

### Approach

Daemon `insight` module owning a lazily-built index over
`~/.claude/projects/**` (recursive — the naive glob misses subagent
files) and `history.jsonl` (monotonic timestamps → tail-read
incrementally). Search is streamed grep with caps, not an index that
can lie. Aggregates (F3/F4/F5/F6) computed on demand, cached by file
mtime. Wire: one `Insight*` command family with echoes. UI: a new
Insight pane (sixth tab) hosting search/digest/analytics/library
sub-views; F9 rides the existing gitview commit context menu; F8 renders
in the Transcript tab; F2 in the Editor context menu.

### Risks and unknowns

- 67 MB scanned naively per keystroke would stall — debounce, cap
  results, and stream per-file with early exit.
- `history.jsonl` `project` → transcript dir mapping is lossy; go
  history→dir only, and tolerate missing dirs (54 projects vs 37 dirs).

### Test strategy

Index unit tests over synthetic trees (incl. nested subagents);
history-parse tests with both `pastedContents` variants; clustering and
recurrence tests; e2e for the endpoint family.

## Notes

Engine landed 2026-07-29: types in `mogeung-core/src/insight.rs`, pure
functions in `mogeungd/src/insight.rs` (no wire, no UI — every function
takes explicit roots, nothing knows where `~/.claude` is). Surprises and
choices worth keeping:

- **No index after all, not even lazily.** The plan said "lazily-built
  index, cached by mtime"; the engine ships as full streaming read
  passes per call instead. At ~67 MB the pass is a few hundred ms, an
  index can silently lie after a file changes underneath it, and caching
  belongs to the caller. Revisit only if dogfooding shows real stalls.
- **`history()` returns a `History` wrapper, not a bare
  `Vec<HistoryEntry>`** — the malformed-line count needed somewhere
  honest to live (`malformed_lines`), and a schema drift in
  `history.jsonl` should be visible, not swallowed.
- Search hits carry a three-way `source`: `history` (the prompt log),
  `prompt` (a human turn inside a top-level transcript), `transcript`
  (everything else). A subagent file's `user` lines are the parent's
  Task prompts, not a human typing — they classify as `transcript` and
  the whole file attributes to the parent session (directory name).
- Prompt clustering is prefix-key only (first 60 normalised chars): for
  prompts shorter than the key that *is* exact matching, so the spec's
  "exact plus prefix" collapses into one rule.
- The digest excludes subagent `user` lines from `turns` for the same
  reason, but folds their tool calls, touched files and tokens into the
  parent — "what did all my agents do today" wants the work counted,
  not the plumbing double-counted as prompts.
- Error normalisation (`lowercase, digits→#, path tokens→<path>,
  whitespace collapsed`) ships *with* each group (`normalized`), so an
  over-merge is auditable rather than hidden. `tool_result` errors over
  400 chars are dropped — two build logs matching proves nothing.
- Decision patterns match on word boundaries — "undecided" is not a
  decision — and every candidate names the pattern that fired, so no UI
  can present the extraction as more than a regex's opinion (pillar K).
  Expect `because` to be the noisy one; that is a pattern-list edit,
  not a redesign.
- The history→transcript-dir slug mapping is implemented generously
  (every non-alphanumeric char → `-`) and only feeds a
  `has_transcripts` boolean, because the mapping is lossy anyway
  (54 projects vs 37 dirs on this machine).
- Lines over 1 MB are skipped everywhere; one such line is still read
  into memory before being dropped — transient, documented, and cheaper
  than a capped reader nobody would maintain.
- R-F2 (file → sessions that touched it) shipped no engine function on
  purpose: its matching heuristic ("with the prompt that drove each")
  needs designing before coding, and a bad heuristic labelled as a
  feature would violate the honesty rule the rest of this module keeps.

