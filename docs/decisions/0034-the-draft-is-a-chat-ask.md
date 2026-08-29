---
title: A model may draft the follow-up prompt; it still leaves only by the clipboard
status: active
updated: 2026-08-29
decided: 2026-08-29
supersedes: ADR-0008
---

# ADR-0034 — The draft is a chat ask, and the daemon relays what it does not keep

## Context

[ADR-0008](0008-build-the-prompt-never-send-it.md) built the follow-up prompt
window: flag hunks while reading, and mogeung assembles them into text with
exactly one action on it — copy. `R-O7` makes that text better by having a
model compose the flags and their notes into **one instruction** rather than
concatenating them into one document.

The feature is small. What it crosses is not, and both crossings are in ADRs
that are load-bearing rather than incidental.

**ADR-0008 said the daemon never learns a prompt was written.** In full:
*"There is no wire command for it. `FlaggedHunk` lives entirely in the client;
the daemon never learns a prompt was written."* A model cannot draft text it
has not been shown, and [ADR-0030](0030-a-model-reads-the-evidence.md) clause 1
— carried forward by [ADR-0031](0031-consent-to-a-named-host.md) — puts the
endpoint on the **daemon** rather than in the client, for a reason that has not
weakened: the client may be a window watching another machine. So the flagged
text has to reach the daemon, and that sentence cannot survive the feature.

**ADR-0031 clause 2 says the wire carries ids, with one exception.** The chat
panel's `ModelChat` is the single free-form family in this protocol, and it is
refused outright on a bind beyond loopback — a daemon anyone can reach must not
become a general-purpose LLM proxy because a text box was convenient. A
`DraftPrompt` command carrying hunks and notes would be the **second**
exception, and the argument for refusing the first would apply to it word for
word.

The pressure ADR-0008 named is also unchanged, and this feature raises it:
*"just paste it for me" is one keystroke from "just send it"*. A drafted
instruction is a materially better paste than a concatenation, which makes the
keystroke more tempting than it has ever been.

## Decision

**Everything ADR-0008 decided stands, and is carried forward here verbatim:**

> mogeung composes the prompt text and puts it on your clipboard. You paste it.
> Flagging a hunk while reading collects it — path, hunk header, the changed
> lines and an optional per-hunk note. The prompt window renders those into
> text you can edit, and offers exactly one action: copy.
>
> **Ruled out:** any code path that sends text to a session, by any mechanism.

One sentence of it does not survive, and this ADR exists to replace it rather
than to let it erode:

1. **The draft is asked as a chat question, through `ModelChat`.** The window
   composes the ask, the daemon relays it, and the wire grows **no new
   free-form family** — ADR-0031 clause 2 still names exactly one exception.
   Everything that refuses a chat question refuses a draft, including the
   refusal that matters: a daemon bound beyond loopback will not take one at
   all, with no flag that opens it.

2. **The daemon sees the text and keeps none of it.** The ask names no
   conversation, which since [ADR-0032](0032-the-chat-panel-remembers.md) is a
   client saying *do not store this*, and the daemon honours it. So ADR-0008's
   *the daemon never learns a prompt was written* becomes **the daemon relays a
   prompt it does not keep and does not know is one**: purpose stays in the
   client, and what is on the wire is a question like any other question.

3. **The raw concatenation is one click away, always.** A draft is a model
   deciding what matters, which means a draft can drop something. What it
   dropped is only visible against what it was drafting from, so both texts are
   in the window and the toggle between them is two buttons.

4. **The clipboard copies what is on screen.** A window showing one text and
   copying another would be worse than having no draft at all.

5. **The draft is asked for, never automatic.** ADR-0031 clause 6 keeps model
   work off anything that runs on its own, and opening this window is something
   that happens every time somebody flags a hunk.

6. **Still exactly one action: copy.** Drafting composes text *inside* the
   window. The clipboard remains the only way anything leaves it, and it is
   still a boundary a human physically crosses.

## Alternatives

**A `DraftPrompt` wire command, with the prompt in `mogeungd`.** The shape
`R-O3`'s reading guide uses, and the one that fits *"the daemon is the product;
every UI is a client with no local authority"* best — a second client would get
the drafting for free rather than reimplementing the meta-prompt. Rejected on
ADR-0031 clause 2: it is a second free-form family on a protocol that carries
ids, and the refusal that protects the first would have to be written again for
this one, in a place where forgetting it is silent. The cost is real and is
accepted: a second client has to compose its own ask, or this moves to the
daemon in a later ADR when there is a second client that wants it.

**Leave `R-O7` unbuilt to keep ADR-0008's sentence true.** Honest, and it was
considered rather than dismissed — that sentence bought something. Rejected
because what it bought was *no record*, and no record is a property of the
**store**, which clause 2 keeps: nothing is written down. What is given up is
that the text never left the client at all, which was never the promise
ADR-0008 was making to a user. The promise was that mogeung does not put words
into an agent's input, and that is untouched.

**Draft from the client's own endpoint.** No daemon involvement, so ADR-0008
survives intact. Rejected as ADR-0030 clause 1 rejected it: the corpus and the
endpoint live on the daemon's machine, and a window watching a Mac from a Linux
desk would silently draft against the wrong box, or against nothing.

**Send the draft, since a model wrote it and a human read it.** Rejected, and
named here because this is where it will be proposed. It is ADR-0003, and a
better draft is an argument for the clipboard rather than against it.

## Consequences

**Good.** The paste is an instruction rather than a pile of quotes, which is
what a reviewer meant when they flagged three hunks in the first place. It cost
no wire surface, no daemon code and no new refusal — every gate the chat panel
already passes through is the gate this passes through.

**Bad — the flagged text now leaves the client.** ADR-0008's boundary was that
it did not, and that is gone. It goes to the daemon, and from there to whatever
`model_url` names, which with mogeung's own llmproxy in front
([ADR-0033](0033-a-proxy-of-our-own.md)) may be a vendor. The disclosure is the
one that clause 6 of that ADR left: the chat panel's admin-button tooltip names
the hosts. That is thinner than it should be for a surface that carries **diff
lines**, and it is recorded as a known weakness rather than as a solved problem.

**Bad — the meta-prompt lives in the window.** It is TypeScript that no Rust
harness can grade, which is exactly the split `R-O2` refused for the reading
guide. `R-O7` rests on no untested assumption, so there is no harness to share
with — but if this ever grows one, the prompt has to move to the daemon first.

**Bad — the pressure is higher than it was.** A good draft makes *send it for
me* a more reasonable-sounding request than a concatenation ever did. Clause 6
is the answer and it is deliberately boring.

## Revisit if

A second client wants the draft, at which point the meta-prompt should move
into `mogeungd` and the wire question reopens — with the answer being to make
`ModelChat` carry a purpose rather than to add a family beside it.

Or if `A37` removes the chat panel. The draft ask travels through chat's door,
so removing the panel must not remove the door; if that happens, this decision
is what says the seam stays.

## Amendment — 2026-08-29: the window offers a second action, and it is send

**Clause 6 above and the *Ruled out* line change.** Clause 6 said *still exactly
one action: copy*, and the ruled-out line said *any code path that sends text to
a session, by any mechanism*. Both were carried forward from ADR-0008 and both
were asked about the same day this ADR was written: *"can I directly send it to
the sessions associated…"*

What replaces them: **the window offers two actions — copy, and send to the
session the flags came from, on a click and a confirmation.** The argument, the
fences and what it costs are in
[ADR-0003's 2026-08-29 amendment](0003-observe-do-not-spawn.md#amendment--2026-08-29-a-human-may-press-send),
which is where *steers* is defined; they are not repeated here.

**The title above is now historical** — it says *it still leaves only by the
clipboard*, which was true when this was decided and is what this document is
for: showing what was believed then. It is left as written rather than tidied,
which is the whole of the amendment convention.

**Everything else on this page stands**, and clause 7 of that amendment is this
one restated: drafting and sending are two buttons with the text on screen in
between, so a model still never composes-and-delivers in one gesture. Clauses 1
to 5 above — the chat door, the daemon keeping nothing, the raw text one click
away, the clipboard copying what is on screen, and the draft being asked for
rather than automatic — are untouched.

---
*ADRs are immutable. A decision that is **narrowed** changes by an
`## Amendment — YYYY-MM-DD` section appended here, with `updated:` bumped. A
decision genuinely **reversed** is superseded: write a new ADR and set
`status: superseded` plus `superseded_by:` here.*
