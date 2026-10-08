---
title: Troubleshooting
status: active
updated: 2026-10-08
---

# Troubleshooting

## No sessions appear

- Does the CLI's home exist? `~/.claude/projects/` for Claude Code,
  `~/.codex/sessions/` for Codex, `~/.qwen/projects/` for Qwen Code,
  `~/.local/share/opencode/opencode.db` for opencode. The daemon warns at
  startup when the machine has none of them.
- Sessions older than 14 days are ignored by design.
- `curl -s localhost:7717/api/sessions | head` — is the daemon seeing them and
  the window not, or neither?
- Against a remote daemon, a `401` means the token, not the daemon
  ([remote.md](remote.md)).

## *needs you* is empty, but an agent is waiting

The default scope shows only sessions mogeung can attach to — ones in a tmux
pane. A `claude` started in a plain terminal is watched but held back, and the
panel says so: *N hidden — not under tmux*. Press that line, use **all**, or
start the agent with `yolomo` (or `codexmo`, `qwenmo`, `opencodemo`) next
time. See [the-queue.md](the-queue.md#under-tmux-only).

## A row says *provisional* and shows nothing

An agent is running in a tmux pane and has not written a line yet — a trust
prompt, a login, a first-run migration, or a fresh opencode waiting for its
first prompt. Open its Agent pane; the row becomes the real session the moment
the agent writes
([ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md)).

## A session shows no changes

Expected in several cases:

- Its cwd is not inside a git repo.
- It only read files. Only `Edit`/`Write` count as touching.
- Its work was committed before the session started: the base is the last
  commit before that moment, and nothing after it has been written yet.

## Something shows as live but the terminal is closed

Should not happen — liveness is checked against the OS, not the registry files
(which are not cleaned up on exit). If you see it, the pid was reused. File it.

## A hunk I already read came back unread

The agent genuinely rewrote it. Anchors ignore indentation and internal
whitespace (`R-D2`), so re-indenting does not count; a changed string or a
changed case does, on purpose.

## `LIMIT` on several sessions at once

A rate limit was hit, and the CLI usually tells every session at once. There
is nothing to fix and nothing to answer; the badge clears when a session moves
again, and Analytics shows the burn that led there. The warning before the
limit is an estimate from past hits, labelled as one, because the CLI writes no
structured limit event to disk (A20).

## The board looks emptier than it should

This is the failure mode to watch for. The CLIs' file formats are private and
undocumented; an update can change them, and the parser is built to ignore
what it does not recognise rather than crash. So a format change looks like
"quiet day", not like an error.

**Ask the tool instead of guessing.** The status bar says *reading everything
it recognises* or amber *N thing(s) it cannot see*; `Alt+h` opens the Health
window, or:

```sh
curl -s localhost:7717/api/health | python3 -m json.tool
```

`headline` gives the verdict in a sentence. What to look for:

| Sign | Means |
|---|---|
| `unknown_types` is non-empty | A CLI emits an event type mogeung has never seen — most likely a format change |
| `lines_malformed` > 0 | Lines that were not readable JSON. Whatever they held is missing |
| `blind_ratio` above 0 | The share of lines mogeung could not account for |
| a `version_changed` alert | A CLI upgraded. Not a problem in itself, but it is when formats move |
| an `unpriced_model` alert | A model in use that the price table has no rate for; Analytics shows it as unpriced, not free |

A high `lines_ignored` count is **not** a problem — that is classified
bookkeeping being skipped on purpose, and it is normally the large majority of
lines. Only `unknown` and `unreadable` mean data you wanted and did not get.
Each CLI reports into its own line of the Health window.

If `unknown_types` names something, run `cargo run -q -p mogeungd --bin sweep`
over the corpus: it names every unclassified shape, and the type and its count
are all that is needed to classify it. See
[health-and-canary.md](../design/health-and-canary.md).

## A session's early history is missing

Transcripts over 4 MiB are followed from near their end instead of being read
whole, so a very long session's early turns never reach the board. This is
deliberate — reading an 11 MB transcript on sight stalls the scan loop to
reconstruct history nobody reviews.

The health panel states exactly how much was skipped, per session. The diff is
unaffected: it comes from git, not from the transcript.

## The Agent pane says the session is not under tmux, but it is

The window was launched from the Dock or a launcher and inherited a `PATH`
without `tmux` on it. Fixed on 2026-09-06 (`R-J87`); if it recurs, `which
tmux` from a login shell versus from the window's environment is the
comparison to make.

## Port already in use

Another `mogeungd` is running. `lsof -i :7717` to find it, or
`./scripts/mogeungd status`.

## Nothing updates

The window reconnects on its own; the dot by the title turns red when
disconnected — hover it for the reason. If the daemon is up, `POST /api/rescan`
forces a scan, and the palette's *rescan* is the same thing.
