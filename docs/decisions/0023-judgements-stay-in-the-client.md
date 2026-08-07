---
title: Your judgements stay in the client
status: active
updated: 2026-08-07
decided: 2026-08-07
---

# ADR-0023 — Your judgements stay in the client

## Context

`R-I12` was filed on 2026-08-02 as an open question, because two decisions one
day apart had answered it differently. [ADR-0015](0015-markdown-is-the-truth.md)
put **notes** on the daemon, owned and mirrored to `~/.mogeung/notes/*.md`.
`R-I11` had just partitioned **labels, tags, pins, hidden and bookmarks** into
client storage, keyed by the watched machine.

Both scope to the machine; they disagree about who holds the bytes. So two
windows on one daemon show the same notes and different pins.

The row argued for moving the judgements to the daemon, and its test was
*would you want a second window to agree?* — yes for labels, pins, hidden and
bookmarks; no for terminal tabs, theme, fonts, layout and zoom. Its strongest
point is still the one worth writing down: **a label is text you wrote, which
makes it a very short note**, and filing it as "client view-state like pins"
(`R-B26`) reads oddly beside a note that gets a table of its own.

What has changed since is that the question stopped being hypothetical without
becoming urgent. The divergence has not bitten, because
[ADR-0013](0013-one-window-one-daemon.md) makes one window per daemon the
normal shape and two windows on one daemon the unusual one. The race that
motivated `R-I11` is fixed and has stayed fixed.

Two corrections to the row, both of which matter to anyone re-opening this:

- It says the migration would be "from `state/<machine>.json`". That path
  belonged to the **egui** client, retired by
  [ADR-0020](0020-the-egui-client-is-retired.md). The React client never wrote
  a file: it keeps one JSON blob in the webview's `localStorage` under
  `mogeung.prefs`, with the judgements under `scoped[machine_id]`.
- That makes the migration a *different* job from the one costed. There is no
  path on disk for the daemon to adopt, so the window would have to hand its
  state over and the daemon would have to accept a client's word for it once —
  a wire family whose only job is a one-time import.

## Decision

**Labels, tags, pins, hidden, bookmarks and the rest of `ScopedPrefs` stay in
the client, machine-scoped, exactly where `R-I11` put them.** `R-I12` is
answered *no* and closed.

[ADR-0015](0015-markdown-is-the-truth.md) is untouched: **notes stay on the
daemon.** The asymmetry this leaves is accepted rather than resolved, and the
line under it is the one ADR-0001 already draws — the daemon holds
*observations* and the things it is the store of record for. A note is a
document with a lifetime of its own. A pin is an opinion about a list on a
screen the daemon cannot see.

## Alternatives

**Move the judgements to the daemon**, as `R-I12` argued. Rejected on cost
against a benefit nobody has needed: four wire families, a one-time import path
that exists only to be run once, and the daemon starting to hold preferences
rather than only observations — which is a wider change to what the daemon *is*
than it looks, and the hardest kind to reverse once clients depend on it. The
divergence it fixes is a hypothetical under ADR-0013.

**Move only labels**, on the row's own "a label is a very short note" argument.
Rejected as the worst of both: it splits one settings object across two owners,
so the window would read a label over the wire and a tag from `localStorage`
and have to merge them per row. A rule that says *these five are yours and this
one is mine* costs more to hold in your head than either whole answer.

**Move notes back to the client** for symmetry. Rejected — that is
ADR-0015's decision and it was made on the merits (markdown is the truth, the
file outlives the tool). Symmetry is not a reason to give up a mirrored
document store.

## Consequences

Easy: nothing changes. No migration, no new wire families, and `R-I11`'s
partitioning keeps working.

Hard, and this is the part the row was right about:

- **Two windows on one daemon still disagree about pins while agreeing about
  notes.** That is now a decision rather than an oversight, which is the only
  improvement this ADR makes to it.
- **Your labels live somewhere with no export and no backup.** `localStorage`
  in a WebKitGTK data directory is not a file you can copy, diff, or put in a
  repository — and a label is text you wrote, which is exactly the class of
  thing ADR-0015 argued should survive the tool. Clearing the webview's storage
  loses every label, tag and pin on that machine, silently.
- Anything that *reasons* from where these live has to be right about it.
  `lib/tags.ts` calls it "a hand-editable preferences file" three times and
  degrades unknown ids gracefully because a human might have typed one. The
  conclusion survives; the premise stopped being true at the port and is
  corrected alongside this ADR.

## Revisit if

You run two windows against one daemon often enough for the divergence to
annoy you — or, more likely first, **you lose labels to a cleared webview
store**. The second is the cheaper failure to guard against and does not need
this decision reversed: an export, or a mirror to `~/.mogeung/` the way notes
already have, would answer it while leaving ownership where it is.
