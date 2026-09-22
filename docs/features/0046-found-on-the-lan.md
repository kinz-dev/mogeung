---
title: Found on the LAN — a beacon, a list that fills itself, and remote sessions in one window
status: draft
updated: 2026-09-22
roadmap: [R-I17, R-I18]
depends_on: [A44, A24]
---

# 0046 — Found on the LAN

Asked 2026-09-21, and stated as a goal rather than a feature:

> I want the UI on my local: can list and discover other running mogeung
> daemon on my local LAN (through the broadcast); can display the list of
> discovered session; Day 1 — allow me to connect to it (it is already built
> but not fully tested); Day 2 — allow me to connect but display the session on
> the same local UI (with some indication that it is a remote session and with
> grouping). I think we have some half-done implementation before. Please
> check?

The check comes first, because the answer shapes everything below.

## What is already there, and what only looks like it is

| Piece | State | Where |
|---|---|---|
| The daemon announces itself over **mDNS** as `_mogeung._tcp`, carrying machine id, version, whether it wants a token, and the Claude home | **Live**, behind `--advertise` / `advertise = true` in `config.toml`; refused on a loopback bind | `crates/mogeungd/src/discovery.rs`, `server.rs` |
| The **browse** half — `Found`, a `Watcher` that accumulates answers, a one-shot `browse()` | **Orphaned.** No wire message carries a result and no client calls it. It served the egui window's Scan panel, which [ADR-0020](../decisions/0020-the-egui-client-is-retired.md) retired | `discovery.rs` |
| The window's *Found on the LAN* panel | **Gone.** [`remote.md`](../guide/remote.md) still describes it | — |
| Switching the window to another daemon: saved connections with a token in `~/.mogeung/connections.json` at `0600`, `setUrl(dialledUrl(c))` | **Built** (`R-I7`, `R-I16`), **not verified in the desktop build** — `R-I16` is `⏳` for exactly that reason | `desktop/src/ui/ConnectionsWindow.tsx`, `lib/connections.ts`, `src-tauri/src/connections.rs` |
| Daemon identity on the wire — `machine_id`, `host`, `version`, `claude_home`; the shell reads the local machine id; client state scoped per machine | **Live** (`R-I5`, `R-I11`) | `mogeung-core/src/wire.rs`, `mogeungd/src/machine.rs`, `src-tauri/src/lib.rs` |
| The daemon a window hosts for itself | **Invisible by construction.** It binds the loopback address the window asked for, is admitted with no token, and reads only the model keys from `config.toml` — so it cannot advertise and cannot be found | `src-tauri/src/daemon.rs` |
| One window showing sessions from several daemons | **Refused** as `R-I9` by [ADR-0013](../decisions/0013-one-window-one-daemon.md), which names *federation in the daemon* as the shape to reconsider first | — |

So: the sending half exists over a different transport; the receiving half has
no client; connecting exists and is untested; and Day 2 is a decision already
made against, with the door it left open written into it.

## Spec

*Owned by the human. What and why, never how.*

### Problem

Two or three machines on one home LAN each run mogeung. Finding one from
another is: know its address, type it, type its token. Nothing on screen says
another daemon exists, let alone that it has a session waiting. And once
connected, the window shows *that* machine's sessions and none of your own —
[ADR-0013](../decisions/0013-one-window-one-daemon.md)'s deliberate loss,
*"three machines means three windows, three trays, and three notification
streams"*, which is exactly the loss the ask names.

### Assumptions

- **A44** — *a UDP datagram to the subnet's directed broadcast address crosses
  the Orbi mesh reliably enough for a beacon every 15 s, where mDNS multicast
  may not* — `UNTESTED`. **The first work is to test it, not to build.** Phase 0
  below is that test, and its verdict decides whether phase 1 is a beacon or a
  port of the browse that already exists.
- **A24** — *a read-only daemon is safe to reach over a trusted network with a
  shared token, without TLS* — `UNTESTED`, unchanged by phases 0 and 1: nothing
  new crosses the wire that a token does not already gate. **Phase 2 deepens
  it**: a daemon that fronts another holds that other's token, which is a
  second hop on a bet with one hop of evidence.

### Acceptance

**Phase 0 — the verdict on A44** (`R-I17`, first slice)

- [x] A throwaway `--bin beacon` sends one datagram every 15 s to each
      interface's broadcast address and prints every one it hears, and runs the
      mDNS browse beside it
- [x] Run on two hosts across the mesh — both on Wi-Fi as it turned out, the
      laptop with power save on and the Mac mini — for ten minutes, the ledger
      says for each transport how many of 40 beacons arrived, and A44 took its
      verdict from that (`SUPPORTED`, 2026-09-22; the Notes have the numbers)

**Phase 1 — found, and connected** (`R-I17`; the ask's *Day 1*)

- [ ] **Every** daemon is heard by every window on the segment within 15 s of
      starting — headless or window-hosted, loopback-bound or not — and a
      window that opens later sees it at once. `beacon = false` in
      `config.toml` silences it ([ADR-0039](../decisions/0039-the-daemon-announces-itself-by-default.md))
- [x] Each found row says how that daemon can be reached: *Connect* for one
      bound beyond loopback, *over ssh* with the target for one that is not,
      and the window says it is announcing on this network
- [x] The Connections window has a **Found on your LAN** section listing each
      daemon heard: hostname, address and port, version, whether it wants a
      token, how many sessions are live and how many need you, and how long
      since it was last heard. This machine's own daemon is left off the list
      — marked at first, removed on the first day it was seen: *"we should
      remove the local machine"*
- [x] A daemon that stops being heard is shown as gone after four missed
      beacons, greyed and kept, not dropped
- [x] **Add** on a found row fills the Saved form with its name and URL; the
      token is typed by a hand; Connect on the saved row is the switch that
      already exists. Nothing dials anything on its own
- [x] A saved row whose daemon is currently being heard shows as live
- [ ] `R-I16`'s panel — the file, its mode bits, the migration, a token
      composed into the dialled URL — is exercised in the **desktop build**
      against a real second daemon, and takes its verdict
- [x] A browser tab says it cannot listen for beacons, rather than showing an
      empty list
- [x] A machine that runs only the window is **found** regardless (its hosted
      daemon beacons too) and can be **connected to**, because the hosted
      daemon honours `listen`, `token` and `ssh_target` from `config.toml` —
      decided 2026-09-22, ADR-0009's amendment: *"otherwise I will have to
      remember starting the daemon on each box manually"*

**Phase 2 — remote sessions in this window** (`R-I18`; the ask's *Day 2*)

- [ ] A followed daemon's sessions appear in this window's Attention list,
      each marked as remote and grouped under the machine they run on, with
      this machine's own first
- [ ] Selecting one opens the same panes a local session opens, and every pane
      answers about *that* machine — transcript, diff, git, files, terminal
      where `R-I6` already reaches it over ssh
- [ ] Ranking across machines is one answer, computed once, in a daemon
- [ ] Collisions stay per machine: two sessions on different machines editing
      the same path are not a collision, and the window never says they are
- [ ] Following and unfollowing is explicit, per daemon, with the token typed
      once and held nowhere a browser can read
- [ ] [ADR-0013](../decisions/0013-one-window-one-daemon.md) has a dated
      amendment before any of this is built, on the terms it set itself

### Explicitly out of scope

- **Auto-connecting.** A beacon is unauthenticated; anyone on the segment can
  send one naming their own port. Found is an offer, `R-I8`'s rule holds, and
  the token never rides the wire.
- **Gossip.** On one broadcast domain a relay adds nothing to what every host
  already hears. Relaying only pays across a segment broadcast does not cross,
  which is what A44 rules on; if it says *neither transport crosses*, that is a
  different feature.
- **Session titles in the beacon.** Counts only. Titles are the daemon's to
  give to a client it has admitted, not to shout at the segment.
- **An aggregating window.** Phase 2 is federation in the daemon or it is not
  done: the window stays single-socket and computes nothing, which is the rule
  ADR-0013 was defending and the routing cost it counted — now **114** send
  sites and **85 of 149** `ClientMsg` variants carrying a bare-`String`
  session id — has only grown.
- Pushing, merging, or any write verb reaching across machines that does not
  reach across today.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

**One codec, three sockets, and the window stays thin.** The beacon's shape
and the peer table — what a beacon says, how it is parsed, when a peer is
*gone* — live in `mogeung-core`, with no sockets, so the daemon that sends and
the shell that receives cannot disagree about what a datagram means: the reason
`discovery.rs` gives for holding both mDNS halves in one file, applied across
the crate boundary the shell actually has (it depends on `mogeung-core`, never
on `mogeungd`). The daemon sends. The **shell** listens, not the daemon, for
the reason [ADR-0036](../decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md)
gave the connection list to the shell: the list is about the LAN *this window*
is on, and you cannot ask a remote daemon who its neighbours are and get your
own. The webview is told by event, the way pty output already reaches it.

**The address comes from the datagram, not the payload.** `R-I8` shipped
`0.0.0.0` in a record and found it undialable; the source address of a received
datagram is the one address known to work, so the beacon carries a port and the
receiver pairs it with `recv_from`'s answer. One datagram per interface, sent
from a socket bound to that interface's address to that interface's directed
broadcast, so the source is right on a multi-homed host too.

**Sending is on by default, and says presence rather than access.** Decided
2026-09-22 as [ADR-0039](../decisions/0039-the-daemon-announces-itself-by-default.md),
which reverses `R-I8`'s opt-in on the evidence of two months in which nobody
turned it on: every daemon beacons unless `beacon = false`, including a
loopback-bound one and the one a window hosts, and the beacon carries a
`reach` — `lan` when the bind is beyond loopback and token-gated, `ssh` with
the target when one is configured, `local` otherwise — so the Found list can
show every mogeung on the segment and say per row whether *Connect* is a press
or an `ssh -L` you run. The **bind** does not move: loopback by default, beyond
it only by a hand and with a token, as `R-I10` requires. Listening is passive
and discloses nothing, so the shell listens from launch; a window that opens the
panel sees a list that is already full.

**Phase 2 is the alternative ADR-0013 named, built as named.** The local
daemon *follows* daemons you chose, holds their tokens in a `0600` file of its
own, opens a client socket to each, mirrors their session summaries onto its own
board tagged with an origin, and forwards a session-scoped command to the
origin daemon and its answer back. The window sees one socket, one board and
one ranking, and every pane behind a click still asks one daemon — through the
one it is already talking to. Ranking merges because it is computed in a daemon;
collisions do not because each origin computed its own.

### Steps

**Phase 0 — probe (`R-I17`, hours)**

1. `mogeung-core/src/lan.rs`: `Beacon` (`v`, `machine`, `host`, `port`,
   `version`, `reach` with its ssh target, `home`, `live`, `waiting`, `seq`), `encode`/`decode`
   with a 1200-byte cap and a version gate, `PeerTable` with `heard(from, beacon,
   now)`, `sweep(now)` and the misses rule — three when planned, four since
   the three-host run. Pure; unit-tested.
2. `mogeungd/src/bin/beacon.rs`: `send` and `listen` modes over the codec, with
   `if-addrs` for per-interface broadcast addresses and `socket2` for
   `SO_BROADCAST` / `SO_REUSEADDR` / `SO_REUSEPORT`; `listen` also runs
   `discovery::Watcher` and prints both columns.
3. Run it across the mesh; write the numbers into A44 and the roadmap row.
   **Stop here and read the verdict** — if mDNS crosses as well as broadcast,
   phase 1's transport is the `Watcher` that already exists and the beacon
   is dropped, not shipped beside it.

**Phase 1 — found and connected (`R-I17`, days)**

4. `mogeungd/src/lan.rs`: the sender task, started in `server::run` beside
   `_advert` under `opts.beacon` (default **on**, `beacon = false` /
   `--no-beacon` off), every 15 s ± 2 s jitter, **as a burst of two datagrams
   a second apart** (ADR-0040 — the lossy end of the probe heard 62 % of
   single datagrams), one burst per interface,
   `reach` from the bind and `ssh_target`, counts from the state's own *needs
   you* predicate. The hosted daemon in `src-tauri/src/daemon.rs` reads
   `beacon` from `config.toml` too, or it stays invisible.
5. `src-tauri/src/lan.rs`: a listener thread bound to the beacon port with
   reuse flags, decoding into a `PeerTable`, sweeping each second, emitting
   `lan:peers` on change; `lan_peers` command for a window that opens later;
   this machine's id marked from `~/.mogeung/machine-id`.
6. `desktop/src/lib/lan.ts` + a `lan` store slice: `listen("lan:peers")`,
   `invoke("lan_peers")`, `isTauri()` guard; `desktop/src/ui/ConnectionsWindow.tsx`:
   the **Found on your LAN** section, Add → form, live dot on saved rows.
7. **Decision:** the hosted daemon honours `listen`, `token`, `advertise` from
   `config.toml` — `acquire` reads the config before it binds, and `admit` gets
   the token — *or* the guide tells a machine that wants to be found to run
   `mogeungd`. The first is the better product and touches
   [ADR-0009](../decisions/0009-the-window-may-host-a-daemon.md)'s reasoning;
   the second is a paragraph.
8. `docs/guide/remote.md`: the *Found* section rewritten for the panel that
   exists; `architecture.md`'s *daemons can announce themselves* paragraph
   gains the second transport, and an ADR if A44's verdict is that broadcast
   crosses where multicast does not.
9. `R-I16`'s verdict, taken in the desktop build against a real second daemon,
   which this phase makes cheap to have for the first time.

**Phase 2 — federation (`R-I18`, weeks; gated)**

10. A dated amendment to ADR-0013, on its own *revisit if* terms, before code.
11. `wire.rs`: `Session` gains `origin: Option<Origin { machine_id, host }>`;
    `ClientMsg::{Follow, Unfollow}`; `ServerMsg::Peers`.
12. `mogeungd/src/federation.rs`: a follow list at `~/.mogeung/peers.toml`
    (`0600`), one `tokio-tungstenite` client per followed daemon with reconnect,
    mirrored sessions tagged and ranked on the local board, session-scoped
    commands forwarded by origin and answers relayed; the `is_local` gate on
    write and run verbs already refuses across the hop.
13. The window: grouping by origin in the Attention list, a remote badge on
    rows and pane headers, Follow/Unfollow on Found and Saved rows, terminal
    through `R-I6`'s ssh route for remote sessions.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/lan.rs` (new) | beacon codec, `PeerTable`, constants — pure, shared |
| `crates/mogeung-core/Cargo.toml` | nothing new — serde is there |
| `crates/mogeungd/src/bin/beacon.rs` (new) | the phase-0 probe; kept as the operator's diagnostic |
| `crates/mogeungd/src/lan.rs` (new), `server.rs`, `Cargo.toml` | the sender task under `advertise`; `if-addrs`, `socket2` |
| `desktop/src-tauri/src/lan.rs` (new), `lib.rs`, `Cargo.toml`, `capabilities/default.json` | the listener, `lan:peers` events, `lan_peers` command; `socket2` |
| `desktop/src-tauri/src/daemon.rs` | step 7, if decided that way: config before bind, token into `admit` |
| `desktop/src/lib/lan.ts` (new), `store/index.ts`, `ui/ConnectionsWindow.tsx`, `lib/tauri.ts` | the Found section and its slice |
| `docs/guide/remote.md`, `docs/design/architecture.md`, `docs/product/assumptions.md`, `docs/product/roadmap.md` | the verdict, the panel that exists, the second transport |
| Phase 2: `wire.rs`, `mogeungd/src/federation.rs` (new), `state.rs`, `api.rs`, `desktop/src/wire/types.ts`, the queue and pane headers, ADR-0013 | as in steps 10–13 |

### Risks and unknowns

- **A44 is the whole bet**, and it can fail three ways: broadcast crosses and
  multicast does not (build the beacon); both cross (the beacon buys nothing —
  port the `Watcher` instead, which is smaller); neither crosses (the mesh
  isolates clients, and no protocol choice fixes that). The probe is cheap
  precisely so the expensive half is never built on the wrong answer.
- **Two daemons on one host** — two Claude homes — both send on one port and
  both are heard; keyed by machine id they collide. Key the table by
  `(machine, home)` from the start.
- **A spoofed beacon** puts a stranger's address in your Found list. Mitigated
  by the only thing that can mitigate it: nothing connects by itself, and the
  token is yours to type. Worth one line in the panel.
- **The hosted-daemon decision (step 7)** changes what a plain `mogeung` launch
  binds to when `config.toml` says `listen = "0.0.0.0:7717"`. That is the
  opt-in `R-I10` requires — the file was written by a hand — but it must say so
  in the window, the way the hosting notice does now.
- **Phase 2 puts one machine's daemon in another's trust path.** ADR-0013
  refused it *for now* on that ground, with one day of evidence under A24. The
  amendment has to say what evidence has accrued since, or it is the same
  decision reversed for no reason.
- **The beacon is a disclosure, and it is on by default.** *"host X runs
  mogeung, 1 session needs you"* every 15 s, on whatever network the laptop
  is on, unless its owner said `beacon = false`. ADR-0039 weighs it and keeps
  the fence tight — presence, counts, a route, never the token — and
  `architecture.md`'s *off and stays off* paragraph is rewritten when the
  sender lands, not before.

### Test strategy

- `lan.rs` codec: round-trip, oversize refused, version-gated, unknown fields
  ignored; `PeerTable`: a peer heard is listed with the datagram's source, three
  misses is gone, one more beacon revives it, two homes on one machine are two
  rows.
- Daemon sender: on by default and off under `beacon = false`; a loopback
  bind sends with `reach: local` or `ssh`, never `lan`; the counts equal the
  *needs you* predicate over a fixture board.
- Shell listener: a datagram on loopback in a test reaches the table and emits
  once per change, not per beacon.
- Window: a `lan:peers` event draws a row; Add fills the form and dials
  nothing; a gone peer stays greyed; this machine is marked; a browser tab says
  so. Component tests, since `R-I16` already has the harness.
- **The verdicts are measured, not asserted**: phase 0's numbers, and `R-I16`
  in the desktop build.

## Notes

*Filled during implementation.*

Planned 2026-09-21. The inventory above is the check the ask asked for, and it
turned up one thing worth saying twice: the window's own daemon is invisible to
the LAN no matter what this feature builds, until step 7 is decided.

**Phase 0 built 2026-09-22**, steps 1 and 2. `mogeung_core::lan` is the codec
and the table, pure, twelve tests; `--bin beacon` is the sockets around it.
Two things changed on contact with a real machine:

- **A beacon is heard once per interface**, because the sender sends once per
  interface and Linux loops a broadcast back to every local listener. The
  first smoke test reported *16/6* — sixteen datagrams for four beacons over
  five interfaces, four of them Docker bridges. So the table counts
  **beacons** (distinct sequence numbers) apart from datagrams, and the
  probe's summary reads *3/5 beacons (15 datagrams)*. The shell will want the
  same distinction for the same reason.
- **`send` advertises over mDNS as well**, by default, from the same host and
  the same identity — so one `listen` compares the two transports from one
  sender, rather than needing a daemon started with `advertise = true` beside
  the probe. On one host both arrived: the beacon within a second, mDNS
  resolved after two.

What the one-host run cannot say is anything about the mesh, which is the
whole question. The run that can:

```sh
# on a host wired to the base, and on one on a satellite's Wi-Fi
cargo run -q -p mogeungd --bin beacon -- send
# on a third host, or on either of those — ten minutes, then the summary
cargo run -q -p mogeungd --bin beacon -- listen --for 600
```

Read the summary's `beacons` column against 40, its longest gap against 45 s
(four misses since the second run), and the mDNS column for whether the same
host resolved at all.
Those three numbers, on both sides of the mesh, are A44's verdict. `--limited`
on `send` adds `255.255.255.255` for the run where the directed address fails.

**The run, 2026-09-22.** Laptop `kinz-xps14` (10.0.0.27, Wi-Fi, power save
on) and Mac mini (10.0.0.39, Wi-Fi on `en1`), both on the Orbi; each ran
`listen --for 600` and `send --count 41`, so both directions were measured at
once, with the mDNS advert and browse running beside them. The summaries:

```
laptop:  Kinzs-Mac-mini.local  25/40 beacons   longest gap 45 s   live
         (one GONE at +447.1 s, BACK at +447.4 s with seq 31)
         mdns: 1 host resolved — itself
mac:     kinz-xps14            36/40 beacons   longest gap 44 s   live
         mdns: 1 host resolved — itself
```

Three readings, in the order they were reached:

- **Broadcast crosses**, and it is lossy the way Wi-Fi is lossy: 90 % into
  the always-on Mac, 62 % into a laptop whose Wi-Fi is in power save — a
  client in power save wakes at the DTIM interval and misses broadcast frames
  sent between. The loss is the client's, not the mesh's. At 62 % the
  three-miss rule flickered once in ten minutes, which is why phase 1 sends a
  burst of two.
- **`mdns-sd`'s browse resolved nothing, either way, in ten minutes.** The
  first reading was *multicast does not cross*, and it was wrong: the Mac's
  own `dns-sd -B _mogeung._tcp local.` listed the laptop's record within a
  second of the laptop advertising, both hosts resolve each other's `.local`
  names, and the reverse cross-check — the Mac advertising, the laptop's
  watcher browsing — again resolved nothing. So the advert works and the
  library's browse does not, beside avahi on the laptop and mDNSResponder on
  the Mac. A finding about `R-I8`'s implementation, not the network.
- **The spec's decision rule had no branch for this.** It wrote *broadcast
  only* and *both* and got *both cross, one implementation delivers*.
  [ADR-0040](../decisions/0040-discovery-is-the-beacon.md) records the choice:
  the beacon, as a burst of two; the mDNS advert kept opt-in as a record for
  other tools; the browse retired.

One more thing the run said in passing: `Kinzs-Mac-mini.local` and
`spark-7ecc.local` resolve from the laptop, so a third host exists on this
mesh for the wired-to-the-base leg the spec asked for and this run did not
have.

**Three hosts, later the same day.** The spark (`spark-7ecc`, arm64 Linux,
wired gigabit at 10.0.0.183 **and** Wi-Fi at 10.0.0.184, both up) joined, its
probe built from a fresh clone. All three listened for 600 s and sent 41,
and the probe's summary had learnt to count per source address in between.

```
into the mac:    laptop 40/40          spark 40/40  (wired ×39, wifi ×39)
into the spark:  laptop 40/40 (×75)    mac    40/40 (×78)
into the laptop: spark  24/40  (wired ×15, wifi ×23)   longest gap 60 s
                 mac    25/40                          longest gap 60 s
                 GONE/BACK: mac ×3, spark ×1
mdns (mdns-sd):  spark resolved both (6 s, 9 s); laptop and mac each resolved
                 the spark (0 s) and never each other
```

Three corrections to the first run's readings, in order of weight:

- **The wire is not the variable; the receiver is.** The spark's wired
  address reached the laptop 15 times and its wireless one 23 — both bad,
  neither better — while the same beacons reached the Mac and the spark 40 of
  40. A laptop in Wi-Fi power save misses broadcast frames whoever sends them.
  Four flickers in ten minutes at three misses; `MISSES_BEFORE_GONE` is four
  now, with the burst of two, and the Found list will still show a
  *gone*-and-*back* honestly when it happens.
- **The spark hears everything twice.** 75 and 78 datagrams for 40 beacons: a
  broadcast reaches both of its interfaces and the socket bound to `0.0.0.0`
  receives it on each. Free redundancy for a dual-homed listener, and the
  reason the table counts sequence numbers rather than arrivals.
- **The mDNS reading was wrong, and A44's second half was right.**
  `mdns-sd` resolved every wired-to-Wi-Fi pair within nine seconds and neither
  Wi-Fi-to-Wi-Fi pair in ten minutes, in both runs. The library works where
  multicast reaches; this mesh does not carry multicast between two Wi-Fi
  clients; and the `dns-sd` sighting on the Mac was its responder's cache of
  the first run's record, listed at the instant the browse began. Recorded as
  an amendment to [ADR-0040](../decisions/0040-discovery-is-the-beacon.md),
  whose decision it strengthens: the beacon is the only transport that reached
  every leg.

**Phase 1, step 4 built 2026-09-22 — the sender.** `mogeungd/src/lan.rs`:
the daemon's beacon task, started in `server::run` beside the mDNS advert
under `Options.beacon`, which is **on by default** and off under
`beacon = false` or `--no-beacon` (the order is tested with the rest of
`resolve`). Every 15 s ± 2 s of jitter derived from the machine id and the
sequence number, a burst of two datagrams a second apart to every interface's
directed broadcast, the interfaces re-read before each beacon so a laptop that
changed networks announces on the one it is on. `reach` from the bind, the ssh
target riding only when it is the route, and the counts from
`AppState::beacon_counts` — `alive`, and `needs_human()` over the same
`rank()` the queue publishes, so the number on a found row is the tab's
number. The window-hosted daemon reads `beacon` from the config file and is
otherwise on too, or the window-only machine would be the one nobody finds.
The probe's `send` now rides the same helpers, so there is one list of
interfaces and one rule for `reach`. Pinned by an integration test that
receives the burst on loopback: one sequence number twice, `reach: local`, the
board's counts, and no `token` in the bytes.

Not yet: the shell's listener (step 5) and the Found section (step 6), so
nothing of ours hears a beacon yet except the probe.

**Phase 1, steps 5 and 6 built 2026-09-22 — the listener and the Found
section.** `src-tauri/src/lan.rs` binds udp/7717 with the reuse flags at app
start, folds every datagram into a `PeerTable`, sweeps each second, and emits
`lan:peers` on every beacon heard and every peer gone — per beacon rather than
per datagram, since the table folds a burst and a multi-homed sender, and a
beacon *is* a change: it moves *last heard*. `lan_peers` answers a window
that opens later with the table as it stands, or with why there is no listener
(a bind that failed is shown, not fatal). The window keeps the list in its own
state while the panel is open rather than in the store — nothing else renders
from it — and ticks *12 s ago* on its own clock from the epoch stamp each row
carries. `lib/lan.ts` holds the pure decisions: a row dials the datagram's
source and the announced port, a saved connection matches by any address it
was heard from, the words for `reach`. Seven component tests and six pure ones.
`remote.md`'s Found section now describes this panel instead of the egui one.

**Verified in tests and in a browser tab, not yet in the desktop build**,
which is what the ✅ waits for: the shell's listener and the hosted daemon's
beacon need `install.sh` and a second daemon on the LAN, and that is
dogfooding. Two of phase 1's acceptance lines stay open for it — *every daemon
is heard within 15 s* and `R-I16`'s verdict — and step 7, whether the hosted
daemon should honour `listen` and `token` from the file, stays a decision.

**First sight, 2026-09-22.** Both machines installed the build and each saw
the other within a beacon, the Mac already reachable with a token. The first
thing said was *"we should remove the local machine"* — the row marked *(this
machine)* was noise above the one row that mattered — so it is off the list
now; the shell keeps it, since hearing yourself is how a listener proves it
works, and the probe still shows it. The `needs you` count carries the tab's
rule, ended-and-unreviewed sessions included, which read as large (126) on a
board that has not been reviewed in a while; left as the tab's number for now.

**First connection, 2026-09-22 — two findings and a redesign.** With the
Mac's daemon bound to the LAN and its token typed, the switch worked and the
board came up **empty of live sessions**. Not the connection: `R-J93`'s tmux
rule, on by default, hides any session without a tmux pane from *needs you*
and *live*, and the Mac had no tmux sessions at all — one `claude` started
outside it. The panel says *N hidden — not under tmux* and the *all* scope
shows them, and the rule is the same for a remote daemon as a local one, since
a pane is what the window can host either way (`R-I6` reaches a remote tmux
over ssh; a session outside tmux it cannot reach anywhere). Worth saying
because the first reading of an empty board after a switch is *the switch
failed*. The second finding was the panel itself: one column of full forms
was unreadable with three daemons, the token could not be checked, and there
was no way to ask a daemon who it was without switching — `R-I19`, built the
same day.

**Correction, the same evening.** The first Test against the Mac was refused
with a token in place, and the reason was in the daemon: the token contained
a character `URLSearchParams` percent-encodes, and the gate compared the
encoded bytes against the plain token — proved with a throwaway daemon and a
token of `ab/cd+ef`, raw 200, encoded 401, Bearer 200. So the entry the panel
composed was refused every time, Connect through it included; the five
sockets to the Mac this laptop showed were WebKit's kept-alive leftovers of
handshakes that had failed, and the empty board after "connecting" was most
likely this and not only `R-J93`'s rule. The gate decodes now
(`wire-protocol.md`, Security), and the reading above stands corrected: the
tmux rule is real and would have hidden the Mac's one session, but it was
not the first thing wrong.

**And the tmux reading was wrong on its own terms.** The Mac has five tmux
sessions; `tmux ls` over a plain ssh reported none because `ssh host cmd`
runs a non-login shell and Homebrew's `tmux` is not on that PATH — the very
gap `spawnAs` in `lib/tmux.ts` documents and works around with a login shell
and a fallback path. So the empty board after the first switch was the 401
and nothing else. Recorded because the wrong reading was written down twice
before the right one, and the next person to see an empty board deserves
the order the causes were actually found in: check the connection first.

**First click on a remote session, 2026-09-22**: the Agent pane refused —
*"that daemon is on another machine, and has not been told how to reach
it"* — which is `R-I6`'s rule working as written: `ssh_target` in the Mac's
config is the one thing missing, and the pane's wrapper was dry-run from the
laptop and found tmux over the login shell it composes.

**Step 7 decided and built, 2026-09-22.** The hosted daemon reads `listen`,
`token`, `ssh_target` and `advertise` from `config.toml` before it binds;
`admit` refuses a public bind with no token and the refusal reaches the window
as a sentence; the hosting status carries what was bound, the port and the
token, and the window composes its dial URL from them — in the store for the
session, never in `localStorage`. Attaching reads the same file, so a daemon
started by hand with that token is probed and dialled with it. ADR-0009's
amendment has the reasoning and the one posture it cannot express, findable
but not connectable, which is a `beacon`-only key if it is ever wanted.

**Step 7's cost, found the same evening (`R-J102`).** Making the window's
own daemon token-gated meant the window had to dial it with the token — and
it did, at launch, and nowhere else. Switching to a remote and back dialled
the saved local entry with no token and was refused, which read as *the
daemon died*. The dial is now composed on every switch (`lib/dial.ts`), the
window hosts this machine's daemon at every launch whatever it shows, and the
remembered address carries no token. The top-bar switcher (`R-I20`) followed,
with a status per saved daemon asked of the shell.
