# mogeung

A supervision layer over the Claude Code sessions you already run.

**Status: v0.2 — observer model.** v0.1 spawned agents and was rightly judged
"a handicapped Claude Code with a single session". See [Limitations](#limitations)
and [SUMMARY.md](SUMMARY.md) for what changed and why. Design rationale is in
[CONCEPT.md](CONCEPT.md).

---

## What it is

mogeung **watches**. It never starts, steers, or stops an agent.

You keep running `claude` in as many terminal tabs as you like, exactly as you
do today — full interactive loop, plan mode, slash commands, permission prompts,
all of it. mogeung reads the files Claude Code already writes for its own
purposes and gives you the layer that doesn't exist:

- **one queue across every session**, ranked by who needs you
- **a diff per session**, risk-ordered, with per-hunk read marks that survive
  the agent rewriting the file

Because it observes rather than wraps, it cannot make any individual session
worse. That was v0.1's fatal flaw.

## What it reads

| Source | What it gives |
|---|---|
| `~/.claude/sessions/<pid>.json` | Live registry: `busy`/`idle` status, cwd, pid, friendly name |
| `~/.claude/projects/<slug>/<id>.jsonl` | Transcript: title, prompts, tool calls, tokens, errors, edited files |

Liveness is checked against the **OS**, not the registry file — those files are
not cleaned up on exit, so a stale entry must never become a phantom session.

Nothing is written to either location. mogeung keeps its own state in
`~/.mogeung/mogeung.db`.

## The attention queue

```
WAITING  → alive and idle: it is waiting for you to type something
FAILED   → hit an API error
REVIEW   → exited, and left changes nobody has read
STALLED  → alive and busy, but silent past a threshold
running  → working normally
idle     → nothing wanted
```

`WAITING` is not a heuristic. Claude Code publishes `status: "idle"` in its own
live registry, so mogeung knows a session is blocked on you as fact. That was
the single biggest gap in v0.1, and the observer model closed it for free.

Tiers are separated widely enough that the within-tier tiebreaker (longest wait
first) can never promote a session past a more urgent one. Every row shows the
reason it ranks where it does.

## Review checkpointing — "never read the same code twice"

Each hunk is identified by a hash of its **content** — path plus added/removed
lines, excluding line numbers and context. Marking a hunk read records that
anchor. When the agent rewrites the file:

- hunks whose content did not change keep their anchor and stay **read**
- rewritten or new hunks get a new anchor and come back **unread**

## Risk-ordered diffs

Files sort by risk, never alphabetically. Path heuristics flag auth, secrets,
migrations, money, infra, CI, and dependency manifests; content heuristics flag
`unsafe`, concurrency, error handling, network I/O, widened public API, large
deletions, and deleted tests. Lockfiles, generated code, vendored trees, and
fixtures are tagged noise and hidden by default. A file scores as its **riskiest
hunk**, so one dangerous change cannot be averaged away by boilerplate.

Diffs are computed with git against the commit the repo was on when mogeung
first saw the session, and include **untracked files** — which plain `git diff`
misses, and which is exactly what an agent creating new modules produces.

When several sessions share a working tree, the diff is **attributed** to the
files each session actually touched, taken from its Edit/Write calls.

## The one thing it launches

**+ New session** opens a terminal running real interactive `claude` in a
directory (optionally in a fresh git worktree first). Nothing is wrapped — you
drive that session normally and it appears in the queue like any other.

This exists because the other half of v0.1's failure was that reaching three or
four parallel sessions was awkward, and the queue is worth nothing at N=1.

---

## Requirements

- Rust 1.85+, `git`
- [Claude Code](https://claude.com/claude-code) (built against 2.1.x)
- macOS — the "open in…" actions and terminal launch are macOS-specific;
  watching and diffing are portable

## Build and run

```sh
cargo build --release

# terminal 1 — the daemon; keeps working with no window open
./target/release/mogeungd

# terminal 2 — the window is just a client
./target/release/mogeung
```

Defaults: daemon on `127.0.0.1:7717`, database at `~/.mogeung/mogeung.db`,
polling every 1.5 s.

```
mogeungd --listen 127.0.0.1:7717 --db <path> --poll-ms 1500
mogeung  --url ws://127.0.0.1:7717/ws
```

There is nothing to configure and no repos to register: if you have run `claude`
somewhere in the last 14 days, it shows up.

## Scripting

```sh
curl -s localhost:7717/api/health
curl -s localhost:7717/api/queue
curl -s localhost:7717/api/sessions
curl -s localhost:7717/api/sessions/<id>/change
curl -s localhost:7717/api/sessions/<id>/events?since=0
curl -sX POST localhost:7717/api/sessions/<id>/review_all
curl -sX POST localhost:7717/api/rescan
```

`GET /ws` carries the same commands plus the live event stream.

A shell one-liner for "who needs me?":

```sh
curl -s localhost:7717/api/queue | jq -r '.[] | select(.reason!="idle")
  | "\(.reason)\t\(.detail)"'
```

## Tests

```sh
cargo test --workspace   # 36 tests, all free — nothing ever spawns an agent
```

`tests/discovery.rs` builds a synthetic `~/.claude` per test and injects it, so
the suite never touches your real session data and runs in parallel.

---

## Limitations

Read this as the roadmap. These are known and deliberate.

**The format is undocumented**
- `~/.claude/sessions/*.json` and the `.jsonl` transcript shape are Claude
  Code's private files. A CLI update can change them without warning. The parser
  ignores what it does not recognise rather than failing, so the realistic
  failure mode is a degraded board, not a crash — but it *can* silently stop
  seeing things.
- Verified against Claude Code 2.1.219/2.1.220 only.

**Attribution and diffs**
- Diffs are git-based, so a session working outside a repo shows no changes.
- The base commit is the repo HEAD when mogeung **first saw** the session. For
  sessions that ran before mogeung was ever started, the base is whatever HEAD
  is now, so their diff will usually be just uncommitted work.
- Per-session attribution uses the files a session edited. If two sessions edit
  the *same* file, both will show it. Git cannot tell them apart, and neither
  can mogeung.
- Committed work disappears from the diff, because the base moves with HEAD.

**Review**
- Hunk anchors are content hashes, so **reformatting a hunk makes it unread
  again** even though nothing semantic changed.
- Risk scoring is **keyword heuristics over diff text**, not semantic analysis.
  Expect false positives (a variable named `password_field`) and false negatives
  (a subtle auth bug in a boring file). It is reading order, never a safety
  guarantee.
- No syntax highlighting, no intra-line diff, no side-by-side. Hunks over 500
  lines are truncated in the UI.

**Not built** (from CONCEPT.md, deferred by design)
- Claim ledger — agent assertions are shown, never verified.
- Signal runner — no test/typecheck/lint execution. "Does it work?" is still
  entirely your job.
- The document layer — inventory, staleness, GC, derived progress,
  agent-instruction hub. **None of it exists.**
- Blast radius, provenance, architecture map, policies, review-debt across HEAD.
- No editor. Handoff to IntelliJ/VS Code only.
- No cost tracking. Token counts only — see SUMMARY.md for why dollars are
  misleading on subscription auth.

**Operational**
- **Single user, no auth.** The daemon binds localhost. Do not expose it.
- Sessions older than 14 days are ignored.
- Polling every 1.5 s, so status changes have up to that much lag.
- The UI re-renders whole diffs each frame rather than virtualising rows.
- macOS-only terminal launch and "open in" actions.

---

## Layout

```
crates/
  mogeung-core/   types + wire protocol; no I/O, no async — the contract
    session.rs    the observed Session
    attention.rs  ranking heuristics, with the tests that pin them
    change.rs     Change/FileChange/Hunk, risk levels, review progress
  mogeungd/       the daemon — the actual product
    watcher.rs    live registry + incremental transcript tailing
    adapter.rs    on-disk transcript format → TranscriptEvent
    git.rs        diffing, risk scoring, hunk anchoring
    state.rs      scan loop, diff attribution, review state
    api.rs        WebSocket + REST
  mogeung-ui/     egui client — a pure projection of daemon state
```

The daemon is the product; every UI is a client. That is what makes a future
thin web client (review from a phone) a packaging decision rather than a rewrite.
