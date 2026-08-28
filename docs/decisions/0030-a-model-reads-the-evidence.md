---
title: A model may read mogeung's evidence; the daemon holds it and the bytes stay on the machine
status: active
updated: 2026-08-28
decided: 2026-08-28
---

# ADR-0030 — A model reads the evidence, the daemon holds it, and the bytes stay on the machine

## Context

Asked 2026-08-27: *"I have a local running AI model that I can use and call as
API."* The brainstorm that followed was run twice — once inside the design
rules and once with them explicitly suspended — and
[feature 0038](../features/0038-a-local-model.md) records the outcome that
matters: **the five features chosen with the rules suspended all sit inside the
rules as written.** Nothing chosen writes a worktree file, so ADR-0019's
concurrent writer is not paid; nothing chosen touches a session's input, so
ADR-0003 is not approached; `R-O7` improves the text in the window ADR-0008
built and leaves its single action alone.

So this ADR is not a carve-out and does not supersede anything. It exists
because three questions have no home yet, and each of them will be answered by
accident on the first pull request if it is not answered here.

**Where does the call live?** The obvious answer is the client — it is where
the panes are, it is on the desk with the GPU, and it needs no protocol change.
It is also the first exception to *"the daemon is the product; every UI is a
client with no local authority"*, and it gets the remote case backwards: a
window watching a Mac (`R-I6`) would read that Mac's sessions with an index
built from the Linux box's transcripts, or fail to build one at all.

**What crosses the wire?** [ADR-0025](0025-run-a-process-you-named-never-an-agent.md)
established a discipline that has held for every command since: the wire
carries `run config #3 of session X`, **never a command string**. Four of the
five features are naturally id-carrying — a session, a hunk, a query. `R-O5`,
the chat panel, is not: it is a free-form string, sent to a daemon, forwarded to
an endpoint. That is a proxy, and `A24`'s claim about a tokenless LAN daemon has
*read-only* as its load-bearing word.

**Where do the bytes go?** A model endpoint is a URL. The corpus is 67 MB of
transcripts containing every prompt, every diff and every secret an agent has
ever printed. Pointing that URL at a host on the internet is not a
configuration detail; it is
[ADR-0014](0014-fetch-is-not-publishing.md)'s line, crossed silently by a
config file.

## Decision

**mogeung may employ a model to read the evidence it already holds. The daemon
owns the endpoint, the model never writes and never steers, and a
non-loopback endpoint is publishing and is refused unless asked for.**

Six clauses, each load-bearing:

1. **The model reads what mogeung already shows.** Transcripts, diffs, commits,
   notes, the index over them. It writes nothing outside `~/.mogeung`, touches
   no worktree file, and has no path — direct or indirect — into a session's
   input. ADR-0003, ADR-0008 and ADR-0019 are unweakened and this clause is
   what keeps them so.
2. **The daemon holds the endpoint, not the client.** The corpus is on the
   daemon's machine and derived state lives where
   [ADR-0016](0016-rebuild-derived-state.md) says it lives. The endpoint is a
   **URL in daemon config** rather than an in-process model, so the machine
   with the GPU and the machine with the sessions may differ.
3. **Loopback, or an explicit flag that says where the bytes go.** A model
   endpoint that is not loopback is refused unless the daemon was started with
   `--allow-remote-model`, on the same reasoning and with the same shape as
   ADR-0025's `--allow-run`. The window states the endpoint host wherever model
   output appears. Sending the corpus off the machine is a decision the user
   makes out loud or not at all.
4. **Ids on the wire, and one named exception.** The `Model*` family carries
   ids like every family before it. `R-O5`'s chat is the single free-form
   variant, it is named as such, and it is refused entirely on a non-loopback
   bind — a daemon reachable over a LAN does not become a general-purpose LLM
   proxy because a text box was convenient.
5. **Model output is never evidence.** `R-N7` holds the line for runs — *a run
   we did must not launder a claim the agent made* — and `Corroboration` has no
   field that merges them. A model's reading gets its own column, labelled, and
   is never merged with a claim, a run, or a fact read from the registry. It
   may order within a view; it may never set an attention tier.
6. **No model is a first-class state.** With no endpoint configured, every
   surface renders what it renders today and says so where the feature would
   have been. Nothing blocks the scan tick on an inference call — `R-J8`'s
   lesson — and a slow or dead endpoint degrades a panel, never the daemon.

## Alternatives

**Call the model from the client.** Cheapest, needs no protocol work, and puts
the call on the machine with the hardware. Rejected on the remote case: the
index has to be built where the transcripts are, and a client-side model makes
a window watching another machine either wrong or empty. It would also be the
first piece of local authority in a client, which is the property ADR-0018's
port was able to claim it never took.

**Free-form prompts everywhere.** Much more useful — *ask the corpus anything*
was the strongest idea in the brainstorm — and rejected for now on the same
ground ADR-0025 refused command strings. It is not refused permanently; it
needs its own ADR that pays for `A24` properly, and that argument is easier to
make once `R-O2` has shown the model is worth listening to at all.

**Blend the model's score into the keyword heuristic.** Tempting, because a
weighted mix degrades gracefully when the model is absent. Refused by
[pillar K](../product/roadmap.md#k-explicitly-not) in advance: *"either keep
honest keyword heuristics or replace them wholesale — something in between
would look authoritative while still being wrong."* Two orderings that can be
switched between are honest; one ordering nobody can explain is not.

**Ship the endpoint wherever the user points it, cloud included.** Rejected as
publishing by config file. The refusal is not squeamishness about hosted
models: it is that ADR-0014 moved the line from *the network* to *publishing*,
and 67 MB of transcripts is the most publishable thing this product holds.

**Do nothing.** The honest baseline, and the reason it loses is `A3`: reading
order has been keyword heuristics resting on an `UNTESTED` assumption since the
ledger was written, and pillar K names real analysis as the only permitted
alternative. Doing nothing means never settling it.

## Consequences

**Easier.** Every model feature has one place to live, one failure mode, one
health row, and one answer to *what happens when it is off*. The remote daemon
keeps working. The security surface is one flag rather than a discussion per
feature.

**Harder.** The GPU and the sessions may be on different machines, and clause 3
means the interesting configuration — a remote daemon reaching back to this
desk — is exactly the one that needs a flag. That friction is intended and it
will be annoying.

**Ruled out.** An agentic model, a model that edits, a model that answers for
you in a session, and a chat panel on a LAN-bound daemon. Also ruled out is the
cheap version of every feature here: a client that calls an endpoint directly
would take an afternoon, and this ADR says it may not.

**The pressure this creates, named in advance.** ADR-0008 wrote down that
*"'just paste it for me' is one keystroke from 'just send it'"*. `R-O7` makes
the paste materially better, which makes that keystroke more tempting than it
has ever been. A reviewer that finds a real bug is one button from fixing it.
When that button is proposed, this paragraph is the record that it was foreseen
and refused rather than never considered.

## Revisit if

- `R-O2`'s harness says the model's reading is not worth the screen space —
  then most of this is moot and the pillar comes out rather than being tuned.
- A free-form corpus query becomes the feature people actually want, in which
  case clause 4's exception needs promoting to a rule and `A24` needs rewriting
  rather than working around.
- The GPU-and-sessions split turns out to be the normal case rather than the
  remote one, which would make clause 2's *daemon holds it* the wrong default
  instead of merely the inconvenient one.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
