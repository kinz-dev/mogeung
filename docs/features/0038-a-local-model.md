---
title: A local model beside the agents
status: in-progress
updated: 2026-08-29
roadmap: [R-O1, R-O2, R-O3, R-O4, R-O5, R-O6, R-O7, R-O8, R-O9, R-O10, R-O11]
depends_on: [A3, A4, A29, A35, A36, A37, A38]
---

# 0038 — A local model beside the agents

Asked 2026-08-27: *"I have a local running AI model that I can use and call as
API"*, then widened the same day to the whole development life-cycle. Five
candidates were chosen out of that conversation and they are what this spec
covers. The rest of the brainstorm is deliberately not recorded — a list nobody
picked from is not a plan, and this repository already has enough documents.

**The finding that shaped the spec: none of the five needs a fence moved.**
The question was posed with the design rules explicitly suspended, and the
features that came back out of it sit inside the rules as written.
[ADR-0003](../decisions/0003-observe-do-not-spawn.md) is untouched — nothing
here starts, steers or stops an agent.
[ADR-0019](../decisions/0019-a-viewer-not-an-editor.md) is untouched — nothing
here writes a worktree file, so the concurrent writer, the cost that decided
that ADR, is not paid.

> **The third claim was wrong, and building `R-O7` is what proved it.** This
> paragraph said [ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md)
> was untouched too. Its *decision* is — the window still offers exactly one
> action and nothing is sent to a session — but one sentence of it was
> *"the daemon never learns a prompt was written"*, and a model cannot draft
> text it has not been shown. That sentence is replaced by
> [ADR-0034](../decisions/0034-the-draft-is-a-chat-ask.md), which supersedes
> ADR-0008 and carries the rest of it forward verbatim. Corrected here rather
> than quietly: *four* of the five needed no fence moved, and the fifth moved a
> smaller one than it first appears.

That is worth stating plainly because it is evidence about the rules rather
than about the features: given permission to break them, the work that looked
most valuable did not need to.

What *is* new is [ADR-0030](../decisions/0030-a-model-reads-the-evidence.md),
and it exists for two reasons that have nothing to do with agents — where the
model call lives, and the fact that `R-O5` puts a free-form string on a wire
that has only ever carried ids.

## Spec

### Problem

Five moments where mogeung already holds the material and cannot use it.

**1. A 22-file diff has no shape.** The Changes pane lists files and the
reading order underneath is keyword heuristics — [A3](../product/assumptions.md),
`UNTESTED` since the ledger was written, evidence in full: *"ranked `auth.rs`
above a lockfile once, in a test."* An agent-made change is mostly mechanical
with three hunks that matter, and finding those three is done by scrolling.

**2. The rationale for a line is on disk and unreachable from the line.**
`R-F2` links a file to the sessions that touched it and `R-F9` links a commit
to the turns nearest its timestamp. Neither answers *why is there a mutex
here* — and the answer is in the transcript, written by the thing that added
it, three panes away and not indexed by the question anyone actually asks.

**3. A quick question means leaving.** Not a question about this repository —
a question about a `sed` flag, a Rust lifetime, a git incantation. mogeung is
the window that is open, and the assistant is somewhere else.

**4. Search finds the words you remember, not the thing you meant.**
[Feature 0017](0017-cross-session.md) put semantic search out of scope with a
reason and a sequence: *"Semantic/embedding search — honest substring/token
search first."* Substring search shipped in 2026-07 and has been in daily use
since, so the condition attached to that refusal is met. The sharpest cost of
its absence is `R-F4`: recurring-failure detection compares **literal** error
text, so the same failure worded two ways is two failures, and a failure worded
freshly each time is invisible.

**5. A review's intent still dies on the way to the terminal.** ADR-0008 built
the prompt window and it works, but what it composes is a concatenation —
flagged hunks, hunk headers, your terse notes — and turning that into a
sentence an agent can act on is still done by hand, in the window, every time.

### Assumptions

| # | Status | Why it matters here |
|---|---|---|
| [A35](../product/assumptions.md) | `UNTESTED` | The pillar's premise: a local model's reading of mogeung's own evidence is worth the screen space. `R-O2` is its test and it comes first |
| [A36](../product/assumptions.md) | `UNTESTED` | `R-O4` rests on the rationale for a change being in the transcript and reachable from the line. The links exist (`R-F2`, `R-F9`); that they answer the question is the bet |
| [A37](../product/assumptions.md) | `UNTESTED` | `R-O5`, and it is [A27](../product/assumptions.md)'s shape exactly — a text box in mogeung competing with a better one already open beside it. A27 is `AT RISK`, which is the precedent to read before building this |
| [A38](../product/assumptions.md) | `UNTESTED` | `R-O6`: that embeddings find what substring search missed **often enough to earn a second list** |
| [A3](../product/assumptions.md) | `UNTESTED` | `R-O3` is the first real attempt to settle it, in the direction [pillar K](../product/roadmap.md#k-explicitly-not) permits |
| [A29](../product/assumptions.md) | `SUPPORTED` | `R-O6` extends the search panel A29 settled rather than adding a sixth box |
| [A4](../product/assumptions.md) | `AT RISK` | As everywhere. A model reading transcripts inherits every shape the parser has not seen |

> **The doc rule applies and is not being waived.** A35 is `UNTESTED` and it
> carries the whole pillar, so `R-O2` — the harness — is the work, and
> `R-O3`–`R-O7` are gated on what it says. `R-O5` is the exception and it is an
> honest one: A37 is a question about habit, and no harness measures habit. It
> is built small and judged by `R-O8`.

### Acceptance

**`R-O1` — the seam**

- [x] A model endpoint is configured on the daemon; with none configured every
      pane below renders exactly what it renders today and says *no model
      configured* where the feature would have been — never a spinner, never an
      empty panel that reads as broken
- [x] Health carries a model row, per endpoint, the way it grew a per-source
      list for a third agent CLI (`R-I15`): reachable, model name, last error
- [x] No model call happens on the scan tick. A pane asks; the answer arrives
      later or does not arrive
- [x] The daemon refuses a non-loopback **model endpoint** unless started with
      an explicit flag, and the window states where the bytes go

**`R-O2` — the harness, before the panels**

- [x] `cargo run -q -p mogeungd --bin judge` runs the reading guide and the
      existing keyword order over every session on this machine that has a
      diff, and prints where they disagree — built 2026-08-28, and it runs the
      shipped `mogeungd::guide` rather than a copy of it
- [ ] It reports semantic recall against grep on a fixed query set: hits grep
      missed, and hits grep found that the index ranked away
- [ ] It exits **non-zero when the model is unreachable or returns nothing**,
      so a broken setup can never read as *no findings* — `--bin sweep`'s
      discipline, for the same reason

**`R-O3` — the reading guide**

- [x] For a session with changed files, the Changes pane offers a model
      ordering with a one-line reason per file and a paragraph naming what
      carries the change and what is mechanical
- [x] The reason is always visible where the ordering is used — the ranking is
      never a black box ([attention-ranking](../design/attention-ranking.md))
- [x] The keyword ordering is one click away, unchanged, and is what shows when
      no model is configured
- [x] The two orderings are never blended. Pillar K allows honest heuristics or
      real analysis and refuses the middle
- [x] Nothing here touches the queue's tiers or scores

**`R-O4` — ask the diff, answered from the transcript**

- [x] `cargo run -q -p mogeungd --bin why` asks one question about a real edit
      moment through both retrievals and prints which turns each answer came
      from — `A36`'s test, and the row's own first commit rather than a gate
      belonging to `R-O2` (built 2026-08-29)
- [x] From a hunk or a line, a question can be asked and is answered in place
- [x] Every answer **cites the turns it used**, and a citation opens the
      Transcript pane at that moment (`R-F9`'s machinery — by timestamp, since
      a transcript line number is not a place a client can navigate to)
- [x] When no transcript covers the line, the answer says so and is labelled as
      read from the code alone — an uncited answer is never presented as
      provenance
- [x] *No reason in these turns* is a first-class answer rather than an error
      state, and an answer citing only assistant turns is labelled **narration**
      rather than rationale (both added by `--bin why`'s first corpus run)

**`R-O5` — the chat panel**

- [x] A chat tool in the right rail takes a question and answers in place,
      conversationally, with no repository context and no tools
- [x] The conversation is **ephemeral**; one gesture copies it into a note
      (`R-L2`'s copy-into-a-note, reused rather than a second store)
- [x] It is refused outright on a non-loopback bind, with **no flag** that
      opens it, and the refusal says why and offers the ssh route instead

> **This line was wrong when it was written and is corrected here rather than
> quietly.** It said *"unless the flag from `R-O1` is passed"*, which
> contradicts [ADR-0030](../decisions/0030-a-model-reads-the-evidence.md)
> clause 4 — *"refused entirely on a non-loopback bind"*. The ADR is the
> decision and this spec was the draft; an escape hatch that exists is one that
> becomes the documented workaround, which is `server::admit`'s reason for
> having no `--insecure` either.

**`R-O6` — semantic search as a second list**

- [ ] The Insight search panel keeps its grep results, first and unchanged
- [ ] A second list, labelled **similar** and never *matches*, shows semantic
      hits, and says which model produced the index and when it was built
- [ ] An index older than the corpus says so rather than answering as though
      current
- [ ] Recurring-failure rows (`R-F4`) cluster failures that share meaning
      rather than literal text, and every cluster can be expanded to the
      literal strings that were joined

**`R-O7` — draft the follow-up prompt**

- [x] The prompt window offers a drafted instruction composed from the flagged
      hunks and their notes
- [x] The raw concatenation is one click away, so what the draft dropped is
      inspectable
- [x] The window still offers exactly **one** action: copy, and it copies
      whichever text is on screen
- [x] The draft is **asked for** and is **kept nowhere** — the ask names no
      conversation, and the answer never appears in the chat panel it borrowed
      the door from (added by the build; ADR-0034 clauses 2 and 5)

**`R-O8` — the verdict**

- [ ] After a fortnight, each row above is kept or removed against the removal
      condition written in its assumption, and A35–A38 take a status

### Explicitly out of scope

- **Any write to a worktree file.** ADR-0019's concurrent writer is unchanged
  and unanswerable; a model that edits while an agent edits is the same
  problem with a new author.
- **Any path into a session's input.** ADR-0003. `R-O7` ends at the clipboard,
  which is where ADR-0008 put the boundary and where it stays.
- **The model choosing an attention tier.** Tiers are facts — the registry's
  `idle`, an unmatched `tool_use`, a recorded API error. A model may order
  within a view; it may not move a badge.
- **Model output as evidence.** `R-N7`'s rule generalises: *a run we did must
  not launder a claim the agent made.* A model's opinion is a third column and
  is never merged with a claim or a run.
- **A cloud endpoint by default.** Sending the corpus to a remote host is
  publishing, which is [ADR-0014](../decisions/0014-fetch-is-not-publishing.md)'s
  line. Loopback unless explicitly and visibly configured otherwise.
- **Agentic loops** — a model that runs commands, iterates, or works unattended.
  Nothing here needs one, and the security argument for one is a different ADR.

## Plan

### Approach

**The daemon holds the model, not the client.** The corpus is on the daemon's
machine, the derived index has to live where [ADR-0016](../decisions/0016-rebuild-derived-state.md)
says derived state lives, and *"the daemon is the product; every UI is a client
with no local authority"* is the rule this would otherwise be the first
exception to. It also decides the remote case correctly: a window watching a
Mac (`R-I6`) gets **that machine's** transcripts read by whatever endpoint that
daemon is configured with, rather than the window's own.

The cost is real and stated rather than discovered: the machine with the GPU may
not be the machine with the sessions. The endpoint is therefore a URL in daemon
config rather than an in-process model, so a remote daemon can be pointed back
at this desk — with `R-O1`'s non-loopback flag as the gate, because that
configuration is exactly the one that puts transcripts on a network.

Everything else follows the shape `insight` already has: a request-path family
that computes on demand, cached by content hash, droppable and rebuildable —
never on the scan tick, which is `R-J8`'s lesson (broadcast only what changed;
never make the poll loop do work).

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/model.rs` | New — request/response types, the prompt builders, the no-model state |
| `crates/mogeung-core/src/wire.rs` | New `Model*` command family; one free-form variant, the rest id-carrying |
| `crates/mogeung-core/src/config.rs` | Endpoint, model name, embedding model, the non-loopback flag |
| `crates/mogeung-core/src/health.rs` | A per-endpoint model row |
| `crates/mogeungd/src/model.rs` | New — the HTTP client, work queue, content-hash cache |
| `crates/mogeungd/src/insight.rs` | Embedding index; `similar` beside the grep path; `R-F4` clustering |
| `crates/mogeung-core/src/review.rs` | Reading-guide ordering beside the keyword one, never blended |
| `crates/mogeungd/src/api.rs`, `server.rs` | REST twins; the non-loopback refusal |
| `crates/mogeungd/src/bin/judge.rs` | New — `R-O2`'s harness |
| `crates/mogeungd/src/why.rs` | New — `R-O4`'s retrieval, prompt and parser, shared with its harness |
| `crates/mogeungd/src/bin/why.rs` | New — `A36`'s test, `R-O4`'s first commit |
| `desktop/src/panes/ChangesPane.tsx` | The guide, the reason column, the fallback toggle |
| `desktop/src/ui/DiffView.tsx` | Ask-from-a-hunk, citations that open the Transcript |
| `crates/mogeungd/src/api.rs` | `ask_about` — the retrieval, the labels, `R-O4` |
| `desktop/src/ui/Rail.tsx` + a new rail tool | The chat panel |
| `desktop/src/panes/InsightPane.tsx` | The second, labelled list |
| `desktop/src/ui/PromptWindow.tsx` | The draft, and the raw view behind it |
| `desktop/src/lib/prompt.ts` | `R-O7`'s ask — composed in the client, ADR-0034 |

### Risks and unknowns

- **A37 is A27 wearing a different hat.** Notes were asked for as directly as
  this and are `AT RISK` because the editor beside mogeung was where writing
  already happened. The chat window beside mogeung is a stronger incumbent than
  a text editor was. This is why `R-O5` stores nothing.
- **Latency.** A reading guide that arrives after you have started scrolling is
  worse than no reading guide. The harness should report time-to-first-guide,
  not only quality.
- **Contention.** A local model doing background work shares a machine with
  three or four agents and a `cargo build`. Indexing is the part that must be
  interruptible.
- **A4, again.** A model summarising a transcript shape the parser silently
  dropped will describe a session confidently and wrongly. Citations are the
  mitigation: an answer that cannot point at turns is labelled as not having
  any.
- **The draft that is nearly right.** ADR-0008 predicted this pressure by name
  — *"'just paste it for me' is one keystroke from 'just send it'"* — and
  `R-O7` makes the paste better, which makes the pressure worse. The refusal is
  written down now, before there is a button worth wanting.

### Test strategy

- Prompt builders and response parsing are pure: unit tests with recorded
  fixtures, no endpoint.
- A fake endpoint (a local HTTP stub) for the daemon path: latency, failure,
  malformed output, and the no-model state — which is the one that must never
  break a pane.
- The ordering rule gets a test that would fail today: with a model configured
  and a model absent, the Changes pane must produce two orderings and never a
  third that mixes them.
- `--bin judge` is the acceptance test for A35 and A38 and is not a unit test;
  it runs against this machine's corpus and prints numbers a human reads.
- Client suites for the fallback rendering, the citation click-through, and the
  prompt window still offering one action.

## Notes

*Filled during implementation. Surprises, dead ends, things the plan got wrong.*

### `R-O1` and `R-O5`, built 2026-08-28

**The plan said the harness comes first and it did not.** `R-O2` still gates
`R-O3`, `R-O4`, `R-O6` and `R-O7`; what was built is the seam and the one row
whose assumption a harness cannot test. That was the plan's own exception and it
is recorded here so nobody later reads the order as the gate having been
skipped.

**The flag-only consent had a hole, and it was the user's own configuration.**
ADR-0030 clause 3 made `--allow-remote-model` a flag with no config-file twin,
copying `--allow-run`'s shape. The copy is imperfect and the difference took an
hour to find: `runs_allowed` reads the **bind** address, so a window-hosted
daemon — always loopback — is permitted without any flag and the missing argv
never bites. The model gate reads the **endpoint** address, and a hosted daemon
can perfectly reasonably want a remote one. So on the shape mogeung is normally
run in ([ADR-0009](../decisions/0009-the-window-may-host-a-daemon.md)), the
consent was unreachable and the endpoint refused for ever, with a message naming
a flag that could never be passed.

The first answer was narrow: the hosted daemon learnt to read `model_url` and
`model_name` from `config.toml` — it had read *nothing* from that file before —
and still could not grant consent, so it reached loopback endpoints only. That
was recorded here as **a decision deferred rather than made**, on the grounds
that it should not be settled an hour after the ADR that set it.

**It survived one install.** Re-installing and clicking the launcher produced
exactly the predicted refusal, which is a refusal with no way out — the thing
`server::admit` taught this codebase not to ship. Settled by
[ADR-0031](../decisions/0031-consent-to-a-named-host.md), which supersedes
ADR-0030 and carries five of its six clauses forward verbatim. The replacement
clause 3 is **not** the argument the deferral anticipated. The question was
never *is a file as deliberate as a flag*; it is **what an explicit act is**,
and a flag turns out to be the weaker one, because a flag is blanket. So
consent names its host: `allow_remote_model = "spark-7ecc"` is consent to that
machine and no other, and moving `model_url` asks again. `true` and the flag
remain as the blanket grant, exactly as strong as before and no stronger.

**The panel's *no table to forget* lasted five hours.** `R-O5` shipped
storing nothing, and it was a designed property rather than an omission: the
one free-form surface in the product, resting on an `UNTESTED` `A37`, built to
be cheap to remove. A history was asked for the same evening, and `R-O9` gives
it up on purpose — recorded in
[ADR-0032](../decisions/0032-the-chat-panel-remembers.md) rather than allowed
to happen as a side effect of a feature request, because what changes is not
the panel but where your questions live.

What survived the change is the part worth naming: the daemon keeps only
**answered** exchanges, which falls out of putting the write on the success arm
rather than from a rule anyone has to remember; the history is refused exactly
where the ask is; and the three gestures that look like each other — *new*,
*clear*, *forget* — are three lifetimes, stated in the panel's own header
table because a delete disguised as a screen-clear is the mistake this shape
invites.

**The consent gate has a shape it cannot see, and `R-O10` is where that got
said out loud.** ADR-0031 decides consent from the *endpoint's* host. A proxy
on `127.0.0.1` is loopback, so the gate passes without asking while prompts go
to a vendor — the mechanism decided in the morning bypassed by the feature
decided in the afternoon, on the same day.

The instinct was to extend the gate: mogeung writes the proxy's config, so it
knows the hosts, so it could refuse them. [ADR-0033](../decisions/0033-a-proxy-of-our-own.md)
clause 6 refuses that, on pillar K's rule rather than on convenience — routing
is decided per request and a target can fail over, so the gate could only ever
be sometimes-right, and a gate that is sometimes right looks authoritative
while being wrong. What mogeung actually knows is *which hosts appear in the
file*, so that is exactly what it says, in the panel where the prompt is typed
and read from the file so it survives the proxy being down.

**The `/models` URL is what people paste.** It is the URL you can `curl`, so it
is the one in shell history and the one that reaches a config file — and asking
for `…/v1/models/chat/completions` fails with a 404 nobody can read.
`normalise_base` strips it rather than refusing, because refusing a URL that
works in curl is a worse first five minutes.

**A thinking model can answer with nothing.** The endpoint on this desk returns
`reasoning_content` beside `content`, and a model that spends its budget
reasoning returns the second empty. Rendering that as an empty bubble reads as a
mogeung bug, so the reasoning is shown **labelled as reasoning** and never
passed off as the answer.

**A failed exchange leaves the thread in both directions.** The first cut
dropped the error from the next request and kept the question that provoked it,
which sends the model a question nobody answered — it may take the next one for
a clarification of the old. Both halves go.

**Verified live, not only at the wire** — `R-J38` is the standing warning about
stopping one step short. All three gates were exercised against a running
daemon: a remote endpoint with the flag answered in 1.9s; the same endpoint
without it refused and sent nothing; a `0.0.0.0` bind with a token refused chat
with no way round it. Then the panel itself, in a browser tab against that
daemon — question in, answer rendered, `RadixArk/Qwen3.8-27B-NVFP4 · 1.3s`
underneath it.

**What is not built:** no REST twin for `model_chat`. Every other command has
one; this one is the free-form string, and a second door into it is surface to
delete if `A37` says the panel goes. Worth revisiting when `R-O3` or `R-O4`
arrives with an id-carrying command that wants one.

### `R-O7`, built 2026-08-29

**The row said no fence had to move and it was nearly right.** The plan's own
preamble claimed ADR-0008 was untouched by this row. Its decision is — one
action, nothing sent to a session — but *"the daemon never learns a prompt was
written"* is a sentence in that decision, and a model cannot draft text it has
not been shown. [ADR-0034](../decisions/0034-the-draft-is-a-chat-ask.md)
replaces that one sentence and carries the rest forward verbatim. Worth saying
plainly: the fence that moved is the one nobody predicted would, which is the
usual shape.

**The draft is a chat ask, and nothing on the wire is new.** The alternative
was a `draft_prompt` command carrying hunks and notes to a prompt builder in
`mogeungd` — the shape `R-O3` uses, and the better one on *"the daemon is the
product"*. It loses on ADR-0031 clause 2: it would be the **second** free-form
family on a protocol that carries ids, and the bind refusal protecting the
first would have to be written again somewhere it can be forgotten silently.
Riding `model_chat` inherits every gate instead, including the one that
matters — a daemon bound beyond loopback will not take one at all. The cost is
recorded rather than waved away: the meta-prompt is TypeScript no Rust harness
can grade, and a second client would have to compose its own.

**The answer comes back through the chat's door and must not come out of it.**
`model_reply` is matched by the client's own request id, which is how two chat
questions in flight already stay apart — so the draft is routed by id before
the chat reducer sees it. That is the test worth having: a drafted instruction
appearing in a conversation somebody is reading would be a leak between two
features that share a wire and share nothing else.

**Bounded by the whole ask, not per hunk.** `R-O3` paid 78 seconds and an empty
answer to learn that a per-item cap is not a bound while the item count is not.
Flagging is done by hand and rarely runs past a handful — but *rarely* is not a
limit, and a draft that fails the day somebody flags forty hunks fails on the
day it was most wanted. 400 lines shared out, and what was cut is stated in the
prompt so the model does not draft from a hunk it believes it saw whole.

**The output contract is the load-bearing half of the prompt.** What comes back
is pasted into an agent's terminal, so a model that opens with *"Here is a
draft:"* has put a sentence into somebody's session that they did not write.
The ask says: the instruction and nothing around it, name each file, say only
what the reviewer's notes ask for, and leave a hunk out rather than inventing a
reason for it.

**Verified live, not only at the wire** — `R-J38`'s standing warning. Against
the running daemon and its own llmproxy, in a browser tab: two hunks of
`docs/client.md` flagged, a note typed, and the draft came back in **5.6s** from
`claude-sonnet-5` as one instruction naming the file and both changes, with no
preamble in front of it. The toggle showed the raw concatenation underneath,
and the chat panel's history still had nothing newer than the day before — the
ask named no conversation, and the daemon kept none.

**What is not built:** the drafted text is not editable in place — it is
rendered, and editing happens after the paste, exactly as it did before. And
the draft is not offered when no model is configured: the button is disabled
carrying the daemon's own refusal as its title, rather than the window
composing a second opinion about why.

### `A36`'s harness, built 2026-08-29 — `R-O4`'s first commit

**The row's own first commit, not a third half of `R-O2`.** `R-O2` was split so
that each assumption is tested by the row that depends on it, and `A36` is the
same shape: `--bin why` measures the retrieval before `R-O4` draws a panel on
it. That is the doc rule read literally — if an assumption is `UNTESTED`, the
work is to test it — and the cost of skipping it here would have been an
L-sized feature built on the wrong end of a conversation.

**It asks one question through two retrievals**, because the doubt `A36` wrote
down in advance was not *is the reason there* but *are we looking at the wrong
place*. `nearest-in-time` is `R-F9`'s existing machinery, which `R-O4` would
have inherited by accident. `leading-up` is the human prompt at or before the
edit plus everything between. `mogeungd::why` holds both, with the prompt and
the parser, and the panel will use this code rather than a second copy — the
rule `R-O3` paid for.

**What 14 edit moments said, asked twice.** A reason was found in **5 of 14**
either way, so *not in these turns* is the majority outcome and the panel has to
render it as an answer rather than as a failure. The gap between the shapes is
where the finding is: nearest-in-time cited **1 human turn against 12 of the
assistant's** and rested **4 of its 5 answers on the assistant's narration
alone**; leading-up cited **5 human turns** and produced **none**. `--show`
makes the mechanism visible in one screen — in a long agent stretch the six
turns nearest the edit in time are all the assistant talking to itself, so the
prompt that caused the edit is never in reach.

**So `A36`'s doubt is a fixable design error rather than a failed assumption**,
which is exactly the distinction the assumption's own row asked for, made before
anything was built on top of it. The status stays `UNTESTED`: a harness says the
reason is reachable when it is there, not that the answers are worth the screen
space, and that is what the fortnight `R-O8` owns is for.

**Two things found by running it.** A reply came back carrying llmproxy's own
routing classification (`R3_LOCAL <parameter name="reason">…`) rather than an
answer, so a reply with no `REASON:` label is now counted apart rather than read
as a reason found — otherwise the number this harness exists to report inflates
itself. And the same moment can answer *no reason* on one run and narrate on the
next, so the output says to run it twice: it is the gap between the shapes that
is stable, not either number alone.

### `R-O4`'s panel, built 2026-08-29 — and the harness decided its shape

**Every design choice here came from `--bin why` rather than from taste**, which
is what building the harness first bought:

- **`leading-up` retrieval, not nearest-in-time.** The turns leading to the last
  edit of the file, six of them — the harness's own shape and its own number, so
  the panel asks what was measured.
- **Three answers, drawn three ways.** *The turns say why* carries citations;
  *the turns do not say why* is stated plainly and is not an error, because it
  is what most moments produce; *no conversation covers this file* reads the
  diff and says so. The label is as much of the feature as the answer.
- **Narration is marked, and the daemon marks it.** An answer whose every
  citation is the assistant's own is the assistant describing its own work, and
  it is shown as that. Deciding it in the daemon is `R-O3`'s rule again — the
  window must not be able to render *the agent said it did this* as *this is why
  it was done*.

**It rides `model_chat` with an `about` field**, which is
[ADR-0034](../decisions/0034-the-draft-is-a-chat-ask.md)'s *revisit if* answered
exactly as that ADR said it would be: a purpose on the one free-form message
rather than a second free-form family. The daemon does the retrieving because it
holds the transcripts (ADR-0030 clause 1) — a window watching another machine
has never read that machine's sessions — so what travels is **ids and a
question**, and the one free-form exception stays at one.

**Citations open the Transcript by timestamp, not by line.** The line number in
a citation is what the transcript file says and is shown for that reason; `seq`
is assigned by the daemon on ingest and the two do not correspond, so
`jumpToMoment` uses the timestamp — the same route the search panel already
takes to a turn in another session.

**One degrade found by thinking about the shape of the field.** A daemon built
before this row ignores `about` entirely (`serde(default)`) and answers the
question as ordinary **chat**, with no transcript behind it — a general answer
arriving where provenance was promised, which is precisely the failure the
labels exist to prevent. It is detectable, because such a reply carries no
`basis`, so the window withholds the text and says why instead of drawing it.

**Verified live** against a second daemon on `127.0.0.1:7795` with its own
database: asked of a hunk in `tengsyu`'s dashboard code, answered in 4.3s by
`claude-sonnet-5` — *the displayed storage total only summed chapter files and
excluded the `.m4b`, so it under-reported 4.7 GB against the housekeeping
sweep's 7.2 GB* — and the answer arrived **marked as narration**, because its
only citation was the agent's own turn. Clicking that citation raised the
Transcript and landed on the moment. The rule the corpus bought, firing on its
first real question.

**What is not built:** no REST twin, for `R-O5`'s reason — this is the
free-form door, and a second way in is surface to delete if the fortnight says
the pillar goes. And no follow-up question: one ask, one answer, per hunk.
