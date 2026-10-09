---
title: Gau Geung — a team of agents
status: draft
updated: 2026-10-09
roadmap: [R-P1, R-P2, R-P3, R-P4, R-P5, R-P6, R-P7, R-P8, R-P9, R-P10, R-P11, R-P12, R-P13]
depends_on: [A1, A24, A39, A51, A52, A53, A54]
---

# 0051 — Gau Geung: a team of agents

## Spec

*Owned by the human. What and why, never how.*

### Problem

mogeung watches sessions one at a time and ranks them. It cannot run a *team*:
start four agents on four parts of one job, in four worktrees, tell each what
the others did, hand a baton from one to the next, and show the whole on one
screen. OpenRig (`~/projects/openrig`) does exactly that and shows it in a
TUI, and the ask on 2026-10-08 was to bring it here: *"I want to add the
features to Mogeung, as a mode where on the UI, I can view something similar
to the TUI provided there, but better, with more info, and looks more like a
real application."*

The gap analysis found that about a third of OpenRig is observation mogeung
already does or could do without a decision. The rest starts, steers and
configures agents, which [ADR-0003](../decisions/0003-observe-do-not-spawn.md)
refused and which v0.1 was thrown away over. So the first deliverable was not
code: it was the list of every decision in the way, read back one at a time,
and the amendments that followed. Those are in (`R-P1`). This spec is what
comes after them.

### Decisions already taken, in the owner's words

All on 2026-10-09, and recorded as amendments so the reasoning stays with the
rule it changed.

- **A seat is owned; nothing else is.** *"1, 2 is fine to just relax … it has
  to be able to spawn agent, and own an agent."* ADR-0003 narrows *never an
  agent* to *never an agent it did not start itself*. Five fences; ownership
  by **birth only** (*"1. birth only"*). ADR-0011, ADR-0025 and ADR-0038
  follow.
- **Hop budget of three, sixty automatic sends an hour.** *"ok, hop budget
  default 3"* and, the same day, *"per hour cap 60?"* An agent-to-agent chain
  is held after three hops and ranks as needing you; zero is *every message
  through you*. A team may make sixty automatic deliveries in an hour — one a
  minute, wakes, watchdogs and handovers included — and the sixty-first is
  held the same way. A pause switch beside both.
- **Nothing is written under any user home; a separate home is opt-in.**
  *"I don't want it affect the normal usage of a claude session"* and *"we
  can just use separate home as opt-in."*
  [ADR-0043](../decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md):
  a seat is configured at launch through its CLI's own flags.
- **The team gets a component of its own, designed in a session with
  wireframes.** *"we need a new UI component to display the 'team' … I quite
  like the TUI concept where it display the overall status, but I want
  another brainstorm and planning session on this UI design."* Recorded, not
  designed: `R-P7`.
- **Team config is built in the window first; import and export later.**
  *"UI come first for the 'team config' and later I can import/export the
  config."* ADR-0043 clause 8 stands: daemon-owned, mirrored one way.
- **One box on day one.** *"let's have a team on the same box only."*
- **A tmux client with a grouped layout, not a TUI.** *"ideally I want it to
  have a special grouped layout."* `R-P8`.
- **A team's worktree is written.** *"I don't think we should keep it as 'a
  viewer not an editor' anymore, at least for the gau-geung mode."*
  [ADR-0019](../decisions/0019-a-viewer-not-an-editor.md)'s amendment: only a
  worktree the daemon created for a team; a save that refuses under a busy
  seat or a file that moved.
- **A handover is model-composed.** *"a handover recap should be model
  composed, which could write much better than a human user."* ADR-0003's
  later amendment, [ADR-0031](../decisions/0031-consent-to-a-named-host.md)
  and [ADR-0034](../decisions/0034-the-draft-is-a-chat-ask.md).
- **Git stays local; hook events are classified.** *"ok"* to both: a seat's
  worktree is created, and removed only when clean on a separate click; hook
  payloads get the transcript parsers' discipline.
- **Deferred, to review again:** MCP for seats, hooks for observed sessions,
  teams across machines. *"defer, but we need to review this again."*
- **A seat's pane is read; no other.** *"seats only for now, write the
  pane-read ADR."* [ADR-0044](../decisions/0044-mogeung-reads-a-seats-pane-and-no-other.md):
  a closed set of shapes per CLI, `unknown` otherwise, the capture never
  kept; a known shape may rank with *(screen)* in its detail; one press,
  Enter, for the one shape that asks for it; trust and login stay yours.
  Observed panes are that ADR's revisit, after a week of seat captures.

### Still open

- **Where the component lives** — a mode on a chord, as the wall is, or a
  home screen that argues with `A1`. Decided in the design session.

### Assumptions

- **A1** `SUPPORTED` — the queue changes where you look. A team of seats is
  more rows on a board built around one queue, and the component has to keep
  a seat that needs you on that queue rather than beside it. The design
  session's constraint.
- **A24** `UNTESTED`, rewritten 2026-10-09 so the assumption column no
  longer says *read-only*: the daemon's write, run and seat verbs sit behind
  the token, `--allow-run` and per-follow consent, and that is what is bet
  on. Every verb this spec adds goes behind the same gates.
- **A39** `SUPPORTED` — a button stays a button. The fence that a human's
  send keeps its confirmation; every automatic send is a different thing with
  its own fences, and this row is not stretched to cover them.
- **A51** `UNTESTED` — hooks and a status line through `--settings` fire for
  that session only.
- **A52** `UNTESTED` — a flags-only launch leaves the user's home as a plain
  launch would.
- **A53** `UNTESTED` — a worktree of a trusted repository shows no trust
  dialog.
- **A54** `UNTESTED` — the gate shapes a CLI draws are few, stable and
  readable from a capture's tail. ADR-0044's whole bet.

> Four are `UNTESTED`, so the work is `R-P2`: test them. Nothing in `R-P3`
> onward is built until `A51` and `A52` have a verdict, because ADR-0043's
> default depends on it.

### Acceptance

- [ ] I can define a team in the window — its seats, each seat's CLI,
      directory or fresh worktree, guidance and permission mode, and the
      team's policy — and the definition survives a daemon restart.
- [ ] I can boot the team and see each seat as a row marked as a seat, in one
      tmux session with the team's layout, before any seat has written a line.
- [ ] A seat that needs me ranks on the same queue as every other session.
- [ ] A seat's message to another seat is delivered, shown on both rows with
      who, what and why, and the fourth hop in a chain is held and ranks as
      needing me; my send resets the chain.
- [ ] One switch pauses every automatic send for a team, and typing in a pane
      always works.
- [ ] A hand-started `claude` on this machine shows nothing of mogeung's in
      `/hooks` and its home is byte-identical before and after a team ran
      (`A52`'s test, run as a check rather than once).
- [ ] A seat in a worktree I had already trusted asks me nothing (`A53`).
- [ ] In a team's worktree I can edit a file in the Code pane and save; the
      save refuses with a diff when a seat changed the file under me, and
      refuses while a seat is busy unless I say so.
- [ ] A restarted seat receives a handover I can read in full on its row,
      labelled with the model and host that wrote it, and a team with no model
      endpoint restarts its seats without one and says so.
- [ ] Stopping a team leaves its worktrees; removing one is a separate click
      that refuses when the worktree is dirty.
- [ ] A session I started by hand never receives a wake, a watchdog or a
      handover, by any path — pinned by a test over the whole send path.
- [ ] A seat sitting on a permission menu, a question, a trust dialog or a
      login says which on its row, with the lines it read; a screen it cannot
      read says `unknown`; and no pane I opened myself is ever captured —
      pinned by a test over the capture's one entry point.

### Explicitly out of scope

- Adopting an observed session as a seat. Birth only.
- A TUI client. The team's tmux session is the terminal view.
- Teams across machines; a seat on a followed machine.
- A read-only MCP surface for seats; hooks for observed sessions (`R-B73`).
- Writing anything under `~/.claude`, `~/.claude.json`, `~/.codex`,
  `~/.qwen` or opencode's directories, ever.
- Pressing through any gate from a screen reading other than *press Enter*:
  a permission menu is the CLI's permission mode, settled at launch; trust
  and login are yours (ADR-0044 fence 5).
- Reading any pane the daemon did not create. Observed panes are ADR-0044's
  revisit, not this spec's.
- OpenRig's agent-driven configuration (its permission-policy skill), its
  bundle installs, and its projection of skills and plugins into a workspace.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

In the order the rows are numbered, and nothing past `R-P2` before `A51` and
`A52` have a verdict.

0. **`R-P2`, phase 0.** Three scripts under `scripts/`, run by hand on this
   machine: launch one `claude --settings` seat with three hooks and a status
   line and watch the daemon's receiver; hash the two home files around a
   seat and around a plain launch; boot into a fresh worktree of this
   repository; and a fourth for `A54` — capture a seat's pane by hand through
   every gate (permission menu, question, plan, trust, login, press Enter,
   exit) for each CLI and save each as a fixture. Verdicts go on the ledger
   rows, and ADR-0043 is amended if the first two fail.
1. **`R-P3` and `R-P4`, the daemon's model.** A `Team` and a `Seat` in
   `mogeung-core`; a `teams` table and a `seat_launches` launch record in the
   store; a `Team*`/`Seat*` wire family carrying ids (ADR-0025's amendment);
   the launcher, which builds the generated files under `~/.mogeung/seats/`,
   composes the CLI's command through the exhaustive match ADR-0029 requires,
   and starts it in the team's tmux session. Re-adoption on daemon start from
   the launch record. Judged per connection like the run verbs.
2. **`R-P5`, signals.** A loopback HTTP receiver on the daemon's existing
   server, a token per seat; the Claude `--settings` document and the
   opencode plugin file generated by the launcher; hook events classified in
   `HANDLED` / `KNOWN_IGNORED` with a corpus fixture.
3. **`R-P6`, sends.** The seat send path reuses `R-B54`'s `send.rs`
   (load-buffer, paste-buffer, Enter, addressed by pane id). The policy
   engine is a small state machine per team — wakes and watchdogs on seat
   events, agent-to-agent delivery through a mailbox the seats write to by a
   command mogeung gives them — with the hop counter, the hourly cap, the
   pause flag, and the send log as rows the panel reads.
4. **`R-P7` and `R-P8`, the component and the layout.** After the design
   session. Wireframes first, as an artifact; then the component in
   `desktop/src/`, on the chord or as a home per the session's decision.
5. **`R-P9`, writes.** `WriteFile` on the wire with team, path, content and
   base hash; the refusal rules in the daemon; Monaco's `readOnly` flipped
   only for a team worktree in Gau Geung mode, with the pane header saying so.
6. **`R-P10`, the handover.** A meta-prompt in `mogeungd`, composed through
   the existing model endpoint under ADR-0031's consent, triggered by the
   policy engine's seat events, sent through the same path as any automatic
   send.
7. **`R-P11`, the screen.** The capture type minted by the launcher and
   accepted nowhere else, the per-CLI shape matchers with a `screens` fixture
   corpus, the `screen` field on the seat row, the ranking hook with
   *(screen)* in the detail, and the one press. Built after `R-P4`, measured
   for a week before `R-P6`'s readiness press is turned on. **`R-P12`** and
   **`R-P13`** after the rest.

### Files touched

To be listed per phase when each is approved. Phase 0 touches `scripts/` and
the ledger only.

### Risks and unknowns

- `A51` and `A52` are the whole of ADR-0043's default, and neither has been
  run.
- Agent-to-agent delivery needs a way for a seat to *address* another seat.
  OpenRig gives agents a `rig send` command and a control socket. mogeung's
  equivalent is a small command on the seat's `PATH`, generated under
  `~/.mogeung`, that posts to the receiver — which is the one place a seat's
  process talks to the daemon and has to be treated as such.
- The pause switch must be the last thing to fail: a daemon that cannot
  reach its own policy engine must behave as if paused.
- A team of four is four transcripts, four diffs and four ranked rows on a
  board tuned for one person's handful of sessions. `R-J57`'s rent applies.

### Test strategy

- The send path has one entry point and a test asserts that no call to it is
  reachable with an observed session's id — fence 1, pinned.
- The hop counter and the hourly cap have table tests, including the reset on
  a human's send and the hold on the fourth hop.
- A launcher test builds every CLI's command line from a definition and
  asserts no write outside `~/.mogeung` and the team's worktree, by running it
  against a fake home per ADR-0006.
- The write verb's refusals — hash moved, seat busy — have tests before the
  pane is made editable.
- `A52`'s home-hash check is a script that can be re-run after any change to
  the launcher, not a one-time measurement.

## Notes

*Filled during implementation. Surprises, dead ends, things the plan got wrong.*

- 2026-10-09: decided before anything was built, on purpose — the epic's
  first phase was the gap analysis and its second was the fences. The design
  session for `R-P7` is next, with wireframes as an artifact.
