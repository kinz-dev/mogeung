---
title: A tmux pane running an agent is a session, provisionally
status: active
updated: 2026-09-10
decided: 2026-09-10
---

# ADR-0038 — A tmux pane running an agent is a session, provisionally

## Context

Every session mogeung knows about is discovered from **the agent's own
bookkeeping**: Claude Code's `~/.claude/projects/**/*.jsonl` and its
`sessions/<pid>.json` registry, Codex's rollout files and index, Qwen's
equivalents. That is the right primary source — it is what carries the
conversation — and it has one failure that no amount of parsing fixes.

**An agent that is running and blocked before it writes anything is invisible.**
`R-J74` found the specific case: Codex asks *"do you trust the contents of this
directory?"* the first time it works in one, and opens no thread until you
answer. Started headless, the question has nowhere to appear, so a real agent
sits in a real tmux pane, waiting for a keystroke, and the board correctly shows
nothing. The same shape covers a login expiry, a first-run migration, an
interactive upgrade prompt, and anything a future CLI invents.

The pane is right there and mogeung can already host it — `tmux_panes()`
enumerates every pane, `is_agent()` already knows which programs are agent CLIs
(it is what ADR-0025 uses to refuse *starting* one), and the Agent pane attaches
by tmux target. What is missing is something to hang a row on.

**The hard part is identity, which is why this is an ADR and not a patch.** A
pane has no session id. Give it one and two problems follow: the row must not be
counted twice when the real session finally appears, and it must not read as a
different session when it does.

## Decision

**A tmux pane whose process tree contains an agent CLI is published as a
session, marked provisional, identified by the pane.**

1. **The identity is tmux's own `pane_id`**, prefixed: `pane:%12`. tmux
   guarantees `%12` unique within its server and stable for the life of the
   pane. The prefix makes it unmistakable for a real session id, which is a
   UUID from a CLI that has never heard of tmux.

   **Not the attach target.** `name:0.0` is what everything else here passes
   around, and it is exactly wrong as an identity: window and pane indices
   renumber when a window is closed, so a target is a *location* and identities
   must not be locations. That is the same distinction ADR-0015 draws about
   notes, one subsystem over.

2. **Reconciliation is by pane, not by name.** Each scan already resolves a live
   session's pid to its pane, walking process ancestry. A provisional session
   whose pane is claimed by a real session is **not published**. There is no
   merge step and no alias table: the real session simply wins, in the same
   snapshot, and the provisional was never anything a client had to unlearn.

3. **A provisional session is never persisted.** It exists only in the snapshot
   and is recomputed from tmux each scan. Nothing in SQLite refers to it, so
   there is nothing to migrate when it reconciles and nothing to orphan when the
   pane closes. It is a **view of a running process**, which is what it is.

4. **It carries only what the pane can prove**, and says so: `provisional:
   true` on the wire. Its cwd is the process's, its `tmux_target` is the pane's,
   its source is read from the program name, and its counts are zero — because
   they are, not because we have not looked. A window that renders zero tokens
   as a fact about a conversation would be lying; one that says *running,
   nothing written yet* is not.

5. **It is still never steered.** ADR-0003 is untouched: adopting a pane means
   showing it and attaching a terminal to it, exactly as for any session. The
   thing that unblocks a trust prompt is you, typing in the pane mogeung put in
   front of you.

## Alternatives

**Publish it as a notice rather than a session.** Honest about what it is, and
it loses the only thing that makes it useful: a notice cannot be opened,
attached to, or acted on. The whole value is that the pane is hostable and the
user needs to reach it.

**Identify it by the attach target.** Fewer moving parts, and wrong for the
reason above: targets renumber. A row whose identity changed when an unrelated
window was closed would be a new session appearing and an old one ending, which
is worse than not showing it at all.

**Identify it by the pane's pid.** Stable and tempting. Rejected because the
interesting pane is often a shell that later *becomes* the agent, and because a
pid is reused by the operating system — rarely, and catastrophically when it
happens, since the row would silently point at somebody else's process.

**Match a provisional to a real session and rewrite the row's id.** The obvious
"proper" answer, and it buys nothing here: it needs an alias table, it makes
every consumer of a session id ask which one it has, and the only thing it
saves is a row briefly changing rank. Not worth a second kind of identity.

**Scan `ps` for agent processes rather than tmux panes.** Wider — it would catch
an agent in a plain terminal too — and it cannot be acted on: without a pane
there is nothing to attach, so the row would say *something is running
somewhere* and offer no way to reach it. `R-I13`'s container case is the same
question and has its own row.

## Consequences

**Easy.** The failure `R-J74` had to explain in a dialog now shows up as a row:
an agent waiting on a trust prompt, a login, or a migration is on the board with
a terminal one click away. `A1` — the queue tells you where to look — stops
being false in the one case where the agent cannot speak for itself.

**Hard, and stated rather than hidden.** *A provisional row changes rank when it
becomes real.* It is ranked on almost no evidence — a running process and
nothing written — and the real session is ranked on its own. The two happen in
one snapshot, so it reads as the row becoming itself rather than moving, but it
is a jump and no design here removes it.

**The cwd is not always knowable.** `process_cwd` reads `/proc/<pid>/cwd`, which
Linux publishes and macOS does not. On macOS a provisional session has no cwd
until the agent writes one, so it cannot be grouped by repository. The honest
alternative was forking `lsof` per pane per scan, which is a cost paid on every
scan for a case that resolves itself.

**A pane running an agent under a different name is missed**, exactly as
ADR-0025's refusal is: `AGENTS` matches the program, so a wrapper script that
calls `claude` walks past. Same list, same limit, and deliberately the same list
rather than a second one that could drift from it.

**More rows, some of them noise.** Every pane with an agent in it becomes a row,
including ones you are actively typing in and do not need told about. The
mitigation is that a real session supersedes it within a scan, so the noise is
bounded by *how long an agent runs before writing* — which is seconds, except in
precisely the stuck case this exists for.

## Revisit if

- **Provisional rows outnumber real ones in ordinary use.** That would mean
  agents routinely run for a long time before writing, and the right answer
  would be a delay before publishing rather than more filtering.
- **Something wants to persist against a provisional id** — a note, a bookmark,
  a held pane. Clause 3 says they cannot, and the day something needs to is the
  day this needs an alias after all.
- **A second adoption source appears** (`R-I13`'s containers, `R-I14`'s pods).
  The identity rule here is tmux-shaped; a container has no pane, and pretending
  otherwise would be the location-as-identity mistake in a new place.
