---
title: Scratch files
status: shipped
updated: 2026-09-08
roadmap: [R-L5]
depends_on: [A27]
---

# 0039 — Scratch files

## Spec

### Problem

There is nowhere in mogeung to type something that is not a worktree file and
not a note. A note (`R-L2`) is markdown in the rail; the thing you reach for
while reading an agent's Java is a Java file — to try a shape, paste a stack
trace, draft a query — and until now the answer was the editor beside mogeung.
Asked 2026-09-03 as IntelliJ's gesture: *"new scratch file shortcut — create a
new scratch file in the mogeung (`~/.mogeung/scratch`) folder"*, first on
`Ctrl+Shift+N` and moved the same day to `Ctrl+Alt+Shift+Insert`, which is
IntelliJ's own.

### Assumptions

[A27](../product/assumptions.md) — the user will write inside mogeung rather
than in the editor already open beside it — is `AT RISK` on a count of two
notes in a week. This is the same bet in a different container, and it is the
one `R-L4` said would tell: a scratch file is what "let me just write this
down" turns into when the thing to write down is code. It is built small so
the count can be taken: a `ls ~/.mogeung/scratch` a fortnight on is the
measurement.

### Acceptance

- [x] `Ctrl+Alt+Shift+Insert` opens a picker of languages; picking one opens a
      new, empty, writable pane named `scratch-<n>.<ext>` with the cursor in it.
- [x] The pane saves as you type, and its header says *saved* or *unsaved*.
- [x] The file is at `~/.mogeung/scratch/<name>` on the daemon's machine, and
      any other tool can open it.
- [x] The picker also lists the scratch files that already exist, newest first.
- [x] A pane left open survives a restart and re-reads its file.
- [x] Every worktree file stays read-only, with the same refusal message.
- [x] A name that is not a bare file name is refused by the daemon.

### Explicitly out of scope

- A list, search or delete in the window. `rm` deletes one; anything more is
  a sign they are becoming documents, and ADR-0035 says where those live.
- Renaming. The name is the daemon's and says the language; that is all it
  says.
- A Mac default chord. A Mac laptop keyboard has no Insert key, so the chord
  there is a rebind, and the Keyboard window says so.

## Plan

### Approach

[ADR-0035](../decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)
is the shape: the daemon mints names, checks every name, and does every
write; the window holds a name and never a path. Four verbs
(`scratch_list`, `scratch_create`, `scratch_read`, `scratch_write`) and three
answers (`scratches`, `scratch_content { fresh }`, `scratch_saved`).

The pane is its own component rather than a writable file pane, and its id
(`scratch:<name>`) is deliberately **kept** by the saved layout where `file:`
ids are stripped: it names nothing that can go stale.

`fresh` is the one bit that opens a pane. The answer to *this window's* create
carries it; a read does not, so a restored pane asking for its body on mount
cannot open a second one, and a second window on the same daemon sees the new
file in its list without a pane appearing on someone else's chord.

### Files touched

| Path | Why |
|---|---|
| `crates/mogeung-core/src/wire.rs` | the four verbs and three answers |
| `crates/mogeungd/src/scratch.rs` | the directory, the name check, create/read/write/list |
| `crates/mogeungd/src/state.rs` | `scratch_dir`, overridable so no test writes to the real folder |
| `crates/mogeungd/src/api.rs` | the handlers; content to the asker, the list to everyone |
| `crates/mogeungd/tests/e2e.rs` | two windows: one creates, the other sees the list and reads |
| `desktop/src/lib/scratch.ts` | languages, pane id, create/open/fetch |
| `desktop/src/panes/ScratchPane.tsx` | the writable editor, debounced save, `Ctrl+S`, flush on close |
| `desktop/src/ui/Palette.tsx` | the `scratch` mode: new by language, or open an existing one |
| `desktop/src/lib/keymap.ts` | `scratch.new` on `Control+Alt+Shift+Insert` |
| `desktop/src/ui/PaneChrome.tsx` | the tab: name, hint, close button, copy path |
| `desktop/src/ui/KeymapWindow.tsx` | the Mac note |
| `desktop/src/store/index.ts` | the `scratch` slice and the three answers |

### Risks and unknowns

- **Autosave overwrites an outside edit.** The pane never pushes the daemon's
  copy back over the body being typed, so a file edited in another editor
  while its pane is open loses to the next keystroke here. Stated in the pane.
- **A remote window writes to the daemon's disk.** By design and by ADR-0001;
  surprising the first time.

### Test strategy

The property that matters is *only a fresh answer opens a pane*, and it is
tested at both ends: the e2e test asserts the second window gets the list and
not the content, and the store test asserts a `fresh: false` opens nothing.
The pane's tests pin one write per pause, flush on close, *saved* only when
the acknowledged body is the current one, and `Ctrl+S`. The daemon's unit
tests pin the name check, first-free numbering, and that a write cannot mint.

## Notes

- Monaco is **uncontrolled** here (`defaultValue`), where the file pane is
  controlled. The library calls `setValue` unconditionally on a read-only
  editor and that is right for a viewer following a file; for an editor it
  would put the caret at the top on every acknowledged save.
- The first pane test mock called `onMount` on every render, which reset what
  the pane believed it had last sent and made flush-on-close send nothing. The
  real editor mounts once; the mock now does too. Worth knowing for the next
  editor test.
- `Dim` does not forward `data-*` attributes, so the status is a plain span.
- **Verdict 2026-09-08: kept, on the decision and not on a count.** `R-L5` is
  closed ✅. The measurement this spec named — `ls ~/.mogeung/scratch` a
  fortnight on, due **2026-09-17** — has **not** been taken, and the empty
  directory on the Linux machine is not it: a scratch file is written on **the
  daemon's machine**, and the reports behind `R-J87`–`R-J89` put mogeung on
  macOS. Read it there. [A27](../product/assumptions.md) stays `AT RISK` and
  deliberately did not move with the row.
- **The keymap stole bare letters from this editor for five days**, and
  `keymap.ts` had predicted it in a comment: *"if a Monaco here ever becomes
  editable, this has to become a per-editor check."* This feature made one
  editable and did not go back for it. Reported 2026-09-08 as not being able to
  type `j` — `queue.next`. Fixed twice over: `j`/`k` removed as bindings at the
  user's ask, and `focusOwns` now gives a writable editor **every** bare key, so
  `[` is safe in a Java or JSON scratch file too. The pane says what it is with
  `data-editor="writable"`; Monaco's own DOM is not asked.
- **The lesson worth keeping is about the comment, not the keymap.** A note
  saying *"if X happens, do Y"* is only as good as the person who does X
  noticing it, and nothing in the checks would ever have failed. What would have
  caught it is a test asserting a bare key survives a writable editor — which
  now exists, and fails without the fix.
- **The "no list in the window" scope was reopened on 2026-09-08**, by one
  third and on purpose. `R-L6` adds a rail panel that lists scratch files and
  opens them — and deliberately no rename, no delete and no search, which are
  the parts of this spec's exclusion that were about *managing* documents
  rather than reaching them. The argument that held: a file you made yesterday
  should not be unreachable without remembering a chord.
