---
title: Troubleshooting
status: active
updated: 2026-07-25
---

# Troubleshooting

## No sessions appear

- Does `~/.claude/projects/` exist? The daemon warns at startup if not.
- Sessions older than 14 days are ignored by design.
- `curl -s localhost:7717/api/sessions | head` — is the daemon seeing them and
  the UI not, or neither?

## A session shows no changes

Expected in several cases:

- Its cwd is not inside a git repo.
- It only read files. Only `Edit`/`Write` count as touching.
- Its work was committed and the base has moved to HEAD.
- It started before mogeung did, so the base is meaningless.

## Something shows as live but the terminal is closed

Should not happen — liveness is checked against the OS, not the registry files
(which are not cleaned up on exit). If you see it, the pid was reused. File it.

## A hunk I already read came back unread

Either the agent genuinely rewrote it, or it was reformatted. Anchors hash
content including whitespace, so re-indenting counts as a change. Known
limitation; roadmap `R-D2`.

## The board looks emptier than it should

This is the failure mode to watch for. Claude Code's file formats are private
and undocumented; an update can change them, and the parser is built to ignore
what it does not recognise rather than crash. So a format change looks like
"quiet day", not like an error.

**Ask the tool instead of guessing.** Click **health** in the top bar, or:

```sh
curl -s localhost:7717/api/health | python3 -m json.tool
```

`headline` gives the verdict in a sentence. What to look for:

| Sign | Means |
|---|---|
| `unknown_types` is non-empty | Claude Code emits an event type mogeung has never seen — most likely a format change |
| `lines_malformed` > 0 | Lines that were not readable JSON. Whatever they held is missing |
| `blind_ratio` above 0 | The share of lines mogeung could not account for |
| a `version_changed` alert | The CLI upgraded. Not a problem in itself, but it is when formats move |

A high `lines_ignored` count is **not** a problem — that is classified
bookkeeping being skipped on purpose, and it is normally the large majority of
lines. Only `unknown` and `unreadable` mean data you wanted and did not get.

If `unknown_types` names something, that is worth reporting: the type and its
count are all that is needed to classify it. See
[health-and-canary.md](../design/health-and-canary.md).

## A session's early history is missing

Transcripts over 4 MiB are followed from near their end instead of being read
whole, so a very long session's early turns never reach the board. This is
deliberate — reading an 11 MB transcript on sight stalls the scan loop to
reconstruct history nobody reviews.

The health panel states exactly how much was skipped, per session. The diff is
unaffected: it comes from git, not from the transcript.

## Port already in use

Another `mogeungd` is running. `lsof -i :7717` to find it.

## Nothing updates

The UI reconnects on its own; the dot by the title turns red when disconnected —
hover it for the reason. If the daemon is up, `POST /api/rescan` forces a scan.
