---
title: mogeung builds follow-up prompts but never sends them
status: active
updated: 2026-07-25
decided: 2026-07-25
---

# ADR-0008 — mogeung builds follow-up prompts but never sends them

## Context

Reviewing produces intent. You read three hunks, decide two of them need better
error handling, and now you want to tell the agent. Today that means switching
to the terminal and retyping from memory what you just read — which is where
review notes go to die.

The obvious feature is "reply from mogeung": a text box, and the daemon writes
into the session. It is also the exact thing that killed v0.1.

[ADR-0003](0003-observe-do-not-spawn.md) records that failure: to feed the
attention queue, v0.1 owned the conversation loop, which made every individual
session worse than just running `claude`. The lesson was not "spawning was a bad
implementation" but "**a supervision layer must be additive**" — anything that
inserts itself between you and the agent has to earn back what it takes away,
and a layer that only observes cannot lose that trade.

The pressure to re-acquire the loop will keep coming back, because each
individual step looks reasonable. "Just paste it for me" is one keystroke from
"just send it", which is one step from owning the session again. The roadmap
already names this: pillar `K` rules out *anything that re-acquires the
conversation loop*.

## Decision

**mogeung composes the prompt text and puts it on your clipboard. You paste it.**

Flagging a hunk while reading collects it — path, hunk header, the changed lines
and an optional per-hunk note. The prompt window renders those into text you can
edit, and offers exactly one action: copy.

There is no wire command for it. `FlaggedHunk` lives entirely in the client;
the daemon never learns a prompt was written. The clipboard is the boundary, and
it is a boundary the user physically crosses.

## Alternatives

**Type into the session's terminal** (via `osascript` keystrokes into the tty).
Mechanically easy and the most requested shape. Rejected because it is steering
with extra steps: mogeung would be putting words into an agent's input, which is
the thing ADR-0003 forbids. It also cannot be made safe — keystroke injection
into whatever happens to be focused is a footgun with no good failure mode.

**Write to a file the agent is told to watch.** Rejected as the same thing
wearing a disguise, plus it requires changing how you run `claude`, which
breaks the "your sessions are untouched" property that makes the observer model
worth having.

**Do nothing; let the user retype.** The honest baseline, and what v0.2 did.
Rejected because the information is right there and losing it is a real cost —
this is the one place where a purely observational tool can help without
touching anything.

**Put it on the clipboard automatically when you flag a hunk.** Rejected as
rude: the clipboard is shared state the user did not ask us to take.

## Consequences

**Good.** The review→instruct loop is closed without owning the conversation.
Quoting the actual diff lines means the agent gets the hunk rather than a
description of it, which is the difference between "fix the error handling in
state.rs" and a specific ask. And it composes across sessions — you can flag in
one and paste into another.

**Bad — it is two steps, forever.** Copy, switch, paste. That friction is
permanent and deliberate, and it will feel silly the tenth time. If it is ever
removed, this ADR should be superseded explicitly rather than eroded.

**Bad — no record.** Since the daemon never sees the prompt, mogeung cannot
later tell you what you asked for or whether the agent did it. That is a real
loss for the verification pillar (`R-E3`, claim ledger), which would want the
ask and the outcome side by side. The trade is accepted: verification can read
the *transcript*, which contains the prompt once you paste it.

**Ruled out:** any code path that sends text to a session, by any mechanism.

## Revisit if

Claude Code grows a documented, sanctioned way for an external tool to enqueue a
prompt — a CLI subcommand or a local IPC socket the user opts into. The
objection here is not "sending text is wrong", it is "there is no way to do it
that does not mean owning the loop or injecting keystrokes". A first-party
mechanism would remove that objection, and the decision should then be retaken
rather than assumed.

Note that `queue-operation` events in the transcript suggest the CLI already has
an internal notion of queued prompts. If that ever becomes public, this is the
first thing to reconsider.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
