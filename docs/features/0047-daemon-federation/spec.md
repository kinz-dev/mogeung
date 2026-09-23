---
title: Daemon federation — every machine's sessions in this window, as if they were local
status: active
updated: 2026-09-23
roadmap: [R-I18, R-I21, R-I22, R-I23, R-I24]
depends_on: [A45, A24]
branch: daemon-federation
---

# 0047 — Daemon federation

## Spec

*Owned by the human. What and why, never how.*

Asked 2026-09-21 as the *Day 2* of [feature 0046](../0046-found-on-the-lan.md):

> allow me to connect but display the session on the same local UI (with some
> indication that it is a remote session and with grouping)

and chosen on 2026-09-23 over two cheaper shapes, costed side by side:

> ok what I really want to achieve is ultimate option 2. Federation.

The three shapes were: an **overview** (remote sessions listed, a click
switches daemons — days); **federation in the daemon** (this — weeks); and an
**aggregating window** holding a socket per daemon, which
[ADR-0013](../../decisions/0013-one-window-one-daemon.md) refuses and which
costs more than federation while delivering less. ADR-0013's amendment of
2026-09-23 admits federation on its own terms; [ADR-0036](../../decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md)'s
of the same day says where the peers' tokens rest.

Work happens on branch `daemon-federation`, cut from `main` at `c06b7e8`.

### Problem

Three machines on one LAN each run Claude Code sessions. `R-I17` made them
find each other and `R-I20` made switching between them one click, and it is
still **one machine on screen at a time**: switching drops the board, the
session waiting on the machine you are not watching is not in front of you,
and there is no single answer to *who needs me most* across them — which is
[A1 and A6](../../product/assumptions.md), the product's thesis, one level out.
ADR-0013 called this *a genuine loss* and bet that per-machine windows would be
good enough. The person running three machines says they are not.

### Assumptions

- **A45** — *one ranked list across machines, with remote sessions opened in
  place, is used instead of switching* — `UNTESTED`. **The first work is to
  test it**: phase A (`R-I21`) is the mirror alone — every machine's sessions
  in one ranked, grouped list, a click still switching daemons — and a week of
  it is the verdict. If the list is not used, phases B–D are not built.
- **A24** — *a read-only daemon is safe to reach over a trusted network with a
  shared token, without TLS* — `UNTESTED`, and deepened: a following daemon
  holds its peers' tokens and reaches them on your behalf. Phases A and B are
  read-only across the hop by construction; **phase C (`R-I23`) is gated on
  A24's verdict** and does not start without one.

### Acceptance

**Phase A — follow and mirror** (`R-I21`)

- [x] From the Connections panel or the top-bar switcher, *Follow* a saved
      daemon; *Unfollow* it the same way. Only a client on this machine can
      follow or unfollow, and no screen or message ever shows a follow's token
- [x] Within one refresh, the followed daemon's sessions are in this window's
      Attention list, marked remote, grouped under their machine — this
      machine first — and ranked together with local ones by one ranking
- [ ] A waiting session on a followed machine notifies here and counts in the
      tray, with the window closed if the daemon is standalone
      (`scripts/mogeungd`)
- [x] A followed machine that goes quiet greys out and leaves the ranking
      within a minute, and comes back when it does
- [x] ~~Clicking a remote session switches the window to its daemon and selects
      it — the phase-A fallback, replaced in phase B~~ — built as phase B
      directly; see *Notes*
- [x] A client connecting to this daemon **from another machine** sees this
      machine's sessions only — never a peer's
- [ ] A45 takes a verdict after a week of use

**Phase B — open remote sessions in place** (`R-I22`)

- [x] Clicking a remote session opens it without switching: transcript,
      changes, diff, files, search, git reading, run configurations listed —
      every pane answers about *that* machine
- [x] The Agent pane and terminals on a remote session attach over ssh to its
      machine (its `ssh_target`), not to this one
- [x] Every write and run verb on a remote session is refused **in words**,
      ~~naming A24~~ naming the follow's *allow changes* consent while it is
      off — see *Notes*
- [x] Two sessions on two machines editing one path are not reported as a
      collision; two on one machine still are

**Phase C — write verbs across the hop** (`R-I23`, ~~gated on A24~~ behind a
per-follow consent — see *Notes*)

- [x] Git writes, run configurations and send-to-session act on a remote
      session as they would on a local one, with the live-session warnings of
      the machine they act on
- [x] ADR-0012 (now ADR-0014), ADR-0025 and ADR-0003 are each checked against a verb
      crossing a hop, with an amendment where one is needed

**Phase D — hardening and verdicts** (`R-I24`)

- [x] A peer on an older build is refused a follow with the version it needs
- [ ] Three peers, one of them on Wi-Fi in power save, for a week without a
      stale row, a lost notification or a leaked token
- [ ] `R-I16`, `R-I17`, `R-I18` take their verdicts

### Explicitly out of scope

- **The aggregating window** (`R-I9`). Refused, still.
- **Following automatically.** A beacon is an offer; following is a hand's
  act, one daemon at a time.
- **Transitive federation.** A peer's peers are not mirrored; no re-export.
- **Cross-machine collisions.** Not a collision; not reported.
- **Merging two machines' views of one repository** — a checkout on the
  laptop and a checkout of the same repo on the dev box stay two worktrees.

## Plan

See [plan.md](plan.md).

## Notes

*Filled during implementation.*

**2026-09-23 — phases A to D built in one go, at the owner's direction**
(*"finish all the phases from A to D for the complete daemon-federation
feature"*), on `daemon-federation`: `47b535e` (the daemon half) and the
commits after it. Two departures from the spec above, each recorded in
[ADR-0013](../../decisions/0013-one-window-one-daemon.md)'s second amendment
of that date:

- **B–D were built before A45's verdict**, not after it. A45 is still
  `UNTESTED`, and still decides: the unticked boxes are the week of use, and a
  `REFUTED` means phase A is the product and B–D come out.
- **Phase C is behind a per-follow consent, not behind A24's verdict.**
  *Allow changes*, off by default, settable only from a window on this
  machine; while off, a write is refused here in words. While on, the peer's
  own gates still decide — send refused over a LAN, runs needing its
  `--allow-run`, git writes needing its token-gated posture — so a relayed
  command never has more authority than a window dialling that peer directly.
  A24 is still `UNTESTED`; the consent is a person opting into it per follow.

**Built directly as phase B**, so the phase-A fallback (*a click switches
daemons*) was never built: a click opens a remote session in place.

**Verified how.** A two-daemon end-to-end suite in one process
(`crates/mogeungd/tests/federation.rs`: mirror and rank, reads answered
through, no re-export, stale and back, following yourself refused, consent
opening the hop while the peer's gate still decides, a pre-federation peer
refused, staleness on a half-open socket measured from last heard); unit
tests for the classification, the id rewrite, the relay filter and the run
flag; window tests for the follow controls, machine grouping, the remote chip,
stale greying, the keyboard walking the grouped order, per-session reach and
the machine-scoped `R-D21` warning. And live, against two debug daemons on
one box reading the real `~/.claude` (92 sessions): follow → up with 92
mirrored; reads relayed with namespaced ids; stale at ~61 s after a kill and
fresh again on restart; the costs in
[architecture.md](../../design/architecture.md#one-daemon-following-others-r-i18-feature-0047-2026-09-23).

**Found while building, and fixed:** a relayed `run_configs` was rewritten to
*runnable* for this machine's windows; `R-D21`'s warning matched live sessions
by path across machines; the Debt pane counted a remote repository against
this machine's checkout; staleness was measured from when a silent socket was
declared dead, and checked only between redials — nearly two minutes late on
Wi-Fi; `j`/`k` walked rank order while grouping by repository reordered rows
on screen (`R-J13`'s bug, one layer down).

**Found on first use, 2026-09-23, and fixed:** *follow* in the top-bar
switcher switched daemons instead — the button sat inside the row's menu item,
and Radix selects an item on pointer-up, so a button that swallowed the
pointer-down made the row click itself; it is an item of its own now, and a
test presses it the way a mouse does. And a follow to a machine rebuilt from
`main` rather than this branch said only *version 0.1.0 predates federation* —
true, and useless, since every build says `0.1.0`; it now says what to do. The
Connections panel was also made bigger and resizable by its corner, with
draggable edges between its parts, on the same day's ask.

**Not verified yet:** a notification and the tray count from a real second
machine; the Agent pane attaching over ssh to a peer's `ssh_target` from the
desktop build (the browser has no pty); three peers and Wi-Fi power save for
a week.
