---
title: The window is replaced, the daemon is not
status: active
updated: 2026-08-04
decided: 2026-08-04
---

# ADR-0018 — The window is replaced, the daemon is not

## Context

Two facts landed within a day of each other.

**The dogfooding week finished, and it passed.** [Item 0](../product/roadmap.md#0-the-non-feature)
had been the gate on everything since v0.1 was thrown away: *use mogeung for a
week with 3–4 terminals open*, because A1 and A6 are the product's own premises
and neither had ever been tested. The verdict on 2026-08-04 was that the tool
carries 70–80% of the interaction with agents. That is the strongest evidence
this project has ever had about itself.

**And what remains to build is UI-shaped.** `R-F11`'s charts, `R-L2`'s
scratchpad, a bookmark panel that summarises, richer search. The complaint that
started it was about the Code pane — *"it looks very immature"* — and the
diagnosis is worse than the complaint: it is not an immature editor, it is a
hand-written text renderer, because the Rust ecosystem has no CodeMirror and no
Monaco and there is no crate to swap in. `explorer_viewer` lays out a
`LayoutJob`, walks the galley's rows and hit-tests clicks against their
geometry, by hand, in 600 lines.

The obvious reading — "so rewrite it" — is also how v0.1 died, and
[ADR-0003](0003-observe-do-not-spawn.md) exists because of that. What makes this
different has to be stated, or the same mistake wears a new framework.

## Decision

**Rewrite the client in TypeScript. Do not touch the daemon.**

- The daemon, `mogeung-core` and `mogeung-tray` are unchanged. `wire-protocol.md`
  is the contract the new client is written against, and pressure to *change*
  the protocol during the port is treated as a signal that logic is leaking into
  the client rather than as ordinary work.
- The new client runs **beside** the old one against one daemon, and grows a
  pane at a time. Multiple clients on one daemon is already the design: the tray
  is one, and `R-C3`'s phone client was one at zero daemon cost.
- The egui client is retired at parity, not before.
- The native shell is Tauri, so its Rust side can hold the ptys — which is what
  keeps [ADR-0010](0010-attach-a-terminal-never-own-one.md) and
  [ADR-0011](0011-own-a-shell-never-an-agent.md) true. A browser-only client
  would force the daemon to own them, and that runs straight into ADR-0003.
- React rather than Vue, on one requirement: docking. `A14` says dockable panes
  are wanted and kept, and it is the single place Vue's ecosystem is genuinely
  thinner. Everything else on the stack is framework-agnostic.

**This is not v0.1 again**, and the difference is precise: v0.1 was thrown away
for building on an unvalidated premise. This replaces a presentation layer
against a validated daemon and a frozen protocol, with the thing being replaced
still running while it happens.

## Alternatives

- **Keep egui and improve the viewer.** Virtualise it, add folding. Real work
  with a real ceiling: it does not produce a find widget, a minimap, breadcrumbs
  or a diff editor, and every one of those would be hand-built. Rejected on the
  size of what is left to build, not on the state of what exists.
- **A different Rust GUI framework** — floem, iced, Slint. None ships a code
  editor either; Lapce's and Zed's live inside their applications. This trades
  the whole client for no editor.
- **A browser-only client**, no Tauri. Cheapest, and it breaks the pty ADRs by
  forcing the daemon to spawn children. Kept as the *development* mode, which is
  useful and honest — a browser tab is a real client — but not as the product.
- **Vue, as first proposed.** Lost only on docking. Recorded because if
  dockview's React binding disappoints, this is the alternative to revisit and
  the reason it lost is narrow.
- **Rewrite daemon and client together.** Nothing argues for it. The daemon is
  the part that has been proven.

## Consequences

- The port is **strictly a client rewrite**: no new wire messages, no ADR
  reversals, no daemon risk. That is a direct consequence of
  [ADR-0019](0019-a-viewer-not-an-editor.md) landing first — an editor would
  have required a write verb and a supersession of pillar K.
- Monaco, CodeMirror, xterm.js, dockview and a charting library arrive as
  dependencies rather than as things to build. The Code pane's 600 lines of
  galley arithmetic become configuration.
- **Two clients means two sets of view state** — `prefs.json` and `layout.json`
  on one side, `localStorage` on the other. They will disagree while both run.
  Tolerable during the migration and not after, which strengthens `R-I12`'s
  argument that the machine-scoped half belongs to the daemon.
- The vendored `egui-term` crate, and the egui version-pinning it forced, go
  away when the old client does.
- A large body of hand-written Rust UI — roughly 15,000 lines across twenty
  modules — is eventually deleted. Its *decisions* survive in the ADRs and the
  roadmap; its code does not.
- Everything is now downstream of a JavaScript toolchain, with the supply-chain
  and churn that implies. The daemon is deliberately untouched by this: it stays
  a Rust binary with a small dependency tree.

## Revisit if

- Parity is not reached in a reasonable run of evenings, or the new client is
  still worse in daily use after a week. The egui client is still there, and
  keeping it is the fallback.
- dockview's React binding cannot carry `A14`'s arrangements. That was the whole
  argument for React over Vue and it deserves to be checked early.
- The port starts wanting protocol changes. That means the boundary was drawn in
  the wrong place, and the boundary is the reason this is safe.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
