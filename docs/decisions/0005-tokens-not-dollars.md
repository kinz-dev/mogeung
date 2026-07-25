---
title: Report tokens, not dollars
status: active
updated: 2026-07-25
decided: 2026-07-25
---

# ADR-0005 — Report tokens, not dollars

## Context

v0.1 displayed `total_cost_usd` from the CLI and reported session spend in
dollars.

## Decision

**Show token counts. Do not show dollar amounts.**

## Rationale

Auth on this machine is OAuth subscription (`apiKeySource: none`, five-hour rate
limit window). The CLI's `total_cost_usd` is *equivalent API cost* — what the
usage would have cost at API rates — not money charged. Displaying it as spend
is misleading.

Embedding a pricing table to compute real cost would go stale and is not the
product's job.

The genuinely scarce resource is the **five-hour window**, and with
`overageStatus: rejected` (`org_level_disabled`), exhausting it causes sessions
to *fail* rather than spill into paid overage. That is the number worth showing.

## Consequences

- Cost columns removed; token counts shown instead.
- The v0.1 "burning money with no diff" attention reason was dropped — it was
  denominated in a misleading unit.
- Surfacing the five-hour window is roadmap `G1`, and matters more than any
  dollar figure would have.
- Revisit if API-key auth is ever used, where dollars would be real.
