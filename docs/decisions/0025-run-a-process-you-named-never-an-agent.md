---
title: mogeung runs a process you named, and never an agent
status: active
updated: 2026-08-09
decided: 2026-08-09
---

# ADR-0025 — mogeung runs a process you named, and never an agent

## Context

Asked 2026-08-09: *"I am thinking about add a 'Run and Debug' feature in
mogeung. To make a real IDE like environment."*

Three existing positions have to be reconciled before a single line of this can
be written, and they do not obviously agree.

**[Pillar K](../product/roadmap.md#k-explicitly-not) says "an editor —
explicitly not. Handoff to IntelliJ/VS Code, permanently."** Run and Debug is an
IDE feature; the screenshot that prompted the ask is VS Code's own panel. Taken
at face value this bullet refuses the whole thing.

**[ADR-0003](0003-observe-do-not-spawn.md) says mogeung never starts, steers or
stops an agent.** That is the decision v0.1 died for, and it is the most
important one in the repository. A daemon that spawns processes is at least
adjacent to it.

**[ADR-0019](0019-a-viewer-not-an-editor.md) says mogeung reads code and never
writes it** — and, crucially, it says *why*: not squeamishness, but four costs,
of which the decisive one was **the concurrent writer**. The agent is editing
these files continuously, and two writers on one file with no coordination has
no good answer.

The reconciliation is that these three refuse **different** things, and Run and
Debug collides with none of them cleanly:

- It is not editing. It creates no write verb, and it has no concurrent-writer
  problem at all: two processes are simply two processes.
- It is not re-acquiring the conversation loop. Nothing here starts, prompts or
  interrupts an agent. `cargo test` is not an agent.
- The precedent is already set. [ADR-0011](0011-own-a-shell-never-an-agent.md)
  let a client own a **shell** — a process the user asked for, running under
  tmux — on the grounds that what it holds is a view rather than a session. The
  shell you open in the terminal panel can already run `cargo test`. Today it
  does so with nothing structured coming back.

And there is a positive argument rather than merely an absence of objection.
ADR-0019 settled the editor question by working out what the human actually
still does: *"the agent does the editing, and what the human does is read."*
That was right and it was incomplete. The human also **verifies** — which is
why `R-E1` records the build and test commands a session ran, and why `R-E3`
binds *"tests pass"*-shaped prose to the evidence for it, or visibly to none.
Both features exist to answer *did it really?* and neither can finish the
sentence: mogeung can show you a claim and the run behind it, and the only way
to check is to leave. Running is the missing half of a loop the product already
built three quarters of.

## Decision

**The daemon may start, watch and stop a process the user named. It never
accepts a command to run over the wire, and it never starts an agent.**

Four clauses, each load-bearing:

1. **Named, not supplied.** A run request over the protocol identifies a
   configuration *by id* — one parsed out of a file already in the repository,
   or produced by a detector with a closed, in-process command set. The wire
   carries `run config #3 of session X`, never a command string. A client
   cannot ask this daemon to execute something the repository does not already
   contain.
2. **Never an agent.** A configuration that resolves to `claude`, `codex` or
   any other agent CLI is refused, by name, and the refusal says why. This is
   ADR-0003's ground and it is not softened: mogeung watches agents you
   started, and there must be no path — not even an indirect one through a
   run configuration — by which it starts one.
3. **The daemon owns the process, not the client.** Run state lives where every
   other piece of state lives, so a run survives the window closing, is visible
   to a second client, and works against a remote daemon — where the debuggee
   runs on the machine that has the files, which is the whole point of `R-I6`.
4. **Spawning is opt-in on any bind that is not loopback**, by a flag, exactly
   the way [`--advertise`](../design/architecture.md) is. See below for why.

## The part that is genuinely a security decision

`A24` says *"a read-only daemon is safe to reach over a trusted network with a
shared token"*, and its evidence line says in terms: **the word "read-only" in
this row is load-bearing.** ADR-0019 counted "A24 voided or rewritten" as one of
the four costs of an editor, and refused to pay it.

A run verb costs the same thing. It does not matter that it executes only what
the repository already contains: anyone who can reach the port can then cause
code to execute on that machine, and "the code was already checked in" is a
mitigation, not an answer. Being honest about this now is cheaper than
discovering it after `--listen`.

So:

- On a **loopback** bind, runs are allowed. That is the same trust boundary as
  the terminal panel, which can already run anything at all.
- On **any other bind**, runs are refused unless `--allow-run` is passed, and
  `R-I10` already requires a token there. Two deliberate acts, not one.
- `A24` is rewritten rather than quietly kept: a daemon with `--allow-run` is
  not read-only, and the ledger must say so.

Clause 1 is what keeps this proportionate. The exposure is *"someone who
reaches the port can run this repository's own test suite"*, not *"someone who
reaches the port has a shell"* — and the difference is the reason the wire
carries an id instead of a command.

## Alternatives

**Do nothing; hand off to the IDE.** The status quo, and pillar K's letter. It
loses the argument on its own terms: the handoff exists so the human can do
what mogeung cannot, and *verify the claim you are looking at* is the one thing
the product is otherwise built end-to-end to support. Rejected — but note it is
rejected on the strength of `A33`/`A34`, which are `UNTESTED`, so this ADR is
more exposed than most.

**Let the client run it, through the tmux shell panel.** Costs no new daemon
authority at all, and would have been the cheap answer. Rejected on three
counts: the output is pty scrollback rather than events, so nothing can be
attributed, counted, or bound to a claim; there is nothing for a debug adapter
to speak to; and against a remote daemon it runs on the wrong machine. The
tmux panel remains, and remains the right tool for anything ad hoc.

**Accept a command string over the wire.** Simpler protocol, and it is what
every CI tool does. Rejected: it turns an unauthenticated localhost port into a
remote shell, which is a different product with a different threat model. The
id indirection costs one lookup and buys the whole security argument.

**Split it: daemon runs, client debugs.** Avoids the daemon holding a DAP
session. Rejected — it puts two halves of one feature on opposite sides of the
wire, and makes remote debugging impossible for no gain, since a DAP session is
a subprocess and a socket, which is not harder to hold than a pty.

## Consequences

- **The daemon owns processes for the first time.** Lifecycle becomes real work:
  a run outliving its client, a daemon restarting under a running child, an
  orphan after a crash. None of it is hard; all of it is new.
- **`A24` is rewritten**, and the README/guide claim that mogeung is read-only
  needs the same qualification. This is a real loss and it is the price.
- **Pillar K's "an editor" bullet stands untouched.** This ADR does not open
  the door to editing, and explicitly does not weaken ADR-0019 — the
  concurrent-writer argument is unaffected by anything decided here.
- **`R-E1` and `R-E3` get a second source of truth**, and it is a better one: a
  verify run mogeung executed itself has a real exit code, not a parsed one.
  How the two sources are shown together is a design question, and the wrong
  answer would let a run *we* did quietly launder a claim the agent made.
- **A refusal path exists that will look like a bug.** A configuration that
  launches an agent is refused, and to the user that is a run button that does
  not work. The refusal has to say which clause it hit.

## Revisit if

- A week of use says the run panel is opened and then abandoned in favour of
  the IDE — that is `A33` failing, and the feature should be removed rather
  than improved.
- A path is found by which a run configuration can start an agent indirectly
  (a shell script in the repo that calls `claude`, say). Clause 2 checks the
  command it launches, not what that command goes on to do, and that limit is
  known rather than overlooked. If it happens in practice, this ADR needs a
  successor that says what mogeung does about it — not a quiet widening of the
  check.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
