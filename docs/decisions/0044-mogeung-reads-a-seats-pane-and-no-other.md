---
title: mogeung reads a seat's pane, and no other
status: active
updated: 2026-10-09
decided: 2026-10-09
---

# ADR-0044 — mogeung reads a seat's pane, and no other

## Context

A pane is the tmux screen: what you see when you attach. The transcript
never carries what a TUI draws there — the permission menu and its three
choices, a plan waiting for approval, a multiple-choice question, the trust
dialog, a login prompt, *press Enter to continue*, a rate-limit banner, a
crashed CLI with a shell prompt where an agent used to be. mogeung infers
`APPROVE` from a tool call with no result ([attention-ranking.md](../design/attention-ranking.md))
and cannot tell those apart; the rest it cannot see at all.

That blindness has been named as a cost three times and never answered.
[ADR-0003](0003-observe-do-not-spawn.md)'s 2026-08-29 amendment: *"mogeung
cannot see the session's screen … an Enter delivered on a human's behalf can
land on a menu nobody read."* [ADR-0038](0038-a-pane-running-an-agent-is-a-session-provisionally.md):
an agent blocked on a trust prompt before it writes anything is invisible,
and the row it got carries *"only what the pane can prove"* — which, without
reading the pane, is a pid and a cwd. [ADR-0010](0010-attach-a-terminal-never-own-one.md)
renders a pane and does not read it: *owning the loop is forbidden;
rendering it is not*, and nothing on that page admits or forbids reading.

OpenRig reads the pane as a matter of course. Its readiness check captures
the pane command and forty lines and assesses them; its *visible chrome* rung
is how its *needs* lane knows a seat is on a permission menu rather than
thinking; its startup gate codes are the same shapes, named. The Gau Geung
epic ([feature 0051](../features/0051-gau-geung.md)) wants that lane, and on
2026-10-09 [ADR-0003's amendment](0003-observe-do-not-spawn.md#amendment--2026-10-09-mogeung-may-own-a-seat-it-started)
gave mogeung **seats** — panes it created itself — which changed the
question from *may mogeung read your terminal* to *may mogeung read its
own*.

Two scopes were put to the owner on 2026-10-09, with the difference spelled
out: seats only, or observed panes too behind an opt-in. The second closes
the cost ADR-0003 named for every session, and it means the daemon reads the
contents of your terminals — whatever the agent printed, secrets in tool
output included. The answer: *"seats only for now."*

Hooks are the precise rung where they exist, and they do not cover this. A
Claude Code seat launched under [ADR-0043](0043-a-seat-is-configured-at-launch-never-in-your-home.md)
gets `PermissionRequest`, `Notification`, `Stop` and the rest through
`--settings`, and an opencode seat's plugin sees `permission.asked`. But a
trust dialog, a login, a crash, a *press Enter* and a rate-limit banner
happen before or outside any hook; and a Codex or Qwen seat has no hooks at
all unless opted into a home of its own. The pane is the one rung every CLI
has.

## Decision

**The daemon may capture a seat's pane each scan and classify what it shows
into a closed set of shapes, honestly `unknown` otherwise. It never captures
a pane it did not create.**

1. **Seats only, by construction.** The capture takes a pane from the seat's
   launch record and from nowhere else: a type the launcher mints and nothing
   else can. A provisional row's pane, an attach target resolved for an
   observed session, a pane of your own — none of those is the type, so none
   can be passed. The scope is not a setting that could be flipped; widening
   it is an amendment here, and the Revisit section says what has to be true
   first.

2. **A closed set of shapes, per CLI, and `unknown` is a value.** The shapes
   are the gates a TUI draws and the transcript never carries: *permission
   menu*, *plan approval*, *question*, *trust*, *login*, *press Enter*,
   *rate limit*, *exited to a shell*, *at the prompt*, *working*. They are
   matched per CLI through the exhaustive `match`
   [ADR-0029](0029-an-agent-cli-is-a-variant-not-a-plugin.md) requires, so a
   fifth CLI has to say what its screens look like. A capture that matches
   none is `unknown`, published as that word, never as the nearest guess —
   the value `R-B4`'s `APPROVE`-versus-`WAITING` argument needed and never
   had. Every shape is pinned by a fixture capture in the tests, the way
   `tests/fixtures/corpus.jsonl` pins transcript lines, and a real screen
   that matches nothing is a decision for that corpus, not a chore.

3. **What travels is a shape and an excerpt. The capture is never kept.** The
   seat's row carries `screen: { shape, excerpt, read_at }`, where the
   excerpt is the few lines the shape was read from — the question's text,
   the menu's options — so the window shows what mogeung read rather than
   asking you to trust it. The capture itself lives in the scan's memory for
   the length of one classification and is not written to SQLite, not sent
   on the wire, not handed to the model (which reads what the row shows,
   [ADR-0031](0031-consent-to-a-named-host.md) clause 1), and never crosses
   a hop; the row's field crosses as any field of a seat row does, the day
   seat rows cross at all.

4. **A known shape may rank; `unknown` never does.** A seat on a *permission
   menu* or a *question* ranks `APPROVE`; at *trust* or *login*, `WAITING`
   with the gate named in the detail; *exited to a shell* ranks `FAILED`. The
   detail says *(screen)* so a reason read from a capture is distinguishable
   from a fact the registry told us, and **a hook's fact outranks a screen's
   reading** when both exist for the same seat. A screen never sets a tier
   for a session that is not a seat, because fence 1 means there is no screen
   for one.

5. **A shape may trigger exactly one press, and it is the one the shape asks
   for.** *Press Enter to continue* may, under the team's policy, send Enter
   — a declared send under ADR-0003's fence 3, logged and counted like any
   other. Nothing else is ever pressed from a reading. mogeung never answers
   a question the CLI asked: a permission menu is the CLI's `--permission-mode`
   business, settled at launch under ADR-0043, and a seat whose policy wants
   approvals skipped is launched in the mode that skips them rather than
   clicked through from the screen. *Trust* and *login* are yours
   ([ADR-0043](0043-a-seat-is-configured-at-launch-never-in-your-home.md)
   clause 6, [ADR-0038](0038-a-pane-running-an-agent-is-a-session-provisionally.md)
   clause 5): the row ranks as needing you, and the thing that unblocks it is
   you, typing in the pane.

6. **One capture per seat per rest, and an unchanged screen costs nothing.**
   `capture-pane` is a fork, and `R-J53`'s lesson is that a fork per session
   per tick is how the machine slowed down. Each seat rests between captures
   as it rests between diffs; a capture whose hash matches the last one is
   not reclassified; a capture that fails — the pane is gone — is `unknown`
   and a note on the row, not an error that stops the scan.

## Alternatives

**Observed panes too, behind an opt-in.** Put to the owner as the other
scope, and the more useful one: every session's row gets its true reason,
and the send confirmation can show what the screen shows before you press.
Deferred rather than rejected, on two grounds. It reads your terminals,
which is a different promise from reading mogeung's own, and should be made
when there is evidence to make it with. And the evidence is the same question
in both scopes — does the classifier read the shapes right — which seats
answer for free without touching a terminal that is yours. The Revisit
section is the door.

**Hooks instead of the pane.** The precise rung, and kept as the one that
outranks this (fence 4). Rejected as a *replacement* because it covers the
gates after a session is up and only for the CLIs that accept a hook at
launch; the trust dialog that started ADR-0038 is before any hook, and a Codex
seat has none.

**Read the pty rather than the pane.** The v0.1 shape. Rejected without
discussion: that is wrapping, and
[ADR-0003](0003-observe-do-not-spawn.md) records what it cost.

**Guess from the transcript harder.** A longer unmatched-tool list, timing
heuristics, the shape of the last assistant turn. Rejected because
`attention-ranking.md` is explicit that `WAITING` is *a fact we are told*,
and a heuristic that pretends to be one is the thing that document was
written against. A screen read is a heuristic too — fence 2's `unknown` and
fence 4's *(screen)* are what keep it from pretending.

**Press through every gate a shape names.** OpenRig's readiness Enter, trust
acceptance and login nudges. Rejected by fence 5: a daemon that answers a
permission menu from a screen read is auto-approval by the back door, and a
daemon that accepts trust on your behalf is the write ADR-0043 clause 6
refuses, one layer up.

## Consequences

**Easy.** The *needs you* lane has reasons rather than badges, and for every
CLI: a Codex seat on its trust prompt, a Qwen seat at a login, an opencode
seat at a question all say so. A seat that exits to a shell is `FAILED` the
tick it happens instead of `STALLED` five minutes later. A readiness press
exists, bounded to the one shape that asks for one. The excerpt on the row
means a misread is visible rather than silent.

**Hard.** A fifth undocumented corpus — screens — that four CLIs redraw
without warning, which is `A4` one surface over: a Claude Code release that
restyles its permission menu turns every seat `unknown` until the fixture is
updated, and the health panel has to say so the way it says a transcript
line went unclassified. A classifier is keyword matching over a terminal
grid, with `A3`'s honesty owed in the same words: it will misread, the
excerpt is the mitigation, and nothing here is a guarantee. One fork per seat
per rest is a cost the scan did not have.

**Ruled out, and said so.** Reading any pane the daemon did not create. The
one-type rule in fence 1 is the mechanism, and a test holds it: no call to
the capture is reachable with a pane that did not come from a launch record.

**What is being bet.** [A54](../product/assumptions.md): that the gate
shapes are few, stable enough across versions to be worth pinning, and
readable from the tail of a capture. `UNTESTED`, and the spec's phase 0
grows one more script because of it.

## Revisit if

- **The owner wants observed panes.** The amendment that admits them should
  not be written until a week of seat captures has been measured — the
  `unknown` rate, the misreads caught by the excerpt — and the shapes read
  right, because that week is the only evidence that reading *your* terminal
  would be worth what it reads. When it is written, the capture's handling
  (fence 3) carries over whole, and the opt-in is per machine and out loud.
- **`unknown` dominates in ordinary use.** Then the shape set is wrong, or
  the tail is too short, or a CLI redrew its chrome; the response is the
  fixture corpus, not a looser matcher.
- **A press from a shape lands wrong**, once. ADR-0003's condition, and it
  applies here with the same answer: fence 5 goes back to nothing rather
  than gaining a second check.
- **A CLI grows a sanctioned control channel** — the ADR-0010 condition. A
  socket that says *I am on a permission menu* makes this rung redundant for
  that CLI, and it should be removed for it rather than kept beside the
  better source.

---
*ADRs are immutable. A decision that is **narrowed** changes by an
`## Amendment — YYYY-MM-DD` section appended here, with `updated:` bumped. A
decision genuinely **reversed** is superseded: write a new ADR and set
`status: superseded` plus `superseded_by:` here.*
