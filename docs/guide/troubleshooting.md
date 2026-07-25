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

Check the CLI version against
[claude-code-formats.md](../design/claude-code-formats.md) — verified against
2.1.219/2.1.220 only. Roadmap `R-A1` will make this loud instead of silent.

## Port already in use

Another `mogeungd` is running. `lsof -i :7717` to find it.

## Nothing updates

The UI reconnects on its own; the dot by the title turns red when disconnected —
hover it for the reason. If the daemon is up, `POST /api/rescan` forces a scan.
