---
title: Health and the format canary
status: active
updated: 2026-08-25
covers:
  - crates/mogeung-core/src/health.rs
  - crates/mogeungd/src/health.rs
---

# Health and the format canary

Everything mogeung knows comes from two undocumented file formats
([A4](../product/assumptions.md)). The parser degrades rather than crashing, so
the realistic failure is **seeing less than it should** — and a board that has
gone half-blind looks exactly like a quiet afternoon.

This subsystem exists to make that indistinguishable case distinguishable.
Roadmap `R-A1`, `R-A2`, `R-A4`, `R-A5`.

## The core distinction

Before this existed, `parse_line` returned `Option<Parsed>` and `None` meant two
unrelated things:

- *bookkeeping we classified and chose to skip* — normal, thousands per day
- *a type from a release we have never seen* — the thing we most need to know

Five outcomes now, and the middle three are all "no data":

| `LineClass` | Meaning | Counts as blindness |
|---|---|---|
| `Parsed` | Understood, produced something | no |
| `Ignored` | Known type, deliberately skipped | **no** |
| `Barren` | Handled type, yielded nothing this line | no |
| `Unknown` | A `type` nobody has classified | **yes** |
| `Malformed` | Not JSON, or no `type` field | **yes** |

`blind_ratio` counts only the last two. Skipping `mode` lines 1,119 times is not
blindness, and a metric that says otherwise is one you learn to ignore.

`Barren` earns its own class because it is the early warning for a *shape*
change rather than a *type* change: if `assistant` lines keep arriving but stop
producing anything, the type is still recognised and the count still moves.

## Alerts are facts, not thresholds

There is no smoothing, no ratio tuning, no windowing. A never-before-seen event
type is interesting the first time it appears, and a threshold would only delay
saying so.

| Alert | Raised when | Urgent |
|---|---|---|
| `UnknownEventType` | any unclassified `type`, ever | yes |
| `MalformedLines` | any unreadable line | yes |
| `VersionChanged` | the *current* Claude Code version moves | yes |
| `HistorySkipped` | a transcript was over the size cap | no |
| `UnknownRunConfigType` | a `launch.json` / `tasks.json` `type` nobody classified (`R-N2`) | no |

`HistorySkipped` is deliberately not urgent. It is a stated limitation working
as designed, not a fault — and treating it as an emergency would train the user
to dismiss the whole panel.

**`UnknownRunConfigType` is not urgent either, and the reason is worth stating
because the two look alike.** An unclassified *transcript event* is urgent
because the line is **dropped** — mogeung is lying by omission. An unclassified
*run configuration* is **listed**, named and refused in the panel, so nothing is
hidden and nothing is wrong; it is a decision waiting to be taken. They are
counted separately in the tracker for the same reason, so that a pile of run
configurations can never make it look as though data is going missing.

Every alert carries a `message()` written for someone who has not read the
source. An alert that says `unknown_type: 3` is a log line; the panel needs a
sentence.

## Version tracking, and the bug it started as

`VersionChanged` compares the version behind the **newest transcript line seen**
against what that was before.

The first implementation recorded versions in encounter order and reported a
change between the last two. Run against a real 30-session corpus it announced:

> Claude Code changed from 2.1.209 to 2.1.210

while the machine was actually running **2.1.220**. Transcripts are scanned
newest-file-first and each carries whatever release wrote it, so encounter order
has nothing to do with time — the alert had picked two historical releases and
put them in the wrong order.

A confidently wrong alert is worse than no alert: it is exactly the "crying
wolf" failure this feature exists to prevent, committed by the feature itself.
Ordering now comes from each line's own `timestamp`. Pinned by
`old_sessions_scanned_after_new_ones_do_not_fake_a_downgrade`.

A session that genuinely spans an upgrade still reports one, because its own
lines cross the boundary in time order. That is a true positive.

## Size cap

`MAX_TRANSCRIPT_BYTES` = 4 MiB; oversized files are followed from a line
boundary within the last `TAIL_BYTES` = 1 MiB.

Starting at a **line boundary** matters more than it looks: a tail beginning
mid-line would hand the parser a fragment, which classifies as `Malformed` and
pollutes the very signal this subsystem provides. Pinned by
`a_large_file_is_followed_from_a_line_boundary`.

Skipping history is a real loss of information, so it is recorded per session
and surfaced. The previous code carried a comment promising "only if it is not
enormous" and performed no check at all — its one guard compared file age
against `HISTORY_DAYS`, which `scan_transcripts` has already filtered on, so it
could never fire.

## Where it surfaces

- **Top bar** — grey `health` button normally; amber `⚠ N unseen` when something
  is urgent. Always present, because the failure it reports is silent.
- **Health window** — alerts first, then counts, versions and limits.
- **`GET /api/health`** — the same data, curl-able. Answering "is the board
  empty because nothing is happening, or because mogeung went blind?" should not
  require a window.
- **`ServerMsg::Health`** — pushed after every scan, unsolicited. A client
  should never have to ask whether what it is showing is complete.

## What this does not do

It notices; it does not adapt. There is no attempt to guess the meaning of an
unrecognised type, and no fallback parser. When a format moves, a human decides
whether the new type matters and adds it to `HANDLED` or `KNOWN_IGNORED`.

Counters are in-memory and reset when the daemon restarts. Persisting them would
make "have I seen this type before?" survive restarts, which matters more once
the alert has been dismissed a few times — not yet built.

## Codex and rate limits (2026-07-29)

The canary now watches two corpora. Codex rollout lines classify through
the same outcome shape (`codex.rs` mirrors `LineOutcome`); unknown kinds
surface as `codex/<kind>` alerts and in `Health.codex_unknown`, replaced
wholesale each scan. Rollouts are read **incrementally** (2026-08-05:
`codex::ScanCache` tails appended bytes rather than re-reading every file
per pass), so the per-thread counts the replacement is built from are
cumulative in the cache — the merged totals equal what a full re-read
would have counted, without the full re-read. A present install with zero
threads is reported as exactly that: present, watched, empty.

## A third corpus, and a list instead of a slot (2026-08-25)

`R-I15` added Qwen Code, and the shape the Codex work left behind did not
survive it. `HealthTracker` held **one** `Option<(bool, u32, …)>` and `Health`
carried **four flat `codex_*` fields**, so a third CLI's canary had nowhere to
report — and a canary with nowhere to report is indistinguishable from a format
that has not drifted, which is the exact failure this whole file exists to
prevent.

So the tracker keys a `BTreeMap` by source name and `Health` carries
`agents: Vec<AgentHealth>` — `{source, present, threads, error, unknown}` per
CLI. Alerts are prefixed by that source, so `qwen/system/telepathy` and
`codex/thought` sit in one list and stay tellable apart. The four `codex_*`
fields are **still populated**: a snapshot is a wire type, dropping a field is a
break, and a client built before this change keeps working. They are marked
superseded, and `desktop/src/lib/agentHealth.ts` reads the list when it is
present and reconstructs a single Codex slot from the old fields when it is
not — falling back on *absent*, never on *empty*, because an empty list from a
new daemon is a real answer and reading the old fields over it would resurrect
a chip the daemon just said not to show.

Qwen's taxonomy descends one level, like Codex's but for a different reason:
its `system` records are roughly three lines in five and their real
discriminator is `subtype`, so an unclassified one is reported as
`system/<subtype>` and drift there is exactly as loud as drift at the top.
`--bin sweep` now walks three corpora and folds their unknown counts rather
than summing two hand-written terms — that sum would have been a silent zero
for the third, and so a clean bill of health for a format nobody swept.

**Nothing unobserved sits in `HANDLED`.** A guessed structured
rate-limit type did for a day — `R-G1` was written believing the CLI
emits one, and the arm was kept "in case a future CLI does" after the
sweep found zero across 235 transcripts (A20). That is backwards: a
handled type raises no alert, so pre-handling a shape nobody has seen
spends the canary on exactly the event it was built for. The arm is
gone, and the corpus line invented to exercise it with it; such a line
now classifies as `Unknown` and says so loudly. The real limit signal is
the synthetic assistant message, folded in `state.rs`.
