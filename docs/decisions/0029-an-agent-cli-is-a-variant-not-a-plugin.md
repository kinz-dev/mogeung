---
title: An agent CLI is a variant on the Session, not a plugin
status: active
updated: 2026-08-25
decided: 2026-08-25
---

# ADR-0029 — An agent CLI is a variant on the Session, not a plugin

## Context

mogeung watches Claude Code. [ADR-0003](0003-observe-do-not-spawn.md) settled
that it observes rather than spawns, and `R-I1` added a second CLI — Codex — to
find out whether the `Session` model generalised
([A23](../product/assumptions.md)). `R-I15` adds a third, Qwen Code, and three
is the number at which the shape of the extension point stops being a detail.

The two existing readers are independent modules of free functions over
`serde_json::Value`. There is no trait. `adapter.rs` and `codex.rs` share
exactly one thing: they both converge on `LineClass`, so both CLIs' drift feeds
one health vocabulary. Everything else — the discovery walk, the record shapes,
the status heuristic — is written twice.

Three facts made this a real decision rather than an obvious one.

**The formats have nothing in common below the envelope.** Claude's transcript
is Anthropic-shaped, Codex's rollout is a nested `payload.type` union read
alongside a SQLite index, and Qwen's is a Gemini `Content` — `{role: "model",
parts: [...]}` — with a nineteen-way `system` subtype union carrying roughly
three lines in five. A trait over "parse a line" would have one honest method
returning an outcome, and every other useful thing about each CLI would sit
outside it.

**The generalisation that did hold was the `Session`, not the parser.** Adding
Qwen added no field to `Session`. The git observer, the diff base, attention
ranking, the queue, snooze, labels and tags all worked on a Qwen session the
first time they saw one, because they were written against `Session` rather than
against Claude.

**The thing that actually broke was not the parser at all.** Two guards in
`state.rs` read `if s.source == SessionSource::Codex { continue }` and *meant*
"if this is not Claude". With two variants those are the same sentence. With
three they are not: a Qwen session fell into Claude Code's liveness pass and was
marked dead on every tick, while its own scan had just marked it alive.

## Decision

**A supported agent CLI is a variant of `SessionSource` and a module beside
`adapter.rs`. There is no plugin interface, no registry and no dynamic
loading.** Adding one is: a variant, a module, a scan function, a health slot, a
sweep corpus, and a test file.

**Questions about a source are asked as named properties, never as equality
against a variant.** `SessionSource::in_claude_live_registry()` and
`has_claude_event_history()` each `match` exhaustively, so the next CLI added is
a compile error at every site that has to think about it, rather than a silent
wrong answer at runtime.

**Every CLI's canary reports into one list.** `Health.agents` is a `Vec<AgentHealth>`
keyed by source. The four flat `codex_*` fields it replaces are still populated
for older clients and are marked superseded.

**A parser degrades and never panics**, per
[ADR-0007](0007-classify-every-transcript-line.md), and classifies every line —
including one level below the top-level type where that is where the taxonomy
really lives (`system/<subtype>` for Qwen, `<kind>/<item>` for Codex).

**mogeung never starts any of them.** Each CLI's program name joins
`run::AGENTS`, the never-start list [ADR-0025](0025-run-a-process-you-named-never-an-agent.md)
clause 2 requires.

## Alternatives

**A `trait Adapter` with a blanket scan loop.** Rejected on inspection of what
the three implementations would actually share. The scan loops differ in kind,
not in detail: Claude and Qwen tail per-session files listed from a per-pid
registry, Codex reads a SQLite index and resolves rollouts that may have moved.
Forcing one signature over both produces a trait whose associated types and
optional methods encode the differences anyway, and a reader then has to hold
the trait *and* the impl in their head to know what happens. The duplication
between `codex.rs` and `qwen.rs` is real and is roughly a hundred lines of
tailing logic — cheaper than the wrong abstraction, and visible enough to unify
later if a fourth CLI makes the shape obvious.

**A plugin interface — describe a format in TOML/JSON, ship adapters
separately.** Rejected because it inverts where the risk is. These are
undocumented private formats that move without warning; `A4` records three
Claude drift events in four weeks. The valuable thing mogeung does is *notice*
when one moves, which requires the classification lists to be compiled in,
swept by `--bin sweep`, and pinned by tests against a real corpus. A
declarative plugin would be a second source of truth that goes stale on its own
schedule — the exact failure `--bin sweep` exists to prevent by refusing to
carry its own copy of `HANDLED`.

**A `SessionSource::Other(String)` catch-all so unknown CLIs degrade
gracefully.** Rejected: a source mogeung has no reader for produces no sessions,
so the variant would never be constructed. It would only add an arm that every
`match` has to answer meaninglessly, which is the cost of exhaustiveness with
none of the benefit.

**Keep the single `codex` health slot and add `qwen_*` fields beside it.**
Rejected. Four flat fields per CLI is the shape that made the third one's canary
have nowhere to report, and a canary with nowhere to report is indistinguishable
from a format that has not drifted.

## Consequences

**Easy.** Adding a CLI is mechanical and the compiler names most of the sites.
Each reader is a single file that can be read start to finish and tested
against real bytes without a framework. Drift in any CLI raises the same alert
in the same list, prefixed by source.

**Hard.** The tailing and folding logic is now written three times. A change to
how incremental reads handle a truncated final line has to be made in three
places, and nothing forces the third. This is a known, bounded cost and it is
the one to watch: if a fourth CLI arrives, the right move is to extract the
tailer first and only then write the parser.

**Ruled out.** Third-party adapters. Anyone wanting mogeung to watch a CLI it
does not know has to change this repository — which is the honest position,
because supporting a format means committing to sweep it after every upgrade,
and that is not a commitment a plugin author can make on our behalf.

**The uncomfortable part.** Qwen Code does not record whether it is waiting for
you. Its `streamingState` — `idle` / `responding` / `waiting_for_confirmation` —
lives in React state and is never written down, and the one record that would
settle it (`turn_result`) is written only by its ACP/serve path, never by the
interactive CLI a human runs. So a Qwen session blocked on a tool approval and
one busily running that tool are **indistinguishable on disk**, and both are
reported as working. That is the precise distinction this product exists to
draw (`R-B4`), and for one of its three CLIs it currently cannot. It is recorded
here rather than smoothed over, because a queue that guesses confidently is
worse than one that admits a blind spot.

## Revisit if

A fourth agent CLI arrives — extract the shared tailer before writing its
parser, and reconsider whether the trait now has enough shape to be worth
having.

Or Qwen Code begins writing turn state to disk (a `turn_result` from the
interactive path, or a status field on `sessions/<pid>.json`). That would close
the blind spot above and is worth checking on every Qwen upgrade.

Or a CLI appears whose sessions are not files on this machine —
[`R-I13`](../product/roadmap.md)'s container case — since the assumption that a
source is a directory under `$HOME` is baked into every reader here.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
