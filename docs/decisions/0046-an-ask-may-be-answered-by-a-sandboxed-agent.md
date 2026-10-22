---
title: An Ask may be answered by an agent mogeung starts, inside a sandbox it cannot leave
status: active
updated: 2026-10-22
decided: 2026-10-22
---

# ADR-0046 — An Ask may be answered by an agent mogeung starts, inside a sandbox it cannot leave

## Context

Reported 2026-10-22, about Ask Mogeung (`R-O5`): asked to summarise a blog
post from its link, the panel answered *"I cannot read or access the content
of that URL … the URL contains a future date"*. The same question to the
owner's own `opencode`, on the same model, read the page and summarised it.

The model was the same, so the gap is in what it was given. opencode hands
the model a `webfetch` tool and runs the loop that calls it. Ask sent the
conversation to `/chat/completions` with no tools and **no system message at
all**, so the model could not read a page and did not know the date, which is
where *"a future date"* came from.

Two answers were weighed (feature 0053). In the first, the daemon fetches the
links you paste and hands the text to the model, with no agent involved. In
the second, the Ask is answered by opencode itself. The owner chose the
second, *"because I can get all the opencode agent capability"*, and asked
what could be done about the risk that an agent *"can run program and do
other stuff"*.

That choice crosses three lines that were drawn on purpose:

- [ADR-0025](0025-run-a-process-you-named-never-an-agent.md) clause 2 says the
  daemon never starts an agent, narrowed on 2026-10-09 to *except a seat*.
- [Pillar O](../product/roadmap.md#o-a-local-model-beside-the-agents) lists
  *an agentic loop* among the things it refuses.
- [ADR-0031](0031-consent-to-a-named-host.md) clause 1 says a model writes
  nothing outside `~/.mogeung`. opencode's default `build` agent resolves to
  `* → allow`: bash, edit and write, anywhere the user can.

What made it answerable was measuring rather than arguing (feature 0053,
*Notes*). On this machine, opencode 1.19.2 ran with:

- a config passed in the environment;
- HOME and every XDG directory moved;
- a bubblewrap sandbox that is read-only everywhere and has an empty tmpfs
  over `/home`, `/tmp`, `/run`, `/var`, `/mnt` and `/media`.

Run like that, opencode fetched a page and answered. With bash deliberately
allowed:

- `touch` outside the sandbox failed with *Read-only file system*;
- `~/.zshrc` was *No such file or directory*;
- the session bus and the ssh-agent socket were gone.

Before `/run` was hidden, the owner's `docker.sock` was connectable. A
read-only mount does not stop `connect()` on a socket, and membership of
`docker` is root. That one finding is why this ADR lists what is hidden
rather than saying "a sandbox".

## Decision

**An Ask may be answered by opencode, started by the daemon, for that one
question, inside a sandbox the kernel enforces. Everything else in pillar O
stays as it was.**

1. **Only the Ask panel, and only what you typed.** The agent path is taken
   when the panel asks for it (`model_chat` with `agent: true`). The draft,
   ask-the-diff, the command line and the handover never take it. Those carry
   mogeung's evidence (transcripts, diffs, notes), and an agent with the
   network must never be handed the corpus. A page it reads can tell it to
   fetch a URL with your data in the query string, and the only thing it then
   has to leak is the conversation you typed.
2. **The kernel is the fence, and opencode's own permissions are not.** The run
   is `bwrap` with the root bound read-only, a fresh pid namespace,
   `--die-with-parent` and `--clearenv`, and an empty tmpfs over `/home`,
   `/run`, `/var`, `/mnt`, `/media` and `/tmp`. Only DNS's
   `/run/systemd/resolve` and the opencode binary are mounted back. The one
   writable place is a size-capped tmpfs, and the agent's HOME, XDG
   directories and working directory all live in it. When the process ends,
   all of it is gone: **nothing an Ask does is written to disk.** The network
   is shared, because reading the web is the point.
3. **opencode reads nothing of yours.** Its config arrives in
   `OPENCODE_CONFIG_CONTENT`, built per ask from the daemon's own model
   settings (the endpoint the chat would have used, including mogeung's
   llmproxy). It runs with `--pure`, and with model fetching, autoupdate,
   project config, external skills and Claude Code compatibility switched
   off. Your opencode config, plugins, MCP servers and skills are not loaded,
   and [ADR-0043](0043-a-seat-is-configured-at-launch-never-in-your-home.md)
   holds: nothing is written in your agent homes.
4. **Full tools, confined; no web search.** Inside the sandbox the agent may
   use any tool: bash, edit, write and webfetch, each confined by clause 2.
   `websearch` is denied, because it sends the query to a third party (Exa).
   That needs consent naming that host, and none is asked for here.
   `question` and `doom_loop` are denied because nobody is there to answer
   them, and the run has a step cap and a wall-clock limit.
5. **Linux first, and fail closed.** No `bwrap`, a `bwrap` that cannot start
   (AppArmor, no user namespaces), no `opencode`, or any platform but Linux
   means the Ask is answered as plain chat, and the reply says why. **The
   agent never runs unsandboxed**, and there is no setting that makes it.
   `ask_agent = false` in the config file turns it off.
6. **Output is still a column.** What the agent says is model output under
   ADR-0031 clause 5. Its tool steps are shown under the answer as what it
   did, never as evidence. It has no path into any session's input. It is not
   a seat, has no pane, and never appears on the board: its opencode session
   lives and dies in the tmpfs, outside any directory mogeung watches.
7. **The ask is not the agent's to keep.** The whole conversation travels on
   every ask, as it does for the chat. It is rendered into the message, which
   goes in on **stdin**, so neither argv limits nor quoting are part of it.
   Each ask is a fresh run. The kept history (ADR-0032) holds what you asked
   and what it answered, not its tool calls.

ADR-0025 clause 2, ADR-0003 and pillar O's refusal are amended to point here.

## Alternatives

**The daemon fetches the links you paste, and no agent runs.** It was
recommended, and it is still the narrower answer. The model never chooses a
URL, so nothing it reads can make it send anything. It works with models whose
tool calling is unreliable. It needs no new kind of process. It lost on reach:
it reads a page and does nothing else, and the owner wants opencode's whole
toolbox. If the agent path is withdrawn, this is the fallback, and it needs
no ADR beyond a line about fetching.

**A checkbox choosing the agent or plain chat.** It has all of this ADR's
costs plus a mode to remember, and you find out you chose wrong from the
refusal. Rejected. The daemon decides from what it can actually do, and says
so.

**opencode's permissions alone, with no sandbox.** It is cheaper and portable.
It is also opencode policing itself: one prompt injection, one missed tool
name, one bug in its glob matching, and it can write in your home. Rejected
as the fence. It is kept as the layer above the fence.

**Bash denied, webfetch only.** It is safer on paper. Inside clause 2's
sandbox, the bash it denies can touch nothing a page fetch cannot already
reach. The owner chose full-but-confined.

**A network namespace with a filtering proxy**, which closes the LAN and the
abstract-socket namespace too. That is the right next step if either becomes
a real exposure. It is not done now: it needs a forwarder inside the
namespace, and nothing reachable over either path was found here (the X
server refuses without its cookie, and the cookie is hidden).

**A long-lived `opencode serve` inside the sandbox**, for speed and real
multi-turn sessions. It is deferred. A trivial ask costs about 6 s cold, which
is tolerable, and one sandbox per ask is easier to reason about than one that
outlives the question.

## Consequences

**Easy:** Ask reads pages, runs a calculation and checks its own work, with
the same model, through the same endpoint and consent, and the panel shows
what it did.

**Hard:** mogeung now starts a third kind of process: neither a run you named
(ADR-0025) nor a seat (ADR-0003), but a disposable agent with a question.
`A24`'s *read-only* is unaffected, because the chat is loopback-only and no
LAN client can reach this path. An agent ask is slower than a streamed
answer, roughly 6 s for a trivial one and 10–30 s with a tool step, and its
text arrives in whole paragraphs, not tokens. opencode's `--format json`
stream is one more undocumented format (`A4`), with a parser that degrades.

**What stays exposed, said plainly:**

- **The conversation itself.** A page can inject an instruction and the agent
  may follow it, but only what you typed is there to send.
- **The LAN.** It can GET a host on your network.
- **Abstract unix sockets.** They are shared with the network namespace.
  Today only an X server that refuses without auth was listening.
- **CPU and memory** are bounded only by the time limit and the tmpfs cap.

**Ruled out:**

- an agent run without the sandbox;
- an agent handed mogeung's corpus;
- the agent answering anything but the Ask panel;
- web search, without an ADR that names its host.

## Revisit if

- **macOS is wanted.** `sandbox-exec` is the equivalent, and it needs its own
  probe of what it hides before this ADR's clause 2 can be claimed there.
- **A secret is found reachable from inside**, by `askagent`'s containment
  test or by anyone.
- **The kept history should carry the tool steps.**
- **A second panel wants the agent.** Clause 1 is the line, and moving it is
  the decision.
