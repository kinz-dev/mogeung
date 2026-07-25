# mogeung — a workbench for people who supervise agents

Status: draft concept, nothing built yet. Written 2026-07-25.

---

## 1. The problem

The daily job has changed but the tools have not.

An IDE like IntelliJ or VS Code is built around one assumption: **a human types code into files, one character at a time.** Everything great about them — completion, refactoring, inline errors, go-to-definition — is an optimization of that loop. The file tree is the root object. The cursor is the center of the universe.

That is now ~20% of the work. The other 80% looks like this:

- Prompting and steering 1–4 agents (Claude Code, Codex, Gemini CLI), often in parallel worktrees.
- Reviewing large volumes of code you did not write, arriving faster than you can read it.
- Deciding what to test, and checking whether what the agent *claimed* it did is what it *actually* did.
- Holding the architecture in your head while it is being modified underneath you.
- Drowning in generated prose: `PLAN.md`, `PROGRESS.md`, `NOTES.md`, `IMPLEMENTATION.md`, ADRs, status reports — half of them stale, several contradicting each other, none of them deletable with confidence.

The bottleneck moved from **writing** to **reviewing, verifying, and remembering**. No tool owns that.

The gap is not "IDE plus a chat sidebar." Bolting a chat panel onto a file editor keeps the file at the center. The center has moved.

## 2. Thesis

> **The root object is not the file. It is the change.**

Files are a projection of accumulated changes. In an agent-driven project, the units you actually reason about are:

- *a run* — one agent session, with intent, transcript, diff, cost, and evidence
- *a change* — the net semantic delta it produced
- *a claim* — what the agent says it did
- *a decision* — what you and the agent agreed the system should be

mogeung is a workbench where those are first-class, and where the editor is one mode among several rather than the whole application.

A second thesis, equally important:

> **Progress documents should be derived, not written.**

`PROGRESS.md` goes stale because it is prose written at a moment in time. If progress is instead *computed* from plan items bound to real diffs, tests, and commits, it cannot lie and never needs GC.

## 3. Non-goals

Being explicit here, because "build a better IDE" is a multi-year trap:

- **Not** rebuilding language intelligence. Use LSP + tree-sitter. Do not write a refactoring engine.
- **Not** a debugger, profiler, or build system. Shell out; keep IntelliJ for the deep-editing 20% until mogeung earns that job.
- **Not** another agent. mogeung *drives* Claude Code / Codex / Gemini; it does not compete with them.
- **Not** a team/cloud product on day one. Single developer, local-first, own machine. Multiplayer can come later.
- **Not** a chat client. Chat is an input method, not the product.

## 4. Core objects

The whole data model, deliberately small:

| Object | What it is |
|---|---|
| **Workspace** | A set of repos (mono or poly) + their worktrees. Multi-repo from day one. |
| **Run** | One agent session: intent, agent/model, worktree, transcript, tool calls, diff, cost, duration, status. |
| **Change** | A reviewable delta: a run's net diff, a commit range, or the dirty working tree. |
| **Review** | Human passes over a Change: per-hunk state (unseen / accepted / flagged), anchored by content hash so it survives rewrites. |
| **Note** | A review comment. Convertible into an instruction sent back to an agent. |
| **Claim** | An assertion an agent made ("tests pass", "backwards compatible") + its verification state. |
| **Signal** | Machine evidence: test run, typecheck, lint, build, coverage delta, benchmark. |
| **Doc** | A managed markdown artifact with a lifecycle: draft → active → superseded → archived. |
| **Decision** | A durable architectural commitment, extracted from docs/chat, with the code it governs. |
| **Policy** | A guardrail rule ("no agent touches `infra/` without review"). |

Everything else in the UI is a view over these.

## 5. Feature inventory

Priority tags: **P0** = needed for the thing to be useful at all · **P1** = the reason you'd keep using it · **P2** = later.

### Pillar A — Agent cockpit (orchestration & attention)

The problem it solves: *when four agents are running, you do not know where to look.*

- **A1. Run board** — every agent session across every repo/worktree as a card: intent, agent, branch, status, elapsed, tokens/cost, diff size. **P0**
- **A2. Attention router** — the single most valuable feature. One ranked queue: *blocked on your input* > *finished, needs review* > *failed* > *running long, probably stuck* > *burning cost with no diff progress*. Inbox-zero semantics. **P0**
- **A3. Agent-agnostic adapters** — Claude Code, Codex CLI, Gemini CLI, Aider. Normalize each into the Run model via headless/JSON modes, hooks, and PTY capture. **P0**
- **A4. Interject** — steer a running session without switching terminals or losing its context. **P1**
- **A5. Stall detection** — heuristics for looping, thrashing on one file, retrying a failing command, waiting on a prompt nobody answered. **P1**
- **A6. Worktree manager** — one-key "new task → new worktree → new run", auto-cleanup on merge, disk/port/dev-server collision handling. **P1**
- **A7. Race mode** — same task, two agents/models, side-by-side diff, pick one or graft the best parts. **P2**
- **A8. Cost & burn ledger** — per run, per task, per day, across providers. **P1**
- **A9. Ambient digest** — "what happened while I was away," written from evidence (diffs, tests) rather than from agent self-reports. **P1**

### Pillar B — Review workspace

The problem it solves: *you re-read the same code repeatedly and still miss the risky 5%.*

- **B1. Change-first navigation** — open a Change, not a folder. File tree exists but is secondary. **P0**
- **B2. Risk-ordered review** — sort hunks by risk, not alphabetically. Auth, migrations, money, concurrency, error handling, CI config, dependency additions, deleted tests, widened public API first. Lockfiles, formatting, generated code, fixtures collapsed by default. **P0**
- **B3. Review checkpointing / diff-of-the-diff** — mark hunks reviewed; when the agent rewrites the file, only what changed *since your pass* comes back. Anchored to content hashes so moved code does not re-surface. **P0** — this alone is worth the project.
- **B4. Altitude control** — one keystroke between three zoom levels: *architectural* (which modules, contracts, and boundaries moved) → *file* → *hunk*. **P1**
- **B5. Semantic narrative** — a 40-commit thrashing session re-presented as "here are the 6 logical changes," ordered by concept rather than chronology. **P1**
- **B6. Blast radius** — for any changed symbol: callers, tests that cover it, configs and docs that mention it, and whether the tests actually ran. **P1**
- **B7. Note → instruction** — a review comment becomes a task dispatched to an agent, with the hunk as context. Round-trip in one keystroke. **P0**
- **B8. Trust policy** — auto-accept test-only or format-only changes; always hand-review the sensitive set. Configurable per path glob. **P1**
- **B9. Review debt meter** — what fraction of current HEAD has never been looked at by a human, broken down by module and risk. Makes the invisible backlog visible. **P1**
- **B10. Test-first review mode** — show the tests before the implementation. Highest-leverage reading order. **P2**

### Pillar C — Verification (the claim ledger)

The problem it solves: *agents assert things, and checking each assertion by hand is slow enough that you stop doing it.*

- **C1. Claim extraction** — parse each run's summary into discrete claims: "added tests", "all tests pass", "no breaking change", "handled the error case". **P1**
- **C2. Auto-verification** — bind each claim to a Signal where possible. "Tests pass" → did a test command actually execute in this worktree, on this tree hash, and exit 0? Not "did the agent say so." **P1**
- **C3. Signal runner** — continuous typecheck/lint/test/build per worktree, results attached to the Run card. **P0**
- **C4. Coverage delta on changed lines** — not project coverage; coverage *of this diff*. **P2**
- **C5. Untrusted-until-verified badges** — a run cannot show green until its claims are backed by signals. **P1**

### Pillar D — Document & knowledge layer

The problem it solves: *the plan/progress/notes sprawl you named directly.*

- **D1. Doc inventory** — every markdown artifact in the workspace, classified (plan / progress / spec / ADR / note / readme / agent-instruction), with lifecycle state. **P0**
- **D2. Staleness detection** — a doc describes module X; X changed 40 commits ago and the doc did not. Score and flag. **P0**
- **D3. Duplicate/contradiction clustering** — three plans for the same feature; a doc that says the opposite of another doc or of the code. **P1**
- **D4. Doc GC** — propose archive/merge/delete with evidence, batch-approve. Turns a scary directory into a five-minute chore. **P1**
- **D5. Derived progress** — plan checklist items bound to real commits/diffs/tests. Progress is computed. Catches "claimed done, no code" and "code exists, plan never updated". **P1** — the structural fix for progress-file rot.
- **D6. Decision log** — extract durable decisions out of chat transcripts and plan docs into a queryable ADR set, each linked to the code it governs. **P1**
- **D7. Agent-instruction hub** — `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, `.cursorrules` are the same knowledge in four formats, always drifting. One source, generated projections, drift warnings. **P0** — small, concrete, immediately useful.
- **D8. Ask-the-project** — retrieval over docs + decisions + transcripts + diffs: "why is this like this?", "what did we decide about retries?" **P2**

### Pillar E — Architecture & memory

The problem it solves: *the system is being modified faster than you can re-learn it.*

- **E1. Provenance / prompt-blame** — for any line: which run, which prompt, which model, which commit, reviewed by a human or not. `git blame` for intent. **P1**
- **E2. Timeline scrubber** — replay the project's evolution across runs; diff the architecture between two points in time. **P2**
- **E3. Live module map** — dependency graph from imports, diffed over time, with new-edge alerts. **P2**
- **E4. Drift alarms** — declare intended layering/boundaries; alert when an agent violates them. **P2**
- **E5. Context packer** — you are the architect, so curating what an agent sees is your main lever. Assemble files + docs + decisions + a failing test into a bundle and hand it to any agent. **P1**

### Pillar F — Editing (the 20%)

Decided scope: a **light editor at v0.4**, tuned for surgical fixes. Not an IntelliJ replacement — see Non-goals.

- **F1. Light editor** — tree-sitter + LSP, your keymap, good enough to fix a hunk the agent got wrong without leaving. **P1 (v0.4)**
- **F2. Surgical-edit bias** — optimized for "correct this hunk," not for greenfield authoring. Explicitly *not* multi-file refactoring. **P1**
- **F3. Handoff** — "open this in IntelliJ/VS Code" as a first-class action, no shame, forever. **P0**
- **F4. Structured transcript, not a terminal** — *revised*. Do not embed a VT100 emulator. The daemon owns the PTY, so render the run as structured events: prompts, tool calls, file writes, commands with exit codes, diffs, signals. Searchable, linkable, diffable — a terminal is none of those. "Open my real terminal at this worktree" covers the rest. **P0**

### Pillar G — Guardrails

- **G1. Path policies** — agents may not touch `infra/`, `.github/`, secrets, migrations without explicit approval. **P1**
- **G2. Dependency gate** — new package in a manifest ⇒ blocking review item. **P1**
- **G3. Destructive-op watch** — force pushes, history rewrites, mass deletions, `.env` writes surface immediately. **P1**

### Pillar H — Reach

- **H1. Local daemon + browser UI** — so the same session is reachable from a laptop, a tablet on the sofa, or a phone. Review and unblock agents away from the desk. **P1**
- **H2. Push notifications** — "run 3 needs you" on the phone. **P2**
- **H3. Big-screen ambient mode** — glanceable board for a second monitor. **P2**

## 6. The signature features

If mogeung is remembered for three things, these are the three. Everything else is support.

1. **The attention router (A2)** — turns N parallel agents from chaos into a queue. This is the difference between running one agent and running four.
2. **Review checkpointing (B3)** — you never read the same code twice. Directly attacks "I can't keep up with the pace."
3. **Derived progress + doc GC (D5, D4)** — the doc sprawl stops growing because progress is computed and dead docs get collected.

A fourth, close behind: **the claim ledger (C1–C2)** — the trust layer. Once you have it, you stop hand-checking whether the agent really ran the tests.

## 7. Architecture

### Shape

```
┌────────────────────────────────────────────────────────────┐
│  Clients                                                    │
│  native Rust UI (primary) · thin web UI (phone) · CLI       │
└───────────────────────┬────────────────────────────────────┘
                        │ HTTP + WebSocket (local)
┌───────────────────────┴────────────────────────────────────┐
│  mogeungd — local daemon (the actual product)               │
│                                                             │
│  Run supervisor    PTY/JSON adapters per agent CLI          │
│  Repo watcher      fs events, git plumbing, worktrees       │
│  Change engine     semantic diff, risk scoring, anchors     │
│  Review store      hunk state keyed by content hash         │
│  Signal runner     test/typecheck/lint/build scheduler      │
│  Doc engine        classify, staleness, clustering          │
│  Index             tree-sitter symbols + embeddings         │
│  Policy engine     guardrails, gates                        │
│                                                             │
│  Store: SQLite (state) + files (transcripts, blobs)         │
└─────────────────────────────────────────────────────────────┘
```

The daemon is the product. UIs are clients. That choice buys three things: the tool keeps working when no window is open, reach from any device on the network comes free, and a native shell later is a packaging decision rather than a rewrite.

### Stack: decided

**Rust core, Rust-native UI, web UI as an optional second client.**

Because the daemon is the product and UIs are clients over one API, "native Rust UI" and "web UI" are not competing choices — they are two clients. The API boundary is the load-bearing decision; everything else is replaceable.

**Core — `mogeungd`**

| Concern | Choice | Note |
|---|---|---|
| Async runtime | `tokio` | |
| API | `axum` (HTTP + WebSocket) | The one contract every client speaks |
| Git plumbing | shell out to `git` first, `gix` for hot paths | Shelling out is more *correct* on worktrees; optimize later where it hurts |
| Diffing | `imara-diff` | Plus our own hunk-anchor layer for review checkpointing |
| File watching | `notify` | |
| Process/PTY | `portable-pty` | Wezterm's; drives agent CLIs |
| Parsing/symbols | `tree-sitter` (+ `syntect` for highlight) | |
| Store | SQLite via `rusqlite` (bundled) | Transcripts and blobs on disk, not in the DB |

This is the low-risk half. Rust's ecosystem for git, watching, PTY, and parsing is genuinely excellent, and these are exactly the components that must stay fast for years.

**UI — `egui` (via `eframe`)**

The Rust GUI field, judged against *this* app:

| Option | Verdict |
|---|---|
| **egui** ✅ | ~13M downloads (≈10× iced and slint combined), best docs, fastest to a working dashboard, proven on serious tooling (Rerun). Immediate-mode suits a live-updating cockpit. Weak on rich text editing and terminal emulation. |
| **GPUI** | Tempting — it powers Zed, so it clearly can do editors. But it is maintained *for Zed*, not for outsiders: pre-1.0 with explicit no-API-stability, ~3 publishes in 18 months trailing monorepo HEAD, thin standalone docs, ~101k lifetime downloads. A foundation that moves under you. |
| **Iced** | More native-feeling, Elm-style, but slower to set up and a thinner widget ecosystem. |
| **Floem** | Same maintained-for-one-app (Lapce) risk as GPUI. |
| **Xilem** | Best long-term architecture, best team. Not production-ready. Watch it. |
| **Slint** | Strong for polished product UI; licensing model and text-heavy work are poor fits here. |

**Consequence — no embedded terminal.** egui's genuinely weak case is VT100 emulation, and we should not want one anyway. The daemon spawns agent runs under its own PTY, so the right view is a **structured transcript** — tool calls, diffs, signals, and claims as first-class objects — not emulated terminal output. That is truer to the thesis *and* sidesteps the hard case. "Open my real terminal here" covers the remainder. (See F4, revised.)

**Known deferred risk:** the v0.4 light editor is the one place egui will fight us. Three outs when we get there: build it on egui + tree-sitter, embed a webview panel running CodeMirror for editing only, or keep the IntelliJ handoff. The daemon boundary makes this cheap to defer — do not spend design budget on it now.

**Web UI — deliberately not a second full client.** Scope it to *review and unblock from a phone or tablet*: the attention queue, a readable diff, approve / comment / reply-to-agent. Nothing else. A second full-fidelity client is not maintainable solo and is not the point.

### Agent integration

The dependency risk in this whole project is that agent CLIs are moving targets. Contain it in one layer:

- **Adapter interface**: `start(intent, worktree) → RunHandle`, `stream() → events`, `send(text)`, `interrupt()`, `result() → {diff, claims, cost}`.
- **Claude Code**: streaming JSON output, hooks, session resume, SDK — the richest integration; build here first.
- **Codex / Gemini CLI**: JSON or PTY capture with a parser per CLI.
- **Universal fallback**: PTY wrap + git-diff observation. Loses transcript fidelity, still yields Runs and Changes. Guarantees nothing is un-integrable.
- Treat every adapter as untrusted and version-fragile: parse defensively, degrade to the fallback rather than break.

## 8. Roadmap

**v0.1 — "Where do I look?" (the wedge — decided)**
`mogeungd` + egui client. Run board (A1) + **attention router (A2)** + Claude Code adapter (A3) + worktree manager (A6) + structured transcript (F4) + a basic change view + open-in-IntelliJ (F3). Review checkpointing (B3) arrives here only if it is cheap; otherwise it leads v0.2. No editor. No second agent. No web client.

Success test: for one week, do not open a terminal tab to check on an agent. If that holds, the project is real.

**v0.2 — "Is it true?" / "Not twice"**
Review workspace proper: risk ordering (B2) and review checkpointing (B3) if deferred. Then:

Signal runner, claim ledger, blast radius, note → instruction round-trip, cost ledger.

**v0.3 — "Stop the sprawl"**
Doc inventory, staleness, GC, derived progress, agent-instruction hub. Turn the doc pile into a managed surface.

**v0.4 — "Own the 20%"**
Real editor (LSP, tree-sitter, keymaps), context packer, provenance/prompt-blame.

**v0.5+ — "Depth"**
Multi-agent adapters, race mode, architecture map and drift, timeline, mobile/notifications, policies.

Rule for the whole roadmap: **dogfood from v0.1 on**, building mogeung with agents inside mogeung. If a feature does not change how the next day feels, cut it.

## 8b. Course correction — the observer pivot (2026-07-25)

v0.1 was built and rejected in use: *"a handicapped Claude Code with a single
session."* The judgement was right, and the error was in this document, not in
the implementation.

**What §8 got wrong.** It picked the attention router as the v0.1 wedge, which
was defensible — but then assumed mogeung had to *spawn* runs to populate it.
That forced replacing the interactive loop, and the interactive loop is the part
Claude Code is genuinely good at. The result was strictly worse than a terminal
until three or four sessions were running, while making three or four sessions
awkward to reach. A ranked queue of one item is just a label.

**The correction.** mogeung observes the sessions you already run, by reading
what Claude Code writes for itself:

* `~/.claude/sessions/<pid>.json` — a live registry with a first-party
  `status: busy|idle`
* `~/.claude/projects/<slug>/<id>.jsonl` — the transcript

This is purely additive, so it cannot degrade a session. It also *improves* the
product: A2's hardest problem — knowing a session is blocked on a human — stops
being an inference and becomes a fact published by the CLI.

**What this changes in the pillars above.** A1–A2 stand, now over observed
sessions. A3 (adapters) becomes "read other CLIs' on-disk formats", which is a
smaller job than driving them. A4 (interject) and A6 (worktree manager) are
**deleted** — you interject by typing in your own terminal. A7 (race mode) and
A8 (cost ledger) are deferred. Everything in Pillars B, C, D and E is unaffected,
because none of it ever depended on mogeung owning the agent.

**The durable lesson.** The value was always in the review and attention layer.
Owning the conversation loop was never a requirement for it — it was an
assumption, and it was the expensive kind: it cost the whole product.

## 9. Decisions

Settled 2026-07-25:

1. **Stack** — Rust core (`mogeungd`) + native Rust UI (egui/eframe). Web UI later as a thin phone-shaped review client, not a second full client. *(held up)*
2. **Wedge** — attention router leads v0.1. *(right feature, wrong delivery — see §8b)*
3. **Agent scope** — Claude Code only. *(held up)*
3b. **Relationship to the agent** — observe, never spawn. Added v0.2 after §8b.
4. **Editor** — light editor at v0.4, surgical fixes only. IntelliJ handoff stays a first-class action permanently.
5. **Terminal** — none. Structured transcript instead (see F4).

### Still open

6. **Git model** — worktree-per-run as the default unit, or plain branches? Leaning worktree-per-run: it is what makes parallel agents safe, and it makes "race mode" (A7) nearly free later. Cost is disk and dev-server port collisions.
7. **Scale target** — what is the largest repo this must stay fast on? Sets the indexing budget and decides how early `gix` replaces shelling out to `git`.
8. **Attention ranking** — hand-written heuristics or learned from which runs you actually open? Start hand-written; the signal is worth collecting from day one either way.
9. **Cross-machine** — does the daemon ever need to run on a remote box (dev server, cloud sandbox) with the UI local? Changes nothing architecturally, but affects auth and transport decisions if the answer is yes.
