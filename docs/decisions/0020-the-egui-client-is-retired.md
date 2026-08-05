---
title: The egui client is retired, and its code is deleted
status: active
updated: 2026-08-05
decided: 2026-08-05
---

# ADR-0020 — The egui client is retired, and its code is deleted

## Context

[ADR-0018](0018-a-second-client-in-typescript.md) chose to rewrite the client in
TypeScript and set the terms of the changeover in one line: *the egui client is
retired at parity, not before*. `R-M4` then listed what parity meant — the
health, keymap and connections windows, the blame gutter, the symbol outline,
the markdown preview — and required a week of use after that.

Between 2026-08-04 and 2026-08-05 the list was finished. `R-I7`'s connections
window, `R-B28`/`R-B29`'s outline and blame, the markdown preview, the launch
window, the ambient board, the colour tags: all of them landed in the React
client, and every one of them landed *there only*. That is the real signal, and
it is stronger than the checklist: for two days every feature and every bug fix
has gone to one client, and the other has been running only to prove it still
can.

The `/clear` label bug on 2026-08-05 is what made the cost concrete rather than
theoretical. It had been found and fixed in the egui client in July; the React
client was ported without the fix and reproduced the bug a fortnight later. Two
clients do not cost twice the work — they cost twice the work *plus* the
divergence, and the divergence is the part nobody budgets for. Keeping a client
alive that nobody develops does not preserve a fallback; it preserves a second
place for the same bug to live.

The week of use `R-M4` asked for has not happened. It is being waived
knowingly: the dogfooding it was meant to protect against has been happening
against the React client daily since 2026-08-04, and what remains untested by
it — a fresh install on another machine — is not something running the egui
window in parallel would have caught.

## Decision

**Delete `crates/mogeung-ui` and `crates/egui-term`.** The Tauri client in
`desktop/` is the window.

- The daemon, `mogeung-core` and `mogeung-tray` are untouched, exactly as
  ADR-0018 said they would be. The only edit outside the deleted crates is the
  tray's launcher, which now spawns `mogeung-desktop`.
- The vendored terminal widget goes with it. It existed because no egui
  terminal crate could be pinned to one egui version; xterm.js replaced it in
  `R-M3` and nothing else ever depended on it.
- `scripts/install.sh` stops installing a window. The Tauri bundler produces a
  `.deb`, `.rpm` and an AppImage that carry the icon and desktop entry properly,
  which the hand-rolled `sed` into `~/.local/share/applications` never did well.
- The history stays. Deleting a crate does not delete the feature docs that
  describe how it was built, and the `Files touched` tables in
  `docs/features/0001`–`0028` still name `crates/mogeung-ui/...` on purpose:
  they record what was done at the time, and rewriting them to point at
  TypeScript would be a lie about the past.

**Delete rather than archive.** A branch or an `attic/` directory is a
maintenance claim nobody intends to honour, and git already keeps every version
of every file. `git show a16699d:crates/mogeung-ui/src/app.rs` is the archive.

## Alternatives

- **Keep it building, unmaintained.** The cheapest thing to type and the most
  expensive to live with: `cargo test --workspace` is a gate on every daemon
  change, so a client nobody uses would keep failing that gate, and each fix
  would be paid for by someone whose change had nothing to do with it. It also
  keeps the egui-versus-wgpu dependency tree — around 3,700 lines of
  `Cargo.lock` — in every build of the daemon.
- **Keep it as a protocol conformance test** — a second implementation proves
  the wire protocol is not accidentally shaped around one client. Genuinely
  useful, and the wrong tool for it: a conformance test should be a test, not a
  40,000-line GUI. If the argument is felt again, the answer is a test that
  drives the protocol directly.
- **Archive to a branch.** Rejected above. A branch that is never merged and
  never built is a directory of files that compile against a `Cargo.toml` that
  no longer exists.
- **Wait out `R-M4`'s week.** The week protects against discovering the new
  client is unusable. Two days of exclusive use have answered that with more
  force than a parallel week would, because nothing was falling back.

## Consequences

- **There is one client.** Every fix lands once, and the divergence class of bug
  — the same defect fixed in one window and live in the other — is gone.
- **The Rust side is smaller and faster to build.** The workspace drops from
  five crates to three, and no daemon build pulls in a GPU stack again.
- **Rust view-state is stranded, not lost.** `~/.mogeung/prefs.json` and
  `~/.mogeung/state/<machine_id>.json` still hold labels, pins, bookmarks and
  hidden sessions; the React client keeps its own in `localStorage` and cannot
  read them. Nothing deletes those files, so an importer remains possible.
  `R-I12` — move this state into the daemon — is the honest fix and is now the
  *only* fix, since there is no second client to disagree with.
- **`cargo build --release` no longer produces a window.** Building one needs
  node and webkit, which is a heavier requirement on a fresh machine than
  `cargo build` was. Stated in the README rather than smoothed over.
- **We can no longer claim two independent clients as evidence** that the daemon
  is client-agnostic. The claim was demonstrated twice (`R-C3`'s phone client,
  then this one) and both demonstrations were retired after making their point.
  The tray is what remains of it, and it is a real second client.

## Revisit if

The Tauri client cannot be built or run on a platform that matters — a machine
where webkit is unavailable, or a remote box where only a terminal is possible.
That is not an argument for restoring egui; it is an argument for a client with
*fewer* requirements than either, against the same unchanged protocol.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
