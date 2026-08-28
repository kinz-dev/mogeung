---
title: Consent to a remote model endpoint names the host, and may be given in the config file
status: active
updated: 2026-08-28
decided: 2026-08-28
supersedes: ADR-0030
---

# ADR-0031 — Consent to a named host, in the file the daemon can actually read

## Context

[ADR-0030](0030-a-model-reads-the-evidence.md) was decided and built on the
same day. Its clause 3 said a non-loopback model endpoint is refused unless the
daemon was started with `--allow-remote-model`, *"on the same reasoning and
with the same shape as ADR-0025's `--allow-run`"*.

The shapes are not the same, and the first install proved it.

`--allow-run` gates an ability decided from the **bind** address: a daemon on
loopback is as trusted as the terminal panel, so a window hosting its own
daemon ([ADR-0009](0009-the-window-may-host-a-daemon.md)) is permitted without
ever needing the flag it has no argv to receive. The model gate is decided from
the **endpoint** address instead, and a hosted daemon can perfectly reasonably
want an endpoint on the GPU box down the hall. So on the shape mogeung is
normally run in — click the launcher, the window takes the port and hosts —
consent was not merely inconvenient. It was **unreachable**, and the endpoint
was refused for ever with a message naming a flag that could never be passed.

`R-O1` answered that narrowly at the time: the hosted daemon learnt to read
`model_url` and `model_name` from `config.toml`, and still could not grant
consent. That is a refusal with no way out, which is precisely what
`server::admit` taught this codebase not to ship — *a refusal that does not say
what to do instead gets worked around by whatever the internet suggests first.*
Here the workaround is running a second daemon in a terminal before launching
the app, for ever, which is not a product.

The question ADR-0030 got wrong is not *should consent be explicit*. It is
**what an explicit act is.** It assumed a flag is an act and a file is a
setting somebody once wrote and forgot. But a flag is *blanket*: `--allow-run`
and `--allow-remote-model` say yes to everything, for the run. A line in a file
can say something a flag cannot.

## Decision

**Consent to a model endpoint elsewhere names the host it is consent for, and
may be given in `~/.mogeung/config.toml`.**

ADR-0030 is superseded as a whole, because ADRs are immutable and one clause of
six has changed. Clauses 1, 2, 4, 5 and 6 are carried forward **verbatim in
force** — the model reads what mogeung already shows and writes only
`~/.mogeung`; the daemon holds the endpoint, not the client; ids on the wire
with `model_chat` the single named free-form exception, refused entirely on a
non-loopback bind with no flag; model output is never evidence; no model is a
first-class state. Nothing below weakens any of them.

Clause 3 is replaced by this one:

3. **Loopback, or consent that says where the bytes go — and to whom.** A
   model endpoint that is not loopback is refused unless consent covers its
   host. Consent has three states and the middle one is the point:
   - `allow_remote_model = "spark-7ecc"` — this host and no other. Moving
     `model_url` to a different host asks again, with a refusal that names both
     the host it consents to and the host it was asked for.
   - `allow_remote_model = true`, or `--allow-remote-model` — any host, the
     blanket grant, exactly as strong as the flag has always been and no
     stronger.
   - absent — no. The default, and the state of every install that has not
     been told otherwise.

   The flag still exists and still wins, because a flag is *this invocation*
   and the file is the standing preference; there is deliberately no way for
   the file to narrow a flag that was passed. The window states the endpoint
   host wherever model output appears. Sending the corpus off the machine
   remains a decision the user makes out loud or not at all.

The named form is the recommended one and is what the docs and the config
editor write. It is **strictly more out-loud than the flag it replaces**: the
flag consented to whatever the URL happened to say, at any time, including
after someone changed it. This asks per host.

`--allow-run` keeps having no config-file twin. That is not an inconsistency to
be tidied away later: it grants *running processes*, this grants *reading an
endpoint*, and the reasoning that makes a file adequate here — the file is on
the machine, hand-written, and sits directly beneath the `model_url` it
authorises — does not transfer to a clause that can start a process.

## Alternatives

**Leave it; run `mogeungd --allow-remote-model` yourself.** Zero code, and the
design stands as argued. Rejected because it makes the installed application
permanently unable to do the thing it was built for on the machine it was built
on, and because "run a second copy in a terminal first" is the kind of
instruction that gets written into a README and then into a shell alias, at
which point the consent is exactly the forgotten setting the flag was supposed
to prevent.

**A flag on the window binary** — `mogeung-desktop --allow-remote-model`,
passed through to the hosted daemon. Satisfies ADR-0030 clause 3 verbatim and
needs no new ADR. Rejected because clicking the launcher icon still refuses:
the fix would live in a `.desktop` file's `Exec=` line, which `dpkg` overwrites
on every install, so the consent would silently evaporate on upgrade. A consent
that a routine upgrade revokes is worse than one that is hard to give.

**A consent button in the Chat panel.** Fewest keystrokes and the clearest
possible statement of what is being agreed to. Rejected as the *mechanism*
because it makes a client grant daemon authority, against *"the daemon is the
product; every UI is a client with no local authority"* — and because it needs
somewhere to persist the grant, which is this key anyway. Not rejected as a
*surface*: a config editor that writes this key is a client editing a file,
which is what an editor is, and it is the same act as opening the file in vim.

**Blanket consent in the file, only.** `allow_remote_model = true` and nothing
finer. Simpler, and it is what a flag-shaped mind reaches for. Rejected because
it would have made the file exactly the weaker thing ADR-0030 feared, with no
compensating property. The named host is what earns the file its place.

## Consequences

**Easier.** The installed application works on the machine it is installed on,
with no terminal. Consent survives upgrades, because it is in `~/.mogeung`
rather than in a packaged file. The refusal now has two sentences instead of
one — *you never said* and *you said, about somewhere else* — and they send you
to different lines of the same file.

**Harder.** There is now more than one way to consent, which is a thing to keep
documented in one place. And a named host is a string comparison with **no
DNS**: `spark-7ecc` and `spark-7ecc.local` are two names here even when they
are one machine. That will annoy someone. It is the correct direction to fail
in — the cost is re-reading one line of a config file, and the cost of being
clever is consent that silently covers a host nobody named.

**Ruled out.** Suffix matching, wildcards, and a list of hosts. All three are
the same idea and all three end with consent whose extent nobody can state from
reading it. One host, or all of them, said out loud.

## Revisit if

- Someone needs two endpoints — a local one for `R-O3`'s reading guide and a
  bigger remote one for `R-O4` — at which point consent is per endpoint rather
  than per daemon, and `model_url` is a list before this key is.
- A config editor makes the file the normal way to configure everything, in
  which case the flag/file distinction that this ADR spends its argument on has
  quietly stopped mattering and should be retired rather than maintained.
- `R-O2`'s harness says the model's reading is not worth the screen space, in
  which case pillar O comes out and this goes with it.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
