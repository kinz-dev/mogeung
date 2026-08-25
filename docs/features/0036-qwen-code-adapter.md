---
title: Qwen Code adapter
status: in-progress
updated: 2026-08-25
roadmap: [R-I15]
depends_on: [A4, A23]
triage: ready-for-agent
---

# 0036 — Qwen Code adapter

**Honesty note.** Every shape this adapter parses was read from real transcripts
on the author's machine (Qwen Code 0.22.0, `~/.qwen`, 2026-08-25) and
cross-checked against the shipped bundle's own source, which is esbuild-bundled
but **not minified** and keeps its `// packages/core/src/...` banners and JSDoc.
That is stronger evidence than `R-I1` had for Codex, where every rollout shape
was inferred from a binary against an empty install.

It is still thin. The corpus is **two sessions and 43 lines**, one model, one
provider, one auth type, no git project, no subagent transcript, no archived
session, no compaction, and **no tool-approval prompt**. Seven of Qwen's
nineteen `system` subtypes have been seen; the other twelve are classified from
the writer's source and have never met a real byte. So the row stays ⏳ and the
canary discipline is doing real work here — see *Risks* for the one gap that is
not a matter of corpus size.

## Spec

### Problem

A Qwen Code session running beside Claude Code sessions is invisible to the
queue. The user runs `qwen` against a local model and mogeung — whose whole job
is *which of my sessions needs me* — silently answers as though those sessions
do not exist.

`R-I1` asked whether the `Session` model generalises beyond Claude Code and
could not finish answering, because `~/.codex` on this machine has never held a
session. `~/.qwen` holds real ones. So this is not only a third adapter; it is
the first chance to actually test [A23](../product/assumptions.md) end to end.

### Assumptions

- **[A4](../product/assumptions.md)** — *the on-disk formats are stable enough
  to depend on* — `AT RISK`, and always will be. Qwen's position is **better
  than Claude's**: four of its six file formats carry an explicit version field
  and their readers reject a version above their own, which is how they signal a
  break. Its `.runtime.json` sidecar even carries a documented contract naming
  *"status daemons"* as intended consumers. The transcript itself, which is what
  this adapter mostly reads, has **no version field** — so that is the format
  most likely to drift silently, and it is the one we depend on most.
- **[A23](../product/assumptions.md)** — *the Session model generalises beyond
  Claude Code* — `UNTESTED`.

> The rule says: if an assumption is `UNTESTED`, the work is to test it, not to
> build the feature.

**This work is that test, and the spec has to argue it rather than assume it.**
`R-I1` named building the adapter as A23's stated test and then could not run
it, because there were no Codex sessions to run it against. Qwen supplies what
Codex could not: real sessions, in a real project, from a real user. If
`Session` absorbs them without gaining a field, A23 has been exercised for the
first time. If it does not, that failure is the finding and is worth more than
the feature.

The honest counter-argument, recorded because it will be raised: the roadmap's
own 2026-08-20 note says this product is *"several deep"* in building past
untested assumptions. Adding a third adapter on a generalisation claim that two
adapters have not settled is exactly that pattern. The defence is that this
adapter is cheap to reverse, produces a measurement either way, and is the only
route by which A23 can stop being `UNTESTED` — not that the rule does not apply.

### Acceptance

- [x] A Qwen session in `~/.qwen` appears in the queue with its cwd, prompt,
      turn count, tool calls and tokens.
      *Verified by `tests/qwen_scan.rs::a_finished_qwen_turn_is_a_session_waiting_on_you`
      against a synthetic install, and by the parser tests against verbatim
      real records.*
- [x] A finished turn reads as **waiting on you**; a turn with a tool call in
      flight reads as **working**.
      *`a_finished_qwen_turn_is_a_session_waiting_on_you` and
      `a_trailing_tool_call_is_a_session_still_working`.*
- [x] Liveness is the OS's answer, not the file's — a registry record whose
      process is gone does not resurrect a session.
      *`a_stale_registry_record_is_not_a_live_session`. Qwen unlinks the record
      on a clean exit, but a crash leaves it behind.*
- [x] A session that has registered but written nothing yet still appears.
      *`a_just_started_session_appears_before_it_has_written_anything` — the
      `R-J30` mistake, avoided one directory over.*
- [x] `Session` gains **no field** for Qwen. This is A23's actual test.
      *`blank_session` is unchanged; the diff base, git observer, attention
      ranking and queue all worked unmodified.*
- [x] An unrecognised record type or `system` subtype raises a named health
      alert, prefixed by source.
      *`an_unknown_qwen_shape_reaches_the_health_alerts` asserts on
      `qwen/system/telepathy`.*
- [x] `--bin sweep` sweeps `~/.qwen` and exits non-zero on anything
      unclassified.
      *Run 2026-08-25: 2 files, 43 lines, 0 unclassified, exit 0.*
- [x] mogeung refuses to **start** `qwen`, per ADR-0025 clause 2.
      *`run::AGENTS` gained `qwen` and `qwen-code`;
      `an_agent_is_recognised_however_it_is_spelled` covers both.*
- [x] A Qwen session started under tmux can be **hosted** in the Agent pane,
      not merely pointed at — and `scripts/qwenmo` is the `yolomo` that puts
      it there.
      *`a_qwen_session_under_tmux_can_be_hosted_rather_than_only_seen` drives a
      real tmux pane. Verified live: a `qwenmo -d` session resolved
      `mogeung-qwen-qwendemo:0.0`, while one started with bare `qwen` resolved
      `null` — seen, not hostable.*
- [x] A closed session stops reading as live. Liveness is the **same process**,
      not merely the same pid, and a session nothing found is not running.
      *`a_zombie_is_not_a_live_session` builds a real defunct process;
      `a_reused_pid_does_not_inherit_a_dead_session` and
      `a_qwen_session_whose_transcript_vanishes_stops_being_alive` cover the
      other two routes.*
- [ ] Dogfooded: a real Qwen session, running beside real Claude sessions, tiers
      correctly in the window over a working day.
      *Not yet. This is the box that keeps the row ⏳ rather than ✅.*

### Explicitly out of scope

- **Transcript events.** A Qwen session gets queue presence and status, not a
  readable transcript in the detail pane. Codex set this precedent and the
  `EventKind` mapping is a separate job.
- **Dollars.** Qwen here runs against a local endpoint (`http://spark-7ecc:8000/v1`).
  There is no public price for it and `pricing.rs` deliberately returns `None`
  rather than `Some(0.0)` — the model lands in `unpriced_models` and the
  Analytics view says so. Per [ADR-0024](../decisions/0024-equivalent-cost-in-dollars.md),
  inventing a rate would be worse than showing none.
- **Analytics token burn.** `UsageScanner` walks `~/.claude/projects` only, so
  Qwen contributes nothing to the Analytics view — the same gap Codex has. Noted
  as a deliberate omission rather than an accident; the per-session counters on
  the queue are unaffected.
- **Subagent transcripts** (`agent-<id>.jsonl`) and **archived sessions'**
  contents. The archive directory *is* scanned for sessions; subagent files are
  skipped by the filename pattern.
- ~~**Launching Qwen from the New-session window.** That window starts `claude`
  and nothing else; widening it is its own decision.~~ **Taken 2026-08-25 as
  `R-J51`**: the window offers the CLI as a choice, the daemon owns one recipe
  per source, and Qwen's is `qwenmo`'s — `--approval-mode yolo`, whose blind
  spot below is the reason the flag is `yolo` rather than `auto`.
- **Qwen's skills and memory** (`~/.qwen/skills`, `projects/*/memory/MEMORY.md`).
  The Kit pane is Claude-only and stays so.

## Plan

### Approach

Qwen's install is *shaped like Claude's* and its *records are shaped like
Gemini's*, so the module follows `codex.rs`'s file layout and `watcher.rs`'s
discovery model:

- `sessions/<pid>.json` — a live registry, read like Claude's, liveness by
  `kill(pid, 0)`. **No status field**, unlike Claude's; that is the gap below.
- `projects/<sanitizeCwd(cwd)>/chats/<session-id>.jsonl` — the transcript,
  tailed by byte offset, two directory levels deeper than Claude's and sharing
  its directory with four kinds of sidecar that must not be read as sessions.
- Classification descends one level: a `system` record's real discriminator is
  `subtype`, and roughly three lines in five are `system`.

[ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md) records
why this is a variant and a module rather than a plugin interface, and why the
"is it Claude?" questions became named methods on `SessionSource`.

### Files touched

- `crates/mogeungd/src/qwen.rs` — **new.** Discovery, registry, parser, tailer,
  status heuristic.
- `crates/mogeung-core/src/session.rs` — `SessionSource::QwenCode`, plus
  `in_claude_live_registry()` / `has_claude_event_history()`.
- `crates/mogeung-core/src/run.rs` — `qwen`, `qwen-code` on the never-start list.
- `crates/mogeung-core/src/health.rs` — `AgentHealth`, `Health.agents`.
- `crates/mogeungd/src/health.rs` — per-source slots replacing the single tuple.
- `crates/mogeungd/src/state.rs` — `AgentHomes`, `qwen_home`, `qwen_cache`,
  `scan_qwen`, `absorb_qwen`, and the two guards.
- `crates/mogeungd/src/bin/sweep.rs` — a third corpus, and a folded unknown count.
- `desktop/src/wire/types.ts`, `lib/agentHealth.ts` (new), `lib/queue.ts`,
  `panes/InfoPane.tsx`, `ui/HealthWindow.tsx`.
- `scripts/qwenmo` — **new**, `yolomo` for Qwen; `scripts/install.sh` installs
  it; `scripts/yolomo` gained the same one-line tmux fix (below).

### Risks and unknowns

**The one that is not about corpus size: waiting and working are the same bytes.**
Qwen does not persist turn state. `streamingState` (`idle` / `responding` /
`waiting_for_confirmation`) is React state; `turn_result` is written only by the
ACP/serve path; `goal_state.snapshot.activity` looks like the flag we need and
is hardcoded to `"idle"` on write, so reading it would be worse than reading
nothing. A session **blocked on a tool approval** and one **running that tool**
both end on an `assistant` record carrying a `functionCall`. Both are reported
as working. `open_tools` is deliberately left empty rather than synthesising an
approval the way the Codex pass can, because mogeung must not claim evidence it
does not have. This is `R-B4`'s distinction, and for Qwen it is currently
unavailable.

*Mitigation considered and not taken:* Qwen spawns `systemd-inhibit` while a
turn is streaming, and its own setting text says idle time and permission
prompts do **not** inhibit sleep — so the presence of that child process is very
nearly the missing bit. Rejected for now: Linux-only, suppressed under headless
SSH, user-disableable, and it is process-table state rather than a contract Qwen
promises to keep. Worth revisiting if the blind spot bites in practice.

**Twelve unseen subtypes.** Classified from the writer's source. `--bin sweep`
is the instrument; run it after every Qwen upgrade.

**Two model names for one call.** `assistant.model` is the configured alias
(`qwen3.8-sglang`), `uiEvent.model` is the wire model
(`RadixArk/Qwen3.8-27B-NVFP4`). The alias is shown, because it is the string the
user typed into their own settings.

**Double counting was a live trap.** The same call's usage appears twice — on
the `assistant` record and on an `api_response` telemetry record. Only the
former is counted; a test pins it.

**`promptTokenCount` already includes the cached share.** Adding them would
double every cache hit. A test pins that too.

**`gitBranch` is in the writer and absent from every observed record** (both
projects are non-git). Treated as optional and unverified against a git project.

**Everything is relocatable** — `QWEN_HOME`, and a settings-level base dir this
adapter does not read. A user who has moved it gets "no sessions" rather than a
wrong answer.

**`general.chatRecording` can be turned off**, in which case no transcript is
written and mogeung sees only the registry. Not currently distinguished from
"no sessions".

### Test strategy

- `crates/mogeungd/src/qwen.rs` unit tests — 13, over records copied verbatim
  from `~/.qwen`, plus a completeness test asserting the nineteen subtypes are
  each classified exactly once.
- `crates/mogeungd/tests/qwen_scan.rs` — 8 end-to-end tests over a synthetic
  `~/.qwen`, including one that writes **the test process's own pid** into the
  registry to get genuine liveness rather than a mock.
- `desktop/src/lib/agentHealth.test.ts`, `queueSource.test.ts` — 11 client tests.
- `--bin sweep` over the real corpus, which must exit 0.

## Notes

**2026-08-25 — what the build turned on.**

*What is evidence:* every record shape, both directory encodings (`sanitizeCwd`
for `projects/`, full `sha256` for `tmp/`, both verified by computation against
the real directory names), the registry schema, the token vocabulary, and the
nineteen-subtype list — all read from real files and confirmed against the
shipped bundle and the upstream repository.

*What is inference:* the status heuristic, entirely. Also the twelve subtypes
never seen, and the claim that `promptTokenCount` behaves the same on a
provider other than the one configured here.

**The bug worth recording is not in the parser.** Two lines in `state.rs` read
`if s.source == SessionSource::Codex { continue }`. Both *meant* "if this is not
Claude", and with exactly two variants those are the same sentence. Adding a
third made them different, and the failure mode was quiet and total: every Qwen
session marked dead on every tick by Claude Code's liveness pass, moments after
its own scan had correctly marked it alive. Both are now exhaustive `match`es
behind named methods, so the fourth CLI is a compile error rather than a bug
report. `a_live_qwen_session_survives_the_claude_liveness_pass` scans three
times specifically to catch a regression that a single pass would hide.

**A test found a real hardening.** `kill(0, 0)` signals the caller's own process
*group* and therefore succeeds, so a truncated registry record naming pid 0
would have read as a live session forever. Found by writing the stale-record
test with pid 0 as an obviously-dead pid, and fixed in `pid_alive`.

**The sweep found a shape the analysis had missed.** The corpus grew from 18 to
43 lines during the work, and a `qwen-code.api_error` telemetry event appeared
that had not existed when the format was first read. It is surfaced as activity
and deliberately **not** as `Session.error`: attention ranks an error at tier
900, ahead of everything but a permission prompt, and these are routinely
retried — a transient 429 would otherwise pin a healthy session as failed for
the rest of its life. This is the sweep doing exactly the job `R-J28` built it
for, inside a single day's work.

**Running it against the real `~/.qwen` found two things the tests did not.**
Both are the kind that only a real corpus produces, and both were wrong in the
direction that looks fine on a synthetic fixture.

*A session opened with a slash command had zero turns and no prompt.* The
tic-tac-toe session's only `user` record is the **synthetic** goal-continuation
one; the human's actual words — `/goal createa a web base tic-tae-toe game…` —
were in a `system`/`slash_command` record, which was being read for its activity
line and not counted as a turn. A command the human typed *is* a turn, and for
that session it was the only one. `hiddenInvocation` distinguishes the CLI
invoking a command on its own behalf, and the `result` half of the pair is an
echo that must not count twice.

*A tool result was overwriting the tool's name with the word "tool result".*
The `assistant` record puts `edit` or `read_file` into `last_activity`; the
following `tool_result` replaced it with a generic label whenever
`resultDisplay` was absent. `result_display` now returns `None` in that case and
the caller leaves the better answer standing.

Verified after the fix against the live session: 1 turn, the `/goal` prompt,
14 tool calls, `last_activity: "edit"`, and `alive`/`busy` correct while the
agent was genuinely mid-turn.

**Reported the same day: "it doesn't work as it is not being wrapped by tmux".**
Two separate gaps, and the second one was the adapter's.

*There was no `yolomo` for Qwen.* Now there is — `scripts/qwenmo`, a deliberate
sibling rather than a flag on `yolomo`, on the same reasoning ADR-0029 gives one
directory over. It differs in two lines that matter: the binary, and
`--approval-mode yolo` in place of `--dangerously-skip-permissions`. It also
resolves the binary itself, because Qwen installs to `~/.local/bin` and that is
not on every shell's PATH — a "command not found" from inside a detached tmux
session is invisible.

*And even wrapping it by hand would not have worked.* `tmux_target` was resolved
**only inside Claude Code's liveness pass**, which every other source correctly
skips. So a tmux-wrapped Qwen session was right in every visible field and still
could not be attached to. `scan_qwen` now does its own pane lookup, skipped
entirely when nothing is alive so a machine with no Qwen sessions pays no forks.

*A pre-existing bug in `yolomo`, found by copying it.* The line
`tmux set-option -t "=$name" window-size latest` fails on tmux 3.6 with
`no such window` — `window-size` is a *window* option and needs `-w` and a
trailing `:`. It has been failing silently on stderr, after the session is
already up, for as long as it has been there, which means the protection its
comment describes (a narrow mogeung pane shrinking your full-screen terminal)
has never actually been applied. Fixed in both scripts.

There is a pleasing consequence of running Qwen under `--approval-mode yolo`:
the blind spot above cannot be hit, because there are no approval prompts to be
blind to. That is a mitigation, not a fix — `--approval-mode auto` brings it
straight back — and it is noted in the script rather than relied on.

**Reported next: a closed session still showing as live, then STALLED.** Three
causes, all of them mine, and the shape of each is worth keeping.

*`kill(pid, 0)` was answering the wrong question.* It says whether **some**
process holds that pid, not whether it is **the same** process. A qwen killed
with its parent still around stays in the table as a zombie until reaped, and
signals to it succeed — so a stale registry record went on reporting a finished
session as busy, and then STALLED when it inevitably stayed silent. Qwen guards
this itself with `procStart` (a boot id and the process's start time) and
`pidNs`, and the first draft of this adapter read both fields and checked
neither. It does now, including the zombie state character. Where `/proc`
cannot answer — macOS, where Qwen writes `procStart: null` for the same reason —
it degrades to liveness alone rather than refusing every session.

*Pid reuse was the same hole by another route.* Qwen unlinks its record on a
clean exit, but a crash or a `tmux kill-session` leaves it behind, and pids
wrap. `procStart` closes this one too.

*And a structural hole with no bad data required.* `scan_qwen` walks what it
**finds** — transcripts on disk plus the registry — where the Claude pass walks
every id it **knows** and marks the missing ones dead. So a Qwen session that
dropped out of both, by ageing past `HISTORY_DAYS` or being archived, was never
revisited and kept the `alive` it was last given, permanently. The scan now
sweeps its own known-and-not-found sessions at the end of the pass.

The three tests for these are worth more than the fix: two of them make the
real thing rather than describing it — an actual defunct process, and an actual
tmux pane — because a mock of a zombie is a mock of the assumption that was
wrong in the first place.

**A23 is not flipped by this document.** The adapter runs and its tests pass,
which is the structural half. The end-to-end half is a working day of real use
with Qwen and Claude sessions in one queue, and that has not happened yet.
