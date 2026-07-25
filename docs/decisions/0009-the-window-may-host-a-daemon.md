---
title: The window may host a daemon, in its own process
status: active
updated: 2026-07-25
decided: 2026-07-25
---

# ADR-0009 — The window may host a daemon, in its own process

## Context

[ADR-0001](0001-rust-core-with-egui-ui.md) split mogeung into a daemon that owns
all state and a window that is a pure projection of it. That is the right
architecture and nothing here changes it.

It does have a cost the architecture does not pay for: **two processes to start,
in two terminals, in the right order.** For a tool whose entire pitch is
reducing the friction of supervising work, "run these two things first" is a bad
first impression, and it is friction paid every single day.

The daemon genuinely should be able to outlive the window — that is what makes
notifications, the phone client and busy→idle tracking work while nothing is on
screen. But that is a *capability*, not a requirement for every launch. Most of
the time there is one person, one machine, and one window.

## Decision

**The window binds the daemon port at start-up. If it wins, it hosts a daemon on
a thread in its own process. If it loses, it attaches to whatever is already
there.**

A daemon the window started dies with the window. A daemon that was already
running is left alone. `mogeungd` remains a separate binary and remains the way
to get one that outlives every window.

Two details carry most of the value:

**The bind is the test.** Not "probe, then start if nothing answers" — that
races: two windows opened together both probe, both see nothing, both start a
daemon, and one dies on a port conflict. Whoever gets the socket is the daemon;
there is no gap in between.

**The hosted daemon is a thread, not a child process.** The natural description
of the requirement is "spawn it, remember the pid, kill it on exit", and that is
what was asked for. A thread delivers the same observable behaviour with none of
the bookkeeping: no pid file to go stale, no cleanup to skip when the window is
`SIGKILL`ed, no orphan holding the port that the next launch has to reason
about. The operating system does it, and the operating system cannot forget.

## Alternatives

**Spawn `mogeungd` as a child process and kill it on exit.** The literal ask.
Rejected because every failure mode is a mess someone has to debug: a crash or
`kill -9` leaves an orphan daemon holding 7717, and the next launch then attaches
to a daemon nobody knows the age of. It also needs the binary to be *findable*,
which reintroduces the packaging problem this was meant to solve.

**Always host, never attach.** Simpler, and wrong: two windows would fight over
the port, and it would make a long-running `mogeungd` — the thing that keeps
notifications alive — impossible to use alongside a window.

**Always require a separate daemon.** The status quo. Rejected as the friction
described above, paid daily, for a benefit most launches do not use.

**Drop the daemon and put everything in the window.** This is the one worth
naming explicitly, because it is where "just make it one executable" leads if
followed far enough. It would kill the web client, notifications while closed,
and any second client — and it would undo [ADR-0001](0001-rust-core-with-egui-ui.md)
for a start-up convenience. The window hosting a daemon keeps the boundary: it
still talks over the same websocket, still has no local authority, and still has
no idea whether the daemon is in this process or another.

## Consequences

**Good.** One executable is enough. Order of starting no longer matters. A
running `mogeungd` is transparently reused, so the dev setup (`mprocs`,
`start.sh`) keeps working unchanged.

**Bad, and the one to watch: closing the window can now stop the watching.**
When the window hosts the daemon, closing it ends notifications, the phone
client and all session tracking. That is correct — you started it implicitly, so
it is yours — but it is *not* what closing a window usually means, and someone
relying on `--notify` will be surprised exactly once, at the worst moment. The
window therefore says `hosting` on screen rather than only in a tooltip, and the
text spells out what closing it will do.

**Bad — the window binary now links the whole daemon**: axum, rusqlite, tokio.
Larger binary, longer build, and a daemon panic can in principle take the window
with it.

**Bad — a port conflict is now silent-ish.** If something that is not mogeung
holds the port, the window reports "nothing is serving" and shows an empty
board. That is checked (the probe requires a mogeung-shaped health response) and
tested, but it is a state that did not exist before.

## Revisit if

- Hosting turns out to surprise people about notifications stopping. The fix
  would be to make the hosted daemon survive the window — which means a child
  process after all, and this ADR should be superseded rather than quietly
  amended.
- A second client (the web UI on a phone, `R-C3`) becomes the primary way people
  use mogeung, at which point the daemon's lifetime should stop being tied to a
  desktop window at all.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
