---
title: The Tasks panel, redesigned — the document is the folder
status: draft
updated: 2026-10-21
roadmap: [R-L12, R-L13, R-L14, R-L15, R-L16, R-L17]
depends_on: [A6, A27]
---

# 0052 — The Tasks panel, redesigned: the document is the folder

## Spec

*Owned by the human. What and why, never how.*

### Problem

Asked 2026-10-21: *"We need a redesign of the Task function, the UI is not
usable and not user friendly at all."* A design critique the same day
(impeccable, two isolated assessments, scored 13/40 — *poor*) found the
reasons, and they are specific rather than aesthetic:

- **A click on a task row can lose a note.** The row opens the note in the
  Notes panel, but the editor's draft is not re-read when the open note
  changes from outside, so the editor shows an empty or earlier body with
  *save* lit and the window flagging unsaved changes nobody made; `Ctrl+S`
  then writes that body over the note and every task in it disappears.
  A 3 px miss on the 12 px tick is enough to get there.
- **Grouping is shown wrongly as soon as there are two notes.** Tasks come
  ordered by note id, so documents sort in an order nobody can predict and
  one that disagrees with the Notes list; groups are cut by heading name
  across the whole stream, so a headingless note's tasks render under the
  previous note's last heading — with the owner's own two notes, *Open this
  panel with Alt+T* reads as a Mogeung task. Meanwhile every row spends a
  line on a sublabel that repeats the note's name.
- **The most frequent action is the smallest target.** Ticks are 12 px,
  twisties 11 px, 15 px apart, inside a 38 px row whose own click does
  something larger. The tick is a button labelled *close*, not a checkbox.
- **Adding is blind and mis-files.** A new task appends under whatever
  heading is last in the Tasks note, lands off-screen with no scroll and no
  highlight, the box is cleared before the daemon answers, and `Alt+T`
  focuses nothing.
- **Six words for one state and a 10 px floor.** done / closed / close /
  reopen / open / fold; "4 of 6" beside "1 of 3 left"; *show 3 done* in the
  list and *3 closed today* in the footer, which are different quantities.
  Every label in the panel is the 10 px step.
- **mogeung's own context is thrown away.** A note carries a session and a
  repository tag, which the Notes panel shows and the Tasks panel drops;
  the add box sends no tag even with a session selected.

Decided 2026-10-21, in the owner's answers: **structure first**; **Tasks
stays a rail tool**, redesigned, rather than a view inside Notes; **all five
issues in one pass**; a new task lands **at the top of the Tasks note, or in
the focused folder**; tasks **know their session** — tagged on add, shown,
and filterable; a note opens **in the Notes panel below**, with the draft
bug fixed.

### Assumptions

- [A6](../product/assumptions.md) — three or four concurrent sessions —
  `SUPPORTED`. The *this session* filter only earns its chip if there is more
  than one session to tell apart.
- [A27](../product/assumptions.md) — notes are written here rather than in
  the editor beside — `AT RISK`. The two real notes that carry tasks are the
  evidence either way; this redesign removes the reasons the panel gave for
  not using it, and `R-L12`'s verdict is a count of notes with tasks a month
  on.

No assumption here is `UNTESTED`.

### What does not change

A task is a `- [ ]` line in a markdown note and nothing else
([ADR-0015](../decisions/0015-markdown-is-the-truth.md)). Ticking rewrites
that one line. Ticking a parent never ticks its children. A done parent with
an open child stays visible. Grouping is the nearest heading; nesting is
indentation, two spaces or four. Fenced code, block quotes and indented code
are not tasks. Notes stay daemon-owned and mirrored one way.

### Acceptance

- [ ] **The document is the folder.** Each note with tasks is a folder named
      by its title, in the order the Notes list uses; its headings are
      sub-folders inside it; tasks above the first heading sit directly under
      the document; two notes never merge, whatever their headings say. With
      the owner's two notes, *Open this panel with Alt+T* sits under *Tasks
      examples*, not under *Mogeung*.
- [ ] A folder for a note tagged with a session shows that note's repository
      as a chip; the per-row note-name line is gone.
- [ ] **A tick is a checkbox.** The control announces as a checkbox with its
      checked state, its hit area is at least 24 × 24 px, it flips at once
      and reverts if the daemon refuses; clicking the text of a row ticks
      nothing and opens nothing.
- [ ] **Opening a note is an explicit act**: an *open in Notes* control on the
      row, shown on hover and focus and reachable with Enter on a focused
      row. The Notes editor then shows that note's body, scrolled to the
      task's line, and *save* is not lit until something is typed. A dirty
      draft in the editor is never replaced silently.
- [ ] **Adding lands where you can see it.** With nothing focused the new
      line goes right under the Tasks note's title, before its first heading;
      with a folder focused it goes to the end of that folder. The row scrolls
      into view and is washed for a moment. The box keeps its text until the
      daemon's broadcast carries the new task. `Alt+T` opens the panel with
      the box focused. The box names where the task will go.
- [ ] **One vocabulary.** *open* and *done* everywhere; the tick's label is
      *mark done* / *mark open*. One footer line: `N open · N done · N closed
      today`, where *N done* is the toggle that shows done rows in place.
      Group headers say *N open* and, while done is shown, *· N done*. No
      text in the panel is smaller than 11 px.
- [ ] A screen reader hears a folder's name and count, whether it is open,
      and a change in the footer's counts.
- [ ] **Tasks know their session.** With a session selected, the add box
      targets the note tagged with that session's repository (creating
      `Tasks — <repo>` tagged so on first use) and says so; *this session*
      filters the panel to notes tagged with the selected session's
      repository, and the footer counts follow the filter.
- [ ] **Closed today is a list.** The footer's count opens the closures since
      midnight with their times, and a done row shown in place carries the
      time it was closed.
- [ ] Empty state: one instruction, in plain words, with the box focused.
      All-done state: the folders say so rather than vanishing.
- [ ] Nothing in `ADR-0015` moves: dropping the derived tables and
      restarting loses history and nothing else, and no code path ticks a
      line the user did not tick.

### Explicitly out of scope

- A second task model: ids, due dates, assignees, priorities, drag to
  reorder. None can be written on the line.
- Editing a task's words in the panel; that is the Notes editor's job.
- Merging tasks across notes, or a task belonging to more than one session.
- Tasks in the Attention list beside a session. A question for after this
  has been used: the chip and the filter are the first step towards it.
- Moving Tasks into the Notes panel. Considered in the critique and declined
  by the owner on 2026-10-21.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

In the order the owner set — structure first — one roadmap row per step,
each usable before the next:

1. **`R-L12` — the document is the folder.** `taskTree.ts` groups by
   `note_id` first, then by heading inside a note; the daemon orders
   `note_tasks` by the note's `updated` desc to match the Notes list, or the
   window sorts by the note it already holds. The row's sublabel goes; the
   folder header carries the title, the repository chip where the note has a
   `repo`, and the count. Tests: the two-notes bleed, same-named headings in
   two notes, a headingless note.
2. **`R-L13` — a tick you can hit, and a row that is not a link.** The
   `Checkbox` primitive (or a restyled button with `role="checkbox"` and
   `aria-checked`) in a 24 px hit box; optimistic flip in the store with
   revert on `Error`; the row's `onClick` removed; an *open in Notes*
   `IconButton` at the row's end, visible on hover/focus, plus Enter on the
   focused row. `NotesTool` resyncs `draft` to the open note when it is not
   dirty, and `openNote` carries the task's `ord` so the editor can scroll
   to the line. The stale-draft test fails on today's code.
3. **`R-L14` — adding lands where you can see it.** `addTask` inserts after
   the title line (or at the end of the focused folder's section); the store
   keeps the draft until a `tasks` broadcast contains the text; the new
   row's key is remembered for a `--state-focus` wash of ~600 ms and
   `scrollIntoView`; `Alt+T` consumes `focusRail` as `ChatTool` does; the box
   shows its destination.
4. **`R-L15` — one vocabulary, one footer line, nothing under 11 px.** Copy
   and `aria-*` throughout; the footer with the toggle; `text-2xs` leaves
   the panel (and the rail's tool header moves to 11 px, which touches
   `Rail.tsx` for every tool — one change, called out).
5. **`R-L16` — tasks know their session.** `addTask` passes the selected
   session's `repo` and tags the note; the destination chip; the *all / this
   session* segmented control filtering by `repo` (the durable tag; session
   ids churn per conversation).
6. **`R-L17` — closed today is a list.** `Tasks` gains
   `closed: Vec<{note_id, text, at}>` from `task_events` since local
   midnight (`#[serde(default)]` so an older window still parses); done rows
   shown in place carry `at`; the footer count opens the list.

A dated amendment to ADR-0015 records the two durable choices: the folder is
the document, and a task's session is its note's tag.

### Files touched

- `desktop/src/ui/tools/TasksTool.tsx`, `desktop/src/lib/taskTree.ts`,
  `desktop/src/lib/notes.ts`, `desktop/src/ui/tools/NotesTool.tsx`,
  `desktop/src/store/index.ts` (optimistic tick, pending add, open-note
  line), `desktop/src/ui/Rail.tsx` (header size), `desktop/src/wire/types.ts`.
- `crates/mogeung-core/src/wire.rs` (`Tasks.closed`),
  `crates/mogeungd/src/store.rs` (closures since, order), `crates/mogeungd/src/api.rs`.
- Tests beside each; `docs/design/the-window.md`, `docs/design/wire-protocol.md`,
  ADR-0015 amendment, this spec's Notes.

### Risks and unknowns

- The rail header size change (`R-L15`) touches every rail tool; it is the
  right fix for the 10 px floor but it is a window-wide change hiding in a
  panel redesign. Called out so it can be split off.
- Ordering by the note's `updated` means a folder moves when its note is
  edited — including when a task in it is ticked. If that reads as jumping,
  pin the order to the note's creation time instead; the spec says *the
  order the Notes list uses*, and the Notes list can move with it.
- "This session" filters by repository, not session id, because a note's
  session tag names one conversation and a repository outlives it. A user
  who expects the exact session may see sibling sessions' notes. The chip
  says *repo*, not *session*, for that reason.

### Test strategy

- `taskTree.test.ts`: two notes with the same heading stay two folders; a
  headingless note's tasks sit under its own name; order follows the given
  note order.
- `TasksTool.test.tsx`: tick has `role="checkbox"` and `aria-checked`; a
  click on the text ticks nothing; *open in Notes* opens the note and the
  editor shows its body (the stale-draft case, failing today); the pending
  add survives a broadcast without it and clears on one with it; the footer
  toggle has `aria-pressed`.
- `notes.test.ts`: insertion before the first heading; insertion at the end
  of a named section; the destination note for a selected session.
- Daemon: `closed` on `Tasks` since local midnight; order of `task_list`.

## Notes

*Filled during implementation. Surprises, dead ends, things the plan got wrong.*

- 2026-10-21. The critique's score and findings are kept under
  `.impeccable/critique/` (gitignored); the mock is a design canvas the owner
  holds. The owner's real corpus at the time of the review: two notes with
  tasks, thirteen tasks, five done, both nesting styles in use.
