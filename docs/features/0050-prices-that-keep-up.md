---
title: Prices that keep up
status: draft
updated: 2026-10-08
roadmap: R-J124
depends_on: [A4, A49, A50]
---

# 0050 — Prices that keep up

## Spec

*Owned by the human. What and why, never how.*

### Problem

Asked 2026-10-08: *"there are a lot of new model being released, but the
analytics chart still show old model … Plan for a change that can auto poll for
update or trigger this scanning every like 1 week (that doesn't have to be very
update)"*.

The price table in `pricing.rs` is typed in by hand ([ADR-0024]), and on
2026-10-08 it was two months old. In that time Anthropic released Opus 5.5,
Sonnet 5.5, Haiku 5.5 and Fable 5.1, and this machine moved most of its work
onto them. The table priced none of them, so **46% of the history's equivalent
cost was missing**: $7,265 shown against $13,469 once `R-J123` refreshed it.
Today's cost read *−$0.00*, because everything run that day was on a model the
table did not know.

The design worked as written. Unpriced models were named under the Analytics
summary, and nothing was shown as free. But the only cure was a person reading
Anthropic's pricing page, and ADR-0024 says so: *"It still needs a human to
notice; nothing here checks Anthropic's pricing page."* The human noticed seven
weeks late, from a chart that looked wrong.

### Assumptions

- **A4** `SUPPORTED` — formats mogeung reads are undocumented and change
  without warning. The pricing page is a document written for people, and the
  parser has to be built the way the transcript parsers are: degrade, never
  panic, and say so.
- **A49** `UNTESTED` — the pricing page stays readable by a program **week
  after week**: the `.md` address keeps serving markdown, and the *Model
  pricing* table keeps its six columns in their order. One reading supports it
  (below); a month of readings would settle it.
- **A50** `UNTESTED` — a model's name on the pricing page gives its API id by
  rule (`Claude Opus 5.5` → `claude-opus-5-5`), checked against the ids
  published on the model-deprecations page. One reading: 20 of 21 rows mapped,
  and the one that did not, Haiku 3.5 (`claude-3-5-haiku-20241022`), uses the
  old naming and is retired.

> A49 and A50 are `UNTESTED`, so **phase 0 is the work**: a check that reads
> and compares, and changes no price, until the readings say the page can be
> trusted.

### Acceptance

- [ ] A model released after the last build is priced in the Analytics view
      within a week of its first use, with no rebuild and no hand edit.
- [ ] The summary says where the rates came from and when: *rates as of
      2026-10-15, read from platform.claude.com*, or *built in, 2026-10-08*.
- [ ] Nothing is fetched unless it has been switched on, and the switch names
      the host it reaches.
- [ ] A page that has changed shape changes no price. The Health window says
      the page could not be read, and why, and the last good rates stay in
      force.
- [ ] A price change applies from the day it was read, not to the history
      before it. The exception: a model's **first** price covers the days it
      was used before anyone read one, since that was its price from launch.
- [ ] With the refresh off, a model in use that has no price raises a Health
      notice the day it first appears, not seven weeks later.

### Explicitly out of scope

- **Footnotes.** Introductory prices, the 1.1× US-only multiplier and the
  Batch discount are in prose, not in the table. A price that changes shows up
  at the next reading, up to a week late, which the ask allowed. US-only
  inference is `not_available` on all 67,543 messages here, and Batch is never
  used by Claude Code.
- **Codex, Gemini, Qwen and any model on someone else's price list.**
  Unpriced, as now.
- **Dollars anywhere but Analytics.** ADR-0024 is unchanged on that.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

**Two halves, the cheap one first.**

**1. The notice, no network.** The daemon already knows which models in use
have no price (`unpriced_models`). Raise it where the format canary raises an
unclassified event type: in the status bar's *things it cannot see* and the
Health window, naming the model and its message count. An unpriced model is
the pricing twin of an unknown transcript line. This alone would have caught
the 2026-10-08 gap in August. It ships whatever is decided about the network.

**2. The weekly reading, opt-in, read-only, named host.**

- **Consent** in `~/.mogeung/config.toml`, the ADR-0031 pattern:
  `refresh_pricing = "platform.claude.com"`. Absent means no fetch. The Config
  window gets a checkbox that writes it, worded with the host. Being a timer
  rather than a keystroke, this is a new kind of outbound call
  ([ADR-0014] made the first, and it was behind a key), so it gets **ADR-0042**
  and a dated amendment to ADR-0024's *nothing checks the pricing page*.
- **Schedule.** Checked at daemon start and every six hours; a fetch happens
  only when the last **successful** reading is seven days old. A failure
  retries at the next check, not in a loop. The last attempt and its outcome
  are kept, so a restart does not refetch.
- **Fetch.** Two GETs: `…/about-claude/pricing.md` and
  `…/about-claude/model-deprecations.md`. They go through `curl` as
  `model.rs` already does, so no HTTP client is added. 15 s timeout, no
  cookies, and nothing about the machine or its sessions in the request.
- **Parse** the *Model pricing* table, the *Fast mode pricing* table, and the
  `(for prompts over N tokens)` qualifier into the shape `pricing.rs` already
  has. The cache-read multiple is derived from the page's own columns
  (hit ÷ input), not read from a footnote. Names become ids by A50's rule, and
  a name whose id is not on the deprecations page is dropped and listed rather
  than guessed.
- **Validate before trusting.** The page states its own invariants: a
  five-minute write is 1.25× input and an hour write is 2×, on every row. A
  reading where any row breaks that, or that finds fewer than ten models, or
  that lost a column, is **rejected whole**. The page has changed meaning or
  shape, and half a table is worse than an old one.
- **Store** each accepted reading as a dated snapshot,
  `~/.mogeung/pricing/2026-10-15.json`, append-only, never under `~/.claude`.
  `pricing::rate` asks the snapshots first: the newest one dated on or before
  the day being priced. For a model no earlier snapshot knew, it takes the
  model's first reading; failing all of those, the built-in table. That keeps
  ADR-0024's *rates carry the day* with readings rather than code as the
  record.
- **Wire.** `UsageReport` gains `rates_source` (`"built-in"` or the host) next
  to `rates_as_of`, which becomes the date of the newest reading in force. The
  Health report gains the last reading's outcome. Both are public
  `mogeung-core` types, so `cargo check --manifest-path
  desktop/src-tauri/Cargo.toml` is on the checklist.

**Phase 0, before any of that applies a price:** ship the reading as
**compare-only**. It fetches, parses, validates, and reports in Health whether
the page still agrees with the built-in table, rows added or changed, but
changes nothing. Four weekly readings in agreement settle A49 and A50, and the
apply step is then a small change behind them.

### Files touched

- `crates/mogeung-core/src/pricing.rs` — a `Snapshot` type; `rate` takes the
  snapshots; the built-in table becomes the floor.
- `crates/mogeungd/src/pricing_refresh.rs` (new) — schedule, fetch, parse,
  validate, store.
- `crates/mogeungd/src/usage.rs`, `server.rs`, the config loader, the health
  report.
- `desktop/src/ui/ConfigWindow.tsx`, `HealthWindow.tsx`, `StatusBar.tsx`,
  `desktop/src/panes/InsightPane.tsx`, `desktop/src/wire/types.ts`.
- `docs/decisions/0042-…` (new), an amendment to ADR-0024, `wire-protocol.md`,
  `architecture.md`.

### Risks and unknowns

- **The page is for people.** A redesign could keep the numbers and move the
  table, and the parser would reject it. The design accepts that: rejecting is
  safe, and the notice in half 1 still fires.
- **A wrong mapping is worse than none.** `Claude Opus 5.5` could one day be
  served under an id the rule does not produce. Checking against the
  deprecations page bounds this; a dropped row is listed, not silent.
- **A price that drops** (Opus 5.5 is $4 against Opus 5's $5) is as much a
  change as one that rises. Snapshots by day handle both the same way.
- **Offline machines** never read. They keep the built-in table and get the
  notice, which is today's behaviour plus the notice.

### Test strategy

- The parser against `tests/fixtures/pricing-2026-10-08.md`, today's page,
  must produce **exactly** the built-in table. This keeps both honest: the
  next hand edit to `pricing.rs` that the page does not agree with fails a
  test, and so does a parser change that misreads it.
- Validation rejects the same fixture with two columns swapped, with a row's
  write price changed, and with the table removed.
- Snapshot selection: a price change on the 15th leaves the 14th alone; a model
  first read on the 15th prices the 10th.
- No consent, no `curl` — the spawn is a seam and the test counts calls.
- Half 1: a report with an unpriced model in use raises the Health notice.

**Alternative, not recommended as the product but possible today:** a weekly
scheduled Claude Code routine that repeats `R-J123` by hand. It would read
the page, edit `pricing.rs`, run the tests and commit, and the installed window
would offer the update through `R-J105`. No product code, and every price is
reviewed in git; but it costs an agent run a week, needs the repository
reachable from the cloud, and helps only installs built from this repository.

## Notes

*Filled during implementation. Surprises, dead ends, things the plan got wrong.*
*This section is the most valuable one later — write it honestly.*

**The probe behind A49 and A50**, 2026-10-08, before this spec. The `.md`
address answered `200 text/markdown` (49 KB, against 943 KB of HTML).
Twenty-one rows were parsed from *Model pricing*. Every input, output and
cache-read multiple agreed with the table `R-J123` had typed in by hand that
morning, and 20 of the 21 names mapped to a published id. One reading, so
`UNTESTED` still.

[ADR-0024]: ../decisions/0024-equivalent-cost-in-dollars.md
[ADR-0014]: ../decisions/0014-fetch-is-not-publishing.md
