---
title: One window watches one daemon
status: active
updated: 2026-07-31
decided: 2026-07-31
---

# ADR-0013 — One window watches one daemon

## Context

`R-I4`–`R-I7` shipped remote reach: a window can be pointed at any daemon, and
moved between them while running. The obvious next row is `R-I9` — one window,
several daemons, one merged queue — and it has been sitting in the roadmap
since the pillar was opened, gated on this ADR.

It is worth writing down what "merged queue" would actually cost, because the
sketch is much smaller than the change.

**The identifier gives no help.** `SessionId` is `pub type SessionId = String`
([`session.rs`](../../crates/mogeung-core/src/session.rs)) — an alias, not a
newtype. Making a session origin-qualified means every `String` that is really
a session id keeps compiling while meaning the wrong thing. There are 30
`ClientMsg` variants carrying a `session_id` out of 45, 58 `net.send` sites in
the window, and one `Net`. A router would have to sit under all of them, and
the compiler would not find a single site we missed.

**The queue is the cheapest thing to merge and the least valuable.** Everything
downstream of picking a session — the diff, the git pane, the explorer, the
transcript, the terminal, the Insight views — is single-origin and asks its
daemon directly. Merging the list gives one ranked column whose every click
lands you back in a one-daemon world. The merge stops at the first thing you do
with it.

**The intelligence does not merge at all.** `detect_collisions` runs in the
daemon over that daemon's sessions
([`state.rs`](../../crates/mogeungd/src/state.rs)), and so does the whole of
pillar `F`. Two sessions on *different* machines editing the same path are not
colliding, so the daemon is right to compute this per-origin — but it means the
window cannot merge the answer, only the list. Either the panes silently answer
for one origin while the queue answers for all, or the window recomputes them
itself. The first is worse than not merging, because it looks merged. The
second is the problem below.

**It would make the window an authority.** The project rule is that the daemon
is the product and every UI is a client with no local authority. A window that
ranks sessions across daemons owns a second implementation of the ranking, and
two implementations of attention ordering that must not drift is exactly the
kind of thing that drifts.

**Terminal tabs are keyed by machine-free strings.** A tab is
`Shell { root, ordinal }`, and `shell_session_name(root, ordinal)` derives the
tmux name from the worktree path with no machine in it
([`term.rs`](../../crates/mogeung-ui/src/term.rs)). That is fine while one
window drives one machine. It stops being fine the moment one window drives
two, and the normal case — the same checkout path on a laptop and a dev box —
is the colliding one.

**And [A24](../product/assumptions.md) has no verdict.** The premise under the
whole pillar — that watching a remote machine is worth doing — was tested for
the first time on 2026-07-31 and has one day of use behind it. Building the
aggregator now is [item 0](../product/roadmap.md#0-the-non-feature) being
ignored at pillar scale.

The honest cost of *not* doing it has to be counted too, and it is not zero:

- `~/.mogeung/prefs.json` is one fixed path written whole
  ([`prefs.rs`](../../crates/mogeung-ui/src/prefs.rs)). Two windows on one
  machine fight over it, last writer wins. **This is a defect today**, not a
  hypothetical cost of this decision.
- `R-C2`'s tray counts waiting sessions for one daemon. Two windows means two
  counts and no total.
- There is no single "who needs me most" across machines — which is
  [A1 and A6](../product/assumptions.md), the product's actual thesis, applied
  one level out.

## Decision

**A window watches exactly one daemon at a time. It does not aggregate, and it
never computes what a daemon computes.**

`R-I9` is refused for now on that basis, and stays in the roadmap with its
reasoning rather than being deleted.

In its place, make the cheap alternative honest — it is what people will
actually run, and it currently has a bug in it. Filed as `R-I11`:

- client state that two windows fight over gets scoped per daemon, starting
  with `prefs.json`;
- terminal tab keys and the derived tmux name carry the machine;
- the tray says *which* daemon a count belongs to.

That is a fraction of `R-I9`'s cost, fixes something broken now, and is the
experiment `R-I9` needs: run N windows for a week and the failure that argues
for merging will name itself.

## Alternatives

**Merge the queue only** — the `R-I9` sketch. Rejected on the "stops at the
first click" argument above: the pane you land in still belongs to one daemon,
so the merge buys a column and nothing behind it, at the cost of a router under
30 message types with no compiler backstop.

**Merge everything, with the window recomputing ranking and collisions across
origins.** Rejected because it puts a second implementation of attention
ordering in the client, against the rule that a UI has no local authority. It
is also the version that would need `SessionId` to become a real type first —
which is worth doing on its own merits, and is not worth doing *for* this.

**Federate in the daemon: one daemon fronts others, the window stays thin.**
The most interesting alternative, and the only one that survives the authority
rule — the aggregate would be computed where every other aggregate already is.
Rejected *for now* rather than on principle: it puts one machine's daemon in
the trust path of another's, which is a security question we have only just
finished answering for a single hop (`R-I10`), and it is a large build to make
on an assumption with one day of evidence under it. **This is the shape to
reconsider first**, not the aggregating window.

**Do nothing, including `R-I11`.** Rejected because the shared `prefs.json` is
already wrong for anyone running two windows, which this decision now tells
them to do.

## Consequences

Easy: every pane stays single-origin. No routing layer, no origin-qualified
ids, no change to the wire, and `SessionId` can stay a `String` until something
else demands otherwise. The remote work already shipped needs nothing added.

Hard, and worth saying plainly: **watching three machines means three windows,
three trays, and three notification streams.** There is no one place that
answers "who needs me most" across all of them. That is a genuine loss, it is
the exact thing `R-I9` was for, and this ADR is a bet that per-machine windows
are good enough — not a claim that merging has no value.

Ruled out: any pane that answers about two machines at once. If one is ever
wanted, it belongs behind the federation alternative, not in the window.

## Revisit if

A week of running several windows produces a **named** failure rather than a
feeling — the likely candidate being a waiting session missed on the machine
whose window was behind another window. `R-C2`'s per-daemon tray is the cheap
test of that specific one.

Two things should be true before this is reopened:
[A24](../product/assumptions.md) has a verdict, and `R-I11` has shipped so the
alternative is being judged at its best rather than with a known bug in it.

And when it is reopened, the question is *federation*, not an aggregating
window.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
