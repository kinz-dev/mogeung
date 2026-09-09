---
title: The editor writes under ~/.mogeung/scratch and nowhere else
status: active
updated: 2026-09-09
decided: 2026-09-03
---

# ADR-0035 — The editor writes under `~/.mogeung/scratch` and nowhere else

## Context

[ADR-0019](0019-a-viewer-not-an-editor.md) made the Code pane a viewer,
permanently: Monaco runs read-only, and the daemon has no verb that writes a
worktree file. It held for a month and it is not in question here.

On 2026-09-03 a scratch file was asked for (`R-L5`): IntelliJ's
`Ctrl+Alt+Shift+Insert`, a language, and a file called `scratch-3.java` open
with the cursor in it. That is an editor. The want is the one `R-L4` predicted
would either survive having notes or not, and it survived — a note is
markdown in a rail tool, and what was wanted is a Java file with Java
colouring in the pane where files live.

So there is a real decision: the window's editor is about to write something,
and the line ADR-0019 drew has to be either moved or held. This ADR holds it.

## Decision

**Monaco may write a file under `~/.mogeung/scratch`, and no other path.** The
Code pane's read-only rule stands for every worktree file; a scratch pane is a
different component with a different id, and the difference is enforced by
where the name comes from rather than by a flag in the pane:

1. **The daemon mints every name.** `ScratchCreate { ext }` answers with
   `scratch-<n>.<ext>`, first free `n`, created with `create_new` so two
   windows cannot collide. The window never proposes a name.
2. **A name is a bare file name.** Every verb — read, write, list — checks the
   name before touching the filesystem: no separator, no leading dot, no `..`,
   letters, digits, `-`, `_` and `.` only. A path cannot be spelled.
3. **A write is an edit, never a create.** `ScratchWrite` on a name the daemon
   did not mint is an error, so the write verb cannot be used to place a file
   with a name of the window's choosing even inside the directory.
4. **The daemon does the writing**, atomically, in the directory it already
   owns beside the notes mirror. The window has no local authority
   ([ADR-0001](0001-rust-core-with-egui-ui.md)) and this does not give it
   any: a browser tab and a remote window get exactly the same verb.
5. **Not in the write family.** Like notes, these change the daemon's own
   directory rather than a repository, so they are not behind `may_write`; the
   token layer covers them when the bind is not loopback, as it covers
   everything else.

## Alternatives

- **Make the file pane writable for paths under the scratch directory.** One
  component, one flag. Rejected because the file pane is bound to a session
  and re-reads on focus, which is right for a viewer following an agent and
  wrong for an editor — a reload would put the caret at the top under your
  hands — and because a writable file pane is a flag away from a writable
  worktree, which is the erosion ADR-0019 named.
- **Scratch files as notes.** They already exist, the daemon already owns them,
  the mirror already writes `*.md`. Rejected because the ask was for a file
  with a language, and a note is markdown by definition; and because a note
  lives in a store with a mirror that is never read back, where a scratch file
  has to *be* the file so anything else on the machine can open it.
- **Let the window write through a filesystem plugin.** Rejected for the
  reason `R-B43` rejected it for exports: a general write verb in the webview
  is a different thing from being able to save the one file you are looking
  at, and the daemon is where authority lives.
- **A scratch directory per repository.** Rejected for now: IntelliJ's are
  global to the IDE, the ask named `~/.mogeung/scratch`, and a per-repo
  directory is a worktree write wearing a different name.

## Consequences

- The window gains its first write path in Monaco, and it is a pane that
  cannot name a path. Every argument ADR-0019 makes about worktree files is
  untouched, and a reader of that ADR should find this one beside it.
- Scratch files are plain files with no row anywhere: no store, no mirror, no
  search. Deleting one is `rm`. That is the point — they are the user's, on
  the user's disk, in a folder any tool can open — and it is also the limit.
- A file edited outside mogeung while its pane is open is overwritten by the
  next keystroke in the pane. Autosave has that property everywhere; it is
  stated in the pane's own comment rather than hidden.
- A remote window writes to the **daemon's** scratch folder, not its own
  machine's. Correct — the daemon owns the directory — and worth knowing.

## Revisit if

- A second directory wants the same verb. The next one is not a scratch
  folder, and this ADR does not stretch to cover it; that is ADR-0019's
  question again.
- Scratch files turn out to want a list, a search or a delete in the window.
  Any of those is a sign they are becoming documents, and `R-L2`'s store is
  where documents live.

## Amendment — 2026-09-09

**The *Revisit if* below fired, twice in two days, and the remedy it prescribed
does not fit.** Scratch files were given a list in the window on 2026-09-08
(`R-L6`), and a right-click menu with rename, duplicate and delete on
2026-09-09 (`R-L7`), at *"enhance the scratch path panel with right-click menu
to support all file related operations."*

The condition said any of those *"is a sign they are becoming documents, and
`R-L2`'s store is where documents live."* The first half was right and the
second half turns out to be unusable: **a note is markdown by definition**, and
what these files are for is Java, SQL and JSON with colouring. There is no
migration to offer. So the prediction was a good one that pointed at a door
which is not there, and the honest move is to widen this ADR rather than to
send the user somewhere that cannot hold their file.

**What changed.** Three verbs — `ScratchRename`, `ScratchDelete`,
`ScratchDuplicate` — and the *Consequences* line saying *"deleting one is `rm`.
That is the point… and it is also the limit"* no longer holds. The limit moved.

**What did not change, and is the whole reason this is an amendment rather than
a supersession.** Rules 2, 4 and 5 stand untouched: `check_name` governs every
verb, so no separator, leading dot or `..` can be spelled; the daemon does every
write, atomically, in the directory it owns; and none of this is behind
`may_write`, because it is still not a repository. Rule 3 — *"a write is an
edit, never a create"* — also stands: `ScratchWrite` still refuses a name the
daemon did not mint, and `ScratchDuplicate` mints its own.

**The concession, stated plainly.** Rule 1 said *"the daemon mints every name…
the window never proposes a name."* **`ScratchRename` breaks that**, and it is
the only verb that does. It exists because a file called `scratch-3.java` is a
file you cannot find again, which is the same argument that got these files a
list. The fences it comes with: the target is checked exactly as any other
name, and a target that **already exists is refused rather than replaced** —
`std::fs::rename` would silently destroy it, and silently destroying a file the
user did not name is the one outcome a rename must not have.

**What it costs.** The window can now name a file inside this directory, so the
argument that *"a path cannot be spelled"* is doing more work than it was: it is
now the only thing between a rename and an arbitrary write, where before it was
the second line after minting. That is a smaller margin, and it is why the
name check has its own test asserting `../`, `sub/dir`, a leading dot and a
space are all refused **by rename specifically**.

**A new consequence.** A file can now vanish from under an open pane, which
before took `rm` in another terminal. Every `Scratches` broadcast closes the
panes of files it no longer names, because a pane whose file has gone looks
editable and autosaves into a write the daemon refuses.

## Revisit if — 2026-09-09

The original two conditions are spent. What would move this line again:

- **Scratch files want folders.** A directory inside the scratch directory is a
  path, and every fence here is built on there not being one.
- **Something outside the panel wants to name a file** — a template, an import,
  a "save this transcript as". Rule 1 survived this amendment with one
  exception; a second exception is not an exception any more, and at that point
  the honest question is whether the daemon should mint names at all.
