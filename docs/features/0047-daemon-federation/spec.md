---
title: Daemon federation — every machine's sessions in this window, as if they were local
status: draft
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

- [ ] From the Connections panel or the top-bar switcher, *Follow* a saved
      daemon; *Unfollow* it the same way. Only a client on this machine can
      follow or unfollow, and no screen or message ever shows a follow's token
- [ ] Within one refresh, the followed daemon's sessions are in this window's
      Attention list, marked remote, grouped under their machine — this
      machine first — and ranked together with local ones by one ranking
- [ ] A waiting session on a followed machine notifies here and counts in the
      tray, with the window closed if the daemon is standalone
      (`scripts/mogeungd`)
- [ ] A followed machine that goes quiet greys out and leaves the ranking
      within a minute, and comes back when it does
- [ ] Clicking a remote session switches the window to its daemon and selects
      it — the phase-A fallback, replaced in phase B
- [ ] A client connecting to this daemon **from another machine** sees this
      machine's sessions only — never a peer's
- [ ] A45 takes a verdict after a week of use

**Phase B — open remote sessions in place** (`R-I22`)

- [ ] Clicking a remote session opens it without switching: transcript,
      changes, diff, files, search, git reading, run configurations listed —
      every pane answers about *that* machine
- [ ] The Agent pane and terminals on a remote session attach over ssh to its
      machine (its `ssh_target`), not to this one
- [ ] Every write and run verb on a remote session is refused **in words**,
      naming A24
- [ ] Two sessions on two machines editing one path are not reported as a
      collision; two on one machine still are

**Phase C — write verbs across the hop** (`R-I23`, gated on A24)

- [ ] Git writes, run configurations and send-to-session act on a remote
      session as they would on a local one, with the live-session warnings of
      the machine they act on
- [ ] ADR-0012, ADR-0025 and ADR-0003 are each checked against a verb
      crossing a hop, with an amendment where one is needed

**Phase D — hardening and verdicts** (`R-I24`)

- [ ] A peer on an older build is refused a follow with the version it needs
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
