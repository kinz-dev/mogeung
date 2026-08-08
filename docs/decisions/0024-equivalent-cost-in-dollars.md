---
title: Show equivalent API cost in dollars, in one place, labelled
status: active
updated: 2026-08-08
decided: 2026-08-08
supersedes: ADR-0005
---

# ADR-0024 — Show equivalent API cost in dollars, in one place, labelled

## Context

[ADR-0005](0005-tokens-not-dollars.md) said **tokens, never dollars**, on three
arguments. Two of them are still true and one has been overtaken.

Still true: on subscription auth the CLI's cost figure is *equivalent API cost*
rather than money charged, so presenting it as spend misleads. Still true: a
pricing table embedded in a program goes stale, and nothing in the program
knows when it has.

Overtaken: *"the genuinely scarce resource is the five-hour window"*. That was
written when one model did all the work. Asked 2026-08-08: *"how much do I
spend today and historical… a Opus or Fables 5 model cost a lot more than an
older / smaller model"* — and a sweep of this machine's own corpus the same day
says the question is real. 54 transcripts, 9,018 assistant messages, **five
models**: `claude-fable-5` (4,930 messages), `claude-opus-5` (3,705),
`claude-opus-4-8` (306), `claude-opus-4-7` (38), `claude-sonnet-5` (35). Fable
is priced at twice Opus 5 per token. A token count cannot answer *where is the
money going* once the tokens are not fungible, and mogeung was throwing the
model away: `message.model` was read in exactly one place, to recognise the
`<synthetic>` limit message.

The five-hour window remains the scarce resource for *finishing today's work*.
It is not the answer to *which model is expensive*, and ADR-0005 conflated
them.

## Decision

**Show equivalent API cost in dollars, per model and per day, on the Analytics
view — and nowhere else.** Three conditions, none of them optional:

1. **It is labelled *equivalent API cost, not charged*** wherever it appears.
   The auth on this machine is a subscription; no money moves per token.
2. **The rates carry the day they were read** (`RATES_AS_OF`), and every
   surface that shows a dollar shows that date.
3. **A model with no published rate is *unpriced*, never free.** Its tokens are
   counted and its name is listed; its dollars are absent from every total, and
   the client says which models are missing.

Everywhere else stays tokens: the status bar, the wall, the Info pane, and the
prompt/failure charts, which are counts and where a money axis would be three
lines of code away and would look reasonable in a screenshot.

## Alternatives

**Keep ADR-0005 unchanged.** Rejected because it answers a question nobody is
asking any more. The ask was specifically about the *mix* of models, and the
one thing tokens cannot express is that Fable costs 2× Opus 5 and 10× Haiku.

**Per-model tokens with no dollars.** The cheapest option and genuinely useful
— it was built first and is the substrate for everything here. Rejected *as the
whole answer*: it moves the arithmetic into the reader's head, and the reader
does not have the price sheet.

**Weighted "cost units" — Opus-equivalents rather than currency.** Attractive
because ratios drift more slowly than prices, and it keeps ADR-0005 intact in
spirit. Rejected because the unit is invented: nobody has an intuition for 4.8
Sonnet-equivalents, and the number would have to be explained every time it was
shown. A dollar is a unit people already read.

**Dollars only under API-key auth** — ADR-0005's own Revisit-if, honoured
literally. Rejected on evidence: `apiKeySource` appears nowhere in the corpus
as a field, so mogeung cannot tell which auth a session used. Implementing it
would mean a per-session setting maintained by hand, which is a promise the
daemon cannot keep (`ADR-0003`'s posture — never claim what you cannot see).

## Consequences

**The staleness ADR-0005 refused is now ours to carry.** The price table lives
in `crates/mogeung-core/src/pricing.rs` and will be wrong at some point after a
price change. Three things make that survivable rather than silent: the
as-of date ships with the numbers, an unlisted model is unpriced rather than
free, and rates are applied **per day** so a change does not retroactively
reprice history. It still needs a human to notice; nothing here checks
Anthropic's pricing page.

**The token fold had to be split four ways before any of this was possible.**
`tokens_in` summed fresh input, cache reads and cache writes, which have prices
in the ratio 1 : 0.1 : 1.25–2. This corpus reads **2.14 billion** cached
tokens against 627k fresh ones, so pricing reads as fresh input would have
reported the history at ~$12,250 rather than ~$2,602 — 4.7× high. That split
is the substantive change; the multiplication is trivial.

**A number that looks like money will be read as money by someone**, however it
is labelled — a screenshot outlives its caption. That is the cost of this
decision and it is accepted knowingly.

**The wire grew.** `UsageReport` carries a per-model breakdown per day, so a
60-day report is larger. Bounded by models × days and unnoticeable next to the
transcript payloads.

**What this does not do:** it does not track a budget, warn on spend, or rank
sessions by cost. The attention queue is unchanged and still ignores money —
ADR-0005's deletion of the *"burning money with no diff"* attention reason
stands, because the reason it was dropped was that it denominated *urgency* in
a misleading unit, and that argument survives this ADR intact.

## Revisit if

Anthropic publishes a cost figure per message in the transcript, at which point
measuring beats computing and the price table should be deleted rather than
maintained. Or if the pricing table is found stale in the wild — the failure
that argument predicts — in which case the answer is a smaller table, an
imported one, or a return to tokens.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
