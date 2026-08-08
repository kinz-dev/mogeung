---
title: Cost per model
status: shipped
updated: 2026-08-08
roadmap: [R-J21]
depends_on: [A4, A31]
---

# 0033 — Cost per model

What today cost, what the history cost, and which model spent it.

## Spec

### Problem

Asked 2026-08-08: *"the most interesting question is how much do I spend today
and historical. Now we have a Token burn per day, but we don't have a actual $
equivalent. Different model costs differently. A Opus or Fables 5 model cost a
lot more than an older / smaller model."*

The ask contains its own justification, and a sweep of this machine's corpus
the same day confirms it. 54 transcripts, 9,018 assistant messages carrying
usage, **five models**:

| model | messages | output tokens |
|---|---:|---:|
| `claude-fable-5` | 4,930 | 5.49M |
| `claude-opus-5` | 3,705 | 3.92M |
| `claude-opus-4-8` | 306 | 58.6k |
| `claude-opus-4-7` | 38 | 32.1k |
| `claude-sonnet-5` | 35 | 12.4k |

Fable costs twice Opus 5 per token and ten times Haiku. **A token count cannot
answer "where is the money going" once tokens are not fungible** — and mogeung
was discarding the only field that could: `message.model` was read in exactly
one place, to recognise the `<synthetic>` limit message.

### Decision

[ADR-0024](../decisions/0024-equivalent-cost-in-dollars.md) supersedes
ADR-0005 and permits dollars **on the Analytics view only**, labelled
*equivalent API cost, not charged*, dated with the day the rates were read,
and with unpriced models named rather than silently costed at zero.

### The part that was not the multiplication

The interesting work is the token split, not the arithmetic on top of it.

`tokens_in` summed three buckets that are priced differently:

| bucket | price, relative to fresh input |
|---|---|
| fresh input | 1× |
| cache read | 0.1× |
| cache write, 5-minute tier | 1.25× |
| cache write, 1-hour tier | 2× |

This corpus reads **2.14 billion cached tokens against 627k fresh ones** — a
ratio of roughly 3,400 to 1. Pricing those reads as fresh input would have
reported the history at about **$12,250 instead of $2,602**: 4.7× high, and
high is the direction that gets believed. Measured 2026-08-08 by summing the
buckets across the corpus independently of the daemon, which is also how the
first draft of this doc came to quote a figure that was two orders out — the
earlier sweep counted *messages carrying* each field rather than the tokens in
them. `TokenSplit` separates all four; `tokens_in` keeps its old
meaning so nothing that already read it changed.

The two cache tiers come from `usage.cache_creation`, an object carrying
`ephemeral_5m_input_tokens` and `ephemeral_1h_input_tokens` beside the flat
`cache_creation_input_tokens`. Present on every usage-bearing message in the
sweep, and **degraded rather than trusted** (`A4`): the flat number is the
arbiter, and an object that does not account for it — a third tier, a rename —
leaves everything on the cheaper 5-minute tier. An under-count that shows up as
a wrong total beats a panic on a format nobody documents.

### Rates

`crates/mogeung-core/src/pricing.rs`, per million tokens, read 2026-08-08:

| model | input | output |
|---|---:|---:|
| `claude-fable-5`, `claude-mythos-5` | $10 | $50 |
| `claude-opus-5`, `claude-opus-4-8`, `claude-opus-4-7`, `claude-opus-4-6` | $5 | $25 |
| `claude-sonnet-5`, `claude-sonnet-4-6` | $3 | $15 |
| `claude-haiku-4-5` | $1 | $5 |

Three things this table does that a hard-coded constant would not:

- **Rates apply per day.** Sonnet 5 is $2/$10 introductory through
  2026-08-31 — in force *today* — and $3/$15 after. Because the fold already
  keys by local day, an expiring rate does not retroactively reprice the weeks
  it was actually in force.
- **Fast mode is a separate row**, priced at the Fable tier on Opus 5 and
  Opus 4.8. Nothing in this corpus uses it (`usage.speed` is `standard`
  everywhere), so the row earns its place by what it prevents: without it, the
  day someone runs fast mode is reported at half.
- **An unlisted model is unpriced, not free.** No rate is invented from a
  family resemblance. `cost_usd` is an option; `null` propagates to
  `unpriced_models`, and the client says which models are missing from the
  total.

### What it shows

- A **running counter** — today, and every day on record — with the caveat and
  the rates' date beside it, not behind a tooltip.
- A **per-model bar** under the counter, so the mix is readable at a glance.
- **Cost per day, by model**: one stacked bar per day, bands ranked over the
  whole period so a colour means one model as you read across. Past six models
  fold into `other` rather than cycling the palette, because two bands the same
  colour is a chart that lies.
- The token chart stays, retitled as what the cost is computed from.

### Out of scope

No budgets, no spend warnings, no cost column in the attention queue. The
queue's ranking is about who needs you, and ADR-0005's deletion of the
*"burning money with no diff"* attention reason stands — it was dropped for
denominating urgency in a misleading unit, and that argument survives.

## Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/pricing.rs` | New. The rate table, the cache multipliers, `RATES_AS_OF` |
| `crates/mogeung-core/src/usage.rs` | `TokenSplit`, `ModelBurn`; `DayBurn` and `UsageReport` carry cost |
| `crates/mogeungd/src/usage.rs` | Fold per (model, speed) and per day; split the cache buckets; price the rollups |
| `desktop/src/wire/types.ts` | The same shapes on the client |
| `desktop/src/lib/cost.ts` | The stacked series, as a pure function |
| `desktop/src/panes/InsightPane.tsx` | The counter, the per-model bar, the cost chart |

## Test strategy

Rust: the multipliers priced one bucket at a time (`pricing.rs`); the fold
attributing burn to the model that spent it; **a cached read costing a tenth
while still counting whole**, which is the regression the split exists to
prevent; the tier degrade; and unpriced-is-not-free asserted end to end.

TypeScript: `cost.test.ts` on the series — every key on every row so a quiet
day is a zero rather than a hole, bands ranked once over the period rather than
per day, the tail folding without losing its dollars, and an unpriced model
absent rather than drawn at zero. `InsightPane.test.tsx` keeps its money-off-a-
frequency-axis guard, now citing ADR-0024: dollars exist in this product, and
the rule is that they do not leak off Analytics.
