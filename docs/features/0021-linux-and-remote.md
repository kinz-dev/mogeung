---
title: Linux reach and remote daemon
status: in-progress
updated: 2026-07-29
roadmap: [R-I3, R-I4]
depends_on: [A13, A24]
---

# 0021 — Linux reach and remote daemon

R-I3 (the Linux half) and R-I4, built at the 2026-07-29 one-go ask.
**Windows is descoped** — no machine to verify on; the roadmap row
carries the note rather than shipping unverifiable code.

## Spec

### Problem

R-I3: watching and diffing are portable and daily-driven on this Linux
box already, but the two terminal actions are macOS AppleScript with no
Linux sibling: jump-to-terminal fails, and launch-terminal hardcodes
`osascript`. The "open in" helper was made portable during Linux
dogfooding; the terminal paths never were.

R-I4: the daemon binds TCP and takes `--listen`, the UI takes `--addr`
— the plumbing for remote exists, but there is no authentication at
all, so pointing it at a dev box means trusting the whole network path.

### Assumptions

A13 (`SUPPORTED`); A24 (`UNTESTED`) — token-not-TLS is the bet, the
loud non-loopback warning stays.

### Acceptance

- [x] On X11, jump-to-terminal focuses the window owning the session's
      tty via the attempts-table pattern (wmctrl/xdotool); on Wayland
      it reports honestly that focusing is not possible rather than
      failing mutely (R-I3) — *X11 half verified by unit tests only:
      the dev desktop runs Wayland (no wmctrl installed), so the
      wmctrl/xdotool activation has not raised a real window yet; the
      Wayland refusal is the path this machine exercises live*
- [x] Launch-terminal works on Linux via the existing terminal
      candidates table; the tmux path is preferred when available,
      matching ADR-0010 (R-I3)
- [x] Desktop notifications use a Linux path (`notify-send` attempts
      table) alongside the portable push URL (R-I3)
- [x] The daemon accepts `--token`; when set, HTTP and WS require it;
      the UI takes the token with its `--url`; a wrong token is a
      clean 401, not a hang (R-I4) — pinned by `tests/auth.rs`;
      constant-time comparison, bearer header or `?token=` for WS
- [x] A UI connected to a remote daemon degrades honestly for
      local-only actions: open-in / jump-to-terminal / launch say
      "remote daemon — not available", never spawn on the wrong
      machine (R-I4)
- [x] Non-loopback bind still logs its warning, now mentioning whether
      a token is set — and that the token itself travels in clear text

### Explicitly out of scope

- Windows (descoped with note). TLS (A24's next step if the bet
  fails). Any daemon-side action initiation on behalf of the remote UI
  beyond what loopback already allows.

## Plan

### Approach

R-I3: port `focus_terminal`/`launch_terminal` to the `attempts`-table
idiom already proven in `ui.rs::open_in`; tty→window via wmctrl/xdotool
when present; keep macOS paths intact behind cfg/att-order. Notify:
`notify-send` first on Linux. R-I4: constant-time token check as an
axum layer + WS query param; UI plumbs token from CLI/prefs;
remote-awareness flag on the connection drives the honest-degrade
labels.

### Test strategy

Attempts-table unit tests per platform (the existing `open` guard test
pattern); auth-layer e2e (right token, wrong token, no token, loopback
default unchanged); manual acceptance for focus/launch on this desktop.

## Notes

R-I3 build notes (2026-07-29):

- **Launch never used tmux before.** The spec box says "the tmux path
  is preferred when available, matching ADR-0010", but the macOS
  launch has always run a bare `cd && claude` via Terminal.app —
  tmux launching lived only in `scripts/yolomo`. The Linux path now
  composes `tmux new-session -s mogeung-<safe>-<stamp> -c <dir>
  claude` (yolomo's shape: same name sanitisation, stamp instead of
  has-session probing) when tmux is installed, so what launch starts
  is hostable per ADR-0010. macOS is byte-identical to before —
  bringing it the same tmux preference is a separate decision.
- **Focus under tmux redirects to the client.** A tmux session's own
  ancestry dead-ends at the tmux *server*, which owns no window; the
  useful window is the attached client's terminal, so the ancestor
  walk starts from `tmux list-clients`'s pid. No client attached is
  an instructive error (attach command included), not a hunt.
- **notify-send gets a `--` before title/body** so a session label
  starting with `-` cannot be parsed as an option (verified against
  notify-send 0.8.8, which honours `--`).
- Everything is runtime-selected (`cfg!`, not `#[cfg]`), so the Linux
  tables and the Wayland refusal are unit-tested on any OS.
- This dev box is Wayland (XDG_SESSION_TYPE=wayland, xdotool present,
  wmctrl absent, only `x-terminal-emulator` of the candidate
  terminals installed) — hence the tests-only annotation on the X11
  focus box; the live path here is the honest Wayland refusal.

**R-I4 (integrator's notes).** The roadmap was right that remote was
"plumbing already done": the whole feature is a middleware layer, two
CLI flags and three refusals. The refusals matter more than the token —
`FocusTerminal`/`LaunchTerminal`/open-in against a remote daemon would
all have *worked*, on the wrong machine. The remote check reads the
address the window was actually pointed at (`--url` wins over
`--addr`), which was itself a bug found while wiring it. TLS stays out
until A24 fails, as the ledger says.
