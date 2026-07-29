---
title: Health and the format canary
status: active
updated: 2026-07-29
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

`HistorySkipped` is deliberately not urgent. It is a stated limitation working
as designed, not a fault — and treating it as an emergency would train the user
to dismiss the whole panel.

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
wholesale each scan because rollouts are re-read per pass — accumulation
would be the skipped-history trap again. `codex_present` with zero
threads is reported as exactly that: present, watched, empty.

`rate_limit_event` sits in `HANDLED` as a capture-shape arm although no
real transcript has ever carried one (A20): if a future CLI emits it,
the first specimen is recorded as a notice instead of raising an
unknown-type alert. The real limit signal is the synthetic assistant
message, folded in `state.rs`.
