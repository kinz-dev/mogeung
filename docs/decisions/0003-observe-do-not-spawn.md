---
title: Observe sessions, never spawn them
status: active
updated: 2026-10-09
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
transcript ([feature 0003](../archive/features/0003-attached-terminal.md) is founded on
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

## Amendment — 2026-09-23

**The 2026-08-29 amendment's clause 4 is decided per connection.** *"Loopback
only, with no flag"* meant, in its own words, *a person at this machine* — and
it was read from the daemon's bind, which cannot tell a person at this machine
from a stranger once the daemon listens on the LAN. From 2026-09-23 a client
arriving over loopback may send to a session whatever the bind; every other
client is refused exactly as before, still with **no token and no flag**,
because a shared secret on a LAN is still not a person at this machine.

The reason is `R-J104`: `R-I17`'s step 7 lets the daemon a window hosts bind
the LAN from `config.toml`, and the window on that same machine was refused
its own *send*. A loopback client is a person at this machine or at the far
end of an `ssh -L` into it — the route the refusal has always recommended.
Clauses 1–3 and 5–6 are untouched: one shot, never a loop, only a session
mogeung can name a pane for, and the clipboard stays.

## Amendment — 2026-09-23, later

**A send that crosses a hop is decided by the daemon that owns the session,
exactly as it decides a window's.** Federation (`R-I23`,
[ADR-0013](0013-one-window-one-daemon.md)'s second amendment of this date)
forwards `SendToSession` for a mirrored session up a relay socket to the peer,
and the peer applies clause 4 to that relay as to any client: over a LAN it is
not a person at that machine, and the send is refused in the peer's words;
through an `ssh -L` into the peer it arrives on loopback, which is the route
this clause has always named as a person's. Nothing here opens a path around
the clause — no token, no flag, and no *allow changes* consent can make a LAN
relay count as loopback. The consent is a second gate on this machine, never a
substitute for the one on that machine. Clauses 1–3 and 5–6 are untouched.

## Amendment — 2026-10-09: mogeung may own a seat it started

**The decision is narrowed from *an agent* to *an agent mogeung did not start
itself*.** A session mogeung launches in Gau Geung mode is a **seat**, and a
seat is owned: it may be sent to, woken, watched, restarted and stopped. Every
session mogeung merely finds — which is every session this page has ever been
about — stays observed, and everything above stands for it word for word,
including both exceptions and all seven clauses of 2026-08-29.

Asked on 2026-10-09, when the Gau Geung epic set out to port OpenRig's
orchestration — send, broadcast, wakes, watchdogs, a team of agents in named
panes — and the list of what this page refuses was read back one item at a
time. The owner's argument for relaxing it: *"when the mogeung product evolve
to agent harness + orchestration, it has to be able to spawn agent, and own an
agent."* That is a change of product, and it is recorded here as one.

### Why this is an amendment and not a reversal

The failure this page records was **wrapping**, not sending. v0.1 sat inside
the agent's pty, between the human and the CLI, and so could not see what the
CLI publishes about itself — `status: idle` in its own registry, the
transcript line that says a tool is waiting — and had to guess at the one thing
the product exists to know. OpenRig orchestrates without wrapping: real CLIs
in real tmux panes, reading their own configuration, reporting through hooks
they were launched with, written to over `send-keys` by a daemon that is not
in the pty. The lesson under *The durable lesson* is therefore intact and
load-bearing for seats too. What changes is the amendment of 2026-08-29's
definition of steering: **a seat's loop may be owned**, and no other session's
may.

### What a seat is

A seat is a session the daemon started itself, in Gau Geung mode, as the
unmodified CLI in a tmux pane, with the configuration
[ADR-0043](0043-a-seat-is-configured-at-launch-never-in-your-home.md)
describes. The daemon records the birth — the definition it was started from,
the pane, the session id it chose where the CLI takes one — and that record
is the only thing that makes a session a seat. A **team** is a named set of
seats with one policy. An **observed** session is everything else.

### The fences

1. **Birth decides ownership.** A session is a seat because the daemon's own
   launch record says it started it, and for no other reason. **Nothing
   promotes an observed session to a seat**: not a click, not a flag, not
   recognising the pane. The session you started in your own terminal keeps
   the whole of this page, and if you want it orchestrated you start it as a
   seat. "I can see it, so I may drive it" is the superstition this page warns
   about, running the other way.
2. **Never wrap.** A seat is the real CLI in a real pane. mogeung learns about
   it from what the CLI writes, from the flags and hooks the CLI itself
   accepts at launch, and from the pane — never from a pty of its own in the
   middle. A seat that cannot run under tmux is not started
   ([ADR-0011](0011-own-a-shell-never-an-agent.md)'s amendment of this date).
3. **An automatic send is declared text.** A wake, a watchdog or a readiness
   press sends fixed text from the team's policy, readable on the seat's row
   before it ever fires. The local model never composes a send —
   [ADR-0030](0030-a-model-reads-the-evidence.md) is untouched, and its output
   stays a labelled column. An agent's message to another seat is that agent's
   own words, delivered verbatim.
4. **Every automatic send is visible, budgeted and stoppable.** It shows on
   the seat's row: what was sent, to whom, why, when. A team has a **hop
   budget**: one hop is one delivery from an agent to an agent with no human
   between, and a chain may travel **three hops** before the next delivery is
   held and ranks on the queue as needing you; your send starts the count
   again. A hop budget of zero is *every message through you*, and it is a
   setting, not a different mode. A team also has a cap on automatic deliveries
   per hour, whose default the spec sets and the panel shows. One switch pauses
   every automatic send for a team, and a human typing in a pane always wins.
5. **The owning daemon judges every send.** The two amendments of 2026-09-23
   stand as written: a send that crosses a hop is judged by the daemon that
   owns the seat, over loopback only, and no token, flag or consent makes a
   LAN relay count as loopback. The daemon that owns a seat is the one that
   launched it.

### What this does to the clauses of 2026-08-29

Clauses 2, 4, 5 and 6 — exactly the text on screen into exactly one session,
loopback only, only a session with a pane, the clipboard stays — hold for every
send, to a seat or not. Clauses 1, 3 and 7 — a human clicks and confirms, no
code path from an agent's output to an agent's input, the draft is not sent by
the gesture that made it — hold for every observed session and for a human's
own send into a seat. **For a seat, clause 3's path exists**, under fence 4,
and that is the thing this amendment admits with its eyes open: it is the path
that burns tokens in a loop nobody is watching, and the hop budget is the cost
cap on it, not a correctness guarantee.

### What was considered and lost

**A second product for orchestration**, leaving this daemon as the watcher.
Rejected: a seat that needs you belongs on the one ranked board
([A1](../product/assumptions.md)), and two daemons reading one home is the
federation problem with no hop between them.

**Adopting an observed session as a seat**, on a click. The convenient shape,
and the one the next request will ask for. Rejected by fence 1: birth is the
only test a daemon can apply without asking a question it cannot answer, and
the promise this page makes to sessions the user starts by hand is worth more
than the click.

**Model-composed automatic sends.** OpenRig's agents write prompts to each
other, and a local model could write a wake that fits the situation. Rejected:
ADR-0031 keeps the model's words out of every agent's input, and a wake that
is template text is one you can read before it fires.

**Unbounded agent-to-agent delivery.** OpenRig's default. Rejected on the
cost: a reviewer and a builder blaming each other for an hour is a bill, and
a chain that must pass through a human every three hops ends it for the price
of a click.

**Every message through you**, which is the shape of 2026-08-29 repeated. Kept
as the zero setting rather than the default, because with it the "parked with
baton" and "blocked on agents" lanes OpenRig's pulse is built on cannot exist
— nothing is ever parked with an agent when you carry every baton.

### What it costs

**mogeung starts agents.** The first sentence of `CLAUDE.md` said never, and
now says never for a session it did not start. The product is two things — a
watcher and a harness — and every row has to say which it is, because the
guarantees differ.

**A send can land on a menu, with nobody reading a dialog.** The cost named
under 2026-08-29 — mogeung cannot see the session's screen — is now paid by a
policy rather than by a person. A readiness press that reacts to what the
pane shows needs the pane read, which no ADR yet admits
([ADR-0010](0010-attach-a-terminal-never-own-one.md) renders a pane and does
not read it), so until one does a readiness press is not built, and a trust
or login gate on a seat is a row that needs **you**. Fence 4's pause and
budget are the mitigation for the sends that are built.

**The read-only daemon is further gone.** `A24` was qualified by
[ADR-0025](0025-run-a-process-you-named-never-an-agent.md); a daemon that
starts agents on request is qualified again, and the seat verbs are judged
per connection exactly as run verbs are (ADR-0025's amendment of this date).

**More rows, and a new kind.** A team of four is four sessions, each with a
policy and a send log, on a board designed around one queue. Where that lives
in the window is the Gau Geung spec's question, and [feature 0031](../archive/features/0031-the-wall.md)'s
answer — a grid is a bet against the queue, so it lives on a chord — is the
precedent.

**Revisit if** an automatic send ever lands where it was not aimed, which is
the 2026-08-29 condition and applies here with more force. Or if a team's hop
budget is routinely exhausted and raised to silence it — then the budget is
decoration and the honest response is zero, not a bigger number. Or if a
hand-started session is ever found to have received a wake, by any path: that
is fence 1 broken, and the response is to find the path, not to document it.
Or if Claude Code grows a sanctioned way to enqueue a prompt, which should
replace `send-keys` for seats as the 2026-08-29 text already says it should
for a human's send.

## Amendment — 2026-10-09, later: a handover may be composed by a model

**Fence 3 of the amendment above is narrowed: an automatic send is declared
text, *or a handover the daemon had a model compose for a seat*.** Asked the
same day, in the walkthrough that followed: *"a handover recap should be model
composed, which could write much better than a human user."*

A handover is what a seat is told when it starts, restarts, or takes the baton
— what the team is doing, what its predecessor did, where it stopped.
OpenRig's restore passes one. Template text cannot write it; a model reading
the evidence can, and the user is right that it writes it better.

### The fences on a composed handover

1. **Composed from what the model may already read**, under
   [ADR-0031](0031-consent-to-a-named-host.md) clause 1: the team's documents,
   the seats' transcripts and diffs, notes. Nothing a seat said is laundered
   into a fact by passing through — the handover is an *input* to a seat,
   never evidence about one, and ADR-0031 clause 5 stands.
2. **Composed in the daemon, on the seat's own trigger.** The daemon holds the
   endpoint; the trigger is a seat event the team's policy names — start,
   restart, baton passed — never a scan tick, and never a model's own
   decision to speak. With no endpoint configured the seat starts without a
   handover and the row says so (clause 6 of that ADR). A handover that would
   have to be composed on a host the config does not consent to is not
   composed.
3. **Shown in full, and logged as the model's.** On the seat's row before it
   is delivered where the policy says *after you read it*, and after
   otherwise; in the send log as *sent by mogeung, composed by <model> at
   <host>*.
4. **It is an automatic send.** It counts against the team's per-hour cap,
   counts as a hop when it carries what another seat said or did, and stops
   with the pause switch — fence 4 above, unchanged.
5. **Only for a seat.** The model has no path into an observed session's
   input, and a human's draft in the window keeps
   [ADR-0034](0034-the-draft-is-a-chat-ask.md)'s two buttons.

What this does not reopen: the loop with nobody in it. The model composes
*what* is said; the policy, read by a human, decides *when*; the budget and
the switch bound *how often*. A model that chose when to speak to an agent
would be the thing fence 3 was written to prevent, and it is still prevented.

---
*ADRs are immutable. A decision that is **narrowed** changes by an
`## Amendment — YYYY-MM-DD` section appended here, with `updated:` bumped. A
decision genuinely **reversed** is superseded: write a new ADR and set
`status: superseded` plus `superseded_by:` here.*
