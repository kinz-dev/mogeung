# Changelog

## v0.2 — the observer pivot (2026-07-25)

mogeung stopped spawning agents and started watching the ones you run yourself.

v0.1's verdict in use was *"a handicapped Claude Code with a single session"*.
The attention queue is worth zero at N=1, and to feed it v0.1 had removed the
interactive loop — so every session was worse than just running `claude`. See
[ADR-0003](docs/decisions/0003-observe-do-not-spawn.md).

**Added** — session watcher over `~/.claude/sessions` and `~/.claude/projects`;
first-party `WAITING` detection from the live registry; per-session diff
attribution; terminal launch; a synthetic-home test suite.

**Changed** — `Run` → `Session`; attention reasons rebuilt around observed
state; transcript parser reads on-disk `.jsonl` instead of `stream-json`; tokens
replace dollars ([ADR-0005](docs/decisions/0005-tokens-not-dollars.md)); the
watch root is injected rather than read from the environment
([ADR-0006](docs/decisions/0006-inject-the-watch-root.md)).

**Removed** — the run supervisor, permission modes, model selection,
follow-ups, cancel, the New Run dialog.

**Kept** — the git diff engine, risk scoring, hunk anchoring, review
checkpointing, the daemon/client split.

36 tests, all free.

## v0.1 — initial build (2026-07-25)

Spawning model: intent in, `claude -p` in a worktree, diff out. Attention
router, review checkpointing, risk-ordered diffs, worktree-per-run, structured
transcripts. 21 tests.

Superseded the same day. Its build log is preserved at
[docs/archive/2026-07-25-v0.1-v0.2-build-log.md](docs/archive/2026-07-25-v0.1-v0.2-build-log.md).
