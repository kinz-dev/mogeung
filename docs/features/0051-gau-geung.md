---
title: Gau Geung — a team of agents
status: draft
updated: 2026-10-10
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
- **The component is a mode, with both views, and seat detail is a pane.**
  From the wireframes, 2026-10-09: *"1. mode 2. graph and table 3. pane."*
  The team component is a mode on a chord like the wall, never a home
  screen, so the queue stays chrome and `A1` is untouched. Topology shows the
  graph and the table together, not one behind a toggle. A seat's detail is a
  pane in the mode's own tile tree — holdable and splittable like any pane,
  as `R-B49` made the Agent pane — not a drawer that follows the selection.
- **Five lanes for seats in a team; badges for everything else.** *"Can we
  keep the five lanes when it is a part of a team?"* Yes. The pulse is the
  team's, so OpenRig's five lanes — needs you, parked with baton, blocked on
  agents, now, just finished — apply to seats, each card carrying mogeung's
  badge inside; a session outside a team keeps its badge on the queue and
  never enters a lane.
- **The window arranges its own grid; tmux's layout is for a terminal.**
  *"of course the window arrange its own grid. I thought that is not a
  question and by existing design."* It is: each seat's terminal is a pane
  attached on its own, as the Agent pane is, dragged, held and popped out
  like any pane; the daemon's grouped layout of the team's tmux session
  (`R-P8`) is for `tmux attach` from a terminal and the window does not
  mirror it.
- **Two layers for guidance and permission mode; one for the rest.** *"agree
  per your recommendation for the two layers approach."* A team carries a
  guidance every seat gets and a default permission mode; a seat adds its own
  guidance and may override the mode. CLI, model and worktree are per seat
  only, since they are never shared.
- **A seat talks only to its own team.** *"ok, a seat only talks to its own
  team."* A team is the unit of policy, so a delivery between teams would be
  a delivery between two budgets and two pause switches; it is refused, and
  two teams that must talk are one team or talk through you. ADR-0003's
  evening amendment of 2026-10-09.

### Still open


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
- **A51** `SUPPORTED` — hooks and a status line through `--settings` fire for
  that session only. Run 2026-10-09 by `scripts/geung-a51.sh`: three hooks
  and the status line fired, all with the seat's id, and the plain launch
  after it reached none.
- **A52** `SUPPORTED` — a flags-only launch leaves the user's home as a plain
  launch would. Run 2026-10-09 by `scripts/geung-a52.sh`: `settings.json`
  untouched, and every key the seat changed in `~/.claude.json` is one every
  launch changes (the tip rotation is the nuance; the ledger has it).
- **A53** `SUPPORTED` — a worktree of a trusted repository shows no trust
  dialog. Run 2026-10-09 by `scripts/geung-a53.sh`: straight to the prompt,
  and no project entry written for the worktree's path.
- **A54** `UNTESTED` — the gate shapes a CLI draws are few, stable and
  readable from a capture's tail. ADR-0044's whole bet. Twelve fixtures
  pinned 2026-10-09 by `scripts/geung-a54.sh`; the week of seat captures is
  `R-P11`'s.

> Phase 0 ran on 2026-10-09: `A51`–`A53` are `SUPPORTED`, so ADR-0043's
> default stands and `R-P3` may start. `A54` stays `UNTESTED` with its first
> fixtures pinned; `R-P11` measures it for a week before `R-P6`'s readiness
> press is turned on.

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
- [ ] Two teams run at once: the queue ranks both teams' seats on one board
      with each seat's team named, the all-teams view shows every team's
      counters and pause switch, and a seat's attempt to address a seat in
      the other team is refused with the reason shown.
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
- A message from a seat to a seat in another team.
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
   rows, and ADR-0043 is amended if the first two fail. **Done 2026-10-09**:
   `A51`–`A53` supported, twelve fixtures pinned, ADR-0043 unamended.
   **`R-P3` built the same evening** (⏳ until used): the definitions, the
   store, the wire and the Teams window. **`R-P4` built the same night** (⏳):
   the launcher, the launch record, seat rows from birth, Boot and Stop in
   the window, and a worktree removed only when clean.
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
   `HANDLED` / `KNOWN_IGNORED` with a corpus fixture. **Built 2026-10-10**
   (⏳): `hooks.rs` — the receiver's two routes outside the shared-token
   layer, the tables, the generated settings, script and plugin — and the
   launcher handing each CLI what it can carry; a Qwen seat in a home of
   its own gets hooks too, Codex none. The row's tag says *hooked* or
   *plain*; the Teams window shows what the hooks said. Not yet heard from a
   real seat.
3. **`R-P6`, sends.** The seat send path reuses `R-B54`'s `send.rs`
   (load-buffer, paste-buffer, Enter, addressed by pane id). The policy
   engine is a small state machine per team — wakes and watchdogs on seat
   events, agent-to-agent delivery through a mailbox the seats write to by a
   command mogeung gives them — with the hop counter, the hourly cap, the
   pause flag, and the send log as rows the panel reads. **Built 2026-10-10**
   (⏳): `policy.rs` — a ledger per team whose counters are read off the
   log, a judge over the pause, the chain and the hour — and one function
   that types into a seat, taking a launch record and nothing else; the
   `mogeung-send` command on every seat's `PATH` posting to
   `/api/seat/send` and resolved within the sender's team; wakes on `Stop`
   and `Exit` from hooks, from a plain seat's liveness and from its pane;
   watchdogs on the scan's tick; held sends on the recipient's row, ranked;
   the pause switch, release and drop as verbs; the Teams window's live
   sends panel. No real seat has sent yet.
4. **`R-P7` and `R-P8`, the component and the layout.** After the design
   session. Wireframes first, as an artifact; then the component in
   `desktop/src/`, on the chord or as a home per the session's decision.
   **`R-P7` built 2026-10-10** (⏳): `ui/GauGeungMode.tsx` on the bare
   `Alt+g` — a mode over the window with a tile tree of its own (a board, a
   seat pane that follows the pick, a terminal pane per seat you attach),
   the five lanes read off the send log, the graph and the table of the
   same seats, the policy strip with the switch and the held sends, tabs for
   one team or all; `lib/teamMode.ts` is the arithmetic. The Agent pane
   moved to `Alt+e`. Not yet used on a real team. **`R-P8` built the same
   day** (⏳): `seat::layout_team` builds `geung-<team>` around the seats'
   own sessions after a boot — a tiled window of nested clients, every
   seat's window linked in, the status line on — and a stop takes it down
   first; pinned against a real tmux with `sleep` for seats. The mode
   offers the attach command to copy.
5. **`R-P9`, writes.** `WriteFile` on the wire with team, path, content and
   base hash; the refusal rules in the daemon; Monaco's `readOnly` flipped
   only for a team worktree in Gau Geung mode, with the pane header saying so.
   **Built 2026-10-10** (⏳): `FileWrite` carries the session (a seat's row),
   the path, the content, the `base_hash` a `FileContent` now carries, and
   `force`; `write.rs` judges *moved* (no override) and *busy* (until
   `force`), resolves the path under the seat's worktree only, and writes
   by rename; the file pane is editable for exactly that case, keeps its
   draft through reloads, sends the hash the draft began from, and shows a
   refusal with the distance and the one override. Not yet saved over a
   real seat.
6. **`R-P10`, the handover.** A meta-prompt in `mogeungd`, composed through
   the existing model endpoint under ADR-0031's consent, triggered by the
   policy engine's seat events, sent through the same path as any automatic
   send. **Built 2026-10-10** (⏳): `handover.rs` — the prompt from the two
   layers of guidance, the teammates' turns and diffs, the send log and a
   restart's predecessor, every line framed as data; the answer kept whole
   and logged *composed by <model> @ <host>*; a `Handover` send kind that
   is a hop — held under *after you read it*, judged and typed under
   *automatic*, nothing under *none*; a refusal entry with no endpoint. The
   triggers are a boot into a team with history and `SeatRestart`, composed
   in the background. The mode's seat pane shows the last one whole with
   Deliver and Discard and offers the restart. Never composed by a real
   model over a real team yet.
7. **`R-P11`, the screen.** The capture type minted by the launcher and
   accepted nowhere else, the per-CLI shape matchers with a `screens` fixture
   corpus, the `screen` field on the seat row, the ranking hook with
   *(screen)* in the detail, and the one press. Built after `R-P4`, measured
   for a week before `R-P6`'s readiness press is turned on. **`R-P12`** and
   **`R-P13`** after the rest.

### Files touched

To be listed per phase when each is approved. Phase 0, done: `scripts/geung-lib.sh`,
`scripts/geung-a51.sh` to `scripts/geung-a54.sh`,
`crates/mogeungd/tests/fixtures/screens/<cli>/<gate>.txt` with a `.meta`
beside each, and the ledger.

`R-P3`, built: `crates/mogeung-core/src/team.rs` (the types: `Team`, `Seat`,
`Policy`, `PermissionMode`, `Wake`, `Watchdog`), `wire.rs` (`TeamList`,
`TeamSave`, `TeamDelete`; `Teams`), `federation.rs` (both classified *here* /
*theirs*); `crates/mogeungd/src/team.rs` (validate, mirror, the signals a
seat gets), `store.rs` (the `teams` table), `state.rs` (`save_team`,
`delete_team`, `teams_dir`), `api.rs` (the arms and the gate);
`desktop/src/wire/types.ts`, `store/index.ts`, `lib/teams.ts`,
`ui/TeamsWindow.tsx` (opened from the top bar's Users button and
`Alt+Shift+g`; `Alt+g` is kept for the mode), `App.tsx`, `TopBar.tsx`,
`lib/keymap.ts`; tests in each. Docs: `wire-protocol.md`, `data-model.md`.

`R-P4`, built: `crates/mogeungd/src/seat.rs` (the launcher: names, the two
layers of guidance, `compose` per CLI, the marked block, the tmux start, stop,
the clean-only worktree removal), `mogeung-core/src/team.rs` (`SeatLaunch`,
`SeatTag`), `session.rs` (`seat`), `wire.rs` (`TeamBoot`, `TeamStop`,
`SeatWorktreeRemove`; `Seats`), `store.rs` (`seat_launches`), `state.rs`
(`launches`, `seat_row`, `tag_seats`, the seat rows in `refresh_provisional`,
`boot_team`, `stop_team`, `remove_seat_worktree`), `api.rs`; the window's
`types.ts`, `store/index.ts`, `TeamsWindow.tsx` (Boot, Stop, the seat's
status and *remove worktree*), `QueuePanel.tsx` (the seat chip). Docs:
`architecture.md`, `wire-protocol.md`, `data-model.md`.

`R-P5`, built: `crates/mogeungd/src/hooks.rs` (the receiver's tables
`CLAUDE_HANDLED` / `CLAUDE_KNOWN_IGNORED` / `OPENCODE_HANDLED` /
`OPENCODE_KNOWN_IGNORED`, `classify`, the status reading, the token, the
loopback admission, the generated `hook.sh`, Claude `settings.json`, Qwen
`settings.json` and opencode plugin, the capture file), `seat.rs` (`Receiver`,
the hooked arms of `compose`, the token on the record), `mogeung-core/src/team.rs`
(`SignalKind`, `SeatSignals`, `SeatTag.signals`, `SeatLaunch.token`,
`for_the_wire`), `state.rs` (`listen_addr`, `receiver_url`, `seat_signal`,
the token stripped in `seats_msg`), `api.rs` (`seat_router` merged outside
the shared-token layer, the two handlers), `server.rs` (the bind recorded),
`team.rs` (`signals_for` rewritten for Codex); tests in each, plus
`tests/seat_hooks.rs` (the receiver served as `server::run` serves it),
`tests/hooks_corpus.rs` over `tests/fixtures/hooks.jsonl` (synthetic shapes,
no real data), and the restart test extended for the token. The window's
`types.ts` (`SignalKind`, `SeatSignals`), `lib/teams.ts` (`signalsLine`, the
Codex sentence), `TeamsWindow.tsx` (the signals line under a running seat),
`QueuePanel.tsx` (the kind on the chip). Docs: `architecture.md`,
`wire-protocol.md`, `data-model.md`, `cross-session-signals.md`,
`read-boundary.md`.

`R-P6`, built: `crates/mogeungd/src/policy.rs` (the engine: `Ledger`,
`Engine`, `Verdict`, `judge`, the chain and the hour read off the log,
`held_for`, the watchdog memory, the liveness-to-`Stop` reading, the exit
once, `counters_text`), `state.rs` (`type_into` — the one function that
types into a seat, taking a launch record — `deliver`, `seat_send`,
`policy_event`, `policy_tick`, `human_sent`, `release_send`, `drop_send`,
`set_paused`, `sends_msg`, `held_send` on the rows, the `Sender` seam,
`save_team` keeping `paused`), `hooks.rs` (`SEND_PATH`, `send_script`,
`seat_event`, the counters on `status_text`), `seat.rs` (`team_tools_text`,
`bin/mogeung-send` and `PATH`, the token for every seat with `curl`),
`api.rs` (`/api/seat/send`, `TeamPause`, `SendRelease`, `SendDrop`, the
human send logged), `store.rs` (the `sends` table), `mogeung-core/src/team.rs`
(`SendKind`, `SendStatus`, `SendEntry`, `TeamSends`, `SeatEvent::label`),
`session.rs` (`HeldSend`, `Session.held_send`), `attention.rs` (a held send
ranks `WAITING`), `wire.rs`, `federation.rs`; tests in each, plus
`tests/seat_sends.rs` (the receiver, the chain, the hold, the release, the
refusals, the pause, a wake from a hook, a watchdog's tick, your own send,
the restart) and two gate tests in `tests/local_client_gates.rs`. The
window's `types.ts`, `store/index.ts` (`teamSends`), `lib/teams.ts`
(`countersLine`, `sendLine`, `sendsFor`), `TeamsWindow.tsx` (the live sends
panel: counters, the pause switch, held with release and drop, the log; a
seat's own last sends). Docs: `architecture.md`, `wire-protocol.md`,
`data-model.md`, `cross-session-signals.md`, `attention-ranking.md`,
`read-boundary.md`.

`R-P7`, built: `desktop/src/lib/teamMode.ts` (the lanes — `laneFor` reads
*parked* and *blocked* off the send log — `seatViews`, `sessionOf`,
`sourceOf`, `edges`, `ring`, the pane ids and the layout key, `teamTab`),
`ui/GauGeungMode.tsx` (the mode: header with team tabs and counters, the
second dockview, `TeamBoard` — `Pulse`, `Graph`, `Table`, `PolicyStrip` —
`SeatDetail`, `SeatTerminal`, `attachSeatTerminal`), `store/index.ts`
(`showGauGeung`, `gauTeam`, `gauSeat`), `lib/keymap.ts` (`gaugeung` on
`Alt+g` / `Alt+KeyG`; the Agent pane to `Alt+e`), `App.tsx`, `TopBar.tsx`
(the Network button); tests `lib/teamMode.test.ts`,
`ui/GauGeungMode.test.tsx` through the real dockview. Docs: `the-window.md`.

`R-P8`, built: `crates/mogeungd/src/seat.rs` (`team_session_name`,
`nested_attach`, `team_layout_argv`, `layout_team`, `unlayout_team`; the
seat's window named for it in `tmux_argv`), `state.rs` (`boot_team` lays
out, `stop_team` takes down first), `mogeung-core/src/team.rs`
(`SeatLaunch.team_session`); tests in `seat.rs` and `tests/team_tmux.rs`
(a real tmux, skipped where there is none). The window's `types.ts`,
`GauGeungMode.tsx` (the attach command to copy). Docs: `architecture.md`,
`data-model.md`, `the-window.md`.

`R-P9`, built: `crates/mogeungd/src/write.rs` (`hash_bytes`, `Refusal`,
`judge`, `resolve_for_write`, `write_atomic`), `state.rs` (`read_file`
answers the hash; `write_seat_file`), `api.rs` (`FileWrite` behind the write
guard and the runs gate; `FileContent.hash`; the REST reader's `hash`),
`mogeung-core/src/wire.rs` (`FileWrite`, `FileWritten`, `FileWriteRefused`,
`FileContent.hash`), `federation.rs`; tests in `write.rs`, the guard's list,
`tests/seat_write.rs` through the real router, a gate test. The window's
`types.ts`, `store/index.ts` (`FileTab.hash`/`saved`/`refusal`,
`WriteRefusal`, the two events), `lib/explorer.ts`, `panes/FilePane.tsx`
(the editable branch, the draft, the save, the refusal banner),
`FilePane.write.test.tsx`. Docs: `read-boundary.md`, `wire-protocol.md`,
`architecture.md`, `the-window.md`, `assumptions.md` (`A24`).

`R-P10`, built: `crates/mogeungd/src/handover.rs` (`Reason`, `Evidence`,
`Input`, the budgets, `prompt`, `read_answer`, `composed_by`,
`turns_from_events`, `send_line`), `state.rs` (`evidence_for`,
`compose_handover`, `handover_for`, `handovers_after_boot`, `restart_seat`),
`api.rs` (the boot spawns the handovers; `SeatRestart`),
`mogeung-core/src/team.rs` (`SendKind::Handover`, `SendEntry.composed_by`),
`wire.rs` (`SeatRestart`), `federation.rs`, `policy.rs`; tests in
`handover.rs`, `tests/seat_handover.rs` against a fake streaming endpoint
that records the prompt, the gate test. The window's `types.ts`,
`lib/teams.ts` (`sendBy`), `GauGeungMode.tsx` (the handover whole, Deliver
and Discard, Restart with handover). Docs: `architecture.md`,
`wire-protocol.md`, `model-and-proxy.md`.

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
- 2026-10-09, phase 0, the same afternoon. What the scripts got wrong first:
  a tmux pane target is `=name:` with the colon — without it `send-keys` and
  `capture-pane` answer *can't find pane* and every screen dump is empty, so
  the first `A51` run typed nothing and reported no `Stop`. The TUI's idle
  words depend on the permission mode: auto mode says *shift+tab to cycle*,
  manual mode *? for shortcuts*, and the scripts match both. What the runs
  taught: `~/.claude.json` is written on every launch, by every launch, and
  the tip rotation makes two launches differ at leaf level, so `A52`'s rule
  compares key families and keeps project entries exact; a worktree gets no
  project entry of its own; the trust dialog on 2.1.295 says *Accessing
  workspace*; Codex's update notice is a *press Enter* gate the closed set
  did not name but does cover; Qwen and opencode have no trust gate; and
  every Claude capture here carries a file-tree plugin's column, which is
  what the user's screen looks like and so what a seat's will.
- 2026-10-09, `R-P3`. Teams are stored the way notes are — the second kind
  of thing the daemon could not recompute — with one difference the amendment
  forced: a save is judged like a run is started, not like a note is saved,
  because a definition is what a later click starts. The window mirrors the
  daemon's validation so Save is grey with the daemon's words before a click,
  and the daemon's answer still decides. The wireframe's fields are the
  struct's fields, one for one; the only thing drawn and not built is Boot
  and Stop, which are `R-P4`'s and say so in their tooltip. The first
  restart test wrote its `delete` against the real `~/.mogeung/teams` before
  the rebooted state had its mirror pointed away — caught by the assertion,
  harmless because the directory did not exist, and the reason `teams_dir`
  is a `OnceLock` a test must set, like `notes_dir`.
- 2026-10-09, `R-P4`. One tmux session **per seat**, not one per team: the
  window attaches to each seat's pane on its own (decided 2026-10-09) and
  `tmux_panes` already keys everything on the pane id, so a session per seat
  is what the scan, the send path and the Agent pane all already understand;
  `R-P8`'s grouped layout can link the seat windows into a team session for
  `tmux attach` without moving a process. The one write into a worktree is
  Codex's guidance — a marked block in `AGENTS.md`, taken back at stop —
  because Codex has no flag that appends to its instructions and
  `model_instructions_file` replaces them; a Codex seat in the team's
  checkout with guidance and no home of its own is refused in words. Codex
  has no plan mode and is refused that mode by name. A seat whose pane has
  gone is a dead row until the team is stopped, so a seat that died is seen
  rather than silently missing. Nothing here has booted a real team yet: the
  launcher's composition, the records, the rows and the gates are tested,
  the tmux start is `yolomo`'s shape with `-P -F` for the pane id, and the
  first boot is the dogfooding `R-P4`'s ⏳ waits on.
- 2026-10-10, `R-P5`. The receiver is merged **beside** the shared-token
  layer rather than under it: the shared token is a window's key, and a
  hook that had to carry it would put that key into every seat's
  environment. A seat's token is 256 bits minted at launch, compared in
  constant time, persisted with the launch record so a daemon that restarts
  still hears the seat it re-adopts, and stripped in `seats_msg` so no
  client ever holds it — pinned by a test that reads the `Seats` event off a
  real socket. Loopback is judged before the token and the token before the
  body, so a LAN peer learns nothing. What the bundles said, read on this
  machine the same day: Claude Code 2.1.296 names twenty-six hook events —
  seven are subscribed (start, end, prompt, stop, subagent stop,
  notification, pre-compact) and the rest are listed as ignored so one
  arriving anyway is a decision rather than drift; Qwen Code 0.23.0 uses the
  same names and `hook_event_name` in its `settings.json`, so a Qwen seat in
  a home of mogeung's own is hooked the same way (not yet run — a thing the
  first Qwen boot checks); opencode 1.18.35 loads a plugin named by file URL
  in the inline config, and its `event` hook sees every bus event, so the
  plugin filters to nine before posting — `message.part.updated` fires per
  token and a receiver is not a firehose. Codex's hooks live in `config.toml`
  beside trust hashes, a format nobody here has read, so a Codex seat stays
  *plain* even in a home of its own and the sentence beside it says so,
  replacing `R-P3`'s promise. The status line is a reading, not an event: it
  fires on every repaint, so it is never kept, never counted, and broadcast
  only when the fill or the model moved; its reply is the line the seat
  shows. Hook bodies are kept beside the seat's files as `signals.jsonl`,
  capped at 8 MiB, which is `R-P11`'s week of captures started early. The
  phase-0 run directories were gone from `/tmp` by the time this was built,
  so `tests/fixtures/hooks.jsonl` carries documented shapes with synthetic
  values rather than captures; the first real boot replaces that by
  evidence. Nothing ranks on a hook yet — `R-P6` reads `signals` for its
  wakes and watchdogs — and no real seat has posted to the receiver: the
  routes, the tables and the generated files are tested, the first hooked
  boot is the dogfooding ⏳ waits on.
- 2026-10-10, `R-P6`. The counters are **read off the log** rather than
  kept beside it: a chain is the run of hop deliveries since the last entry a
  human caused — a send, or a release — and an hour is the automatic
  deliveries newer than sixty minutes, so there is one state to keep right
  and a restarted daemon counts what it counted before (pinned by a test over
  a reopened store). What counts as a hop was the one call the amendment left
  open: a seat's message is, a wake is — one seat's event typed into another
  is the shape a loop with nobody watching takes — and a watchdog is not,
  because a nudge to a silent seat is caused by nothing another agent did;
  it counts against the hour only. **One function types into a seat**,
  `type_into`, and it takes a launch record: fence 1 is a type, not a check,
  and the test for *no path reaches an observed session* is that `human_sent`
  on an untagged session leaves every ledger untouched and nothing else can
  be called with one. The seat's command posts plain text with the recipient
  as a query parameter, because a JSON encoder in bash 3.2 is a bug waiting
  to be written; the daemon's one-line answer becomes the command's exit code
  — 0 delivered, 2 held, 1 refused — so an agent reading either knows which,
  and a held message is told it is not lost. A plain seat gets the command
  too, so a plain seat now carries a token (the R-P5 rule *a plain seat keeps
  no token* was for a seat with no way back at all, which is now a seat
  without `curl`). The pause switch became its own verb after the first
  design showed the trap: a form opened before the pause and saved after it
  would un-pause the team on its way through, so `TeamSave` keeps the
  `paused` the daemon holds and `TeamPause` is the one writer. A plain seat's
  `Stop` is its registry going busy to idle, read on the tick — Codex's
  turn end reaches the policy that way, since its hooks are unread — and an
  exit is read off the row once per boot. The readiness press stays a flag
  nothing reads: `R-P11`'s week first. Nothing here has sent for real: the
  routes, the judge, the command and the rows are tested with a recording
  sender, and the first hooked boot that runs `mogeung-send` is the
  dogfooding ⏳ waits on.
- 2026-10-10, `R-P7`. The lanes were the one piece of the wireframes that
  needed a definition rather than a drawing: OpenRig's *parked with baton*
  and *blocked on agents* are both idle seats, and on mogeung's queue both
  rank `WAITING`. The send log is what tells them apart — a seat idle after
  a message *arrived* holds the baton and has not passed it on; one idle
  after *sending* waits on the seat it told — so `laneFor` reads the last
  delivery involving the seat and the lane is a fact rather than a mood. A
  watchdog's nudge has no seat at its tail and moves nobody between lanes.
  The mode holds a second dockview rather than panes in the centre, as the
  terminal panel does, because the decision was *a pane in the mode's own
  tile tree*; the board and the seat pane are always present, a terminal
  pane per seat arrives on *Attach terminal* and is a `tmux attach` of its
  own, and the arrangement is saved per machine. The bare `Alt+g` had been
  kept for the mode since `R-P3` and was still the Agent pane's; the pane
  moved to `Alt+e`, its third chord, and the Mac keeps `⌘G`. Nothing in the
  mode is asked of the daemon: it is a reading of the store, and the only
  verbs it sends are the Teams window's three. Not yet opened over a real
  team.
- 2026-10-10, `R-P8`. A pane belongs to one window, so a tiled window of
  the seats could not be made from their panes without taking them out of
  the sessions the scan, the send path and the window all key on. The team
  session is built *around* them instead: nested tmux clients, one per
  seat, tiled in a window named `all` — a tmux inside tmux works once the
  inner one is told it is not inside (`env -u TMUX`) — and every seat's
  window linked in by name for the full-screen case. The property worth a
  real server in the tests is the one `kill-session` has: it closes only
  windows linked nowhere else, so taking the team session down leaves every
  seat standing, and a stop does it first so the nested clients let go
  before the seats end. One thing tmux 3.6 taught on the way: `set-option
  -t` takes a bare session name and answers *no such session* to the
  `=name` exact-match spelling every other target accepts. Not yet attached
  to from a terminal over a real team.
- 2026-10-10, `R-P9`. Two things the plan's one line hid. The hash is the
  **daemon's**: `FileContent` carries sha256 of the whole file as served,
  so the window needs no hash function and a save against a body the viewer
  cut short still fails the comparison it should. And the base is pinned in
  the pane on the **first keystroke**, not read from the store at save time:
  `R-J38`'s reloads fire exactly when an agent writes the file you are
  editing, and a pane that followed the store's new hash would then save
  over the agent with the daemon's blessing. So the draft stays on screen
  through a reload and the save carries the hash it began from, and the
  daemon's refusal is what you see. Fence 1 is a lookup before the path is
  read: the session's seat tag, then the launch record's `worktree` flag —
  an observed session has no record and a checkout seat's says no. Nothing
  here has saved over a real seat yet.
- 2026-10-10, `R-P10`. The handover is the first model answer that goes
  into an agent's input, and the prompt is built for that: every transcript
  line is quoted as data another agent wrote, and the model is told so
  before any evidence appears — a teammate's transcript can say anything,
  and a handover that obeyed it would launder an agent's words into the
  daemon's. The budget is the reading guide's lesson again: so many turns
  per seat, so many characters per turn, a cap on the whole, and the cut
  falls on evidence, never on the directions. Three of the amendment's
  conditions fell out as plain code: *shown in full* is a text kept whole
  (a wrapping fence removed, a cap, never a summary); *logged as the
  model's* is `composed_by` on the entry; *counts as a hop when it carries
  what another seat said or did* became *always a hop*, the conservative
  reading, because the prompt always carries the teammates. *Baton passed*
  is not a trigger: an agent's own message already carries its words, and a
  composed note on every hop would double every send. The trigger that is
  built is a boot into a team with history — a cold boot has nothing to
  hand over and says nothing — and `SeatRestart`, which reads the earlier
  session's evidence before it ends it. One test deadlocked on its own
  mutex, not the daemon's: a `&guard[0]` in a `let` keeps the guard for the
  block. No real model has composed one over a real team yet.
