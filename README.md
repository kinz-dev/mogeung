# mogeung

A workbench for people who supervise agents instead of typing code.

**Status: v0.1 — a working base to evaluate and evolve, not a finished product.**
See [Limitations](#limitations) before forming an opinion. The design rationale
lives in [CONCEPT.md](CONCEPT.md); this file is how to run it and what it does.

---

## The idea in one paragraph

An IDE assumes a human types code into files, so the file tree is the root
object. When most of your day is prompting agents, reviewing code you did not
write, and checking whether an agent did what it claimed, that assumption is
wrong. In mogeung the root object is the **change**, and the primary question the
UI answers is **"where do I need to look right now?"**

## What v0.1 does

### The attention router

Every run across every repo is ranked into one queue:

```
BLOCKED   → tools were denied; it needs a decision from you
FAILED    → exited badly
REVIEW    → finished, diff not yet read
STALLED   → running but silent past a threshold
BURNING   → spending money with no diff to show for it
running   → working normally
```

Tiers are separated widely enough that within-tier tiebreakers (age, then spend)
can never promote a run past a more urgent one. Every row shows the one-line
reason it ranks where it does, so the heuristic is never a black box. The queue
is recomputed on a timer as well as on events, because silence is itself a
signal.

### Review checkpointing — "never read the same code twice"

Each hunk is identified by a hash of its **content** — path plus added/removed
lines, excluding line numbers and context. Marking a hunk read records that
anchor. When the agent rewrites the file:

- hunks whose content did not change keep their anchor and stay **read**
- rewritten or new hunks get a new anchor and come back **unread**

Review marks are keyed to the **session root**, not the individual run, so a
follow-up shows the session's cumulative diff with everything you already read
still marked read. Only genuinely new work asks for your attention.

### Risk-ordered diffs

Files are sorted by risk, never alphabetically. Path heuristics flag auth,
secrets, migrations, money, infra, CI, and dependency manifests; content
heuristics flag `unsafe`, concurrency, error handling, network I/O, widened
public API, large deletions, and deleted tests. Lockfiles, generated code,
vendored trees, and fixtures are tagged as noise and hidden by default. A file
scores as its **riskiest hunk**, so one dangerous change cannot be averaged away
by surrounding boilerplate.

### Structured transcripts, not a terminal

The daemon owns the agent's stdout, so a run is stored as typed events —
prompts, tool calls with one-line summaries, tool results, cost, turns — which
are searchable and linkable. There is no embedded VT100 emulator, deliberately.
"Open my real terminal here" covers the rest.

### Worktree-per-run

Each run can get its own git worktree and branch (`mogeung/<short-id>`), so
parallel agents cannot collide. Diffs are computed against the commit the run
started from and include **untracked files** — which plain `git diff` misses
entirely, and which is exactly what an agent creating new modules produces.

### Follow-ups

A finished run can be continued with new instructions. This resumes the agent's
own session (so it keeps its context), creates a child run, and retires the
parent from the queue so one session never asks for review twice.

---

## Requirements

- Rust 1.85+
- `git`
- [Claude Code](https://claude.com/claude-code) on `PATH` (built against 2.1.x)
- macOS (the "open in…" actions are macOS-specific; everything else is portable)

## Build and run

```sh
cargo build --release

# terminal 1 — the daemon owns everything and keeps running with no UI open
./target/release/mogeungd --repo ~/projects/your-repo

# terminal 2 — the window is just a client
./target/release/mogeung
```

Defaults: daemon on `127.0.0.1:7717`, database at `~/.mogeung/mogeung.db`,
worktrees under `~/.mogeung/worktrees/`.

```
mogeungd --listen 127.0.0.1:7717 --db <path> --repo <path> [--repo <path>...]
mogeung  --url ws://127.0.0.1:7717/ws
```

Closing the window does not stop any agent. The UI reconnects on its own, so
restarting the daemon under a running UI is fine.

## Using it

1. **+ New run** → pick a repo, describe the task, choose a model and permission
   mode, leave "dedicated worktree" on.
2. Watch the queue. Anything wanting you sorts to the top.
3. Open a run → **Changes**. Read top-down; risk order is the reading order.
   Tick hunks as you go.
4. Wrong? Type into the follow-up box at the bottom of **Transcript**. That
   resumes the same agent session.
5. **Mark all read** promotes the run out of the queue.

## Scripting

The daemon is curl-able, so it fits into shell workflows without the UI:

```sh
curl -s localhost:7717/api/health
curl -s localhost:7717/api/runs
curl -s localhost:7717/api/runs/<id>/change
curl -s localhost:7717/api/runs/<id>/events?since=0

curl -sX POST localhost:7717/api/runs -H 'content-type: application/json' -d '{
  "repo_path": "/path/to/repo",
  "intent": "add a health endpoint",
  "model": "sonnet",
  "permission_mode": "acceptEdits",
  "worktree": true }'

curl -sX POST localhost:7717/api/runs/<id>/follow_up \
  -H 'content-type: application/json' -d '{"prompt":"also add a test"}'
curl -sX POST localhost:7717/api/runs/<id>/review_all
```

`GET /ws` carries the same commands plus the live event stream.

## Tests

```sh
cargo test --workspace                          # 20 tests, free
cargo test -p mogeungd -- --ignored --nocapture # spawns a real agent, costs money
```

The ignored test is the real end-to-end proof: it boots a daemon, creates a git
repo, runs an actual Claude Code session in a worktree, and asserts the
untracked file it created appears in the diff and that review-all clears it.

---

## Limitations

Read this section as the roadmap. These are known and deliberate, not oversights.

**Agent support**
- **Claude Code only.** Codex and Gemini are not implemented. The adapter
  boundary exists (`crates/mogeungd/src/adapter.rs`) but has only been proven
  against one CLI, so it is a hypothesis, not a validated abstraction.
- **No live interjection.** You cannot steer a run mid-flight; you wait for it
  to finish and then follow up. Claude Code supports `--input-format stream-json`
  for this, which is the obvious next increment.
- **Permission prompts do not reach you.** Runs are non-interactive, so a tool
  needing approval is *denied*, not queued for your decision. mogeung detects
  this after the fact (`BLOCKED`) rather than asking you in the moment. This is
  the single biggest gap between v0.1 and the intended product.

**Review**
- Hunk anchors are content hashes, so **re-indenting or reformatting a hunk
  makes it unread again** even though nothing semantic changed. Whitespace
  normalisation is not implemented.
- Risk scoring is **keyword heuristics over diff text**, not semantic analysis.
  It will have false positives (a variable named `password_field`) and false
  negatives (a subtle auth bug in a file with a boring name). Treat it as
  reading order, never as a safety guarantee.
- No syntax highlighting in diffs, no word-level intra-line diff, no side-by-side
  view. Hunks over 500 lines are truncated in the UI.
- Untracked files are capped at 200 per run and 512 KB per file.

**Not built at all** (from CONCEPT.md, deferred by design)
- The claim ledger — agent assertions are shown, never verified.
- The signal runner — no test/typecheck/lint execution, so "does it work?" is
  still your job entirely.
- The whole document layer — doc inventory, staleness, GC, derived progress,
  agent-instruction hub. **None of it exists.** If doc sprawl is your sharpest
  pain, v0.1 does not touch it.
- Blast radius, provenance/prompt-blame, architecture map, policies, review-debt
  meter across HEAD, race mode, cost budgets.
- No editor. Handoff to IntelliJ/VS Code only.

**Operational**
- **Single user, no auth.** The daemon binds localhost and anyone who can reach
  the port can start agent processes on your machine. Do not expose it.
- A daemon restart marks in-flight runs `failed` — the child processes die with
  it. There is no reattach.
- Worktrees are only cleaned up when you delete a run. They will accumulate.
- No web client yet, so no phone/tablet review despite the architecture
  supporting it.
- macOS-only "open in" actions.
- Run history is unbounded; nothing prunes the database.
- The UI re-renders whole diffs each frame rather than virtualising rows. Fine
  at the scale tested; not proven on a thousand-file change.

---

## Layout

```
crates/
  mogeung-core/   types + wire protocol; no I/O, no async — the contract
    attention.rs  the ranking heuristics (with the tests that pin them)
    change.rs     Change/FileChange/Hunk, risk levels, review progress
  mogeungd/       the daemon — the actual product
    adapter.rs    Claude Code stream-json → TranscriptEvent (all CLI specifics)
    git.rs        worktrees, diffing, risk scoring, hunk anchoring
    state.rs      run supervisor and lifecycle
    api.rs        WebSocket + REST
  mogeung-ui/     egui client — a pure projection of daemon state
```

The daemon is the product; every UI is a client. That is what makes a future
web client a packaging decision rather than a rewrite.
