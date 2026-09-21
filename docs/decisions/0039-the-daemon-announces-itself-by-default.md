---
title: The daemon announces itself on the LAN by default
status: active
updated: 2026-09-22
decided: 2026-09-22
---

# ADR-0039 — The daemon announces itself on the LAN by default

## Context

`R-I8` (2026-07-31) made announcing opt-in. `--advertise` publishes
`_mogeung._tcp` over mDNS, a loopback bind refuses to, and
[architecture.md](../design/architecture.md) states the reasoning: *the
broadcast is a disclosure in its own right — it names the machine and says
there is something here worth reaching — and no code can tell a home network
from conference wifi, so the default is off and stays off.*

Two months later that default has a record, and it is empty. The browse half
of `R-I8` died unused when the egui client was retired
([ADR-0020](0020-the-egui-client-is-retired.md)); the TypeScript window never
grew a Scan button because nothing ever appeared to scan for; and `R-I16`'s
connection manager has no verdict because nobody had a second daemon to point
it at. A discovery nobody turns on discovers nothing, and the feature above it
never gets used enough to be judged.

On 2026-09-22 the person this tool is built for said what they want: the
machines on their own LAN — a home mesh, two or three boxes, one person —
should find each other without being configured to, *"by default turn ON"*.

One constraint does not move. A loopback bind cannot be dialled from another
machine, and `R-I10` requires a token on any bind that can be. So *announce by
default* must not become *bind the network by default*: the first is presence,
the second is access, and only the second changes what a stranger on the
segment can reach.

## Decision

**Every daemon sends a beacon by default** — the one `mogeungd` runs headless
and the one a window hosts for itself, on a loopback bind or any other. `beacon
= false` in `config.toml`, or `--no-beacon`, silences it.

**The beacon says presence, never access.** It carries the daemon's version,
machine id, hostname, Claude home, listen port, how it can be reached — `lan`
when bound beyond loopback and therefore token-gated, `ssh` with the configured
target when it has one, `local` otherwise — and how many sessions are live and
how many need someone. It never carries the token, never a session title, never
a path beyond the Claude home. A Found list can therefore show every mogeung on
the segment and say, per row, whether *Connect* is one press or an `ssh -L` you
run yourself.

**The bind does not move.** Loopback by default; beyond it only by a hand in
`config.toml` or on the command line, and then only with a token, exactly as
`R-I10` requires today. `A24`'s posture past the token is unchanged.

**Finding is never connecting.** `R-I8`'s rule stands: a beacon fills a list,
a hand types a token, nothing dials on its own.

**mDNS keeps its opt-in for now.** `--advertise` is untouched until `A44`'s
probe says which transport crosses the mesh; if the beacon is the survivor,
`advertise` retires with a dated note here rather than living on beside it.

## Alternatives

**Keep announcing opt-in** — `R-I8`'s stance. The disclosure argument is real
and is kept below as the cost; it was weighed for a stranger's network, and the
price of a silent default turned out to be a dead feature and an unjudged one.
Rejected on the evidence of two months.

**Bind the network by default, with a token generated on first run.** Would
make every found daemon connectable in one press. Rejected because it changes
the security posture of every install to serve discovery — `A24` is still
`UNTESTED` with one hop of evidence — when a beacon gives the discovery without
moving the bind.

**Detect a hostile network and stay quiet on it.** Rejected for the reason
`R-I8` found: no code can tell, and a heuristic that guesses wrong announces
you on exactly the network it was meant to protect you on.

**Announce only while a window is open.** Rejected because the daemon that
most needs finding is the headless one on the dev box, with no window in front
of it.

## Consequences

Easy: a Found list fills without configuration, including the window-hosted
daemon that was invisible by construction. `R-I16` can finally take its verdict
against a real second daemon. The guide stops asking people to add a flag
before a feature exists for them.

Hard, and said plainly: **a laptop on conference wifi announces its hostname,
that it runs mogeung, and that two sessions need someone, every 15 seconds,
unless its owner turned that off.** The announcement grants nothing — the port
it names is loopback unless a hand opened it, and a token gates it when one
did — but presence is information. The mitigations are the opt-out, a line in
the window saying *announcing on this network*, and the beacon's strict
contents, which are the fence: presence, counts, a route, and no more.

Ruled out: a token in a beacon, under any framing; a client that connects to
what it heard without a hand; a beacon that carries anything the mDNS record
did not, other than the two counts and the route, without an amendment here.

## Revisit if

A beacon is ever found carrying more than presence; the person running this
does so routinely on networks they do not own; or a single incident traces
back to something the beacon said. Any of those reopens the default, not the
feature.
