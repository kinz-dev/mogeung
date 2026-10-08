---
title: A timer may read a public page, on consent that names its host, and send nothing
status: active
updated: 2026-10-08
decided: 2026-10-08
---

# ADR-0042 — A timer may read a public page, on consent that names its host, and send nothing

## Context

Every outbound connection mogeung makes has had a person behind it at the
moment it happened. [ADR-0014](0014-fetch-is-not-publishing.md) put the first
one, `git fetch`, behind a keystroke, and said so: *"this is the first time
mogeung makes an outbound network connection at all."* The model seam
([ADR-0031](0031-consent-to-a-named-host.md)) asks only when somebody asks it
something. The update check (feature 0049) reads a git ref on this machine.

[ADR-0024](0024-equivalent-cost-in-dollars.md) took on a price table typed in
by hand and said what that cost: *"It still needs a human to notice; nothing
here checks Anthropic's pricing page."* On 2026-10-08 the human noticed seven
weeks late. Five models in use had no price, and 46% of the history's
equivalent cost was missing (`R-J123`). The ask was for something that looks
by itself, *"every like 1 week"* (feature 0050).

Looking by itself means a timer, so it is a call with nobody behind it at the
moment it is made. That is a new kind of call, and it gets a decision of its
own rather than riding on ADR-0014's.

## Decision

**A timer in the daemon may read a public page, under four conditions:**

1. **Consent names the host, in the config file.** `refresh_pricing =
   "platform.claude.com"`, ADR-0031's named-host shape. Absent is no, and it
   is the default. A value naming any other host is refused and the refusal
   is shown, not reached. There is no blanket form and no flag: a standing
   timer is a standing preference, which is what the file is for.
2. **It reads; it sends nothing about this machine.** GETs of fixed public
   URLs, written in the source, over HTTPS only and never downgraded by a
   redirect, with no cookies, tokens or identifying headers. No transcript,
   session, path or count leaves in a request, because the request has no
   field to carry one.
3. **It runs rarely, and a failure is not a loop.** At most one reading a
   week, and at least six hours between any two attempts. The record of
   attempts survives a restart, so restarting cannot buy an early read.
4. **What it read is evidence, not authority, until shown otherwise.** A
   reading is validated against invariants the page states itself, rejected
   whole when it breaks one, and reported. Whether it may also **change** what
   mogeung shows is a separate step, taken by amendment here once the
   assumptions under it (A49, A50) have evidence.

The one use today is the pricing page and the deprecations page beside it
(feature 0050), and the reading is compare-only.

## Alternatives

**A scheduled agent run that edits the price table and commits it.** No timer
in the product, and every price reviewed in git, which is a real advantage.
Rejected as the product's answer: it costs an agent run a week, needs the
repository reachable from wherever it runs, and helps only installs built from
this checkout. It remains a reasonable thing for one person to set up for
themselves.

**Consent by flag, as `--allow-run` is.** Rejected for ADR-0031's reason: the
window hosts its own daemon and has no argv to pass a flag through, so
flag-only would mean never, on the shape mogeung is normally run in.

**On by default, since it sends nothing.** Rejected. The request still says
that this machine runs mogeung, from this address, weekly. That is small, but
it is a fact about the user leaving the machine without a decision. The line
ADR-0014 drew, *"mogeung is entirely local" stops being true as a blanket
claim*, is not one to move further on the product's behalf.

**The Models API.** It needs an API key, which a subscription user does not
have, and it carries no prices.

## Consequences

**Easy:** a model released after a build is noticed within a week, rather
than when someone thinks a chart looks wrong. The notice that needs no network
(an unpriced model in use is an alert) ships with this and catches the same
gap on any machine.

**Hard:** mogeung now has a background network call, so *"nothing happens
until you do something"* is no longer true of the daemon. It is true by
default, and the Health window says when it is not: which host, when last
read, and what it found. A page written for people is a format nobody promised
to keep. The reader degrades to *could not read it* and keeps what it had, as
the transcript parsers do (`A4`), but every redesign of that page will cost a
reader change.

**Ruled out:** a timer that sends anything; a timer reaching a host the user
did not name; and any reading changing a number on screen before this ADR is
amended to allow it.

## Revisit if

A second page wants reading on a timer. The conditions above should hold for
it unchanged, and if they do not, the difference is the decision. Also revisit
if the pricing page publishes a machine-readable price list: reading a table
written for people is the weak part of this, and a published list would
remove it.

## Amendment — 2026-10-08

**One premise overstated.** "Every outbound connection mogeung makes has had a person behind it at the moment it happened" — the LAN beacon ([ADR-0039](0039-the-daemon-announces-itself-by-default.md)) has been sent every 15 s with nobody behind it since 2026-09-22. The premise holds for connections *beyond the LAN*, which is what this ADR is about; the beacon discloses presence to the segment and reads nothing.
