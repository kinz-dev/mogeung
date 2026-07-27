---
title: Wire protocol
status: active
updated: 2026-07-27
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
| `Snooze` | Silence a session for N minutes; 0 clears it |
| `FetchReviewDebt` | How much of a repo's agent output nobody has read |
| `FetchBlastRadius` | What else references the symbols a file's diff changed |
| `FocusTerminal` | Bring a live session's Terminal tab to the front |
| `ListDir` | One directory of the session's worktree, for the explorer (`R-B24`) |
| `FetchFile` | One worktree file, capped and text-only — there is no write counterpart, by design |
| `ListTree` | Every worktree file path in one answer, for go-to-file (`R-B25`); gitignore-aware, capped at 20k with a `truncated` flag |
| `SearchContent` | Lines matching a literal query across the worktree (`R-B25`); smart-cased, capped at 500 matches |
| `GitLog` | A page of the session repo's commit log (`R-D10`) |
| `GitShow` | One commit's diff, in `Change`'s file/hunk shapes; the sha is validated as hex before git sees it |
| `GitStatus` | The repo's uncommitted state, staged and unstaged distinguished |
| `GitDiffFile` | One uncommitted file against `HEAD` (`/dev/null` when untracked) |
| `GitBlame` | Per-line authorship of a worktree file, capped at 20k lines |

**Note what is absent:** nothing starts, steers or stops an agent
([ADR-0003](../decisions/0003-observe-do-not-spawn.md)).

`FocusTerminal` is not an exception. It moves *your* window; the agent is
untouched and nothing is typed. Nor is "copy as prompt" a command at all — the
client builds the text and puts it on your clipboard, and you paste it
([ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md)).

## Events (`ServerMsg`)

`Snapshot` · `SessionUpdated` · `SessionRemoved` · `Events` · `Queue` ·
`ChangeUpdated` · `Health` · `ReviewDebt` · `BlastRadius` · `DirListing` ·
`FileContent` · `TreeListing` · `ContentMatches` · `GitCommits` ·
`GitCommitDiff` · `GitLocalChanges` · `GitFileDiff` · `GitAnnotation` ·
`Error`

The `Git…` family (`R-D10`) is **read-only by protocol**: there is no
staging, commit, checkout or any other verb that mutates a repository, and
none may be added without an ADR — the observer rule, one layer down.

`ListDir` and `FetchFile` paths are relative to the session root (repo root
when known, else cwd); the daemon canonicalises and refuses anything that
escapes it, symlinks included. `FileContent` is capped at 256 KiB with a
`truncated` flag, and binary files are refused — the explorer is a viewer, and
the daemon offers nothing that writes.

`ListTree` and `SearchContent` walk with the same containment, `.git` always
excluded and gitignore honoured when the root is a repo. Both run on the
blocking pool so a monorepo walk cannot wedge the event loop. Search skips
binary and oversized files *silently* — a search that errors on the one
unreadable file answers nothing. `ContentMatches` echoes the query so a client
can drop the answer to a search the user has since replaced — the stray-session
rule, applied to superseded queries.

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
GET  /                    # the thin web client (R-C3)
GET  /api/health          # liveness *and* whether it is still seeing everything
GET  /api/queue
GET  /api/repos
GET  /api/repos/{repo}/debt
GET  /api/sessions/{id}/blast?path=...
GET  /api/sessions/{id}/ls?path=...    # explorer (R-B24); path optional
GET  /api/sessions/{id}/file?path=...
GET  /api/sessions/{id}/tree           # every file path (R-B25)
GET  /api/sessions/{id}/search?q=...   # literal content search (R-B25)
GET  /api/sessions/{id}/git/log?skip=N&limit=N   # R-D10, all read-only
GET  /api/sessions/{id}/git/show?sha=...
GET  /api/sessions/{id}/git/status
GET  /api/sessions/{id}/git/diff?path=...
GET  /api/sessions/{id}/git/blame?path=...
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

## The web client (`R-C3`)

`GET /` serves one self-contained HTML file that speaks the same WebSocket as
the desktop UI. No build step, no framework, no CDN — a phone client that needed
`npm install` would never get maintained.

Scope is deliberately "triage from the sofa": see the queue, read a diff, mark
hunks read, snooze. Anything wanting a keyboard and a real screen stays in the
desktop client. It is the same authority model — a projection with no local
state ([ADR-0001](../decisions/0001-rust-core-with-egui-ui.md)) — which is
exactly what made it cheap.

## Security

**No authentication.** The daemon binds localhost and anyone who can reach the
port can read your transcripts and open terminals on your machine. Do not
expose it. A remote daemon (roadmap `R-I4`) requires solving this first.

Using the web client from a phone means binding beyond localhost, which means
**anyone on that network has full control of your machine**. The daemon logs a
warning at startup when `--listen` is not a loopback address. Treat it as
suitable for a trusted home network and nothing more; a VPN or SSH tunnel is the
correct answer until authentication exists.
