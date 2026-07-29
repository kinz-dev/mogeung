---
title: File explorer
status: shipped
updated: 2026-07-28
roadmap: [R-B24]
depends_on: [A13, A15]
---

# 0007 — File explorer

A sixth detail pane: the session's worktree as a tree on the left, and a
read-only, syntax-highlighted view of the picked file on the right.

## Spec

### Problem

The Changes tab shows only what the session *edited*. Judging an edit often
means reading a file the agent did not touch — the trait being implemented, the
caller of a changed function, the config that explains a constant. Today that
means leaving mogeung for an editor, which breaks the review loop for what is
usually thirty seconds of reading.

Asked for directly:

> can you help to build a file explorer tab that will show the file tree at the
> left and the editor pane at the main screen?

### Assumptions

- **A13** — the user drives by keyboard. `SUPPORTED`.
- **A15** — reading worktree files inside mogeung is worth a pane, *read-only*.
  `SUPPORTED` by the request itself, same caveat as A14: asking is weaker
  evidence than using.

### Explicitly read-only — and why

The roadmap's "Explicitly not" list says **an editor — handoff to IntelliJ/VS
Code, permanently**. This feature does not reopen that. It is a *viewer*: no
buffer, no save, no write path anywhere in it. The daemon gains `list_dir` and
`read_file` and nothing that writes. Editing stays handed off. If that boundary
is ever revisited it gets its own ADR, not a quiet extension of this pane.

Reading goes through the daemon (`ListDir` / `FetchFile` wire commands) rather
than the UI touching the filesystem, keeping every client a projection with no
local authority ([ADR-0001]) — the web client gets the explorer for free if ever
wanted, and a future remote daemon (`R-I4`) does not strand the feature.

### Acceptance

- [x] An Explorer pane exists in the detail area, reachable by key (`X`), the
      palette, and the tab strip, and dockable like any other pane
- [x] It shows the session's worktree (repo root when known, else cwd) as a
      collapsible tree; directories load lazily on expand
- [x] Clicking a file shows its content with real syntax highlighting
      (grammar-based, not the diff tokenizer)
- [x] A file the daemon cannot read (binary, too large, vanished) says so in
      the pane instead of erroring elsewhere
- [x] `.git` is not listed; a path outside the session root is refused by the
      daemon even if a client asks for it
- [x] Nothing anywhere in the feature writes to disk

### Explicitly out of scope

- **Editing. Permanently** — see above.
- Keyboard cursor movement *inside* the tree (`j`/`k` walking entries). The
  pane is reachable by keyboard; walking the tree is mouse-first in v1. Worth
  revisiting only if the pane earns use.
- Line numbers, search-in-file, go-to-definition. It is a viewer, not an IDE.
- Watching for file changes; content is fetched when opened, refreshed by
  re-opening.

## Plan

### Approach

Two new fire-and-forget commands and two events, following the
`FetchReviewDebt` shape: `ListDir { session_id, path }` →
`DirListing { session_id, path, entries }` and `FetchFile { session_id, path }`
→ `FileContent { session_id, path, content, truncated }`. Paths are relative to
the session root; the daemon canonicalises and refuses anything that escapes it.
Files are capped (256 KiB, truncated flag set) and binary content is refused —
the R-A5 rule, applied to worktrees. Matching REST GETs keep the daemon
curl-able.

The UI caches listings per directory and file content client-side in a new
`explorer` module, cleared when the selected session changes. Highlighting is
`egui_extras::syntax_highlighting` with the `syntect` feature — first-party,
version-locked to egui (the same argument that chose `egui_tiles`), cached per
frame by egui, with real grammars picked by file extension. The hand-rolled
diff tokenizer stays where it is; the two solve different problems.

`Tab::Explorer` joins `ALL_PANES`; saved layouts simply lack the pane until it
is asked for, which `layout::focus` already handles by re-inserting.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | `ListDir`/`FetchFile` commands, `DirListing`/`FileContent` events, `DirEntry` |
| `crates/mogeungd/src/state.rs` | `list_dir`, `read_file`, root containment guard |
| `crates/mogeungd/src/api.rs` | dispatch the two commands; REST `ls` and `file` routes |
| `crates/mogeung-ui/src/explorer.rs` | new — client cache and tree state |
| `crates/mogeung-ui/src/app.rs` | `Tab::Explorer`, `explorer_tab`, ingest arms |
| `crates/mogeung-ui/src/keymap.rs` | `TabExplorer`, default `X` |
| `crates/mogeung-ui/src/layout.rs` | `ALL_PANES` grows to six |
| `crates/mogeung-ui/Cargo.toml` | `egui_extras` with `syntect` |

### Risks and unknowns

- **An unauthenticated daemon now serves file contents.** It already serves
  transcripts and opens terminals, so this widens nothing in principle — but
  the containment guard matters: without it, any client could read arbitrary
  paths by construction rather than by accident. Guard is tested.
- **syntect pulls a native regex dependency** (onig). Build-time cost, accepted
  for real grammars; if it ever blocks a platform, `egui_extras` falls back to
  its built-in highlighter by dropping the feature.
- **Large trees.** Lazy per-directory listing means cost scales with what is
  expanded, not repo size. `node_modules` is listable but never auto-expanded.

### Test strategy

Daemon: the containment guard (inside stays inside, `..` and absolute escapes
refused), `.git` exclusion, dirs-before-files ordering, the binary and size
refusals. UI: explorer cache invalidation on session switch, and the pure
tree-path helpers. Rendering is egui's and not re-tested.

## Notes

**The e2e harness was not hermetic, and this feature found it.** `boot()` used
`AppState::new`, which watches the real `~/.claude` — so
`rescan_is_safe_to_request_over_the_wire` timed out on any machine whose
history is big enough that a first scan outruns the test's 10-second window
(139 MB and 56 sessions on the machine that caught it). The tests now boot
with an empty `claude_home` of their own via `AppState::with_home`, which is
what the parameter existed for. The e2e suite went from ~20 s to 0.1 s.

**`X` for the binding.** `c`/`t`/`i`/`d`/`e` are taken by the other tabs and
every honest letter of "files" already means something else; X as in eXplorer.

**Highlighting cost is a non-issue.** `code_view_ui` memoises per
(code, language, theme) in egui's frame cache, so the syntect parse runs once
per file open, not per frame — no debouncing layer needed.

**The listing pre-expands on arrival.** `ingest_dir` marks the directory
expanded, so the answer to "open this directory" appears the frame it lands
instead of needing a second click. It also makes refresh re-expansion free.

**Root request is lazy, in the pane, not in `set_tab`** — a pane docked
visible beside another tab renders without ever being switched to, so tying
the fetch to the switch would strand exactly the docked arrangement the pane
is for.
