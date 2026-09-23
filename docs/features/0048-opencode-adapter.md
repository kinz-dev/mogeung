---
title: opencode adapter
status: in-progress
updated: 2026-09-23
roadmap: [R-I25]
depends_on: [A4, A23]
---

# 0048 — opencode adapter

**Honesty note.** Every shape this adapter reads was read from real rows of
`~/.local/share/opencode/opencode.db` on the author's machine (opencode
1.18.32, 2026-09-23) — and, unlike any earlier adapter, many were read **while
in flight**, by polling the database read-only during a live turn: a step
still open, a tool mid-run, a tool blocked on a permission prompt, the
`question` tool waiting on a human, a subagent's child session, a hang-up
mid-tool. The corpus was built for this work, on the owner's say-so, by
running opencode against the owner's local `spark` model in scratch
repositories: **11 sessions, 143 rows**. It is small, one model, one
provider, and has never seen a compaction, a provider retry, an image or a
provider error on disk — see *Risks*.

The opencode binary is a compiled Bun executable whose bundle is minified, so
unlike Qwen's the writer's source could not be read alongside the rows. Where
this adapter relies on how opencode behaves rather than on a row it wrote,
the Notes say so.

## Spec

### Problem

An opencode session running beside Claude, Codex and Qwen sessions is
invisible to the queue. Asked 2026-09-23: *"Can we enhance mogeung to support
opencode on top of existing qwencode, codex, claudecode?"* — full parity with
Qwen's adapter chosen, disk only.

### Assumptions

- **[A4](../product/assumptions.md)** — *the on-disk formats are stable enough
  to depend on* — `AT RISK`, always. opencode's position is mixed. Better: the
  database has a **migration table** (38 entries) and a drizzle-generated
  schema, so its layout moves by named migration rather than silently. Worse:
  the row payloads are JSON with no version field, and the binary is minified,
  so a shape change in `data` is only visible in the rows themselves.
- **[A23](../product/assumptions.md)** — *the Session model generalises beyond
  Claude Code* — `UNTESTED`.

> The rule says: if an assumption is `UNTESTED`, the work is to test it, not to
> build the feature.

This adapter is that test a third time, on the hardest case yet: a source
whose sessions are **not files** and which keeps **no liveness registry**. If
`Session` absorbs it without a field, A23's structural half holds under the
two things every earlier source had in common. It did. The end-to-end half —
a working day of real use — is the box below that keeps the row ⏳.

### Acceptance

- [x] An opencode session appears in the queue with its cwd, title, prompt,
      turn count, tool calls and tokens.
      *`tests/opencode_scan.rs::a_finished_opencode_turn_is_a_live_session_waiting_on_you`,
      and live against the real database through an isolated daemon.*
- [x] A finished turn reads as **waiting on you**; a turn in flight — a step
      open, a tool running, the next step not begun — reads as **working**.
      *`opencode.rs` unit tests over verbatim rows; live, the same session
      went working → waiting as the turn ended.*
- [x] **A tool blocked on a permission prompt ranks APPROVE**, naming the tool
      and what it wants to run; approving it clears the prompt.
      *`a_prompt_opencode_wrote_down_ranks_as_approval_and_clears_when_answered`.
      Live: while opencode showed "Permission required", the queue said
      `needs approval for bash: git status --short`; after Enter in the pane,
      working; then waiting.*
- [x] The `question` tool reads as **waiting on you**, with the question as
      the activity. *`the_question_tool_is_the_agent_waiting_on_you_with_its_question`.*
- [x] Liveness is a running `opencode` process in the session's directory,
      not the rows: a turn left open by a crash is not in progress, and
      yesterday's session is not claimed by a TUI opened this morning.
      *`without_an_opencode_process_a_session_is_not_live_whatever_its_rows_say`,
      `yesterdays_session_in_the_same_directory_is_not_claimed_by_a_new_process` —
      both against a real process the kernel calls `opencode`.*
- [x] An opencode session under tmux can be **hosted**, not merely pointed at,
      and `scripts/opencodemo` is the launcher that puts it there.
      *`an_opencode_session_under_tmux_can_be_hosted` drives a real tmux pane;
      live, `mogeung-oclive:0.0` resolved while the prompt was up.*
- [x] What a session wrote is its diff, and the rest of a shared worktree is
      not. *`what_a_session_wrote_is_its_diff_and_the_rest_of_the_worktree_is_not`.*
- [x] **A headless start from the New session window is on the board at once**,
      as *waiting for your first prompt* — before opencode has written anything —
      and the real session takes the pane over when you speak to it.
      *`a_fresh_opencode_in_tmux_is_waiting_for_you_before_it_has_written_anything`;
      live, the pane row became `ses_…` holding the same tmux target, with
      the placeholder gone. Reported by the owner the same day — see Notes.*
- [x] Subagent and archived sessions are not rows of yours; a live session
      archived later stops being alive.
- [x] `Session` gains **no field** for opencode. A23's structural test.
- [x] An unrecognised part type, tool state or role raises a named health
      alert prefixed `opencode/`; a database whose layout moved is reported,
      not fatal. *`an_unknown_opencode_shape_reaches_the_health_alerts`,
      `a_database_whose_layout_moved_is_reported_rather_than_fatal`.*
- [x] mogeung **never writes** to opencode's database — not a byte, not a side
      file. *`the_scan_never_writes_to_opencodes_database`.*
- [x] `--bin sweep` sweeps the database and exits non-zero on anything
      unclassified. *Run 2026-09-23: 11 sessions, 143 rows, 0 unclassified.*
- [x] mogeung refuses to **start** `opencode`, per ADR-0025 clause 2, and the
      New session window can start one, with `--auto` quoted.
- [ ] Dogfooded: a real opencode session, beside real Claude sessions, tiers
      correctly in the window over a working day. *Not yet — this keeps the
      row ⏳.*
- [ ] Seen on a Mac. The process walk there is `ps` and `lsof`, parsed by
      tested functions, but no Mac has run it.

### Explicitly out of scope

- **Transcript events** in the detail pane, as for Codex and Qwen: queue
  presence, status and diff — not a readable transcript.
- **Dollars.** opencode records its own `cost`; [ADR-0024](../decisions/0024-equivalent-cost-in-dollars.md)
  answered *show the CLI's own figure?* with no, and the local model has no
  public price anyway.
- **Analytics token burn**, which walks `~/.claude/projects` only — the same
  gap Codex and Qwen have. Per-session counters on the queue are unaffected.
- **Asking opencode's server.** A running opencode TUI listens on a random
  loopback port and could be asked for session status and pending
  permissions over HTTP. Considered and declined on 2026-09-23 in favour of
  disk only — and then the disk turned out to carry the prompt anyway.
- **opencode's `verify_runs`.** Claude's build/test detection is not mapped
  onto opencode's `bash` calls; an opencode session that changed code is
  never *unverified*, which is the conservative wrong answer.

## Plan

### Approach

A module beside the other three, per [ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md),
shaped like `codex.rs` because the source is a database:

- **Read-only, always.** `SQLITE_OPEN_READ_ONLY | NO_MUTEX`, a 250 ms busy
  timeout, never a side file. The same database holds opencode's `account`
  and `credential` tables; they are never queried.
- **`SELECT * FROM session`**, mapped by column name; a missing required
  column is a health error, not a panic.
- **Change detection from `event_sequence`** — one row per session whose
  `seq` rises on every update, part updates included — so a tick re-reads
  only sessions that moved, and then only rows at or after the last
  `time_updated` seen. An undo deletes rows, so a re-read also prunes ids
  that no longer exist.
- **Status from the latest message and its tool parts** (`derive_status`),
  in this order: question, approval, error, `finish`.
- **Liveness from the process table**: `/proc` on Linux with no fork; `ps` +
  one `lsof` elsewhere. Directory by `$PWD`, then a path argument, then cwd;
  `-s <id>` names the session outright.
- **`transcript_path` is a locator, not a file** — `<db>#<id>` — so the
  panels that stream Claude JSONL find nothing, and the diff still runs.
- **`touched_files`** from `patch` parts and completed `edit`/`write` calls,
  which scopes the diff and feeds collision detection.

### Files touched

- `crates/mogeungd/src/opencode.rs` — **new.** Discovery, classification,
  status, the incremental reader, the process walk and the match.
- `crates/mogeung-core/src/session.rs` — `SessionSource::OpenCode`, on the
  wire as `opencode`.
- `crates/mogeung-core/src/run.rs` — `opencode` on the never-start list.
- `crates/mogeungd/src/state.rs` — `AgentHomes.opencode`, `scan_opencode`,
  `absorb_opencode`, the launch recipe and tmux name, the provisional-row
  source.
- `crates/mogeungd/src/bin/sweep.rs` — a fourth corpus.
- `crates/mogeungd/tests/opencode_scan.rs` — **new**, 12 end-to-end tests.
- `desktop/src/wire/types.ts`, `lib/agentHealth.ts`, `ui/AgentGlyphs.tsx`,
  `ui/SourceMark.tsx`, `ui/LaunchWindow.tsx`.
- `scripts/opencodemo` — **new**; `scripts/install.sh` installs it.

### Risks and unknowns

**The approval rule is behaviour, not a contract.** "Running with no
`metadata`" meaning *blocked on a prompt* was watched for `bash` and `edit`,
and it follows from opencode asking permission before a tool starts and a
started tool reporting metadata at once. A future opencode that writes
metadata earlier would turn every prompt back into *working* — the Qwen blind
spot, silently. A future one that runs a tool for seconds before its first
metadata would raise a false APPROVE. The rule is scoped to the tools it was
seen for, with a 2.5 s grace, and tested on both sides.

**Two opencode processes in one directory** are told apart only by recency,
and may be paired crosswise: liveness right, pane possibly wrong. Stated in
`match_processes`.

**A resumed session you have not typed into** does not read as live until it
moves: nothing on disk says which session a TUI has open. Under tmux the
pane's provisional row covers the gap.

**Unseen shapes.** `subtask`, `compaction` and `retry` parts are classified
from the part union without a real row; no provider error has been seen on
disk (an unknown model made `opencode run` hang rather than write one).

**Federation.** A daemon from before this change that follows one after it
will meet a session whose `source` it cannot parse. Both of the owner's
machines move together, so this is noted rather than engineered around.

### Test strategy

- `crates/mogeungd/src/opencode.rs` unit tests — 18, over rows copied from
  the real database, including the in-flight ones.
- `crates/mogeungd/tests/opencode_scan.rs` — 12 end-to-end tests over a
  database built with opencode's own DDL, with a real process the kernel
  names `opencode` and a real tmux pane.
- Window tests: queue filter, source mark, health colour, launch window.
- `--bin sweep` over the real database, which must exit 0 for opencode.
- **Live**: a second daemon from this build, on its own port and database,
  with a `HOME` holding nothing but a link to the opencode data — so it
  followed no peers and read no Claude sessions — watching a real opencode
  TUI through a permission prompt, an approval, the end of the turn, and the
  pane closing.

## Notes

**2026-09-23 — what the build turned on.**

*The first thing checked was whether the blind spot was real, and it was
not.* The owner chose disk-only on the understanding that opencode, like
Qwen, would not write a pending approval down. Running the TUI in tmux with
`bash` set to `ask`, and polling the database while the prompt was up, showed
the tool part as `{"status":"running","input":{…},"time":{"start":…}}` —
and, after pressing Enter from outside, `{"…","metadata":{"output":""}}`
before any output existed. The same for `edit`; the same shape for the
`question` tool with the question in its input. So the adapter does what the
Qwen one cannot, from the disk alone.

*`opencode run` could not have found this.* It auto-rejects every permission
prompt (`! permission requested: bash (…); auto-rejecting`), which is its own
row shape — a tool `error` with *"The user rejected permission…"*. Only the
interactive UI waits, which is why the corpus has TUI sessions in tmux.

*opencode takes its directory from `$PWD`, not from its cwd.* Found by
accident: a scratch run started with the right cwd and a stale `$PWD` wrote
its file into the stale directory and recorded that as the session's
`directory`. So the process walk reads `$PWD` from `/proc/<pid>/environ`
first. tmux 3.6 sets `$PWD` for `new-session -c` even when it execs the
command directly — checked, because the launcher depends on it.

*The live test found a gap the fixtures had hidden.* The first isolated daemon
showed every finished opencode session as *ended with no changes*, including
one that had edited a file. The diff refuses to run for a session with an
empty `transcript_path` — a guard for sessions that have never been read — and
an opencode session has no file. It now carries a `<db>#<id>` locator, and
`touched_files` from opencode's own `patch` parts, so the diff runs and is
scoped. `what_a_session_wrote_is_its_diff…` fails without the locator.

*The session row does not move when a tool part does.* So `time_updated` on
`session` could not be the change signal; `event_sequence` could, and was
found by counting: every session's `seq` equals its number of events minus
one, part updates included.

*A test harness finding worth keeping.* The first fake `opencode` process was
a symlink to `sleep`, which on this machine is the multi-call uutils binary:
it dispatches on the name it was run as, printed *unknown program
'opencode'* and exited. The kernel names a `#!` script's process after the
script, so the fake is a four-line shell script that forwards `TERM` to its
`sleep`.

*A hang-up is tidy.* Killing the tmux session left the turn closed — the
message stamped `MessageAbortedError`, the tool `error` with
`metadata.interrupted` — so an interrupted turn reads as over. A `SIGKILL`
would leave it open, which is why liveness never comes from the rows.

*One more*: the activity line briefly read "```", because a reply opened with
a code fence. Fences are now skipped.

**Reported the same day: *"I can't start a headless session with the 'start a
session' popup. I clicked on it, and the window closed. But nothing appear in
the ATTENTION list."*** The launch had worked — the journal showed five tmux
panes of opencode's ~900 MB footprint started that minute — and every one of
them had been on the board. The cause was the gap the live test could not
reach, because it always sent a prompt: **opencode writes no session until you
send one**, so a freshly started opencode is only a tmux pane with an agent in
it — an [ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md)
provisional row. And that row carried no status, so it ranked *running*, which
the default *needs you* scope hides for five minutes. The fix is ADR-0038's
2026-09-23 amendment: an unclaimed opencode is at its prompt, so its pane row is
**waiting for you**, clocked from the process's start. Checked live by
starting opencode headless in tmux, reading the isolated daemon, typing a
prompt into the pane, and watching the row hand over to the real session.

*Why the five sessions ended after one to two and a half minutes is not
known.* A headless opencode started the same way stayed up for as long as it
was left — over six minutes — so nothing here closes them.

**Reported again after reinstalling: *"I still can't see it if I start a new
headless opencode."*** The installed daemon had the row — a read-only snapshot
showed `pane:%178` ranked *waiting for you* — and the window still showed
nothing. The fix above had been verified by connecting a fresh client each
time, and a fresh client is exactly the case that worked: **a pane row reached
a window only in the snapshot it got on connecting.** It was never broadcast,
and the queue broadcasts that followed ranked stored sessions only, so an
already-open window never learnt of it and the window drops a queue row whose
session it does not know. That is `R-J75`'s bug rather than this adapter's, and
it had been there since pane rows were built; every other CLI writes a record
within a second, so its pane row was never the only thing on the board for
long enough to be missed.

Fixed in the daemon ([ADR-0038](../decisions/0038-a-pane-running-an-agent-is-a-session-provisionally.md)'s
amendment, second part): pane rows are ranked in every queue broadcast and
announced as they appear, change and go — dead first, then removed, so the
window's succession moves your selection and the pane you are typing into onto
the real session. Checked the way the failure happened this time: a client
connected **before** opencode started saw the row two seconds after, and when a
prompt was typed into the pane saw the real session, the row dead with the
same pid, and the row removed — in that order.
`an_open_window_is_told_when_a_fresh_opencode_appears_and_when_it_becomes_a_session`
fails without the broadcast; `succession.test.ts` pins the window's side.
