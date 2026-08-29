---
title: A human may press send; mogeung still never owns the loop
status: active
updated: 2026-08-29
decided: 2026-08-29
supersedes: [ADR-0003, ADR-0034]
---

# ADR-0035 — A human may press send, into a session a human named

## Context

[ADR-0003](0003-observe-do-not-spawn.md) is the decision this project was
rebuilt around, and its sentence is *"mogeung observes. It never starts, steers
or stops an agent."* [ADR-0008](0008-build-the-prompt-never-send-it.md) — since
[ADR-0034](0034-the-draft-is-a-chat-ask.md) — drew the matching line for text:
mogeung composes a follow-up prompt and puts it on your clipboard; you paste it.
Both of them predicted this request by name. ADR-0008: *"'just paste it for me'
is one keystroke from 'just send it', which is one step from owning the session
again."*

It was asked for on 2026-08-29, the day `R-O7` made the paste worth wanting:
*"can I directly send it to the sessions associated, without manually copy to
clipboard and then paste it into the agent session manual step?"* — with the
argument attached: **it is still a human trigger; mogeung is not owning the
session.**

That argument is right, and the fact that the refusal predicted the request is
not by itself a reason to keep it. What ADR-0003 actually recorded was a
**failure**, and the failure was specific: v0.1 removed the interactive loop —
no steering, no permission prompts, no plan mode, no slash commands — so each
session became worse than just running `claude`. The lesson drawn was *a
supervision layer must be additive*. A one-shot delivery of text a human wrote,
read, and pressed a button to send, into a session that same human is running
in their own terminal, removes nothing from that session. The terminal is still
the front-end. mogeung is still not in the conversation.

Two things changed since ADR-0008 that make this buildable rather than merely
arguable:

**The mechanism is no longer keystroke injection.** ADR-0008 rejected
`osascript` keystrokes into whatever happens to be focused — *"a footgun with no
good failure mode"* — and it was right. [ADR-0010](0010-attach-a-terminal-never-own-one.md)
then made mogeung resolve **which tmux pane belongs to which session**, by
walking process ancestry, so the Agent tab can attach. Delivery can therefore
name its target: `tmux paste-buffer -p -t <pane>`, into that session's pane and
no other, with no dependence on focus and no keystroke synthesis at all.

**The window is already where the prompt is written.** `R-O7` composes the
flagged hunks into an instruction and shows it. The clipboard step is now a trip
between two panes of the same window.

## Decision

**mogeung may deliver text into one session's own tmux pane, on a click, after
a confirmation, and never any other way.**

Everything ADR-0003 decided stands and is carried forward here verbatim:

> mogeung observes. It never **starts** or **stops** an agent. It reads what
> Claude Code already writes for itself — the live registry and the transcript.
>
> The first exception: mogeung may open a **real interactive `claude` in a
> terminal**, optionally in a fresh worktree. That wraps nothing.

The word that changes is **steers**, and it is replaced by a definition rather
than dropped. Steering is **owning the loop**: being in the path between a human
and an agent, or being in the path between an agent's output and its own next
input. Neither happens below.

The second exception, and its fences:

1. **A human clicks, and then confirms.** The confirmation names the session and
   shows the first line of what will be sent. Nothing is ever sent by a timer, a
   rule, a scan, an agent, or a second window. There is no API for it that is
   not a person pressing a button twice.

2. **Exactly the text on screen, into exactly one session.** The text is the one
   the prompt window is showing, which the human has read; the target is the
   session whose diff the flags came from. Flags spanning two sessions offer no
   send at all — a message with an ambiguous recipient is one the clipboard
   should carry.

3. **One shot, and no reading back.** mogeung does not read the agent's reply,
   does not decide anything from it, and never sends a second message on its
   own. **There is no code path from an agent's output to an agent's input.**
   That path is what v0.1 was, and it stays forbidden — this ADR narrows one
   sentence of ADR-0003 and leaves its actual finding untouched.

4. **Loopback only, with no flag.** A daemon reachable beyond loopback refuses
   this outright, as it refuses the chat. The consequence here is worse than the
   chat's — that risk is somebody else's tokens, this one is somebody else typing
   into your agents — so the refusal is the same shape and is not negotiable by
   configuration.

5. **Only a session mogeung can name a pane for.** Sessions started with
   `yolomo` run under tmux and have a target; a session started in iTerm2 does
   not, and the button is absent with the reason on it. That is ADR-0010's
   boundary reused, not a new one.

6. **The clipboard stays, unconditionally.** Copy remains the first action in
   the window and works for every session, including the ones that cannot be
   sent to. If sending were ever the *only* route, mogeung would have become the
   front-end by attrition rather than by decision.

7. **The draft is not sent by the gesture that made it.** Drafting and sending
   are two buttons, and the draft is on screen in between. A single click that
   composed text with a model and delivered it to an agent is the thing this
   whole ADR is trying not to be.

## Alternatives

**Keep the clipboard boundary.** What stood until today, and its argument is not
wrong — it is a boundary a human physically crosses, and no confirmation dialog
is quite as unambiguous as that. Rejected because the cost it charges is now
paid on a trip between two panes of one window, and because the argument had
started to rest on *the boundary is the point* rather than on the failure the
boundary was protecting against. A fence that has forgotten what it encloses is
how a rule becomes a superstition.

**Paste without pressing Enter.** Recommended when this was asked, and refused
deliberately: the text lands in the agent's input and the human presses Enter in
the Agent tab, so the commit is unambiguously theirs. It is the safer shape —
see the hazard below — and it was rejected because it does not remove the step
that was complained about, it only moves it. The confirmation carries that
weight instead.

**Type into the tty with `osascript` / `TIOCSTI`.** ADR-0008 and ADR-0010
rejected both, and this ADR does not revive them. `TIOCSTI` returns `EPERM` off
the controlling terminal and is disabled on Linux; keystroke synthesis cannot
name its target. Neither is needed once a pane id exists.

**Write to a file the agent is told to watch.** ADR-0008's rejection stands
unchanged: it requires changing how you run `claude`, which breaks the *your
sessions are untouched* property that makes the observer model worth having.

**A reply box in mogeung — a real conversation.** Rejected, and this is where
the line now sits. A box you type into, that shows you the answer, that you type
into again, is the interactive loop, and mogeung would then be a worse terminal
in front of a good one. That is v0.1's failure exactly, and clause 3 is what
keeps this from arriving there one convenience at a time.

## Consequences

**Good.** The review→instruct loop closes inside one window: read the diff, flag
what matters, draft the instruction, send it to the session it came from. The
part of mogeung nothing else can do — the flags, the diff, the conversation
beside them — now ends in an action rather than in a clipboard.

**Bad — and this is the real cost: mogeung cannot see the session's screen.**
Permission prompts, multiple-choice questions and plan-mode approval are TUI
rendering and never reach the transcript ([feature 0003](../features/0003-attached-terminal.md)
is founded on that fact). So an Enter delivered on a human's behalf can land on
a menu nobody read. The confirmation is the mitigation and it is not a
guarantee: it says *what* is being sent and *where*, and it cannot say what the
screen is showing. **Bracketed paste** narrows it — the whole text arrives as
one block rather than as lines that could each answer something — and the
residue is accepted knowingly rather than discovered later.

**Bad — the pressure moves up one step.** The next request is *send without
confirming*, then *send automatically when I flag*, then *show me the reply
here*. Clause 3 is the one that matters and it is the one to defend: the reply
never comes back into mogeung's hands.

**Bad — two ADRs are now superseded by one**, and ADR-0003 is the project's
keystone. Its finding is carried forward intact and is still the thing to read
first; what changed is one word in it, and this ADR exists so that the change is
a decision with a date rather than a fence quietly eroded.

**Ruled out, still:** any path from an agent's output back to an agent's input;
any send that a human did not trigger and confirm; any send off loopback; and a
conversation loop in mogeung by any construction.

## Revisit if

The confirmation is being clicked through without being read — which is what a
dialog becomes when it is frequent — or if a send ever lands somewhere it was
not aimed. Either means the fence is decoration, and the honest response is to
go back to the clipboard rather than to add a second dialog.

Or if Claude Code grows a documented, sanctioned way to enqueue a prompt (a CLI
subcommand or a local IPC socket you opt into — ADR-0008 named this and it is
still the better mechanism than a pane id). Then this should be rebuilt on it,
because a first-party queue can be delivered to without touching a terminal at
all, and the TUI hazard above disappears with it.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
