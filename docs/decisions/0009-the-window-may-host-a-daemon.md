---
title: The window may host a daemon, in its own process
status: active
updated: 2026-09-22
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

## Amendment — 2026-09-22

**The hosted daemon reads `listen`, `token`, `ssh_target` and `advertise`
from `config.toml`, and binds and gates as `mogeungd` would from the same
file.** It replaces the clause, implicit above and explicit in the code since
`R-O1`, that the hosted daemon binds the loopback address the window asked
for and reads only the model keys.

The reason is `R-I17`. The beacon made every machine findable, and the one
that ran only the window was the one machine nobody could connect to: its
daemon was loopback-bound, token-less and published no ssh target, so the
Found list said *on its own machine only* and every pane refused. The
alternative — start `mogeungd` by hand on each box before the window — was
the friction this ADR exists to remove, one row over: *"otherwise I will have
to remember starting the daemon on each box manually."*

What it comes with:

- **A plain launch can now listen on the LAN, because a file said so.** That
  is the opt-in `R-I10` requires — the file was written by a hand — and
  `admit` still refuses a public bind with no token; the refusal now reaches
  the window as a sentence rather than a line on a thread. The hosting notice
  says what was bound and that it is token-gated.
- **The window dials its own daemon with the token.** A token-gated daemon
  gates loopback too, so the shell hands the token to the webview with the
  hosting status and the window composes the URL, exactly as it does for a
  remote. The composed URL lives in the store for the session and is never
  written to `localStorage`, which keeps holding the plain address.
- **Attaching reads the same file**, so a daemon somebody started by hand on
  this machine with that token is probed and dialled with it.
- The bind is still the test, the daemon is still a thread, and everything in
  *Consequences* still holds — including that closing the window stops the
  watching, now also for a daemon other machines were reaching.

*Revisit if* gains a line: a machine that should be findable but not
connectable has no way to say so short of leaving `listen` out, and if that
turns out to be a real want, a `beacon`-only posture is one key away.

## Amendment — 2026-09-22, later

**The window acquires this machine's daemon at every launch, whatever daemon
it is pointed at.** This replaces a clause that lived in the client rather
than above — *a non-loopback URL is a decision already made; starting a local
daemon would answer a question nobody asked* — under which a window last used
on a remote hosted nothing.

The reason is `R-J102`, reported the same day: *"when I use the UI to switch
to another session, I can't switch it back to the local session — I think the
mogeungd will be killed once I connect to a remote session."* Two faults
answered to that description. The daemon was never killed; the switch back
dialled it without the token the first amendment made it require. But a
window **relaunched** while pointed at a remote really did leave this machine
with no daemon — unwatched, unannounced to the LAN, and nothing to switch back
to. With the beacon and the first amendment, this machine's daemon is also its
presence on the network, and that is not a thing to lose because of which
board a window last showed.

What it comes with:

- **Which daemon runs and which daemon the window shows are now separate.**
  The hosting chip and the notice mean *this window keeps this machine's
  daemon alive*, whichever board is on screen.
- **Tokens are composed at every dial, never remembered.** `lib/dial.ts`
  gives this machine's daemon the port and token the shell reported, a saved
  remote the token from the `0600` file, and a loopback address on another
  port — an `ssh -L` tunnel — its own saved token rather than this machine's.
  What `localStorage` keeps between launches is the address alone; before this
  it kept the remote's token too, against ADR-0036.
- A laptop that uses `ssh -L` on 7717 for a remote now finds the port taken
  when it launches pointed elsewhere, and should tunnel on another port —
  which the guide's Route A already recommends when a local daemon is running.

