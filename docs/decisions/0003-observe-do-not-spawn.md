---
title: Observe sessions, never spawn them
status: active
updated: 2026-08-29
decided: 2026-07-25
supersedes: The v0.1 spawning model
---

# ADR-0003 — Observe sessions, never spawn them

**This is the most important decision in the project.**

## Context

v0.1 spawned agents: the user typed an intent into mogeung, which ran
`claude -p` in a git worktree and presented the result. It was built, tested,
documented and committed.

On first real use the verdict was: *"a handicapped Claude Code with a single
session."*

## The failure, precisely

1. **The attention queue is worth zero at N=1.** A ranked list of one item is a
   label. The entire product only pays at three or four concurrent sessions.
2. **To feed that queue, v0.1 removed the interactive loop** — no steering, no
   permission prompts, no plan mode, no slash commands. Each individual session
   became *worse* than simply running `claude`.

Together: strictly worse than a terminal until N≥3, while making N≥3 awkward to
reach. The genuinely novel part was the review layer, and v0.1 gated it behind a
worse front-end for something that already has a good one.

The root cause was not a bad plan. It was an **unexamined assumption** — that
populating the queue required spawning — which was never written down and so was
never reviewable. See [assumptions.md A2](../product/assumptions.md).

## Decision

**mogeung observes. It never starts, steers or stops an agent.**

It reads what Claude Code already writes for itself:

- `~/.claude/sessions/<pid>.json` — live registry with first-party `busy`/`idle`
- `~/.claude/projects/<slug>/<id>.jsonl` — the transcript

The single exception: mogeung may open a **real interactive `claude` in a
terminal**, optionally in a fresh worktree. That wraps nothing and addresses the
"reaching N≥3 is awkward" half of the failure.

## Consequences

- Purely additive: it cannot degrade a session, because it does not touch one.
- **"Waiting for you" became a fact rather than an inference.** v0.1 could only
  detect blockage after the fact from permission denials; the live registry
  publishes it directly. The largest documented gap in v0.1 closed for free.
- New dependency on two undocumented file formats — now the top operational risk
  ([A4](../product/assumptions.md)). See
  [claude-code-formats.md](../design/claude-code-formats.md).
- Deleted: the supervisor, permission modes, model selection, follow-ups,
  cancel, the New Run dialog.
- Kept unchanged: the git diff engine, risk scoring, hunk anchoring, review
  checkpointing, the daemon/client split.
- A whole category became possible: the transcript corpus is queryable
  (roadmap section F).

## The durable lesson

Owning the conversation loop was never a *requirement* for the review and
attention layer. It was an assumption — and it was the expensive kind, because
it cost the entire product. [assumptions.md](../product/assumptions.md) exists
to catch the next one.

## Amendment — 2026-08-29: a human may press send

**One word above changes: *steers*.** It is replaced by a definition rather than
dropped, and everything else on this page — the failure, the finding, the first
exception, the durable lesson — stands exactly as written.

> **Steering is owning the loop**: being in the path between a human and an
> agent, or being in the path between an agent's output and its own next input.

Asked for the day `R-O7` made the drafted follow-up prompt worth wanting: *"can
I directly send it to the sessions associated, without manually copy to
clipboard and then paste it into the agent session manual step?"*, with the
argument attached — **it is still a human trigger; mogeung is not owning the
session.** That is right, and the refusal it overturns had started to rest on
*the boundary is the point* rather than on the failure the boundary was
protecting against. A fence that has forgotten what it encloses is how a rule
becomes a superstition.

Two things changed since this page was written, and they are why the request is
now buildable rather than merely arguable. The mechanism is no longer keystroke
injection: [ADR-0010](0010-attach-a-terminal-never-own-one.md) made mogeung
resolve **which tmux pane belongs to which session**, so delivery can name its
target rather than typing into whatever is focused — the thing
[ADR-0008](0008-build-the-prompt-never-send-it.md) rejected as *"a footgun with
no good failure mode"*. And the prompt is now composed in the window, so the
clipboard step had become a trip between two panes of one application.

### The second exception, and its fences

**mogeung may deliver text into one session's own tmux pane, on a click, after a
confirmation, and never any other way.** `R-B54`.

1. **A human clicks, and then confirms.** The confirmation names the session and
   shows the first line of what will be sent. Nothing is ever sent by a timer, a
   rule, a scan, an agent, or a second window.
2. **Exactly the text on screen, into exactly one session.** Flags spanning two
   sessions offer no send at all — a message with an ambiguous recipient is one
   the clipboard should carry.
3. **One shot, and no reading back.** mogeung does not read the agent's reply,
   decides nothing from it, and never sends a second message on its own. **There
   is no code path from an agent's output to an agent's input.** That path is
   what v0.1 was, and this amendment does not touch it.
4. **Loopback only, with no flag.** A daemon reachable beyond loopback refuses
   this outright, and takes no token either: that risk is somebody else typing
   into your agents, and a shared secret on a LAN is not a person at this
   machine.
5. **Only a session mogeung can name a pane for.** ADR-0010's boundary reused;
   a session started outside tmux keeps the clipboard, with the reason shown.
6. **The clipboard stays, unconditionally.** If sending were ever the *only*
   route, mogeung would have become the front-end by attrition rather than by
   decision.
7. **The draft is not sent by the gesture that made it.** Drafting and sending
   are two buttons with the text on screen in between. A single click that
   composed text with a model and delivered it to an agent is the thing this
   amendment is trying not to be.

### What was considered and lost

**Keeping the clipboard boundary**, which is what stood until this date —
rejected because the cost it charges is now paid inside one window, and its
argument had drifted from the failure it was protecting against.

**Pasting without pressing Enter.** The safer shape, and it was recommended when
this was asked: the commit would stay unambiguously with the human. Rejected
because it moves the step complained about rather than removing it; the
confirmation carries that weight instead.

**`osascript` / `TIOCSTI` keystroke injection**, both rejected by ADR-0008 and
ADR-0010 and not revived: neither can name its target, and `TIOCSTI` returns
`EPERM` off the controlling terminal and is disabled on Linux.

**A reply box — a real conversation.** Rejected, and this is where the line now
sits: a box you type into, that shows you the answer, that you type into again,
is the interactive loop, and mogeung would be a worse terminal in front of a
good one. Clause 3 is what keeps a convenience from arriving there one step at
a time.

### What it costs

**mogeung cannot see the session's screen.** Permission prompts, multiple-choice
questions and plan-mode approval are TUI rendering and never reach the
transcript ([feature 0003](../features/0003-attached-terminal.md) is founded on
that fact). So an Enter delivered on a human's behalf can land on a menu nobody
read. The confirmation is the mitigation and not a guarantee — it says *what* is
being sent and *where*, and it cannot say what the screen is showing. That
sentence is in the dialog, not only here.

**The pressure moves up one step.** The next requests are predictable: *send
without confirming*, *send when I flag*, *show me the reply here*. Clause 3 is
the one to defend.

**Revisit if** the confirmation is being clicked through without being read —
which is what a dialog becomes when it is frequent — or if a send ever lands
somewhere it was not aimed. Either means the fence is decoration, and the honest
response is to go back to the clipboard rather than to add a second dialog. Or
if Claude Code grows a documented, sanctioned way to enqueue a prompt: this
should then be rebuilt on it, and the hazard above goes with the terminal.

[A39](../product/assumptions.md) is what is actually being bet — not the
mechanism, which is proven and tested, but that a button stays a button — and it
carries the removal condition.

---
*ADRs are immutable. A decision that is **narrowed** changes by an
`## Amendment — YYYY-MM-DD` section appended here, with `updated:` bumped. A
decision genuinely **reversed** is superseded: write a new ADR and set
`status: superseded` plus `superseded_by:` here.*
