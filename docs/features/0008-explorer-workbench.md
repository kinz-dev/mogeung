---
title: Explorer workbench
status: shipped
updated: 2026-07-28
roadmap: [R-B25]
depends_on: [A13, A15, A16]
---

# 0008 — Explorer workbench

Grow the explorer pane ([feature 0007](0007-file-explorer.md), `R-B24`) from a
single-file viewer into something closer to an IDE's file workbench: it
remembers where you were, reveals the file on display, holds several files open
at once, and finds a file by name or by content. One pass, one unit of work.

Still a viewer. Nothing here writes.

## Spec

### Problem

Three moments where the shipped pane fails the review loop it was built for:

- **It forgets.** Switch sessions and back, and the tree is collapsed, the
  file gone. A review that touches two sessions pays the navigation cost every
  time it crosses.
- **One file at a time.** Judging an edit usually means reading two or three
  files — the changed one, its caller, the trait it implements. Each open
  evicts the last, so comparing means re-navigating the tree from memory.
- **Navigation is the tree or nothing.** Finding `handler.rs` in a repo you
  did not write means expanding directories by guesswork. And "which file
  defines this constant" — the question that sent you to the explorer in the
  first place — cannot be asked at all.

Asked for directly:

> 1. It should remember where it was and be able to show the open file.
> 2. It should be a flexible open file panel, multiple file can be open at the
>    same time. More like a real intellij editor.
> 3. Support file name search, file content search, etc

A staged version of this spec proposed shipping the cheap parts first and
gating the rest on observed use. That was put to the user and **explicitly
declined** — *"make it a single pass"* — which is a product decision, recorded
here and in [A16](../product/assumptions.md), not a drift.

### Assumptions

- **A13** — the user drives by keyboard. `SUPPORTED`.
- **A15** — reading worktree files inside mogeung is worth a pane. `SUPPORTED`,
  by request only.
- **A16** — the explorer earns workbench affordances. `SUPPORTED` by two
  direct asks in one day, with A15's standing caveat: asking is weaker
  evidence than using. The dogfooding week judges the whole pane at once.

### Acceptance

Remember and reveal:

- [x] Switching away from a session and back shows the explorer exactly as it
      was left: same directories expanded, same tabs open, same active file
- [x] Restarting mogeung restores expanded directories and open tabs per
      session; content is re-fetched, never stored
- [x] When a file opens, every ancestor directory expands and the tree scrolls
      to make the file's row visible
- [x] A file named in the Changes tab can be opened in the Explorer with one
      action, which focuses the pane with that file revealed

Side by side (added 2026-07-27, asked for after first use — *"I can't move
them dockable side-by-side"*):

- [x] A tab can be sent to the other side of a resizable split — context
      menu on the tab, or `Alt+S` for the active one — and moving the last
      right-hand tab back collapses the split
- [x] Each side keeps its own active tab and its own preview slot; keyboard
      tab actions follow the side last touched

Multiple open files:

- [x] Files open into tabs inside the pane; switching tabs is instant, with
      no re-fetch
- [x] A single click previews (the next single click replaces the tab); an
      explicit action pins, and pinned tabs are never replaced
- [x] Next-tab, previous-tab and close-tab are bindable keys, and a
      recent-files switcher lists open files most-recent-first

Find by name:

- [x] Typing a partial name into a palette mode filters the session's files
      as-you-type and opens the pick on `enter`
- [x] Ignored trees (`node_modules`, build output) do not drown the results

Find in the open file (added 2026-07-27, asked for after first use —
*"Add the \<ctrl+f\> search in open files"*):

- [x] `Ctrl+F` in the viewer opens a find bar; matches highlight as line
      bands, `⏎`/`Shift+⏎` walk them with a count shown, `esc` closes
- [x] The search never touches the wire — the body is already client-side

Find by content:

- [x] A query returns matching lines across the worktree as `path:line text`,
      capped and saying so when capped
- [x] Picking a result opens the file scrolled to the matching line — which
      brings **line numbers** into the viewer, explicitly reversing 0007's
      exclusion for exactly this reason
- [x] Binary files are skipped silently; ignored trees are skipped; the
      daemon refuses paths outside the session root, same as `ListDir`

Everywhere:

- [x] Nothing anywhere in the feature writes to the worktree
- [x] All listing, walking and searching happens in the daemon; clients stay
      projections ([ADR-0001])

### Explicitly out of scope

- **Editing. Permanently.** Unchanged from 0007 and pillar K.
- Go-to-definition, symbol search, semantic anything. Text search only.
- Search across *sessions* — that is `R-F1`, a different feature with a
  different data source.
- Watching open files for changes; a stale tab is refreshed by re-opening.
- Regex search syntax in v1. Literal substring; widen only if asked.

## Plan

*Drafted and approved 2026-07-27; implemented the same day. One pass; the
order below was build sequencing inside that pass, not gates.*

### Approach

**Client state first.** `Explorer` stops being one flat cache that
`ensure_session` wipes and becomes a map of per-session state. Open files
become `open: Vec<FileTab>` (path, pinned flag, fetched body) plus an active
index; preview semantics live entirely in the open path — an unpinned tab is
replaced by the next single-click open, a pinned one never. Unpinned tabs are
LRU-capped, since bodies run up to 256 KiB each. Listings and bodies stay
evictable; what persists is only shape. A small
`{session → expanded, open paths, pins, active}` file lands in
`~/.mogeung/explorer.json` with the layout's degrade rule: any read failure
yields the default, a warning, and never a blocked launch.

**Reveal** is a pure function — split the active path, insert ancestors into
`expanded`, request missing listings, scroll to the row. The Changes-tab
bridge reuses `layout::focus` to raise the pane with the file opened pinned.

**Two new wire pairs**, both in the `ListDir` shape, both with REST twins to
stay curl-able:

- `ListTree { session_id }` → `TreeListing { session_id, paths, truncated }`.
  The daemon walks with the `ignore` crate (ripgrep's walker — `.gitignore`
  respected when the repo root is known, plain walk minus `.git` otherwise),
  capped at ~20k paths with the flag set. The client fuzzy-filters locally in
  a new palette mode, so keystrokes cost nothing on the wire.
- `SearchContent { session_id, query }` →
  `ContentMatches { session_id, query, matches, truncated }`, a match being
  `{path, line, text}`, capped at a few hundred. Built on the `grep` +
  `ignore` crates; binary detection and root containment identical to
  `FetchFile`. The query is echoed in the event so a superseded search is
  dropped the same way stray-session answers already are.

**The viewer gains line numbers and scroll-to-line**, needed by search
results. `code_view_ui` does not expose match geometry, so this is prototyped
first — see risks — with a stated fallback.

**Keymap** gains next/prev/close-tab, the MRU switcher, and the Changes-tab
bridge; the palette gains go-to-file and content-search modes rather than the
pane growing its own search box (A13: palette before menu).

Suggested build order, dependency-driven: per-session state and persistence →
reveal + Changes bridge → tabs with preview/pin → `ListTree` + go-to-file →
line numbers/scroll prototype → `SearchContent` + results UI.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | `ListTree`/`SearchContent` commands, `TreeListing`/`ContentMatches` events |
| `crates/mogeungd/src/state.rs` | gitignore-aware walk, content search, caps; containment reused |
| `crates/mogeungd/src/api.rs` | dispatch both commands; REST `tree` and `search` routes |
| `crates/mogeungd/Cargo.toml` | `ignore`, `grep` |
| `crates/mogeung-ui/src/explorer.rs` | per-session state map, tabs, LRU, reveal, persistence |
| `crates/mogeung-ui/src/app.rs` | tab strip, scroll-to-row, line numbers, search results, Changes bridge |
| `crates/mogeung-ui/src/palette.rs` | go-to-file and content-search modes |
| `crates/mogeung-ui/src/keymap.rs` | tab cycling, MRU switcher, bridge action |

### Risks and unknowns

- **Scroll-to-line in the viewer** fights `code_view_ui`'s opaque layout. If
  it cannot be done cleanly, search results open the file at the top with the
  match count shown — degraded but honest — and the acceptance box stays
  unchecked until solved. Prototype before building the search UI on top.
- **One pass is a bigger bet.** The staged plan would have bought evidence
  before the daemon work; declined deliberately (see Spec). The mitigation
  left is the build order above — the cheap client-side wins land first, so
  an interrupted pass still leaves the pane better than it started.
- **`ListTree` on a monorepo** can exceed any cap. The `truncated` flag plus
  gitignore-aware walking makes this survivable; go-to-file over a truncated
  tree finds most things, and says the tree was cut.
- **Persisted paths go stale** — a remembered file deleted since last run.
  The existing `FetchFile` refusal path already renders "cannot read" in the
  pane, so restore inherits the degrade behaviour for free.
- **Search cost on the daemon.** A worktree grep is bounded by the walker's
  ignore rules and the match cap, but a pathological query on a huge repo is
  real work; the command is fire-and-forget, so the daemon must not block its
  event loop — search runs off the hot path like the scanners do.

### Test strategy

Daemon: walker cap and `.git`/gitignore exclusion, containment guard on both
new commands (same table as `ListDir`), binary skip, match cap with
`truncated` set, non-repo fallback walk. UI: per-session isolation (state for
session `a` untouched by opens in `b` — extends the existing stray tests),
reveal expands exactly the ancestor chain, preview-replace vs. pinned-survive,
LRU eviction, MRU order, stale-query drop, persistence round-trip and a
corrupt file falling back with a warning. Fuzzy scoring gets pure unit tests.
Rendering stays untested, as ever.

## Notes

**The `grep` crate was not needed.** The plan named `grep` + `ignore`; what
shipped uses `ignore` alone. A literal-substring scan over walker output is a
dozen lines, and skipping the regex engine also skips having to escape the
user's query into a pattern — a whole class of "why did `a.b` match `axb`"
bugs that never got the chance to exist. Smart case (uppercase in the query
means case-sensitive) came along because it is what the hands expect from
ripgrep and costs one `chars().any()`.

**`code_view_ui` turned out to be two lines**, `highlight` + a selectable
`Label`, so the line-number gutter did not have to fight anything: the pane
calls `highlight` itself and lays the gutter label and the code label side by
side. The plan's fallback (open at the top, say the count) was not needed.

**Guessed geometry drifted; the galley is the only honest source.** The
first version computed line positions as `line × text_style_height` and
gave the gutter its own monospace font. Live use caught it at once: the
find bands crept away from their lines, because syntect's real row height
is not the style's monospace height, and a per-line error accumulates.
The fix is to lay the highlight job out once (`fonts.layout_job`, cached,
so the `Label` repays it) and read `galley.rows[n].pos.y` for bands and
scroll-to-line alike — measured, not multiplied. The gutter now wears the
code job's own `FontId` for the same reason: two fonts is two row heights
is the same drift wearing a different hat.

**One preview slot, not an LRU of tabs.** The plan said "unpinned tabs are
LRU-capped"; the invariant that actually holds is *at most one unpinned tab*,
because every preview open reuses it — which is the IntelliJ behaviour the
request asked for. The memory cap moved to where the memory is: bodies. Tabs
are unlimited and weightless; only the 8 most recently used keep their
fetched content, and an evicted tab re-fetches on activation.

**All fetching goes through one door.** The pane's paint asks for whatever
the state wants and lacks — the root, any expanded directory without a
listing, the active tab without a body. Restore-from-disk, reveal, the
refresh button and a plain click are all the same code path, which is why
persistence never stores content: a restored session simply *wants* things,
and the door fetches them.

**The recent-files switcher merged into go-to-file.** An empty query in the
Files palette lists the open tabs MRU-first; typing searches the whole tree.
One surface, two questions, zero new windows — `Ctrl+E` and `Ctrl+P` open the
same mode. Content search is the third palette mode (`Ctrl+Shift+F`), where
Enter is two things in sequence: run the query, then open the picked hit.
Search results and go-to-file picks open *pinned* — a file asked for by name
was not browsed past.

**Stray-answer routing got better, not just preserved.** With per-session
state, an answer for a session you have left lands in *that* session's cache,
where it is not a stray at all; only answers for sessions never shown are
dropped. The old wipe-on-switch tests were rewritten to pin the new contract.

**Ctrl+F arrived as a follow-up, and cost what the brainstorm predicted:
almost nothing.** The 0007-era worry — "scrolling to the match inside
`code_view_ui` is the hard part" — had already been paid for by search
results (goto machinery) and line numbers (row geometry — see the drift
note above). The one honest compromise: matches highlight as whole-line
bands, not per-column boxes, because column positions lie the moment a tab
or a CJK glyph appears, and a band that is always right beats a box that
is sometimes wrong. Smart case matches the worktree search, deliberately —
two search surfaces with two casing rules would be a bug report.

**The pane now displays as "Editor"** — asked for by name, 2026-07-27. The
rename is skin-deep on purpose: `Tab::Explorer` and the action variants are
serialized into every saved `layout.json` and `keymap.json`, so the
identifiers stay and only the labels moved. (The Terminal tab later took the
other route when it became "Agent" — identifiers renamed, on-disk names pinned
with `#[serde(rename)]`; see [0003](0003-attached-terminal.md). Worth it there
because another pane wanted the vacated word, which is not true here.)
What did *not* move is the boundary: the pane has no write path, and pillar
K still says permanently. The name is what the user calls the pane; the
protocol is what it can do.

**The split is a `group` bit, not a layout engine.** Nesting `egui_tiles`
inside a pane that already lives in `egui_tiles` was the obvious and wrong
design; two groups cover the actual want — two files side by side — with
one field on the tab. Focus follows activation, each side has its own
preview slot (browsing right must not eat the preview being read left),
the side that loses its active tab falls back by attention (MRU), not
strip order, and the split collapses when the last right-hand tab leaves.
Old state files load unchanged: a missing `group` is the left side.

**Session state on disk merges, never replaces.** Saving writes the sessions
this run touched over the ones it did not, so a week of single-session days
does not amnesia the rest. A session whose shape emptied out is removed —
the file stays a record of state, not of history.
