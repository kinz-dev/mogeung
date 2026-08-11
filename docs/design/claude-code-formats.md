---
title: Claude Code's on-disk formats
status: active
updated: 2026-08-09
covers:
  - crates/mogeungd/src/watcher.rs
  - crates/mogeungd/src/adapter.rs
  - crates/mogeungd/src/kit.rs
  - crates/mogeung-core/src/kit.rs
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

**This file exists before the transcript does.** Probed 2026-08-09: a `claude`
started under tmux had its registry entry — pid, session id, cwd, `"status":
"idle"`, pane, a derived name — within ~40 ms, and **no `.jsonl` at all** until
the first message was sent. So the registry is a *discovery* source and not only
a liveness one; a board that waited for a transcript could not see the session
you had just opened, which is `R-J30` and is why the scan adopts registry
entries no transcript mentions.

## `~/.claude/projects/<slug>/<session-id>.jsonl` — transcripts

Append-only, one JSON object per line. `<slug>` is the cwd with separators
replaced.

**Written on the first message, not on launch** — see the registry section
above. The exact slug rule is not derived anywhere in this codebase: the path is
learnt by finding the file, never by constructing it from a cwd, because a
mapping nobody documented is a mapping that will move (`A4`).

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
| `bridge-session` | 47¹ | Ignored — the web/desktop bridge's bookkeeping: `bridgeSessionId`, `lastSequenceNum` |

¹ From a **later** sweep and not comparable with the column above: 2026-08-07,
across the 60 newest transcripts of a corpus that had grown to 315.

`queue-operation`, `pr-link` and `frame-link` were **found by the canary**. They
existed in real transcripts throughout v0.2 and were swallowed by a catch-all
arm; nothing recorded that they existed, so nobody could have known whether they
mattered.

**`bridge-session` was found by re-running the sweep by hand**, on 2026-08-07,
and that difference is the lesson worth keeping. The canary only speaks from a
*running* daemon, and it had not been up long enough to say anything — so a type
that appeared some time after 2026-07-25 sat unclassified while every automated
check in the repository stayed green. The classification is only as fresh as the
last long-running daemon, and after a CLI upgrade that is worth checking by hand
rather than waiting to be told. See [A4](../product/assumptions.md), which this
is evidence *for* rather than against: the drift was findable within two weeks.

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

### Where reading resumes (`R-A6`)

Append-only is what makes tailing possible at all, and `Tailer` keeps a byte
offset per file. That offset is **not only process state**: it is seeded from
the database at start-up (`Tailer::seed`) and written back after each batch is
folded in, because a tailer that starts empty over a database that remembers
the sessions re-reads every transcript whole and appends the history a second
time. [data-model.md](data-model.md#read-positions-are-part-of-the-record-r-a6)
has what that cost and how it is repaired.

The one thing append-only does not guarantee is that a file never gets
*shorter*. If it does, it was rewritten or rotated, and the offset is discarded
on sight and the file read again from the start — the same rule whether the
offset came from this process or from the last one.

## `~/.claude/file-history/<session-id>/<hash>@v<n>`

Pre-edit file backups. **Not used** — the hashed filenames have no reliable path
mapping. See [ADR-0004](../decisions/0004-git-for-diffs-not-file-history.md).

## `~/.claude/history.jsonl`

Every prompt ever typed, with `display`, `project`, `sessionId`, `timestamp`.
2,084 entries on this machine. Currently unused; it is the basis for roadmap
section F.

## `~/.claude/skills/<name>/SKILL.md` and plugin skills — `R-F15`

Markdown with YAML frontmatter. Two keys are read and both are optional:
`name` and `description`. A file with neither is still a skill, named after
its directory — which is what Claude Code calls it anyway.

Two roots, and the second is where the count comes from:

| Root | Scope | Depth of `SKILL.md` |
|---|---|---|
| `~/.claude/skills/<name>/SKILL.md` | yours | 2 |
| `~/.claude/plugins/marketplaces/<market>/plugins/<plugin>/skills/<skill>/SKILL.md` | a plugin's | **7** |

Measured on this machine 2026-08-06: 6 user skills, 44 plugin ones. The first
walk capped at six levels, found every user skill and **no** plugin skill, and
reported a complete-looking list — the failure mode `A4` and the health panel
exist for. The cap is 8 now and a test pins a skill at depth 7.

## `~/.claude/projects/<slug>/memory/*.md` — `R-F14`

What an agent decided to remember. Same frontmatter shape, plus a nested
`metadata:` block carrying `type` (`user`, `feedback`, `project`, `reference`).
`MEMORY.md` is the index the rest hang off. 63 files across 18 projects here.

**Only top-level frontmatter keys are read.** The `metadata:` block has its own
`name:` in some shapes, and taking that as the file's own would label every
memory after whatever the block happened to contain.

The `<slug>` is a project path with every separator replaced by a dash, which is
**lossy** — a directory called `perf-test` is indistinguishable from
`perf/test`. `decode_project` is therefore presentation only: it is never
resolved, opened, or compared against a real path, and a test pins the way it
fails so that stays deliberate.

## Reading a body over the wire

`FetchKitDoc` takes a path from the network, and the daemon binds loopback
without a token. So the path is canonicalised — resolving `..` and symlinks —
and must still sit under a published root, or it is refused. Without that
check, "show me a skill" is "read any file this user can read". Same rule the
file explorer uses for a worktree, for the same reason.

## Parsing posture

Unknown event types and unexpected shapes are **ignored, never fatal**. The
realistic failure mode is therefore a degraded board rather than a crash — which
is also the dangerous one, because it looks like "nothing is happening".

`parse_line` returns a `LineOutcome`, never a bare `Option`, precisely so that
"we chose to skip this" and "we have never seen this" cannot be confused. Every
outcome is counted: see [health-and-canary.md](health-and-canary.md).

## Learned in the 2026-07-29 sweep

- **Rate limits are prose, not events.** No structured limit event of any
  name exists in the 235 local transcripts; a hit arrives as an assistant
  message with `message.model == "<synthetic>"` and all-zero usage, its
  reset time embedded in the text. The parser keys on that signature, and
  on nothing speculative — an unobserved type belongs to the canary.
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
