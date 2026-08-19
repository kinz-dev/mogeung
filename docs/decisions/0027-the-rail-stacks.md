---
title: The rail stacks its tools; it does not split into two rails
status: active
updated: 2026-08-19
decided: 2026-08-19
---

# ADR-0027 — The rail stacks its tools; it does not split into two rails

## Context

[ADR-0017](0017-the-rail-is-chrome.md) put the tool-window rail on the right
edge and recorded, in its own consequences, that *"the rail shows one tool at a
time. IntelliJ splits each rail into two stacks; we do not, so Files and Search
cannot be open together."* It then named the condition for coming back:
**"Two tool windows are genuinely wanted at once. That is the two-stack rail,
and it changes the panel's shape rather than extending it."**

That happened on 2026-08-19: *"Now the right hand side bar is files, search,
etc. When one of that is clicked it will display that in the right panel. But
only 1 of the right panel can be display at a time. Can we display multiple
panel at the right hand side."*

The concrete moment behind it is the one the rail's own features keep
producing. `R-F13` put global search in the rail *because* results you act on
have to stay open while you act on them — and acting on a result means opening
the file it names, which is Files, which used to close Search. The same is true
of the Notes tool: `R-B35`'s copy-a-turn-to-a-note opens the rail on Notes,
taking away whatever you were reading when you decided the turn was worth
keeping.

What makes this a decision rather than a one-line change is that ADR-0017 was
right about the shape: a rail holding two things is not the same panel with a
second body, and IntelliJ's answer — splitting the rail into two independent
stacks, each with its own strip — is a bigger thing than what was asked for.

## Decision

**The rail is one column, and every open tool is a section in it.** The strip
toggles a tool's membership of that column; it no longer swaps the column's
contents.

- `prefs.rail` is a **list** of tools, not one tool or `null`. Collapsed is the
  empty list, which is the same state the strip already stood for.
- Sections are drawn in **strip order**, never in the order they were opened.
  A panel that rearranges itself when you add a second one is a panel you have
  to re-find.
- Each section is dragged against its **neighbour only**, and the split is
  stored as a **weight** rather than a pixel height, so it survives a window
  that is a different size on the laptop.
- The chord keeps its one-key-both-ways rule. Only the opening half changes
  meaning: it adds rather than replaces.

**Not** two stacks with two strips. A second stack means a second place a tool
can live, which is a decision to make every time one is opened; a section in
one column is a tool that is either there or not.

## Alternatives

- **Two independent stacks, IntelliJ-fashion** — the shape ADR-0017 named. It
  is the richer answer and it is more window than this one asked for: it needs
  a second strip, a rule for which stack a tool belongs to, and an answer for
  what happens when a stack is empty. Rejected as more machinery than the ask,
  and available later — this ADR does not close it, because a stack of
  sections is what each half of a split rail would be anyway.
- **Tabs across the top of the rail** — cheap, and it fails the actual request:
  tabs are one-at-a-time with extra steps, which is the thing being complained
  about.
- **Tools side by side across the rail's width** — the rail is 300px and half
  of that is not a file tree. A column has room to give.
- **Let a tool window be dragged into the dockview tree instead** — this is the
  arrangement problem solved once, by a system that already does splits. It is
  exactly what ADR-0017 forbids, and for the reason it gives: the collapsed
  strip has no equivalent there, and the strip is what makes closing a tool
  safe rather than destructive. Unchanged.

## Consequences

- Files stays open while you search, and a reveal from a search hit no longer
  costs you the hit list. `revealInFiles` and the Transcript's copy-to-note
  now **add** their tool rather than taking over the rail.
- The rail can be crowded, and nothing stops you opening all four into a
  300px column. The minimum section height is the only guard: a section that
  can be squeezed to nothing looks exactly like one that is closed, except
  that its strip icon is still lit.
- **A preferences file written before today holds a string where the window now
  iterates**, and `"files".map` is a blank window rather than a lost setting.
  `railList` is the boundary that makes that impossible, and it is forgiving in
  both directions for the reason `R-B40` already had: `Prefs` is one
  `JSON.parse`, so a tool name from a newer build must cost that name and not
  every setting in the file.
- The bottom dock still shows **one** tool at a time. That asymmetry is now
  visible and is not accidental: the dock is horizontal, and two stacked dock
  tools would each get half of a panel that is already the shortest thing in
  the window. If it is asked for, it is a different row with a different
  argument.
- ADR-0017 stands. Nothing here moves a tool window into the tile tree, lets a
  tab be dragged into the rail, or takes away the strip.

## Revisit if

- The single column stops being enough — three tools open with none of them
  readable. That is the two-stack rail, and it is the alternative above rather
  than a new idea.
- The same request arrives for the bottom dock. Two stacked tools *there* is a
  different trade, and this ADR deliberately does not decide it.
- Weights prove to be the wrong unit — a tool that wants a fixed height
  (a short list, an editor) fighting a tool that wants the rest.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
