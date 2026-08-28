---
title: The chat panel keeps its conversations, and the daemon is where they live
status: active
updated: 2026-08-28
decided: 2026-08-28
---

# ADR-0032 — The chat panel remembers

## Context

`R-O5` shipped the chat panel five hours before this was asked, and it shipped
with a property stated in three places — the wire doc, the panel's own header,
and `ChatTurn`'s doc comment:

> The daemon stores none of these: the conversation lives in the window and is
> sent whole on every ask, which is what makes `R-O5` **ephemeral by
> construction** rather than by a promise to delete something.

That was not an accident of the first cut. It was the cheapest possible answer
to the fact that this panel is the one place in mogeung that carries a
free-form string, that `A37` is `UNTESTED` and the feature is built to be cheap
to remove, and that the corpus this product already holds is 67 MB of
transcripts nobody wants a second copy of. *No table to forget* is a real
property, and it is the one being given up here.

Asked 2026-08-28: *"add a button to start a new conversation. Add a
conversation history view so that I can find the old conversation."*

The request is ordinary and the reason it is being written down is not. A
history means the questions you type into this panel are on disk. Some of them
will be pasted stack traces; some will contain a token somebody was debugging.
Under the old design there was nothing to leak and nothing to delete. Under
this one there is, and the difference deserves to be a decision rather than a
side effect of a feature request.

## Decision

**The daemon keeps every answered exchange, against a conversation id the
window mints on the first question. The history is daemon state, refused where
chat is refused, capped, deletable one row at a time, and switchable off in the
config file.**

1. **The daemon keeps it, not the client.** Same reasoning as
   [ADR-0015](0015-markdown-is-the-truth.md) for notes and
   [ADR-0031](0031-consent-to-a-named-host.md) clause 2 for the endpoint: the
   conversation is about the machine the corpus is on, a second window must see
   the same history, and a client with local authority over durable state is
   the thing this codebase has never had. It lives in the store's `chats`
   table, in `~/.mogeung`, which clause 1 of ADR-0031 already permits.
2. **Only answered exchanges are kept.** A refusal, a dead endpoint or a
   timeout writes nothing. This falls out of where the write is — after a
   successful reply — and it matches the rule the client already had: a failed
   exchange stays on screen and is not sent back as context. A half-thread on
   disk would be a question nobody answered, presented later as if it had been.
3. **Refused wherever the ask is.** ADR-0031 clause 4 refuses `model_chat` on a
   bind beyond loopback because it is free-form text. The history is the same
   text, kept; a daemon that will not take a question has no business handing
   back the last two hundred. No flag, for the same reason there is none there.
4. **Capped, and pruned by `updated`.** Two hundred conversations, oldest
   *touched* first out. A note is written on purpose, one at a time, and is
   kept for ever; a conversation accumulates simply by using the panel, so it
   is kept like a note and pruned like a log. Pruning by `updated` rather than
   `created` means a thread you came back to yesterday outlives one abandoned
   last month.
5. **Forgetting is explicit, per row, and the only thing that deletes.** The
   panel has three gestures that look alike and are not: *new* makes the window
   forget which thread it is in, *clear* empties the panel and stays in the
   thread, and the ✕ in the history is the one that removes something from
   disk. Each says which it is.
6. **`chat_history = false` is the way back.** The file turns the tap off and
   the daemon answers every ask and keeps none of it, exactly as `R-O5`
   shipped. It does **not** empty what is already there: turning off recording
   and deleting a history are two different intentions, and a setting that did
   the second when you asked for the first is the worst kind of surprise.

## Alternatives

**Keep it in the client.** `localStorage` via `prefs`, no protocol work, and
the panel stays a thing the daemon knows nothing about. Rejected on the same
ground ADR-0015 rejected it for notes: it is per-window and per-browser-profile
state that survives nothing, a second window would show a different history,
and it would be the first durable thing a client owned. It also puts the text
somewhere the user cannot find, delete, or back up, which is worse for privacy
rather than better despite feeling otherwise.

**Keep nothing, and lean on `R-L2`.** *Copy the thread into a note* was the
first cut's honest answer and it is still the right gesture for keeping
something **on purpose**. Rejected as the answer to *this* ask because it
requires knowing in advance that a conversation will matter, and the request is
specifically about finding one you did not know that about at the time.

**A row per turn.** Normalised, and the shape a schema reviewer reaches for.
Rejected: the conversation is read whole, written whole and sent whole — the
wire has never carried half of one — so the common write (append one exchange)
would be a delete-and-reinsert anyway, and nothing would ever query a turn.

**Ask before keeping.** A prompt on the first question, or a per-conversation
*keep this* toggle. Rejected as the default because it makes the feature
useless for its purpose: you do not know which conversation you will want to
find again, which is the whole reason to have a history. The config key is the
same control moved to where it costs nothing to leave alone.

## Consequences

**Easier.** A question asked on Tuesday is findable on Friday. Opening an old
thread continues it rather than forking a copy, so the model gets the context
it already had. And a second window — or the same window after a restart —
sees the same list, because the daemon is the one holding it.

**Harder, and this is the cost being accepted.** What you type into this panel
is now on disk in `~/.mogeung/mogeung.db`, in plain text, until the cap pushes
it out or you delete it. That includes anything you pasted. Mitigated by
per-row deletion, the cap, the loopback refusal and the off switch — but not
eliminated, and it is no longer true that there is nothing to forget.

**Ruled out.** Search across conversations (that is `R-O6`'s problem, over a
much larger corpus, and doing it here first would build the wrong index),
sharing a conversation between machines, and any automatic use of past
conversations as context for a new one. The model sees exactly the thread you
are in, as it always did.

## Revisit if

- `R-O2`'s harness or `A37`'s fortnight says the panel is not used — the
  history goes out with it, and the cap means there is a bounded amount to
  delete rather than an unbounded one.
- Someone wants the questions but not the answers kept, or a retention in days
  rather than conversations. Both are the same shape as clause 4 and neither
  needs a new decision, only a number.
- `R-O6` lands semantic search, at which point *find the conversation where I
  asked about X* is a better question than *scan the titles*, and the list
  becomes a fallback rather than the way in.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
