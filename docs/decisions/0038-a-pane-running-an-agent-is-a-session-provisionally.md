---
title: A tmux pane running an agent is a session, provisionally
status: active
updated: 2026-10-09
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

## Amendment — 2026-09-23

**Clause 4 is widened by one CLI: a provisional opencode row carries a status,
*waiting for you*.** What changed, why, and what it replaces.

*What.* A provisional session is still recomputed, never persisted, and
identified by its pane (clauses 1–3 untouched). But for opencode —
`R-I25`, [feature 0048](../features/0048-opencode-adapter.md) — the row is
published with `live_status: Idle` and named *waiting for your first
prompt*, so attention ranks it **WAITING**, and its *waiting since* clock is the
opencode process's own start time rather than the scan's. Every other CLI's
provisional row is unchanged, and the choice is an exhaustive `match` on the
source so the next CLI has to make it.

*Why this is still "only what the pane can prove".* opencode writes its
session row the moment a prompt is sent, and the opencode pass then claims the
process for that session by its directory and start time — which suppresses
this row, by clause 2. So an opencode whose pane is still unclaimed **has sent
nothing**: it is at its prompt. That is not a guess about what the agent is
doing; it is what the absence of a row, from a CLI that always writes one when
spoken to, means. The Codex pass makes the same call for a thread nobody has
spoken to (`R-J73`), from its lock.

*Why it mattered.* Left as clause 4 first had it, the row carried no status
and ranked **running** — and the default *needs you* scope hides running rows
until they have been silent for five minutes. A headless opencode started from
the New session window therefore appeared nowhere the owner looked, though it
had been on the board the whole time. Reported 2026-09-23: *"the window
closed. But nothing appear in the ATTENTION list."* For the other CLIs the gap
is a moment, not a state — each writes a registry record or a lock before any
prompt, which its own pass turns into a real session that claims the pane.

*What it replaces.* The sentence in clause 4 that a provisional row's
counts are zero *"because they are"* stands; the implication that it says
nothing about status does not, for opencode.

*And a second change the same day, to clause 3.* Clause 3 said a provisional
session *"exists only in the snapshot"*, and the code took that literally: a
pane row was sent to a window in the snapshot it got on connecting, and in no
broadcast after — not even in the queue broadcasts, which ranked stored
sessions only. So a window that was already open when an agent started in a
pane never saw the row, and a window that had seen it lost it from its queue
at the next broadcast. For a CLI that writes a registry record within a second
this went unnoticed; for an opencode that writes nothing until spoken to it
meant a headless start appeared nowhere. Reported the same evening: *"I still
can't see it if I start a new headless opencode."*

Clause 3's substance stands — **never persisted**, recomputed from tmux each
scan, nothing in SQLite refers to it. What changes is what *"only in the
snapshot"* is taken to mean: a pane row is now **ranked in every queue
broadcast** and **announced as it appears or changes**, keeping the times it
was first seen so an unchanged row is not re-announced every tick. When it
goes, it is announced **dead, then removed** — dead first because the usual
reason it goes is that its agent spoke and a real session, published earlier
in the same scan, now claims the pane; a dead row and a live session sharing a
pid and a cwd is the window's succession rule, which moves the selection and
any held pane onto the real session. For that, the row's `pid` is now the
agent's own rather than the pane's, which is usually a shell. Pane rows are
still left out of notifications, as they always were.

## Amendment — 2026-10-09

**A seat is not a provisional row.** A pane the daemon created for a seat
([ADR-0003's amendment of this date](0003-observe-do-not-spawn.md#amendment--2026-10-09-mogeung-may-own-a-seat-it-started))
is known from birth: the daemon's launch record names the pane, the definition
it was started from, and the session id it chose where the CLI takes one
(`--session-id` for Claude Code and Qwen). So it is never published as
`pane:%N`. The seat's row exists before the pane has written anything, carries
`seat: true` on the wire and the definition's name, and — unlike every row this
ADR is about — **is persisted**, as a launch record under `~/.mogeung`, because
a seat survives the daemon: its pane is tmux's
([ADR-0011](0011-own-a-shell-never-an-agent.md)). When the CLI's own session
appears it claims the seat row by the chosen id, or by the pane where the CLI
chose its own, and clause 2's rule — the real session wins, in the same
snapshot — holds with the seat's identity carried over rather than lost.

**Clause 5 stands for every provisional row, and is now the fence between two
kinds of pane.** A pane mogeung *found* is observed, whatever is running in
it and however it got there; that is fence 1 of the amendment above, and
recognising an agent in a pane is exactly the thing that does not make it a
seat. The trust prompt `R-J74` found is, on a seat, still yours to answer
([ADR-0043](0043-a-seat-is-configured-at-launch-never-in-your-home.md),
clause 6) — the row ranks as needing you, and the thing that unblocks it is
you, typing in the pane mogeung put in front of you, as this clause has always
said.

