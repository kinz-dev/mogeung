---
title: The editor writes under ~/.mogeung/scratch and nowhere else
status: active
updated: 2026-09-03
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
