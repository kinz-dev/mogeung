# mogeung v0.1 — build summary

Written 2026-07-25, at the end of the session that produced it.
This is the "what happened and what to do next" document. [README.md](README.md)
is how to run it; [CONCEPT.md](CONCEPT.md) is why it exists.

---

## What was delivered

A working, tested v0.1 of the wedge chosen in CONCEPT.md §8: **"where do I
look?"**

| | |
|---|---|
| Rust | ~4,460 lines across 3 crates |
| Tests | 21 (20 free + 1 real-agent, `#[ignore]`d) |
| Release binaries | `mogeungd` 6.0 MB · `mogeung` 14 MB |
| Warnings | none |
| Verified against | Claude Code 2.1.220, git 2.50.1, rustc 1.94, macOS |

### Shipped

- **Attention router** — one ranked queue across all repos, with wide tier
  separation and a visible reason string per row. Recomputed on a timer so
  silence registers as a signal.
- **Claude Code adapter** — `stream-json` normalised into typed transcript
  events; forgiving parser that degrades rather than dying on schema change.
- **Worktree-per-run** — isolated branch per run, diffs against the run's base
  commit, **including untracked files**.
- **Risk-ordered diffs** — path + content heuristics, noise suppression, file
  scored as its riskiest hunk.
- **Review checkpointing** — content-hash hunk anchors, review marks keyed to
  the session root so follow-ups never re-present read code.
- **Follow-ups** — resume the agent's own session, child run, parent retired
  from the queue.
- **Structured transcript** — no terminal emulator, by design.
- **Daemon + client split** — WebSocket state stream plus a curl-able REST
  surface; UI reconnects on its own; agents survive the window closing.
- **egui client** — queue, run detail, transcript, diff review with per-hunk
  checkboxes, new-run dialog, open-in-IntelliJ/VS Code/Terminal/Finder.

### Deliberately not built

The claim ledger, the signal runner, and **the entire document layer** (doc
inventory, staleness, GC, derived progress, agent-instruction hub). Also: no
editor, no live interjection, no second agent adapter. Full list in the README's
Limitations section, which is the honest roadmap.

---

## Decisions taken during the build

Judgement calls made without stopping to ask, and why.

**1. Review state is keyed to the session root, not the run.**
This was found by testing, not designing. The first follow-up test showed the
child run re-presenting code already read in the parent — which would have
destroyed the feature's whole premise. Fix: runs carry a `root` id, review
anchors hang off `root`, and a follow-up inherits the parent's base commit so
its Change is the session's cumulative diff. A follow-up now shows everything
you read still marked read, and only genuinely new work unread.

**2. A follow-up retires its parent from the queue.**
Otherwise one session asks for review once per turn and the queue fills with
stale entries.

**3. No embedded terminal.**
egui's weakest case is VT100 emulation — but the structured transcript is the
better product anyway, because tool calls and results become searchable,
linkable objects instead of scrollback. Concept doc F4 was revised accordingly.

**4. Shell out to `git`, do not link a library.**
On worktrees the CLI is the definition of correct. Untracked files are handled
with `ls-files --others` + `diff --no-index` rather than `add -N`, specifically
so mogeung never mutates your index behind your back.

**5. A curl-able REST surface alongside the WebSocket.**
Added while debugging, kept because it makes the daemon scriptable without a UI
and is how the e2e tests and any future shell workflow drive it.

**6. Runs are stored as JSON blobs in SQLite.**
The schema is still moving. At this scale, being able to change `Run` without a
migration matters more than query planning.

**7. egui 0.35 required a migration mid-build.**
0.35 replaced `App::update(ctx)` with `App::ui(&mut Ui)` and collapsed
`SidePanel`/`TopBottomPanel` into a unified `Panel`. Adapted rather than pinning
to an older release, since this is meant to be a base to build on.

---

## Spend

**Directly measured — agent runs mogeung itself spawned:**

| Purpose | Cost |
|---|---|
| CLI schema probes (2 sessions, before any code) | $0.153 |
| Smoke run, first cycle | $0.143 |
| Review-checkpointing test, run 1 | $0.146 |
| Review-checkpointing test, follow-up run | $0.116 |
| Real-agent e2e test | ~$0.10 |
| **Total agent spend** | **≈ $0.66** |

All on Sonnet, all against throwaway repos.

**Not measured here:** the cost of the Claude Code session that *wrote* mogeung.
That is not visible from inside the daemon — check `/usage` in the session that
produced this. Expect it to dominate the figure above by a wide margin.

**Cost note for real use:** the probe runs showed a trivial one-word prompt
costing $0.068, almost entirely cache-creation tokens. Short runs are not cheap.
The `BURNING` heuristic defaults to $1.00 with no diff, which is roughly "a few
turns of real work with nothing to show" — tune `AttentionConfig` in
`crates/mogeung-core/src/attention.rs` once you have a feel for your own runs.

---

## Honest assessment

**What is genuinely good.** The attention router and review checkpointing both
work and both were proven against real agent sessions rather than fixtures. The
checkpointing test is the one that matters: `auth.rs` stayed read while a
rewritten `main.rs` came back unread, across a session boundary. The
daemon/client split is right, and it is what makes a web client cheap later.

**What is thin.** Risk scoring is keyword matching over diff text. It works well
enough to be a useful reading order and it will embarrass itself on any adversarial
example. It is not analysis and must never be trusted as a safety check.

**The biggest real gap.** Non-interactive runs mean a tool needing permission is
*denied* rather than *queued for your decision*. mogeung notices afterwards and
ranks the run `BLOCKED`, which is a consolation prize. Live interjection via
`--input-format stream-json` closes both this and the "steer a running agent"
gap at once — it is the highest-value next increment by some distance.

**What might be wrong.** The adapter abstraction has only ever met one CLI, so
it is a guess. Adding Codex is the test of whether the Run model generalises,
and it may force changes to `Run` and `EventKind`.

---

## Suggested next steps, in order

1. **Dogfood for a week.** The v0.1 success test from CONCEPT.md: do you stop
   opening terminal tabs to check on agents? Nothing below matters until this is
   answered.
2. **Live interjection** (`--input-format stream-json`) — closes the permission
   gap and the steering gap together.
3. **Signal runner** — run tests/typecheck per worktree and attach results to the
   run card. This is the cheapest large step toward "is it true?", and it is a
   precondition for the claim ledger.
4. **Second adapter (Codex)** — falsify or confirm the adapter abstraction while
   the codebase is still small enough to change.
5. **Thin web client** — the daemon already supports it; scope it to
   review-and-unblock on a phone, nothing more.
6. **Document layer** — only if a week of dogfooding says doc sprawl still hurts
   more than the above.

Deliberately *not* next: the editor. Keep handing off to IntelliJ.
