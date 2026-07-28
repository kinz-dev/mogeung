---
title: Assumption ledger
status: active
updated: 2026-07-27
---

# Assumption ledger

Every belief the product rests on, and whether we have actually checked it.

**This file exists because of a specific failure.** v0.1 was built and thrown
away not because the plan was bad, but because an assumption underneath it — *"we
must spawn agents in order to populate the queue"* — was never written down, and
so was never reviewable. It survived a whole design document, an implementation,
and a commit before contact with reality killed it in one sentence.

## Status values

| Status | Meaning |
|---|---|
| `UNTESTED` | We believe it. We have no evidence. **Build carefully.** |
| `SUPPORTED` | Evidence exists and points our way |
| `AT RISK` | Evidence suggests it may not hold |
| `REFUTED` | Known false. Linked to the decision that responded |

## The rule

No feature spec may be written without a `depends_on:` naming its assumptions.

**If a spec depends on an `UNTESTED` assumption, the work is to test the
assumption — not to build the feature.**

## Ledger

| # | Assumption | Status | Evidence | Resolution |
|---|---|---|---|---|
| A1 | A cross-session attention queue changes where the user looks | `UNTESTED` | Never used in anger. v0.1 died before reaching the question | — |
| A2 | mogeung must spawn agents to populate the queue | `REFUTED` | v0.1 use, 2026-07-25: "a handicapped Claude Code with a single session" | [ADR-0003](../decisions/0003-observe-do-not-spawn.md) |
| A3 | Keyword heuristics over diff text are good enough for reading order | `UNTESTED` | Ranked `auth.rs` above a lockfile once, in a test | — |
| A4 | Claude Code's on-disk formats are stable enough to depend on | `AT RISK` | Undocumented private files. 13 event types classified against a 20,648-line corpus; 3 had been silently swallowed. Canary reports 0 unknown, 0 unreadable | Canary shipped ([0001](../features/0001-trust-the-tool.md)); drift is now loud, not silent |
| A5 | Content-hash hunk anchors keep review marks stable across rewrites | `SUPPORTED` | Verified live: `auth.rs` stayed read while a rewritten `main.rs` came back unread | — |
| A6 | The user will run 3–4 concurrent sessions in normal work | `UNTESTED` | The whole product depends on this. Never measured | — |
| A7 | Reviewing agent output is a distinct activity worth its own tool | `UNTESTED` | Stated in [concept.md](concept.md) §1, never validated | — |
| A8 | Per-session diff attribution by edited files is accurate enough | `AT RISK` | Cannot separate two sessions editing the same file | — |
| A9 | Git is the right diff base for observed sessions | `SUPPORTED` | Works, but the base is HEAD-when-first-seen; sessions predating mogeung diff meaninglessly | — |
| A10 | Doc sprawl is a real and painful problem worth tooling | `UNTESTED` | Stated as the opening complaint; two versions shipped without touching it | — |
| A11 | An egui terminal widget can render Claude Code's TUI well enough to answer a prompt | `SUPPORTED` | Live use, 2026-07-26: the pane renders a real session and takes typed **and dictated** text. Three acceptance items remain unchecked — arrows-and-Enter on a menu, `Shift+Tab` plan mode, `Ctrl+C` — and the three defects that made the pane unusable on first open were all in focus handling, not rendering, so the widget itself is doing better than feared | Attached terminal shipped ([ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md)); see the acceptance list in [feature 0003](../features/0003-attached-terminal.md) |
| A12 | Starting sessions with `yolomo` is a change the user will actually adopt | `UNTESTED` | First real use 2026-07-26, which is a start and not yet a habit. tmux cannot be retrofitted onto a running session, so the whole feature is worth nothing if the habit does not stick. Answerable only by looking, in a week, at how many live sessions have a `tmux_target` | — |
| A13 | The user drives by keyboard and will reach for a palette before a menu | `SUPPORTED` | Stated directly — *"ppl using this is often a pro-level user who love to use keyboard to navigate"* — and the keymap system exists because of it. What is still unchecked is whether the palette gets used *after* the bindings are learnt, or is abandoned once it has taught them. Either outcome is a success for the feature | [feature 0005](../features/0005-reachable-by-keyboard.md) |
| A14 | The user wants two detail views at once and will arrange them rather than leaving the default | `SUPPORTED` | Asked for directly: *"put info and debt side by side"*. Wanting a layout is weaker evidence than keeping one, so what is still open is whether the default tab container is ever actually dragged apart in a week of use | [feature 0006](../features/0006-dockable-panes.md) |
| A15 | Reading worktree files inside mogeung — read-only, never editing — is worth a pane | `SUPPORTED` | Asked for directly, 2026-07-27: *"a file explorer tab that will show the file tree at the left and the editor pane at the main screen"*. The ask said "editor"; the roadmap says an editor is permanently out of scope, so what shipped is a viewer — whether reading without editing satisfies the want is exactly what a week of use will show | [feature 0007](../features/0007-file-explorer.md) |
| A16 | The explorer earns enough use to deserve workbench affordances — tabs, search, remembered state | `SUPPORTED` | Asked for twice in one day, 2026-07-27: the enhancement list itself, then — offered a staged plan that would gate the expensive parts on observed use — an explicit *"make it a single pass"*. A deliberate commitment is stronger than a wish, and weaker than use; A15's caveat stands, and the dogfooding week now judges the whole workbench at once rather than in instalments | [feature 0008](../features/0008-explorer-workbench.md) |
| A17 | Hand-applied session labels beat derived titles for triage, and will keep being applied | `SUPPORTED` | Asked for directly, 2026-07-27, badge design included — a specific ask is stronger than a vague one. The failure mode is not week one but week two: labels are cheap to add and cost a hand-edit each to *maintain*, so the honest test is whether any label applied in the dogfooding week is still accurate at the end of it | [feature 0009](../features/0009-session-labels.md) |
| A18 | Commit history, per-commit diffs and blame are worth a pane beyond the Changes tab's session diff | `SUPPORTED` | Asked for directly, 2026-07-27, with features named and IntelliJ's changelist given as the reference — though the presentation was explicitly left open (*"I don't have a good idea how should we show it"*), so the spec carries open questions rather than answers. Weaker than the other asks in exactly that way | [feature 0010](../features/0010-git-view.md) |
| A19 | The git pane earns commercial-grade reading depth — branches, stashes, graph, re-blame — beyond log and status | `SUPPORTED` | Asked for as *"on-par with a commercial product"*, then — offered priority tiers — an explicit *"build P1 & P2 in one-go"*, 2026-07-28. The A16 shape: a deliberate one-go commitment, stronger than a wish, weaker than use. The dogfooding week judges each section separately; an unused section is a removal candidate, and the read-only fence is restated in the spec precisely because this is where "just add staging" pressure will arrive | [feature 0011](../features/0011-git-depth.md) |

## Notes on the most dangerous ones

**A1 and A6 are the product.** If either is false, mogeung has no reason to
exist, and no amount of polish on the review layer compensates. They are also
the cheapest to test: use it for a week. Everything on the roadmap is
speculation until they are resolved.

**A4 is the operational risk.** Everything rests on two undocumented file
layouts. The parser degrades rather than crashing, so the realistic failure is
mogeung quietly seeing *less* than it should — the worst kind, because it looks
like "nothing is happening" rather than an error.

It stays `AT RISK` — instrumentation does not make a private format stable. What
changed is that the failure is now *detectable*: every line is classified, and
an unrecognised type raises a named alert. The instrumentation immediately
earned its place by finding three event types that had been discarded silently
for the whole of v0.2. Nobody had noticed, which is precisely the point.

**A10 deserves scrutiny.** It was the opening complaint and remains untouched
after two versions. Either it matters and we have been avoiding it, or it
mattered less than stated. Worth deciding honestly rather than drifting.
