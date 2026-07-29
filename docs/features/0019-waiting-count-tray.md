---
title: Waiting count in the tray
status: in-progress
updated: 2026-07-29
roadmap: [R-C2]
depends_on: [A25]
---

# 0019 — Waiting count in the tray

R-C2, built at the 2026-07-29 one-go ask. The roadmap left this open
deliberately — a fourth binary is a bigger commitment than the rest of
pillar C combined, and R-C1 banners may already cover the need. A25
records that doubt; an unglanced tray is a removal candidate.

## Spec

### Problem

"Is anything waiting on me" requires the window. When it is closed or
on another workspace, a session can sit blocked for an hour unseen —
the exact failure the product exists to end.

### Acceptance

- [ ] A `mogeung-tray` binary shows a tray/menu-bar item with the
      current WAITING count, updating without the window running
- [x] It reads the daemon over the existing wire only — no `~/.claude`
      access, no local authority, reconnects quietly when the daemon
      restarts, shows a distinct "daemon unreachable" state rather than
      a stale count
- [ ] Its menu lists waiting sessions by name; activating one raises or
      launches the window
- [ ] Works on this Linux box's tray (StatusNotifierItem); the macOS
      menu-bar half rides the same crate and is noted untested here

### Explicitly out of scope

- Any action on sessions from the tray beyond raising the window —
  the tray observes the observer.

## Plan

### Approach

New small crate `mogeung-tray`: `ksni` (pure-Rust StatusNotifierItem)
+ the existing WS client shape from mogeung-core wire types; subscribe
to the queue broadcast, count `WAITING`, re-render title/tooltip; menu
from the queue snapshot; "open" spawns/raises `mogeung-ui` via the
`open_in`-style attempts table.

### Risks and unknowns

- Tray availability varies by desktop (GNOME needs an extension);
  degrade to a log line, never crash.

### Test strategy

Count-derivation unit test over queue snapshots; manual acceptance on
this desktop; e2e reuse of the WS harness for the subscribe path.

## Notes

Built 2026-07-29 as `crates/mogeung-tray`. What surprised, in order of
how much it shaped the code:

- **ksni 0.3 is async-first, and its blocking wrapper is a trap here.**
  The `blocking` feature drives a private internal tokio runtime via
  `block_on`, so calling it from inside our own runtime (where the
  websocket lives) would nest runtimes and panic. The fix was the
  opposite of the obvious one: use ksni's *async* API and run tray and
  websocket on one shared current-thread runtime.
- **"Connected" is not a TCP fact.** The tray flips out of its
  unreachable state only when a `snapshot`/`queue` message has actually
  said what is waiting — never on mere connect — so the unreachable
  state can never dress up an old count. Same reasoning on disconnect:
  the count is discarded, not dimmed.
- **The daemon broadcasts every answer to every client.** The tray
  receives git pages, file listings, other clients' errors. `Model::
  apply` keeps only `snapshot`/`queue`/session messages and skips the
  rest, which is also the degrade path for wire growth: an unparseable
  or unknown message changes nothing.
- **WAITING means `AwaitingInput` only.** APPROVE (`AwaitingPermission`)
  is deliberately excluded — the tray answers one question, "is anything
  waiting for me to type", and a count that quietly means three things
  stops being glanceable. If dogfooding says APPROVE belongs in the
  number, it is a one-arm change in `model.rs::waiting`.
- **The workspace moved underneath the build.** `Session` grew
  `limit_hit_at`/`limit_resets` from parallel in-flight work (0015)
  between two compiles, and `mogeungd` was transiently broken by that
  same work — none of it this crate's doing, but a reminder that the
  test constructor duplicates `attention.rs`'s and will need the same
  new-field touch whenever `Session` grows.
- ksni is a Linux-only target dependency; on other OSes the binary
  exits cleanly with a message. The macOS menu-bar half stays unbuilt,
  as the acceptance already noted.

Left for the desktop pass: the item rendering (theme icon names —
`network-offline` / `user-available` / `dialog-warning` — borrowed from
the desktop's own theme rather than shipped pixmaps), menu clicks
raising the window, and behaviour under a GNOME session without the
AppIndicator extension (expected: clean exit with a pointer to it).

