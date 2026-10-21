---
title: A config file that cannot be read whole stops the daemon, and says why
status: active
updated: 2026-10-21
decided: 2026-10-21
---

# ADR-0045 — A config file that cannot be read whole stops the daemon, and says why

## Context

Since `R-J3` the loader tolerated everything: a `config.toml` that did not
parse was read as the defaults, with one warning line in the log, on the
argument that *a typo in a preferences file must not stop the daemon
starting, because the daemon is how you find out anything is wrong at all*.
`Config` is `deny_unknown_fields`, so a mistyped **key** fails the whole file
too — which the editor (`R-J79`) turns into a refusal before a save, and a
hand edit does not.

On 2026-10-21 that rule met the shape it was not written for. A daemon on
another machine listened on `0.0.0.0:7717` with a token, both from its
`config.toml`. The owner added `Notify = true` by hand — one capital letter —
and restarted. The daemon started on the defaults: `127.0.0.1:7717`, no
token, nothing from the file, and the warning went to a log nobody was
reading. From the window on the other machine the Connections probe could say
only *refused — unreachable, or the token was wrong; a socket cannot tell
which*, and `scripts/mogeungd status` said *access this machine only*. A
daemon that starts on the defaults is not how you find out anything is
wrong; on a LAN it is how you do not.

The owner's words: *"can you add error message to help with the
troubleshooting and stop it from starting silently with default setup"*.

## Decision

**A config file that cannot be read whole is an error, not the defaults**,
everywhere the file is read at start.

- **`mogeungd` does not start.** It prints the file, the line, the fault,
  the key the unknown one was probably meant to be when one is near
  (`Notify` → `notify`; `notifyy` and `push-url` too), the keys this build
  knows when none is, and the way out — fix the line, or move the file aside
  to run on the defaults on purpose — and exits non-zero. `scripts/mogeungd
  start` already prints the daemon's last lines; `status` repeats the refusal
  while nothing runs.
- **The window hosts nothing on it.** The refusal comes back as the reason
  nothing is serving, where a bind refused for want of a token is shown.
- **`why`, `beacon` and `judge` print the same and exit.** A tool reporting
  what the daemon would read, from the defaults, misleads.
- **A missing file is still the defaults.** Not having one is the ordinary
  state of a fresh install.
- **A read after start falls back and says so.** The weekly pricing consent,
  the switcher's look for the local token and the window's update settings
  read the file again while running; a file broken *then* reads as the
  defaults with a log line, because the daemon already refused it at start
  and a file broken later must not stop a running daemon.
- **One sentence.** The editor's check and the loader describe a fault in
  the same words, so a save and a start refuse a file in one voice.

## Alternatives

- **Keep tolerating, and be loud in Health** — an alert per dropped key and
  one for an unreadable file. Rejected: on the shape that failed, a LAN
  daemon fallen to loopback, no window reaches that Health panel, and the one
  that could is on the machine the person is not at.
- **Read the file key by key, dropping only the bad line.** Considered first;
  it answers the `Notify` case well, since `listen` and `token` survive.
  Rejected by the owner's second sentence — a daemon running on a
  configuration that is not the one in the file, however loudly, is still
  running on the defaults for the keys that failed — and because strict at
  save and lenient at start would be two readings of one file.
- **Refuse only when the file names a bind beyond loopback.** Rejected: it
  needs the file read to know, and a daemon that is sometimes strict is
  harder to reason about than one that always is.

## Consequences

- A typo stops the daemon. The message is the mitigation: it names the line
  and the key, guesses the intent, lists the keys when it cannot, and shows
  the `mv` that runs on the defaults on purpose.
- `R-J3`'s sentence — *a broken config costs you your settings, never the
  daemon* — no longer holds, and its test is inverted. A broken config costs
  you a start, and says so.
- An installed window on a machine whose file breaks opens with no daemon and
  the reason; it does not run blind. Its own preferences from the same file
  (`hotkey`, `update_*`) take the defaults for that run.
- `Config::load` returns a `Result`, so every caller decides, and a new
  caller cannot ignore a broken file by accident.

## Revisit if

- The refusal is hit for typos often enough that the key-by-key reading is
  wanted after all. It should then carry this same message, and name the keys
  it dropped at start, not in a log.
- A deployment appears where the daemon is started by something that cannot
  show its stderr, and a refused start there is worse than a loopback one.
