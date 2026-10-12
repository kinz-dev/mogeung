---
title: Health and the format canary
status: active
updated: 2026-10-10
covers:
  - crates/mogeung-core/src/health.rs
  - crates/mogeungd/src/health.rs
  - crates/mogeung-core/src/pricing.rs
  - crates/mogeungd/src/pricing_refresh.rs
  - crates/mogeung-core/src/model.rs
  - crates/mogeung-core/src/llmproxy.rs
  - crates/mogeungd/src/codex.rs
  - crates/mogeungd/src/qwen.rs
  - crates/mogeungd/src/opencode.rs
  - desktop/src/lib/agentHealth.ts
---

# Health and the format canary

Everything mogeung knows comes from four CLIs' undocumented formats — three of files, one a database
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
| `UnpricedModel` | an Anthropic model in use that the price table has no rate for (`R-J124`) | yes |
| `PricesDiffer` | a weekly reading of the pricing page disagreed with the built-in table (`R-J124`) | yes |

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

- **Status bar** — *reading everything it recognises* normally; amber *N
  thing(s) it cannot see* when any alert is raised, with `Alt+h` opening the
  window. The window counts every alert, urgent or not, where the core's
  `Health::headline()` counts only urgent ones — a gap filed as `R-J127`.
  Always present, because the failure it reports is silent.
- **Health window** — alerts first, then counts, versions and limits.
- **`GET /api/health`** — the same data, curl-able. Answering "is the board
  empty because nothing is happening, or because mogeung went blind?" should not
  require a window.
- **`ServerMsg::Health`** — pushed when it changed and on a 30 s heartbeat
  (`R-J55`), unsolicited. A client should never have to ask whether what it is
  showing is complete.

## The model row (`R-O1`, 2026-08-28)

`Health.model` carries what the daemon was configured to talk to and whether it
may: `configured`, `host`, `model`, `remote`, `allowed`, `chat_allowed`, a
`refusal` sentence, and the residue of the last ask (`last_error`,
`last_ok_ms`).

**It is never a probe.** ADR-0031 clause 6 (carried forward from ADR-0030) keeps model calls off the scan tick,
so nothing here reaches the endpoint to find out whether it is up —
`last_error` is what an ask somebody made actually did. *Reachable* is
deliberately not a field: it would be a claim about a moment that has already
passed by the time it is rendered, which is the failure this whole document is
written against.

**A refusal is not an error.** Nothing configured is the ordinary state of a
fresh install; an endpoint elsewhere that consent does not cover is a decision
nobody made. Both fill `refusal` and neither touches `last_error`, because a
health row blaming an endpoint that was never asked is the kind of wrong that
costs an afternoon.

There are **two** refusals for the second case and they are not the same
sentence ([ADR-0031](../decisions/0031-consent-to-a-named-host.md)): *you never
said*, and *you said, about somewhere else*. Consent names a host, so pointing
`model_url` at a different machine produces the second — which has to name both
hosts, or the reader is left comparing a config file against a message that
only quotes half of it.

**The proxy row (`R-O10`).** With mogeung's own llmproxy in front, `host` is
`127.0.0.1` and says nothing useful about where prompts go. `proxy.forwards_to`
is the field that does: every non-loopback host in the proxy's config, read
from the **file** rather than asked of the process, so a failed start still
reports where it would have sent things. A sentence about the bytes that only
appears when everything is working is the wrong way round. See
[ADR-0033](../decisions/0033-a-proxy-of-our-own.md) clause 6 for why this is a
disclosure and not a gate.

**The host, never the URL.** A URL can carry a key in a query string, and this
row is rendered in a window and pasted into bug reports.

It is folded in by `AppState::health` rather than tracked by `HealthTracker`:
the tracker counts what the parsers saw, and this is a configuration plus a
residue. Reading it costs nothing.

## The price table's canary (`R-J124`, 2026-10-08)

**`UnpricedModel` is the format canary for prices.** A model released after the
price table was typed in is counted in tokens and is missing from every dollar
total. Nothing is wrong with the transcripts, but the Analytics view is wrong
by omission, which is the same kind of failure the event canary exists for. On
2026-10-08 it was five models and 46% of the history's equivalent cost, seen
only as one line under the Analytics summary, for seven weeks.

It is folded in by `AppState::health` from the last usage report, as the model
row is, because the tracker counts parser lines and this is a fact about a
table. Two things keep it answerable:

- **Only Anthropic's models** (`pricing::unpriced_is_a_gap`). A non-Claude
  model reached through Claude Code has no Anthropic price, and an alert that
  can never be answered teaches people to ignore the panel. Models known to be
  unpublished (`KNOWN_UNPRICED`: `claude-mythos-preview`) are left out for the
  same reason.
- **It does not need Analytics to be opened.** The pricing task runs the usage
  report every six hours as well as on request. The report is incremental, so
  each run after the first reads only what was appended.

Health is republished when the **set** of unpriced models changes, not on
every report. Analytics asks for a report each time it opens, and a broadcast
per open would be the ungated publishing `R-J55` removed.

**`Health.pricing` is the price table's row**: the day it was read
(`built_in_as_of`), and the weekly reading of Anthropic's page if one is
consented to ([ADR-0042](../decisions/0042-a-timer-may-read-a-public-page.md)).
The row holds `host`, a `refusal` when consent named the wrong host, the last
attempt and success, the models read, the `differences`, and the names that
mapped to no id. Like the model row it is **never a probe**. The fields are the
residue of readings that happened, on the task's own schedule, and the record
is kept in `pricing/state.json` beside the database so a restart reports the
last outcome rather than *not read yet*. `PricesDiffer` is raised from
`differences`. The reading is compare-only, so the alert means *the build's
table is out of date*, not *a price changed on screen*.

## What this does not do

It notices; it does not adapt. There is no attempt to guess the meaning of an
unrecognised type, and no fallback parser. When a format moves, a human decides
whether the new type matters and adds it to `HANDLED` or `KNOWN_IGNORED`.

Counters are in-memory and reset when the daemon restarts. Persisting them would
make "have I seen this type before?" survive restarts, which matters more once
the alert has been dismissed a few times — not yet built.

## Four corpora, one tracker

The canary watches every CLI the daemon reads, and each reader reports
through the same outcome shape (`codex.rs`, `qwen.rs` and `opencode.rs` mirror
`LineOutcome`). The tracker keys a `BTreeMap` by source name and `Health`
carries `agents: Vec<AgentHealth>` — `{source, present, threads, error,
unknown, trusted_dirs}` per CLI — because a canary with nowhere to report is
indistinguishable from a format that has not drifted, which is the exact
failure this file exists to prevent. Alerts are prefixed by source, so
`qwen/system/telepathy`, `codex/thought` and `opencode/part/telepathy` sit in
one list and stay tellable apart; `unknown` is replaced wholesale each scan. A
present install with zero threads is reported as exactly that: present,
watched, empty. `--bin sweep` walks all four corpora and folds their unknown
counts rather than summing hand-written terms — a sum would be a silent zero
for a corpus nobody swept, and so a clean bill of health for a format nobody
checked.

| Source | Where drift hides | Reported as |
|---|---|---|
| Claude Code | `type`, and the shapes beneath it that the sweep inventories: models, usage keys, content blocks | `UnknownEventType`, the bare type |
| Codex | three levels — `item_completed` wraps an `item.type`, so drift can hide one layer deeper than `kind/item` | `event_msg/item_completed/<Type>`; `KNOWN_COMPLETED_ITEMS` holds only what has been observed, so a tool call or a file change announces itself the first time one happens |
| Qwen Code | `system` records are three lines in five and discriminate on `subtype` | `system/<subtype>` |
| opencode | rows of `opencode.db`, classified by row id so a re-read at the watermark does not count one twice | `part/<type>`; `tool/<status>` outside `pending`/`running`/`completed`/`error`; `message/<role>` |
| a seat's hooks (`R-P5`) | the event names a CLI posts to the daemon's receiver — twenty-six for Claude Code, a bus for opencode — classified against `CLAUDE_HANDLED` / `OPENCODE_HANDLED` | `hook:<name>`, the bare event name, prefixed so it can never be mistaken for a transcript type |
| a seat's pane (`R-P11`) | the screen a TUI draws, read into [ADR-0044](../decisions/0044-mogeung-reads-a-seats-pane-and-no-other.md)'s closed set of shapes by keyword; a release that restyles a menu turns a shape `unknown` | `screen:<cli>`, once per changed capture that matched nothing — a pane that could not be captured is not drift and is not counted |

**Codex.** Rollouts are read incrementally — `codex::ScanCache` tails appended
bytes rather than re-reading every file per pass — so the per-thread counts the
replacement is built from are cumulative in the cache, and the merged totals
equal what a full re-read would have counted. Two streams carry the same
words: `response_item/message` replays the model-facing transcript (system
prompt, skills block, an `<environment_context>` blob under `role: "user"`),
and turns and text are read from `item_completed` alone; reading both would
count every turn twice and show a system preamble as the last thing you typed.
`trusted_dirs` (`R-J74`) sits on the slot because it is *what mogeung knows
about that CLI's install*, the same kind of thing as `present`: Codex opens no
thread in a directory until you have answered its trust prompt, so a headless
launch there stops on a prompt no window can show, and carrying the list lets
the New session window say so before you click. Read from
`~/.codex/config.toml`, never written — answering for you would be a launcher
quietly widening what an agent may touch — and empty for a CLI with no such
notion, so a window that gets no list warns about nothing rather than guessing.

**opencode.** A database can drift in a way a file cannot: its **layout**. The
`session` table is read with `SELECT *` and mapped by name, as Codex's index
is, and a column the reader depends on going missing is put in the slot's
`error` — *"opencode.db's session table has no directory — its layout has
changed"* — rather than silently producing no sessions. The sweep walks the
database too, read-only, with the scan's own `classify_part` and
`classify_message`, and inventories `finish` values, error names and tool
names beneath them.

**The four `codex_*` fields are still populated**, marked superseded: a
snapshot is a wire type, dropping a field is a break, and a client built before
the list keeps working. `desktop/src/lib/agentHealth.ts` reads the list when it
is present and reconstructs a single Codex slot from the old fields only when
the list is *absent*, never when it is empty — an empty list from a new daemon
is a real answer, and reading the old fields over it would resurrect a chip
the daemon just said not to show.

**Nothing unobserved sits in `HANDLED`.** A handled type raises no alert, so
pre-handling a shape nobody has seen spends the canary on exactly the event it
was built for. A guessed structured rate-limit type did for a day — `R-G1` was
written believing the CLI emits one, and the sweep found zero across 235
transcripts (A20). The arm is gone, and the corpus line invented to exercise
it with it; such a line now classifies as `Unknown` and says so loudly. The
real limit signal is the synthetic assistant message, folded in `state.rs`.

How each corpus arrived — Codex `0.149.1` renaming its turn boundary and a
read rollout falling to two understood shapes of eight (`R-J70`), the single
`Option` slot a third CLI outgrew (`R-I15`), the database the fourth brought
(`R-I25`) — is in those roadmap rows.
