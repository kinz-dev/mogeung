---
title: mogeung may run a routing proxy of its own, and must say where it forwards
status: active
updated: 2026-08-28
decided: 2026-08-28
---

# ADR-0033 — A proxy of our own, and the sentence that replaces a gate

## Context

`R-O5` asks one endpoint one question. A routing proxy in front of it turns
that into *the right model for this question* — a local model for "what does
this flag do", a subscription-backed one for "why did this diff regress" —
without mogeung growing an opinion about models, which is `R-O2`'s job and not
a thing to guess at.

The proxy already exists and is already on this desk. `llmproxy` speaks
OpenAI-compatible chat completions, which is exactly what
[ADR-0031](0031-consent-to-a-named-host.md)'s model seam already calls, so
pointing `model_url` at it needs no mogeung code at all.

Asked 2026-08-28, after that was established: *"I don't want to share a
llmproxy server instance, because I want to be able to define its own llmproxy
rules. So I think we need to start a llmproxy instance with mogeung (and kill
it when mogeung is stopped)."*

The motivation is right and is not about isolation for its own sake: Ask
Mogeung asks different questions than a coding agent does, and a routing table
tuned for one is wrong for the other. Two questions follow, and both are ones
this codebase has already answered in a *different* direction, which is why
they are worth writing down rather than assuming.

**May the daemon own a long-lived child at all?**
[ADR-0003](0003-observe-do-not-spawn.md) threw away a version of this product
for spawning things, and [ADR-0025](0025-run-a-process-you-named-never-an-agent.md)
permits process execution only under a discipline this does not meet: a
*named* configuration, on an *explicit* click, gated on the bind and
`--allow-run`. An automatic proxy at start-up is none of those.

**What happens to consent?** ADR-0031 clause 3 decides consent from the
**endpoint's host**. A proxy on `127.0.0.1` is loopback, so the gate passes
without asking — while the proxy forwards prompts to a vendor. The consent
mechanism decided this morning would be silently bypassed by a feature decided
this afternoon.

## Decision

**mogeung may run one llmproxy of its own, off by default, adopted rather than
tracked — and because it cannot gate what a proxy forwards, it reports it.**

1. **Off unless the file says otherwise, and file-only.** `llmproxy = true` in
   `~/.mogeung/config.toml`. No flag: this starts a long-lived child, which is
   a standing arrangement rather than a property of one invocation, and a
   daemon that spawned a proxy because of a flag somebody typed once would
   leave one behind exactly when nobody was expecting it.
2. **Its own instance and its own rules.** `~/.mogeung/llmproxy.toml`, never
   `~/.llmproxy/config.toml`. llmproxy keys its daemon metadata on the bound
   address, so a second instance on a second port is a first-class arrangement
   there rather than a collision. The file is written **once** if absent and
   never touched again — it is the user's from the moment it exists.
3. **The port is derived, not remembered.** `daemon port + 1000`. A random port
   would have to be written down for the next start-up to find what it left
   behind, and a file recording where a process is, is precisely the stale
   pid-file failure [ADR-0009](0009-the-window-may-host-a-daemon.md) rejected
   for this daemon's own port. Derivation is recomputed, so it cannot go stale.
4. **Adopt, then spawn; stop by address, not by signal.** Start-up probes the
   port and uses an llmproxy already answering there. The orphan after a
   `SIGKILL` is not prevented — it cannot be — it is *reused*, which turns a
   leak that accumulates into one process that gets found again, and shutdown
   stops an adopted instance as well as a hosted one, because the port is
   derived from this daemon's own and an llmproxy there is ours from a previous
   life.

   Stopping is `llmproxy --listen <addr> --shutdown`, **not** a signal. This
   was found by building it: llmproxy re-execs itself as `--foreground` and
   detaches, so the process mogeung spawns exits within a second and the daemon
   holding the port was never its child — a recorded pid names something
   already gone. The port is the identity that persists, it is what llmproxy
   itself keys its metadata on, and stopping by it leaves any other instance
   alone (verified against the one already running on this desk).

   `PR_SET_PDEATHSIG` is rejected on top of that: it fires on the death of the
   parent **thread**, under a tokio runtime free to retire the worker that
   spawned; it does not exist on macOS; and it would not have reached a
   detached grandchild anyway.
5. **A failure degrades the panel, never the daemon.** If the proxy will not
   start, the model seam is left pointing where it already pointed and `R-O5`
   works exactly as before. A routing convenience must not be able to take down
   the thing it was meant to improve — `A4`'s degrade-never-panic discipline
   applied to a dependency that is a process.
6. **Where it forwards is reported, not gated.** The Health row and the chat
   panel name every non-loopback host in the proxy's config. This is a
   deliberate refusal to extend clause 3's gate, and the reason is
   [pillar K](../product/roadmap.md#k-explicitly-not)'s rule: routing is decided
   **per request** and a target may fail over, so a gate here could only ever
   be sometimes-right, and a gate that is sometimes right is worse than an
   honest sentence. The disclosure is read from the config **file** rather than
   asked of the running process, so it is still answerable when the proxy is
   down.
7. **The starter config forwards nowhere new.** Its only provider is the
   endpoint the daemon was already using, so turning the proxy on does not move
   a single byte. Adding a subscription-backed provider is an edit the user
   makes in a file mogeung told them is theirs — and that edit *is* the
   out-loud decision clause 3 asks for.

ADR-0003 is untouched: llmproxy is not an agent. It is worth naming that
`llmproxy --claude` *launches* one — and that mogeung therefore starts it with
**no mode flag at all**. `--proxy`, `--intercept` and `--integrated` are
routing modes *for an agent launch* and llmproxy's own argument parser requires
one of `--claude`/`--codex`/`--copilot` beside them. Bare is the plain HTTP
server, which is the whole of what is wanted, and it is the invocation that
cannot start an agent even by mistake.

## Alternatives

**Point at the instance already running.** Zero code, works today, and it is
what was recommended first. Rejected on the stated motivation: one instance
means one routing table, and the rules that serve a coding agent are not the
rules that serve a chat panel.

**A target in the existing config instead of an instance.** `MOGEUNG_ASK:` as
an in-band verb pinning a dedicated `[integrated.targets.*]`, giving Ask
Mogeung its own *rule* without its own *process* — no ADR, no dependency, no
child. Genuinely cheaper and it was offered. Rejected by the person whose desk
this is, on the grounds that shared spend, health and sticky-routing
accounting is not the separation they wanted.

**Extend clause 3's gate to the proxy's upstreams.** Consistent on its face:
mogeung wrote the config, so it knows the hosts, so it could refuse them. The
reason it does not is clause 6's, and it is the same argument pillar K makes
about risk scoring — either the gate is authoritative or it is not, and one
that cannot see a per-request failover would look authoritative while being
wrong. Reporting is the honest half of what mogeung actually knows.

**Manage routing in mogeung and call providers directly.** Rejected as
rebuilding llmproxy inside a product whose whole discipline is not building the
thing next door.

## Consequences

**Easier.** One thing to start. Ask Mogeung gets a routing table of its own,
per-question model choice through llmproxy's in-band verbs with no mogeung UI,
and the panel keeps working when the proxy does not.

**Harder, and named.** mogeung now spawns a long-lived child process, which it
has never done — the failure modes of that (an orphan holding a borrowed OAuth
token) are mitigated by adoption rather than eliminated. It gains a runtime
dependency on a second program. And there is now a second configuration file
with a second syntax, which the config editor (`R-J79`) does not yet edit.

**The gap left open, deliberately.** With the proxy on, mogeung can no longer
promise where a prompt goes — only report where it might. Anyone reading
ADR-0031 clause 3 as a guarantee should read this clause 6 next.

## Revisit if

- The orphan turns out to accumulate in practice rather than be adopted — the
  answer then is a supervised restart, not a pid file.
- `R-J79`'s config editor grows a second tab for the proxy's rules, at which
  point "mogeung wrote it once and never touches it" needs restating.
- `R-O2`'s harness gives mogeung a real opinion about which model to ask, which
  would make routing mogeung's own job and this proxy the wrong shape for it.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
