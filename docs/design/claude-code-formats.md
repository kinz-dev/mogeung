---
title: Claude Code's on-disk formats
status: active
updated: 2026-07-25
covers:
  - crates/mogeungd/src/watcher.rs
  - crates/mogeungd/src/adapter.rs
---

# Claude Code's on-disk formats

**These are private, undocumented files.** A CLI update can change them without
warning. This is the project's top operational risk
([A4](../product/assumptions.md)).

Verified against Claude Code **2.1.219 / 2.1.220** only.

## `~/.claude/sessions/<pid>.json` — live registry

One file per running session, keyed by process id.

```json
{
  "pid": 46614,
  "sessionId": "a3413ae1-794e-4a46-b7f7-f4a6ef8a52d8",
  "cwd": "/Volumes/t7touch/projects/mogeung",
  "startedAt": 1784995957755,
  "version": "2.1.220",
  "kind": "interactive",
  "name": "mogeung-95",
  "status": "busy",
  "statusUpdatedAt": 1784996682891
}
```

`status` is `busy` or `idle`. **`idle` on a live process means it is waiting for
the human** — this is the single most valuable field in the system, and the
reason the observer model beats the spawning model.

**These files are not cleaned up on exit.** Liveness must be checked against the
OS (`kill(pid, 0)`), or every session that ever ran looks alive. Pinned by a
test.

## `~/.claude/projects/<slug>/<session-id>.jsonl` — transcripts

Append-only, one JSON object per line. `<slug>` is the cwd with separators
replaced.

Event types observed, and what we do with each:

| `type` | Used for |
|---|---|
| `user` | Human turns and tool results. String content = a real prompt; an array may carry `tool_result` blocks instead |
| `assistant` | `text`, `thinking`, `tool_use` blocks; `usage` token counts; `isApiErrorMessage` |
| `ai-title` | Claude Code's generated conversation title — the best session label available |
| `last-prompt` | Most recent human prompt |
| `file-history-delta` | `trackingPath` — a file the session is tracking edits to |
| `system` | `turn_duration` and similar; ignored |
| `mode`, `permission-mode`, `attachment`, `file-history-snapshot` | Bookkeeping; ignored |

Common top-level fields: `timestamp`, `cwd`, `gitBranch`, `sessionId`,
`version`, `isSidechain`, `uuid`, `parentUuid`.

- `gitBranch` is `"HEAD"` when detached — treated as absent.
- `isSidechain: true` marks subagent messages. They count toward tool totals but
  never become the session's headline activity.

## `~/.claude/file-history/<session-id>/<hash>@v<n>`

Pre-edit file backups. **Not used** — the hashed filenames have no reliable path
mapping. See [ADR-0004](../decisions/0004-git-for-diffs-not-file-history.md).

## `~/.claude/history.jsonl`

Every prompt ever typed, with `display`, `project`, `sessionId`, `timestamp`.
2,084 entries on this machine. Currently unused; it is the basis for roadmap
section F.

## Parsing posture

Unknown event types and unexpected shapes are **ignored, never fatal**. The
realistic failure mode is therefore a degraded board rather than a crash — which
is also the dangerous one, because it looks like "nothing is happening".

Roadmap `A1` (format canary) exists to make that failure loud.
