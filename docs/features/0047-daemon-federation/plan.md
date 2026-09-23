---
title: Daemon federation — plan
status: shipped
updated: 2026-09-23
---

# Plan — daemon federation

*Drafted 2026-09-23, before any code. The estimate the choice was made on:
three to four weeks read-only, one to two more for write verbs across the hop.*

## Approach

**This machine's daemon becomes a client of its peers, and the window does
not change shape.** The daemon keeps a *follow list* (`~/.mogeung/peers.json`,
`0600` — ADR-0036's amendment). For each followed peer it holds one **watch
socket**, reads the peer's snapshot and updates, and mirrors every session onto
its own board with an `origin` and a namespaced id. The board is ranked once,
published once, notified once — the window, the tray and the notifier see one
daemon, as ADR-0013 requires, and none of them learn anything new except that
a session can have an origin.

**Commands for a remote session are forwarded, and the answers come back the
same way they went.** The wire has no request ids: a daemon answers on the
socket that asked, or broadcasts. So a forwarded command cannot be matched to
its answer on a shared socket. The daemon instead opens a **relay socket per
local client and peer**, lazily, the first time that client asks something of
that peer: the command goes up it with the id un-namespaced, and whatever comes
down it that is not board traffic goes back to that one client with ids
namespaced again. One relay per client and peer means no correlation table, and
a peer's reply lands on exactly the window that asked — the same property a
direct socket has. **The alternative, request ids on the wire**, is a protocol
change every daemon and every client would have to agree on across upgrades;
rejected for now, and the thing to revisit if relays turn out to be expensive.

**Every message type is classified, exhaustively.** 54 of 94 `ClientMsg`
variants and 29 of 66 `ServerMsg` variants carry a session id. Each `ClientMsg`
is one of: *local only* (settings, notes, scratch, chat — never forwarded),
*read, forwardable* (phase B), *write or run, refused across the hop* (until
phase C). Each `ServerMsg` is *board* (mirrored via the watch socket), *answer*
(relayed to the asking client) or *not ours* (dropped). Written as `match`es
with no wildcard arm, so a variant added next month fails to compile until
somebody decides — `sweep`'s `HANDLED`/`KNOWN_IGNORED` discipline, applied to
the wire.

## Steps

**Phase A — follow and mirror** (`R-I21`, ~1½ weeks). The A45 test, and the
foundation everything else stands on.

1. `wire.rs`: `Session.origin: Option<Origin { machine_id, host, ssh_target,
   stale }>`, serde-defaulted so older clients ignore it; `ClientMsg::{Follow {
   url, token, name }, Unfollow { machine_id }, Peers {}}`;
   `ServerMsg::Peers { peers: Vec<PeerState> }` — name, address, state,
   counts, never a token. Mirror in `desktop/src/wire/types.ts`.
2. `mogeungd/src/federation.rs`: the follow list (read, write atomically,
   `0600`), a watch client per peer on `tokio-tungstenite` (a dev dependency
   today, a real one after this) with reconnect and backoff, the id namespace
   (`<machine_id>/<session_id>`, one function each way, round-trip tested), and
   the fold from a peer's snapshot and updates into `AppState.sessions`.
3. `state.rs`: mirrored sessions ranked with local ones in `publish_queue`,
   left out of `detect_collisions`, marked stale and dropped from the ranking
   when their peer is quiet for a minute; `api.rs`: `Follow`/`Unfollow`
   accepted from loopback clients only; mirrored sessions filtered from every
   message to a non-loopback client (**no re-export**).
4. The window: *Follow* on the Connections panel and the switcher, a
   *following* state beside each; the Attention list grouped by machine
   (beside `groupByRepo`, this machine first), a remote badge; clicking a
   remote session switches daemons and selects it — the fallback phase B
   replaces.
5. Tests: a two-daemon end-to-end in one process — follow, mirror, rank,
   unfollow, peer lost and back, no re-export to a non-loopback socket — plus
   the namespace round-trip and the follow list's mode bits.
6. **Dogfood a week; A45 takes its verdict.** Stop here if it is `REFUTED`.

**Phase B — open remote sessions in place** (`R-I22`, ~2–2½ weeks).

7. The classification: every `ClientMsg` and `ServerMsg` variant, exhaustively
   matched, with the few that need more than an id rewrite (the git family
   echoes `repo`; `Workspace` carries roots on the peer's filesystem) handled
   by name.
8. Relay sockets: opened lazily per (client, peer), closed with the client,
   answers relayed and re-namespaced; the peer's own board traffic on a relay
   dropped, since the watch socket already carries it.
9. Write and run verbs on a remote session refused in words naming A24.
10. The window: panes stop switching on click; the Agent pane and terminals
    take their reach from the session's `origin.ssh_target` rather than the
    daemon's identity (`lib/tmux.ts`'s `reachFor`, per session).
11. Tests: the two-daemon harness extended to every forwardable read verb, one
    test per family; a compile-time guarantee that nothing is unclassified.

**Phase C — write verbs across the hop** (`R-I23`, ~1–2 weeks, gated on A24).

12. Only once A24 has a verdict. Git writes with the owning machine's
    live-session warnings, `RunStart` under the peer's own `--allow-run` gate,
    `SendToSession` under ADR-0003's 2026-08-29 terms — each checked against
    ADR-0012, ADR-0025 and ADR-0003 for a verb crossing a hop, amended where
    needed.

**Phase D — hardening and verdicts** (`R-I24`, ~3–4 days).

13. A capability in `DaemonIdentity` so a follow to an older peer is refused
    with the version it needs; relay and watch costs measured with three peers
    (a peer's snapshot is ~1.2 MB, and a relay opens one per client); the
    design docs — `architecture.md`, `wire-protocol.md`, `remote.md` — and the
    verdicts for `R-I16`, `R-I17`, `R-I18`.

## Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | `Origin` on `Session`; `Follow`, `Unfollow`, `Peers`; `PeerState` |
| `crates/mogeungd/src/federation.rs` (new) | follow list, watch clients, id namespace, the fold, relays, the classification |
| `crates/mogeungd/src/state.rs` | mirrored sessions in the ranking, out of collisions, stale handling |
| `crates/mogeungd/src/api.rs` | loopback-only follow; no re-export; forwarding through relays |
| `crates/mogeungd/Cargo.toml` | `tokio-tungstenite` from dev to real dependency |
| `desktop/src/wire/types.ts` | the mirror of the above |
| `desktop/src/ui/ConnectionsWindow.tsx`, `ConnectionSwitcher.tsx` | *Follow*, *following* |
| `desktop/src/ui/QueuePanel.tsx`, `lib/queue.ts`, `store/prefs.ts` | group by machine, remote badge |
| `desktop/src/lib/tmux.ts` and the three `reachFor` callers | reach per session, from its origin |
| `crates/mogeungd/tests/federation.rs` (new) | two daemons in one process |
| `docs/design/architecture.md`, `wire-protocol.md`, `docs/guide/remote.md` | the design as built |

## Test strategy

Two daemons in one test process, each with its own home and port, one
following the other — the `e2e.rs` harness twice. What must fail today and
pass after: a follow mirrors; a peer's waiting session ranks above a local
idle one; a non-loopback socket never sees a mirrored session; a peer lost
greys and leaves the ranking; a forwarded read comes back to the client that
asked and to no other; a write across the hop is refused in words. And one
guarantee by construction: no `ClientMsg` or `ServerMsg` variant compiles
without a classification.

## As built (2026-09-23)

Where the build departed from the steps above, and why:

- **Step 2's fold goes into a map of its own, not into `AppState.sessions`.**
  Every pass that walks the board — liveness, collisions, persistence,
  retention — would have needed a guard against another machine's session;
  a separate map needs none, and the merge happens only at the outputs:
  `snapshot`, `rank_board` behind `publish_queue`, and the notifier.
- **The id is `@<machine_id>:<id>`**, not `<machine_id>/<id>`. A machine id
  is hex and never holds a colon, so splitting at the first one recovers an
  id that has its own (`pane:%3`); and no id a CLI mints starts with `@`, so
  one character says *mirrored* without a lookup.
- **Step 1's messages as shipped:** `Follow { id, name, url, token,
  allow_writes }` keyed by the connection's own id, `Unfollow { id }`,
  `SetPeerWrites { id, allow }`, `FetchPeers`, and `Peers { peers }`.
  `DaemonIdentity.federation` carries the protocol for step 13.
- **Step 4's click-to-switch fallback was not built**; phase B came in the
  same change.
- **Step 12 is behind a per-follow consent instead of A24's verdict**, at the
  owner's direction — [ADR-0013](../../decisions/0013-one-window-one-daemon.md)'s
  second amendment of 2026-09-23.
- **Staleness is measured from when the peer was last heard**, and the watch
  loop wakes at the moment it is due; measured from the end of the silence
  window and checked between redials, it came in nearly two minutes late for a
  peer asleep on Wi-Fi.
- **Step 13's measurement** is one peer, on one box, in debug builds — see
  [architecture.md](../../design/architecture.md#one-daemon-following-others-r-i18-feature-0047-2026-09-23).
  The relay was cheap there (10–30 ms over direct on a warm socket, ~140 ms to
  open one); three peers over Wi-Fi remain to be measured, and request ids on
  the wire stay the alternative if that bites.

## Risks

- **Relay cost.** A relay socket is sent the peer's full snapshot on connect.
  One per client and peer is bounded, and it is measured in phase D; if it
  bites, request ids on the wire are the alternative, and a bigger change.
- **Version skew.** Three machines upgrade at different times. Every new field
  is serde-defaulted, and phase D refuses a follow the peer cannot serve.
- **A24.** Two hops of a bet with one hop of evidence. Phases A and B read
  only, phase C waits for the verdict — which means the six-week estimate has
  a gate in it that the calendar does not control.
- **A45 could be refuted.** Then phase A is the product, and it is still worth
  having: one list, notifications and the tray across machines.

## Out of scope

The aggregating window; automatic following; transitive federation;
cross-machine collisions; merging two checkouts of one repository.
