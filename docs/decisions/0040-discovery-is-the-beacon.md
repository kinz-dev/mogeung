---
title: Discovery is the beacon; mDNS stays an advert
status: active
updated: 2026-09-22
decided: 2026-09-22
---

# ADR-0040 — Discovery is the beacon; mDNS stays an advert

## Context

`R-I8` (2026-07-31) built LAN discovery on mDNS: the daemon advertises
`_mogeung._tcp` through the `mdns-sd` crate and the same crate browses for it.
The browse half lost its only client when the egui window was retired
([ADR-0020](0020-the-egui-client-is-retired.md)), and `R-I17` asked for
discovery again, over a broadcast beacon, resting on
[A44](../product/assumptions.md) — that broadcast crosses the Orbi mesh where
multicast may not.

A44 was measured on 2026-09-22, ten minutes across the mesh between a Linux
laptop on Wi-Fi with power save on and a Mac mini on Wi-Fi, each sending a
beacon every 15 s and each listening, with the mDNS browse held open beside
the listener and the mDNS advert running beside the sender.

- **Broadcast crossed both ways.** The Mac heard 36 of the laptop's 40
  beacons, longest silence 44 s; the laptop heard 25 of the Mac's 40, longest
  silence 45 s, with one *gone* that came *back* a third of a second later.
  Each host heard itself 40 of 40.
- **`mdns-sd`'s browse resolved nothing**, in either direction, in ten
  minutes.
- **Multicast itself crosses.** The Mac's own `dns-sd -B _mogeung._tcp` saw
  the laptop's record within a second of it being advertised, and each host
  resolves the other's `.local` name — so the mDNS advert works, and the
  half that does not is the browse, running beside avahi on one side and
  mDNSResponder on the other.

That is not the verdict the spec wrote a branch for. It expected *broadcast
crosses and multicast does not* or *both cross*, and got a third thing: both
transports cross the network, and only one implementation delivered.

## Decision

**Discovery is the beacon.** The daemon sends it, the shell listens for it,
and the Found list is built from nothing else. The codec and the table are
`mogeung_core::lan`; the sender rides `R-I17`'s phase 1.

**Each beacon is sent as a burst of two datagrams a second apart.** The lossy
end of the run heard 62 % of single datagrams, and at that rate three misses
in a row — the *gone* rule — happens about once in ten minutes, which it did.
Two datagrams a second apart turn 62 % into roughly 86 % per beacon and three
consecutive misses into a rarity. The table already counts a sequence number
once, so a burst costs nothing on the receiving side.

**The mDNS advert stays, opt-in, as a record for other tools.** `--advertise`
is untouched: its record is real, the system browsers see it, and a person
with `dns-sd` or `avahi-browse` in hand can find a daemon with it. **Nothing
of ours browses mDNS any more**: `discovery::Watcher` and `discovery::browse`
retire when the shell's listener lands, and `R-I8`'s ✅ stands for the record
it advertises and not for the search it once did.

## Alternatives

**Port `mdns-sd`'s browse to the shell** — the branch the spec wrote for *both
cross*. Rejected because it is the half that did not work, on both platforms,
held open for ten minutes, on a network where the system browsers found the
same record in a second. The `Watcher` was already the second attempt at it
(`R-I8`'s notes), and a third would be built on the same library beside the
same system responders.

**Browse through the platform's own responder** — avahi over D-Bus on Linux,
`dns-sd`/`NSNetServiceBrowser` on macOS, and Windows' own. Rejected for now:
three implementations of one list, each with its own way of not working, to
reach a record that a 175-byte datagram already reaches with one. Worth
revisiting only if the beacon fails on a segment where multicast works.

**A multicast beacon instead of a broadcast one.** Rejected on the same
evidence: whatever `mdns-sd` stumbled on lives in multicast handling beside
the system responders, and broadcast has no group to join and no membership to
get wrong. The probe's `--limited` flag is there for the segment where
directed broadcast fails.

**Turn the mDNS advert on by default as well.** Deferred, as
[ADR-0039](0039-the-daemon-announces-itself-by-default.md) said it would be
until this verdict: the beacon is the survivor, and a second default-on
announcement would be a second disclosure for a search nothing of ours makes.

## Consequences

Easy: one codec, one transport, one code path on every platform the shell
runs on, and a list that fills from a datagram the probe has already measured
across the mesh. A Found list can say *heard 12 s ago* and mean it.

Hard, and said plainly:

- **IPv4 only.** There is no IPv6 broadcast; a v6-only segment is invisible
  to this. The beacon's `v` is there for the day that matters.
- **Lossy by nature on Wi-Fi.** A client in power save misses broadcast frames
  and nothing above the driver fixes that; the burst and the three-miss rule
  are the mitigation, and a *gone* that comes *back* is a thing the Found
  list will show and must show honestly rather than hide.
- **Unauthenticated**, which [ADR-0039](0039-the-daemon-announces-itself-by-default.md)
  already fenced: finding is never connecting, and the token is typed.
- The record `R-I8` built keeps working for tools that are not ours, and
  stops being a thing this product searches with.

Ruled out: a Found list fed by two transports at once, with rows that
disagree about who is there.

## Revisit if

A segment where the beacon fails and multicast works; an IPv6-only LAN; a
platform where a shared UDP port cannot be bound; or a browse through the
system responders that turns out to be one function rather than three.
