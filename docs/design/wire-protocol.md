---
title: Wire protocol
status: active
updated: 2026-07-25
covers:
  - crates/mogeung-core/src/wire.rs
  - crates/mogeungd/src/api.rs
---

# Wire protocol

One WebSocket carries live state: commands in, events out. Bulk reads are also
available as plain REST so the daemon is curl-able without a UI.

## Commands (`ClientMsg`)

| Command | Effect |
|---|---|
| `Subscribe` | Re-send the full snapshot |
| `SetHunkReviewed` | Mark or unmark one hunk |
| `ReviewAll` | Mark every hunk in the current diff |
| `RefreshChange` | Recompute a session's diff |
| `FetchEvents` | Replay stored transcript events from `since` |
| `ForgetSession` | Stop tracking; drop review state |
| `LaunchTerminal` | Open a real interactive `claude`, optionally in a new worktree |
| `Rescan` | Scan now instead of waiting for the next poll |
| `FetchHealth` | Ask what mogeung can and cannot currently see |

**Note what is absent:** nothing starts, steers or stops an agent
([ADR-0003](../decisions/0003-observe-do-not-spawn.md)).

## Events (`ServerMsg`)

`Snapshot` · `SessionUpdated` · `SessionRemoved` · `Events` · `Queue` ·
`ChangeUpdated` · `Health` · `Error`

`Health` is pushed after **every** scan, unsolicited. A client should never have
to ask whether the board it is showing is complete — see
[health-and-canary.md](health-and-canary.md).

## Design rules

**Commands are fire-and-forget.** Their effect returns on the event stream like
any other change, so clients stay pure projections with no correlation layer.

**Snapshot is unsolicited on connect**, so a client is useful before it sends
anything and reconnects self-heal.

**A slow client is dropped, not tolerated.** On broadcast lag the client is told
to reconnect rather than wedging the channel for everyone else.

**Malformed commands produce an error, not a disconnect.** Pinned by a test.

## REST

```
GET  /api/health          # liveness *and* whether it is still seeing everything
GET  /api/queue
GET  /api/sessions
GET  /api/sessions/{id}
GET  /api/sessions/{id}/events?since=N
GET  /api/sessions/{id}/change
POST /api/sessions/{id}/review_all
POST /api/sessions/{id}/review     {"anchor": "...", "reviewed": true}
POST /api/rescan
```

`/api/health` returns a `headline`, `blind_ratio`, plain-language `alerts`, and
the full `detail` object. It is deliberately curl-able: "is the board empty
because nothing is happening, or because mogeung went blind?" should not require
a window.

## Security

**No authentication.** The daemon binds localhost and anyone who can reach the
port can read your transcripts and open terminals on your machine. Do not
expose it. A remote daemon (roadmap `R-I4`) requires solving this first.
