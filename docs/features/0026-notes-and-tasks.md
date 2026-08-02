---
title: Notes, documents and tasks
status: draft
updated: 2026-08-02
roadmap: [R-B35, R-L1, R-L2, R-L3]
depends_on: [A1, A6, A27]
---

# 0026 — Notes, documents and tasks

The output of the `R-L1` design session, held 2026-08-02. The decisions it
reached are recorded in
[ADR-0015](../decisions/0015-markdown-is-the-truth.md); this is what gets
built and in what order.

## Spec

### Problem

There is nowhere in mogeung to write anything down.

The concrete moment: you are reading a transcript, notice that the agent
justified a change with a claim you do not believe, and want to record *that*
— against that turn, so it is there when you come back. Today the options are
a terminal, a browser tab, or remembering. All three lose it.

The second moment is larger and vaguer, which is why it is filed second: the
thinking that happens *around* a session — what to try next, what went wrong
last time, the three things to check before merging — has no home either, and
ends up in a scratch file whose name you forget.

### Assumptions

- **A1 / A6** (`UNTESTED`) — the product's own premises. This feature does not
  rest on them any more heavily than the rest of the window does.
- **A27** (new, `UNTESTED`) — *the user will write notes inside mogeung rather
  than in the editor already open beside it.* This is the bet, and it is the
  same shape as `A26` was for git writes: a competing tool is one keystroke
  away and already good at this.

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is to
> test the assumption, not to build the feature.

That is exactly why the order below starts where it does. `R-B35` — a note
against a transcript turn — is the smallest thing that tests A27 honestly,
because it is the one place a note has an obvious home and no competing tool
is closer to hand. If notes are not written *there*, they will not be written
in a dedicated pane either, and `R-L2` should not be built.

**The removal condition, agreed in advance:** if a week of use produces fewer
than a handful of notes, this comes out rather than being decorated.

### Acceptance

- [x] A turn in the Transcript can carry a note, written and edited in place
- [x] Notes are visible without opening each turn — the transcript shows where
      they are
- [ ] A note survives the session ending, being forgotten, and the window
      restarting
- [x] Notes are one set across windows: two windows on one daemon show the same
      note the moment it is saved
- [x] Every note also exists as a `.md` file under `~/.mogeung/notes/`, and
      deleting mogeung's database loses history but not the writing
- [ ] A document can be written, listed, renamed and deleted from a pane
- [ ] A `- [ ]` line in any document appears in a task list, and ticking it in
      either place agrees in both
- [ ] "What did I close today" is answerable, and the answer survives the
      checkbox being ticked and unticked
- [ ] Dropping the derived table and restarting loses the history and nothing
      else

### Explicitly out of scope

- **Editing worktree files.** Pillar K stands; `R-L4` records the question and
  nothing more. A document under `~/.mogeung` is not a repository file.
- **Due dates, assignees, priorities, recurrence.** Not asked for. A task is a
  checkbox; anything more is the project manager ADR-0015 declined.
- **Reading the disk mirror back in.** It is an export, not an input.
  ADR-0015's revisit condition covers the case where that turns out wrong.
- **Sharing, sync, or notes in the repository.** ADR-0015.
- **Rich text.** Markdown, rendered by the previewer `R-B29` already built.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

Four stages. The first is deliberately the smallest useful thing, because it
is also the test of A27.

**`R-B35` — a note on a turn.** A `notes` table in the daemon store, a wire
family (`NoteList` / `NoteSave` / `NoteDelete`), and the Transcript gaining an
affordance per turn. A note is `(id, body, created, updated)` plus optional
`session_id` and `repo` **tags**. This stage builds the whole storage layer and
spends it on one feature, which is what makes the later stages cheap — the same
shape `R-D19` had for the git writes.

**`R-L2` — documents.** A pane listing documents with a markdown editor and the
existing preview. Same table, same wire; a "note on a turn" is simply a
document that carries a `session_id` tag. There is no second model.

**`R-L3` — tasks.** Parse `- [ ]` / `- [x]` out of every document on save,
into a derived table carrying the transitions. The list is a view over that;
ticking a box in the list rewrites the line in the document, which is the only
direction that writes.

**Mirror.** On every save, write `~/.mogeung/notes/<id>-<slug>.md`. One way,
never read back, and stated as such in the pane so nobody edits one expecting
it to take.

### Files touched

- `crates/mogeungd/src/store.rs` — `notes` and `note_tasks` tables, and the
  rebuild path that makes the second droppable
- `crates/mogeungd/src/notes.rs` (new) — parse, mirror, and the task extractor
- `crates/mogeung-core/src/wire.rs` — the note family, and `Note` on the wire
- `crates/mogeungd/src/api.rs` — dispatch, plus REST for grep-ability
- `crates/mogeung-ui/src/app.rs` — the transcript affordance, then the pane
- `docs/design/data-model.md` — the first daemon table that is not derived

### Risks and unknowns

- **A27 is the whole bet**, and the competing tool is better than the one
  competing with `R-D20` was: an editor is already open, already has your
  keybindings, and already holds the file you are talking about. The
  mitigation is the order — test it at `R-B35` scale before building `R-L2` —
  and the removal condition above.
- **Two representations exist**, which this codebase has avoided everywhere
  else. ADR-0015's rebuild rule is the entire defence and needs a test that
  actually exercises it: drop the derived table, restart, assert nothing but
  history is missing.
- **Notes follow the daemon, not the window.** Watching a remote machine shows
  that machine's notes, while pins and labels for the same machine sit in a
  file on the machine the *window* runs on (`R-I11`, one day earlier). They
  agree about scope and disagree about who holds the bytes, so two windows on
  one daemon show the same notes and different pins. Both are defensible and
  the pair will confuse somebody; the guide has to explain them together rather
  than separately. **`R-I12` records the argument that `R-I11` should move**,
  which would remove the asymmetry — filed rather than acted on, because it
  reverses a shipped decision that has not been used yet.
- **Ticking a box rewrites a document under the user.** If the document is open
  in the editor pane at the time, two writers exist. Unknown whether to refuse,
  reload, or merge — and it is the same class of question `R-D21` asked about
  switching branches under a running agent, which was answered by warning
  rather than by guessing.
- **`- [ ]` appears in ordinary prose**, including in any note that quotes this
  spec. A parser that is too eager turns a quotation into a task. Fenced code
  blocks must be excluded at minimum.

## Notes

*Filled during implementation.*
