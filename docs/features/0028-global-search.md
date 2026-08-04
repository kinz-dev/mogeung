---
title: Global search
status: shipped
updated: 2026-08-03
roadmap: [R-F13]
depends_on: [A13, A22, A29]
---

# 0028 — Global search

Asked for 2026-08-03 with a screenshot of RustRover's Find-in-Files tool
window: a query box at the top, results below, grouped, with a preview.

It is the second tool window in the rail that
[feature 0027](0027-right-rail.md) builds, and it exists in the rail rather
than as a tab because a search whose results you act on has to stay open while
you act on them — [ADR-0017](../decisions/0017-the-rail-is-chrome.md)'s rule
for what is chrome.

## Spec

### Problem

Three searches exist, and you have to know where the answer is before you can
go looking for it.

The concrete moment: you remember reading something about the watch root and
cannot remember whether it was in a transcript, in a source file, or in a
session from last week. Today that is three different boxes —

| what you want | where it lives today | scope |
|---|---|---|
| this conversation | the Transcript's find box, `R-B36` | selected session |
| this session's files | the palette's Search mode | selected session |
| every session ever | the Insight tab's Search view, `R-F1` | all sessions |

— with three shapes of result, and choosing the wrong one means the answer
looks like it is not there at all. Each is good. Nothing puts them together.

### Assumptions

- **A13** (`SUPPORTED`) — keyboard-first. A search panel that needs the mouse
  to read its own results has failed this before it starts.
- **A22** (`UNTESTED`) — cross-session mining yields triage-worthy signal.
  `R-F1` shipped on it and it has never been judged; this feature puts its
  results in front of you without your having to go and ask for them, which is
  a fairer test of A22 than the Insight tab was.
- **A29** (new, `UNTESTED`) — *one box over three corpora beats three separate
  searches.* This is the bet, and it is a bet about attention rather than
  about capability: nothing here can find anything the three boxes could not.

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is to
> test the assumption, not to build the feature.

A29 is cheap to test for the same reason it is a weak-looking feature: all
three searches already exist and are already wired. The build is federation
and presentation — no new engine, and (see the gap below) at most one new wire
message.

**Removal condition, agreed in advance:** if the palette stays what you
actually reach for, this panel comes out and the three boxes stay separate.

### Acceptance

- [x] One box, three groups, each labelled with the scope it searched
- [x] Transcript results appear as you type; the two daemon groups run on Enter
      and say so before you press it
- [x] A slow group never delays a fast one — each fills when its answer lands
- [x] Selecting a result previews it without leaving the panel. **Below the
      list, not beside it** — a rail is ~300pt wide, and side by side would
      have been two columns of nothing. The divider is draggable
- [x] Enter on a result opens it where it lives: a file in the Editor at the
      line, a transcript turn in the Transcript at that turn, an Insight hit in
      that session's Transcript at that timestamp. Arrows walk the results
      without leaving the query box; double-click and an `open` button do the
      same by mouse
- [x] An answer to a query you have since retyped never displaces the current
      one
- [x] A group with no hits is distinguishable from a group still running, and
      both from a group nobody has pressed Enter on yet
- [x] The palette's Files and Search modes still behave exactly as before

### Explicitly out of scope

- **Replacing the palette.** Both stay. They do different jobs — the palette is
  for typing and jumping, this panel is for reading results — and IntelliJ,
  which is the reference for all of this, ships both for that reason.
- **The other Insight views.** "Insight" here means `R-F1`'s corpus:
  transcripts and prompt history. Digest, Analytics, Prompts, Failures,
  Decisions and Docs are not searches and are not folded in. `R-F10` is the row
  that covers searching *across* the Insight views.
- **Regex, replace, and any write.** Literal queries, read-only results.
- **Files of sessions other than the selected one.** `SearchContent` is
  per-session and stays that way.
- **Indexing.** Both daemon searches are scans today and remain scans; if that
  is too slow, that is a finding, not a stage of this feature.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

A `SearchPanel` holding the query, a per-group status (`idle` / `running` /
`done`), per-group results, the selection, and the preview. The panel renders
all three group headers immediately and fills each independently — a design
constraint rather than an optimisation, because the three latencies are not
comparable.

**The fan-out**, all of it already built:

| group | how | when |
|---|---|---|
| Transcript, this session | `crate::search::best` over loaded events (`R-B36`) | per keystroke — it is in memory and free |
| Files, this session | `ClientMsg::SearchContent` → `ContentMatch` | on Enter |
| Insight, all sessions | `ClientMsg::InsightSearch` → `SearchHit` | on Enter |

Both daemon messages already echo the query back in their answers, which is
what lets a stale reply be dropped. That discipline exists and gets reused
rather than reinvented.

**The preview differs by group**, honestly and visibly:

- a file hit → `FetchFile` and the existing viewer, scrolled to the line
- a transcript turn → the turn, rendered the way the Transcript renders it
- an Insight hit → the `SearchHit.preview` clip, which is ~200 characters and
  all the daemon returns

**The one gap, named rather than designed around:** an Insight hit has no
context beyond that clip. Getting the surrounding lines needs a new wire
message — roughly `ContextAround { session_id, line, radius }`. Ship without
it. Add it only if the clip proves too thin in use, because a search you can
open in the Transcript in one keystroke may not need a preview at all.

### Files touched

- `crates/mogeung-ui/src/app.rs` — the panel body, the fan-out, and the three
  open-where-it-lives paths (all three already exist as jump targets)
- `crates/mogeung-ui/src/search.rs` — reuse; the transcript matcher and its
  engine-naming live here
- `crates/mogeung-ui/src/explorer.rs` — `SearchState` gets a second consumer;
  see the first risk
- `crates/mogeung-core/src/wire.rs`, `crates/mogeungd/` — **only** if the
  context gap proves real
- `docs/design/architecture.md` — on ship, not before

### Risks and unknowns

- **`SearchState` has one slot per session and the palette owns it.** Two
  searchers writing `st.search` will overwrite each other's results and each
  other's `in_flight` flag. Either the panel gets its own slot or the two share
  deliberately — this must be decided before the code, not discovered by a
  search that keeps going blank.
- **The scopes do not match, and the panel has to say so.** Files and
  Transcript are the selected session; Insight is every session that has ever
  run. Unlabelled, three groups under one box read as one corpus, and the
  results will look wrong rather than look scoped.
- **`R-F1` requires Enter on purpose** — the corpus is tens of MB and a
  per-keystroke scan would punish typing. The screenshot shows a live box.
  Splitting the behaviour (type-ahead for the free group, Enter for the two
  paid ones) reverses no decision, but it must be legible in the hint text or
  it reads as a bug in the two groups that appear not to work.
- **The transcript group can only search what is loaded.** If the Transcript
  pane pages its events, this group's real scope is "what has been read", not
  "this session" — needs checking, and needs labelling if true. A search that
  silently searches less than it claims is worse than one that refuses.
- **Preview fidelity differs by group** and the difference is visible. Better
  than pretending otherwise, but it is the thing most likely to read as
  unfinished.

### Test strategy

- A stale-answer test that would fail today if the drop logic were missing:
  deliver results for query A while the box holds B, assert the panel still
  shows B's state and B's counts.
- Group independence: a still-running group must not blank or delay a
  completed one. This is the constraint most likely to be broken by a later
  refactor and is worth an assertion rather than an eyeball.
- The transcript matcher already has tests in `search.rs`; extend those rather
  than write a parallel set.
- Opening a result is three separate jumps into machinery that already exists
  and is already exercised — checklist, not unit test.

## Notes

Built 2026-08-03, in one pass with [0027](0027-right-rail.md).

**The plan's first risk was real and the fix was better than the plan.** The
palette owns `SearchState`, one slot per session, and the panel needed the same
`SearchContent` answers. The spec said "either the panel gets its own slot or
they share deliberately". It turned out to need neither: both are fed from the
`ContentMatches` arm, each dropping what is not theirs by comparing the echoed
query against its own. No shared mutable slot exists, so there is nothing for
two writers to fight over. The same shape works for `InsightSearchResults`,
where the Insight tab and the panel now both listen.

**Enter had to mean two things, and that needed a rule rather than a guess.**
While the arrows are in the query box, Enter runs the search; once they have
walked into the results, Enter opens the row. Down enters the list, Up off the
top leaves it — deliberately not wrapping, because a wrapping cursor would
leave no keyboard way back to a query you are still refining. That boundary is
`search_move`, and it is tested at both ends.

**Two defects were written and caught before they shipped.** The file preview
called `scroll_to_me` on the matched line every frame, which pins the preview
and puts every other line of the file out of the mouse's reach — the exact trap
the palette's cursor documents. It is now armed by a selection change and
consumed by the paint that has a body to scroll, which may be several frames
later. And pressing Enter surrenders a singleline's focus in egui, so the Down
that walks into the results you just asked for would have gone to the queue
instead; the panel takes its focus back after a run.

**The scope asymmetry needed saying, not solving.** Two groups are the selected
session and one is every session ever. Changing the selection now drops the
session-scoped group rather than re-running it — a click through the queue is
not a search, and re-issuing would be a worktree scan per click.

**The Insight preview gap is still open, as planned.** A cross-session hit
previews as the ~200-char clip the daemon returns, and says so in the pane
rather than looking truncated. Closing it needs a wire message; it was not
built, and nothing in use has yet argued for it.
