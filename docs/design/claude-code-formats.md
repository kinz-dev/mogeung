---
title: Claude Code's on-disk formats
status: active
updated: 2026-07-30
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

Append-only, one JSON object per line. `<slug>` is the cwd with separators
replaced.

**Every `type` is classified — there is no catch-all.** `adapter::HANDLED` and
`adapter::KNOWN_IGNORED` between them must account for every type seen, and
anything else raises an alert. Counts below are from the author's corpus on
2026-07-25: 52 transcripts, 68 MB, 20,648 lines.

| `type` | Seen | Disposition |
|---|---|---|
| `assistant` | 9,019 | `text`, `thinking`, `tool_use` blocks; `usage` token counts; `isApiErrorMessage` |
| `user` | 5,292 | Human turns and tool results. String content = a real prompt; an array may carry `tool_result` blocks instead |
| `last-prompt` | 1,143 | Most recent human prompt. 42 of these carry no `lastPrompt` field |
| `ai-title` | 1,070 | Claude Code's generated title — the best session label available |
| `file-history-delta` | 259 | `trackingPath` — a file the session is tracking edits to |
| `mode` | 1,119 | Ignored — session settings chatter |
| `attachment` | 876 | Ignored — already reflected in the message that used it |
| `permission-mode` | 820 | Ignored |
| `file-history-snapshot` | 445 | Ignored — see [ADR-0004](../decisions/0004-git-for-diffs-not-file-history.md) |
| `system` | 406 | Ignored — `turn_duration`, `local_command` and similar |
| `queue-operation` | 190 | Ignored — queued follow-ups, before they become turns |
| `pr-link` | 6 | Ignored |
| `frame-link` | 2 | Ignored |

The last three were **found by the canary**. They existed in real transcripts
throughout v0.2 and were swallowed by a catch-all arm; nothing recorded that
they existed, so nobody could have known whether they mattered.

Common top-level fields: `timestamp`, `cwd`, `gitBranch`, `sessionId`,
`version`, `isSidechain`, `uuid`, `parentUuid`. Also seen: `effort`, `slug`,
`agentId`, `promptId`, `requestId`, `toolUseResult`.

- `gitBranch` is `"HEAD"` when detached — treated as absent.
- `isSidechain: true` marks subagent messages. They count toward tool totals but
  never become the session's headline activity.
- `tool_result` blocks sometimes omit `is_error`; absence means "not an error".
- `version` is **per line**, and reflects the release that wrote it. A fortnight
  of transcripts routinely spans a dozen releases, so version ordering must come
  from each line's own `timestamp`, never from the order files are scanned.

### Size

The largest transcript in the corpus is 11.2 MB. Files over
`MAX_TRANSCRIPT_BYTES` (4 MiB) are followed from a line boundary near their end
rather than read whole, and the skipped span is reported as a
`history_skipped` alert — see [health-and-canary.md](health-and-canary.md).

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

`parse_line` returns a `LineOutcome`, never a bare `Option`, precisely so that
"we chose to skip this" and "we have never seen this" cannot be confused. Every
outcome is counted: see [health-and-canary.md](health-and-canary.md).

## Learned in the 2026-07-29 sweep

- **Rate limits are prose, not events.** No `rate_limit_event` exists in
  any of 235 local transcripts; a hit arrives as an assistant message
  with `message.model == "<synthetic>"` and all-zero usage, its reset
  time embedded in the text. The parser keys on that signature.
- **Subagent transcripts nest**: `<session>/subagents/agent-*.jsonl`,
  plus a sibling `tool-results/` overflow dir. A flat glob undercounts;
  the usage and insight scanners walk recursively and attribute subagent
  burn to the parent.
- **`history.jsonl`** is uniform: `display`, `pastedContents` (values
  carry inline `content` *or* only a `contentHash`), `timestamp` (unix
  millis, monotonic), `project` (absolute cwd; its directory-name
  encoding is lossy — go history→dir, never back), `sessionId`.
- **`usage.iterations`** can be `null` or an array of per-iteration
  sub-objects; summing both `usage` and its iterations double-counts.
