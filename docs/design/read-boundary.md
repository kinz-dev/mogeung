---
title: The read boundary
status: active
updated: 2026-10-08
covers:
  - crates/mogeungd/src/state.rs
  - crates/mogeungd/src/workspace.rs
---

# The read boundary

What an open socket may read out of this machine, and the one rule under all of
it: **a path is served only from a root somebody authorised, and the daemon
proves containment on the filesystem, not on the string.** The daemon binds
loopback with no token, so an unauthenticated port must not become "read any
file by asking politely". There is no write path to a worktree at all; the
roadmap's *an editor — explicitly not* is a property of the protocol
([ADR-0019](../decisions/0019-a-viewer-not-an-editor.md)).

## A session's own root

The file explorer (`R-B24`) lists and reads files under a session's *own* root
— the repository when known, the `cwd` otherwise. Every path is canonicalised
and refused if it escapes the root, symlinks included. The workbench
(`R-B25`) widened the surface, not the rule: the daemon also walks the whole
tree (for go-to-file) and greps it (for content search), under the same
containment, on the blocking pool so a monorepo cannot wedge the event loop.
`.git` is never listed, gitignore is honoured when the root is a repository,
a file is capped at 256 KiB with a `truncated` flag, binary files are refused,
and search skips unreadable files silently because a search that errors on
one file answers nothing.

What the client keeps — open tabs, expanded directories, pins — is view
state, not authority: file bodies are never persisted and every restore
re-asks the daemon.

## More than one root: the workspace (`R-J40`)

A session can be read through extra folders the user added by hand. They are
kept in `~/.mogeung/workspaces.json` under the session's *repository* so they
outlive it, and `session_roots` is the set every read is checked against. A
relative path still means the session's own root; an absolute one names
itself and is checked against that whitelist. What a client may name is
therefore exactly what somebody authorised through the UI.

Adding or removing a folder is gated with the git write verbs — not because
it writes a repository, but because it widens what this daemon will read out,
and a window dialled in from another machine must not be able to extend the
read surface of the daemon it is watching
([wire-protocol.md](wire-protocol.md#git-writes-and-fetch)).

**Hints are offered, never added** (`R-J39`). The workspace answer also
carries folders mogeung has *seen* the session working in — a `/add-dir` the
CLI confirmed, or a repository it wrote files into — each shown under the tree
with a `+` and a `✕`. Every channel behind a hint is retrospective, so it can
only say where an agent has already been; that makes it a shortcut for a
click and not a basis for a read boundary. Nothing widens until somebody
presses `+`, and a dismissal is a client preference keyed by the same
repository root. The inference is deliberately narrow, and the shape came
from the corpus: of the folders sessions wrote into outside their own root,
every one that was a git repository was a real sibling project and every one
that was not was the harness talking to itself — the agent's memory
directory, a per-session scratchpad, a loose dotfile that rolled up to
`$HOME`. So an edit only suggests a repository, `~/.claude` is refused through
either channel, and a folder wide enough to contain your home directory is
never offered.

**A file dropped on the window is the second gesture that widens it**
(`R-J94`). A drop whose path falls under no root adds the file's folder to the
selected session's workspace, and only then asks for the file. The boundary is
unchanged and so is the rule that only a human widens it — a drag is a hand
saying *this file* — but the act is easy to perform without noticing, so the
window says what it added and where to remove it. The `+` and the drop are the
only two ways in; nothing in the daemon adds a root on its own.

## The Git window's repository is the same fence (`R-D34`)

A git command may name a repository, and the set it is checked against is
`session_roots` — the same one every file read uses. So a repository can be
picked exactly when its folder was already readable, and the daemon refuses
anything else in words: *not in this session's workspace — add the folder in
Files first* for a folder outside the set, *not a git repository* for one
inside it that is not a checkout. The workspace answer says which roots
**are** checkouts (`repos`, the session's own first, order load-bearing),
because a client cannot test for a `.git` it is not allowed to read. `GitBlame`
takes no repository on purpose: blame follows the file you are reading, which
belongs to the session rather than to whichever checkout a window is pointed
at.

## The other read surfaces, briefly

- **Skills and memory under `~/.claude`** (`R-F14`, `R-F15`): a list without
  bodies, and one body at a time by a path that is canonicalised through `..`
  and symlinks and must still sit under a published root
  ([claude-code-formats.md](claude-code-formats.md#reading-a-body-over-the-wire)).
- **The scratch directory** is the one place the editor writes, and its
  containment proof is the strictest here: every segment checked, then the
  joined path canonicalised and required to be under the canonical scratch
  directory, because a symlinked directory inside the folder is invisible to
  any string rule
  ([wire-protocol.md](wire-protocol.md#notes-tasks-and-scratch),
  [ADR-0035](../decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)).
- **The doc inventory** (`R-H1`) answers for watched repositories only; the
  daemon refuses a path no session lives in.

## Two spellings of one path

A session's `touched_files` carry the prefix its transcript wrote, unresolved;
`repo_root` is what `git rev-parse --show-toplevel` answers, resolved through
every symlink. They differ for any checkout reached through a symlink and on
macOS for every temp directory (`/var` is a firmlink to `/private/var`).
Attribution reconciles on the miss path only — the ordinary case stays a
string compare — and canonicalises the longest *existing* ancestor, because
a deleted file is precisely what a diff is about (`R-J27`). Only touches that
resolve to a path *inside* the root are attribution candidates; a session
whose only recorded write was a scratchpad under `/tmp` falls back to the
whole-worktree diff rather than reporting no changes (`R-J62`).
[review-checkpointing.md](review-checkpointing.md#attribution) holds the
filter itself.
