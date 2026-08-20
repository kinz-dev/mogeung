---
title: Run and Debug
status: active
updated: 2026-08-20
roadmap: [R-N1, R-N2, R-N3, R-N4, R-N5, R-N6, R-N7, R-N8]
depends_on: [A4, A32, A33, A34]
---

# 0035 — Run and Debug

## Spec

*Owned by the human. What and why, never how.*

### Problem

An agent finishes a turn and says *"all tests pass."* mogeung already knows a
great deal about that sentence: `R-E1` recorded the `cargo test` the session
ran, `R-E3` bound the claim to it or visibly to nothing, and `R-B4` knows
whether the session is waiting on you or on a tool. What it cannot do is the
one thing you actually want in that moment — **run it yourself and watch.**

So you leave. You switch to RustRover, find the test, run it, and come back to
a queue that has moved on. The verification loop the product spent three
features building ends one step short of the answer, and the step it is missing
is the cheapest one.

Asked directly 2026-08-09: *"I am thinking about add a 'Run and Debug' feature
in mogeung. To make a real IDE like environment."*

### Assumptions

| Id | Claim | Status |
|---|---|---|
| [A4](../product/assumptions.md) | Undocumented formats move without warning | `AT RISK`, permanently — and this feature adds one more to the pile |
| [A32](../product/assumptions.md) | Reading the IDEs' run configurations gives us enough to run without a format of our own | **`AT RISK`** — measured 2026-08-09, and the response is to stop depending on it: detection carries this feature, see below |
| [A33](../product/assumptions.md) | The user will run and debug inside mogeung rather than switching to the IDE | `UNTESTED` |
| [A34](../product/assumptions.md) | Verification is the human's remaining job, so the run belongs beside the claim | `UNTESTED` |

> **The rule applies and it is not a formality.** A32 is `AT RISK` on
> measurement taken while writing this spec, and A33 and A34 are `UNTESTED`.
> **So the first work is not the panel.** It is `R-N1` — the sweep that turns
> A32 from one afternoon's finding into a check that can be re-run — and then
> the smallest thing that puts a real run in front of a real claim, which is
> what A33 and A34 need in order to be judged at all.

**What A32 was measured against**, 2026-08-09, across all 19 git repositories
on this machine:

| Source | Repos |
|---|---|
| `.vscode/launch.json` | 1 (a third-party clone) |
| `.vscode/tasks.json` | 0 |
| `.idea/runConfigurations/*.xml` | 0 |
| `.run/*.run.xml` | 1 |
| `.idea/workspace.xml` → `RunManager` | 10 |
| **mogeung itself** | **none of the above** |

The full reading is in
[ADR-0026](../decisions/0026-other-peoples-run-configurations.md). The short
version: **five checked-in configuration files across nineteen repositories.**
Whatever the IDE split, the dominant case is a project with no shared run
configuration at all, and the second most common is one whose configurations
are private to a machine. What all nineteen *do* have is a manifest that
already says how the project is built and tested.

So **detection is the source**, `launch.json` is a bonus on top, and IntelliJ's
configurations and Java are **deferred** — cut from this scope on 2026-08-09,
with `R-N12` holding the reasons and the condition to take them up.

### Acceptance

Phase 1 — Run:

- [ ] With no configuration file anywhere, opening mogeung on this repository
      offers `cargo test --workspace`, `cargo build`, and `npm test` in
      `desktop/` — inferred, and labelled as inferred.
- [ ] A `.vscode/launch.json` entry appears with its own name, and beats a
      detected entry with the same command.
- [ ] A `launch.json` type mogeung does not understand is **listed and named as
      unrunnable**, not hidden, and raises a health alert.
- [ ] Starting a run shows output as it arrives, and the exit code when it
      ends. Stopping it stops the process.
- [ ] A run started from a session's row is attributed to that session, and
      appears beside the claim it bears on: *"the agent said tests pass; you
      ran it; here is what happened."*
- [ ] Closing the window and reopening it shows the run still going.
- [ ] An `env` value is masked until deliberately revealed — in the panel, in a
      copy, in an export, and in the wire payload.
- [ ] A configuration that would launch `claude` or `codex` is refused with a
      message naming [ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md).
- [ ] A daemon bound to anything but loopback refuses to run at all unless
      `--allow-run` was passed.

Phase 2 — Debug:

- [ ] A breakpoint set in the file pane is hit, and execution stops on it.
- [ ] Call stack, frame-local variables and stepping (over / into / out /
      continue) work for Python.
- [ ] The same, for Rust and Node/TypeScript.
- [ ] With no adapter installed, the panel says **which** adapter is missing,
      where it was looked for, and how to install it — and does not look broken.

### Explicitly out of scope

- **IntelliJ's run configurations, and Java.** Deferred 2026-08-09, not
  refused; `R-N12` holds both, with what would take them up. The sweep keeps
  measuring them regardless — a deferral with no measurement behind it becomes
  permanent by forgetting.
- **Writing any configuration file.** Read-only, permanently as far as this
  spec is concerned — ADR-0026.
- **Editing code.** Unchanged and unaffected;
  [ADR-0019](../decisions/0019-a-viewer-not-an-editor.md) stands.
- **A build system.** mogeung runs what your project already builds with. There
  is no dependency graph, no incremental anything, no task orchestration.
- **Test discovery and a test-tree UI.** Running a *file* or a *test by name*
  is a natural second ask and is deliberately not here: it needs per-language
  test enumeration, which is a feature the size of this one.
- **Remote debugging over ssh** in phase 2. The daemon-side design makes it
  reachable later; nothing is built for it now.
- **Docker / compose configurations**, which the sweep found in the corpus.
  `KNOWN_IGNORED`, listed and named — and held by `R-N14` from 2026-08-10, so
  that this bullet is a deferral rather than the forgetting the one above warns
  about. That row also records why it is not simply another entry in
  `detect.rs`: ADR-0025's refusal matches the *outer* binary, so a service whose
  `command:` is an agent CLI walks straight past it; a container outlives the
  process that started it, so `R-N4`'s stop-by-kill leaks; and compose's
  `environment:`/`env_file:` are `R-N6`'s masking rule again, in a new file.
- **Kubernetes as a run target** — asked about 2026-08-11, and this bullet is a
  **refusal rather than a deferral**, which is the difference the one above
  turns on. The compose hazard is that a container outlives the process that
  started it; in a cluster that is not a hazard but the entire point, because a
  Deployment's job is to restart what you killed and `R-N4` stops a run by
  killing its child. A *stop* button that leaves the workload running is worse
  than no button. It stays in pillar `K`. The unrelated half — **watching** an
  agent that happens to run in a cluster — is an observation question, not a
  run one, and lives at `R-I14` behind `R-I13`.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

Four layers, each usable before the next exists.

**1. Detection is the source.** `detect.rs` offers a closed, compiled-in set of
commands from the manifests present in the tree — `cargo` per workspace member,
`npm` scripts, `pytest`, a gradle or maven wrapper. Nothing is parsed from a
format that can move, and nothing a client sends can influence what it
produces, which is ADR-0025 clause 1 satisfied for free. It is also the only
layer that makes the panel non-empty in this repository.

**2. `launch.json` is a bonus on top.** `runconfig.rs` reads
`.vscode/launch.json` and `tasks.json` when they exist, tolerating comments and
trailing commas, and a human-written entry wins over a detected one with the
same command. Two lists — `HANDLED` and `KNOWN_IGNORED` — decide what each
`type`/`request` means, and anything in neither becomes a health alert and an
`Unsupported` entry that **still appears in the list**. `adapter.rs`'s shape
deliberately, down to the sweep binary. IntelliJ's sources are measured by the
sweep and parsed by nothing (`R-N12`).

**3. The daemon owns runs.** `runner.rs` already exists for terminal launching
and is the wrong shape; this is a new `run.rs` holding a `RunSession` per
process: pid, config id, the session it was started from, a bounded output ring
buffer, and a status. Output is broadcast as events on the existing websocket,
gated exactly as `ChangeUpdated` is. The wire gains `RunStart { session_id,
config_id }`, `RunStop { run_id }`, `RunOutput`, `RunExited` — and, per
ADR-0025, **no verb that carries a command**.

**4. Debug is a DAP client the daemon owns.** `dap.rs` speaks the protocol over
stdio to an adapter found by a documented search order. The window renders
stack, variables and breakpoints; breakpoints are set in the file pane, which
already has Monaco and already knows the file and the line. Language support is
then per-adapter configuration rather than per-language code — which is the
entire reason for choosing DAP.

**Where it meets the agent** is the part that is not IDE parity and is the
reason this belongs in mogeung at all. A run carries the session it was started
from. `R-E1`'s `VerifyRun` records what the *agent* ran and whether the tool
call came back; a mogeung-owned run has a real exit code. The two are shown
together and **never merged** — a run we did must not be able to launder a
claim the agent made. `R-E3`'s wording ("unverified means no completed check,
not no passing check") is the standard to hold to.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeungd/src/detect.rs` | New — closed set of inferred commands; the source |
| `crates/mogeungd/src/runconfig.rs` | New — parse `launch.json`/`tasks.json`, classify, degrade |
| `crates/mogeungd/src/run.rs` | New — process ownership, output buffers, lifecycle |
| `crates/mogeungd/src/dap.rs` | New — DAP client, adapter discovery |
| `crates/mogeungd/src/bin/runconfigs.rs` | New — the sweep, non-zero on anything unclassified |
| `crates/mogeung-core/src/run.rs` | New — `RunConfig`, `RunState`, wire types |
| `crates/mogeungd/src/api.rs` | The four verbs, and the `--allow-run` gate |
| `crates/mogeungd/src/health.rs` | Unclassified config types join the canary |
| `crates/mogeungd/src/state.rs` | Runs hang off sessions; bind to `verify_runs` |
| `desktop/src/panes/RunPane.tsx` | New — the panel from the screenshot |
| `desktop/src/panes/DebugPane.tsx` | New — stack, variables, breakpoints |
| `desktop/src/panes/FilePane.tsx` | Breakpoint gutter |
| `docs/design/run-and-debug.md` | New design doc, `covers:` the above |

### Risks and unknowns

- **A33 is the whole bet and it is untested.** IntelliJ's debugger is
  excellent and free to switch to. If the answer is "I ran it once in mogeung
  and then went back", this should be removed, not improved — the removal
  condition belongs in A33 before any of it is built.
- **Detection has a low ceiling and it now carries the feature.** No arguments,
  no environment, no attach, no *"the way I actually run this service"*. The
  first time that ceiling is hit is the first honest evidence for `R-N12`, and
  it needs recording rather than working around.
- **A wrong inference is a first impression.** Detection is the panel's opening
  content, not a safety net under a parser, so *"why is `cargo run -p
  mogeung-tray` in my list"* has to have a good answer before this ships.
- **DAP adapter discovery is guesswork across platforms** — `codelldb` in a VS
  Code extensions directory, `lldb-dap` in an Xcode toolchain, `debugpy` in
  whichever virtualenv. A wrong guess is a feature that silently does nothing,
  so *say what was searched* is a requirement, not polish.
- **Secrets.** Already observed, not hypothetical: the sweep found a plaintext
  API key in an `<env>` element of a checked-in `.run.xml` in this corpus.
  mogeung will not read that file in this cut, but `launch.json` has an `env`
  block of exactly the same kind. Masking has to be in from the first commit
  that renders a configuration, because a mask added later is a mask that was
  missing.
- **Orphans.** The daemon dies holding children. Process groups and a kill on
  shutdown, and a startup pass that adopts or reaps what it finds.
- **Scope gravity.** Every IDE feature invites the next one — test trees,
  coverage, profilers. The out-of-scope list above is the fence.

### Test strategy

- **The sweep is a test**, in both directions: a corpus it understands exits
  zero, an unclassified type exits non-zero and is named. `R-J28`'s four tests
  are the template.
- **Fixture repositories** under `crates/mogeungd/tests/fixtures/runconfig/`:
  a `launch.json` with comments and a trailing comma, one with a type nobody
  handles, one whose entry duplicates a detected command (the human's must
  win). Parsing is pure and needs no process.
- **Detection is asserted against this repository itself** — a test that says
  mogeung offers `cargo test --workspace` for its own root would fail today and
  keeps failing if the detectors regress.
- **Process lifecycle against `/bin/echo` and `sleep`** — no language toolchain
  in the suite, and nothing that costs anything. Exit codes, stop, output
  ordering, buffer bounds.
- **The ADR-0025 refusals get tests**, both of them: a config resolving to
  `claude` is refused by name, and a non-loopback bind without `--allow-run`
  refuses to start anything.
- **Masking is a test, not a review item**: an env value must not appear in the
  wire payload, the export, or the copy.
- **Detection is asserted against the corpus `R-N1` walks**, not only against
  fixtures: the number that matters is how many of the nineteen repositories
  here get at least one usable entry, and it should be 19.
- **DAP against `debugpy`**, marked `#[ignore]` and run by hand — it needs an
  installed adapter, and a suite that silently skips is worse than one that
  says it skipped.

## Notes

*Filled during implementation. Surprises, dead ends, things the plan got wrong.*

- **2026-08-09, before any code.** The sweep that produced the A32 table was
  written to answer *"should we read `launch.json` or IntelliJ's XML?"* and
  answered a different question instead: **neither, on its own, would have
  worked.** The plan of record before it ran was launch.json-first, which would
  have shipped a panel that found exactly one configuration on this machine, in
  a vendored third-party repository. Detection went from fallback to first-class
  because of a fifteen-line shell loop. It is the cheapest thing in this
  document and it changed the design.
- **2026-08-09, an hour later.** The scope was then cut — *"may be just drop the
  intellij's runConfiguration and the Java support for now"* — and the same
  table read a third way, which is the one that stands. Counting *files* rather
  than *IDEs* gives five checked-in configurations across nineteen
  repositories: the split between VS Code and IntelliJ is a detail, and the
  finding is that most projects have **none**. Worth recording that the cut
  improved the plan rather than merely shrinking it — the IntelliJ-first
  version would have spent its largest parser on a gitignored file with an
  open-ended type namespace, to reach ten repositories that detection reaches
  anyway.
- **2026-08-11, `R-N1` built — and the first run disagreed with the table it
  was written to reproduce.** Same tool, different machine: Linux, **58**
  repositories rather than nineteen. VS Code is unchanged and if anything worse
  — `launch.json` in **1** of 58, still `github/codex`, `tasks.json` in none —
  but `.idea/runConfigurations/*.xml` is in **5** repositories where the Mac
  measured **zero**, and `.run/*.run.xml` in **8** where it measured one. All
  thirteen are the user's own projects. **35 of 58 (60%) still carry nothing at
  all**, so `R-N3`'s rank is untouched and detection still has to cover the
  majority alone. What moved is the deferral: A32's own SUPPORTED condition
  (*"configurations exist for the projects actually worked in"*) is now met, in
  the one format this cut declined to read. Recorded against `R-N12` rather
  than acted on, which is what ADR-0026 asked for — and worth noticing that the
  sweep produced its first real finding on the day it was written, exactly as
  the shell loop it replaces did.
- **Three things the plan did not have**, all found by writing it.
  **The classification key is `type/request`, not `type`.** ADR-0026 says *"a
  `type` we run goes in `HANDLED`"* and the corpus is one word short of that:
  the single real `launch.json` carries `lldb` twice, `launch` and `attach`,
  and Phase 1 can honour exactly one of them. The compound is reported the way
  `codex.rs`'s nested taxonomy already is.
  **IntelliJ's types must never fail the run.** They are an inventory, not a
  verdict — nine distinct types appear here, including `docker-deploy` — and a
  check that can never pass is a check people stop running, which would cost
  the measurement `R-N12` depends on.
  **The walk skips dot-directories.** A deleted checkout in
  `~/.local/share/Trash` answered as a repository on the first real run, which
  read the one number this tool exists for 13% low.
- **2026-08-11, `R-N3` built, and the acceptance criterion earned its place.**
  The test strategy above asks for detection to be *"asserted against this
  repository itself — a test that says mogeung offers `cargo test --workspace`
  for its own root would fail today and keeps failing if the detectors
  regress."* It found **three** bugs on its first run, none of which a fixture
  would have shown.
  **`str::parse::<toml::Value>()` does not parse a document.** In `toml` 0.9 it
  parses a *value*, and fails at column 12 of `[workspace]`. Every `Cargo.toml`
  was rejected silently, so the list came back with npm entries and no cargo
  ones at all — the failure a fixture-only suite would have reproduced
  faithfully, since the fixture would have been written against the same wrong
  call. `toml::from_str` is correct.
  **A `tests/` directory is not a Python project.** ADR-0026 says *"pytest from
  a `pyproject.toml` or a `tests/` directory"*, and every Rust crate in this
  workspace has the latter — so `mogeungd` was offered `python -m pytest`. That
  is the *"wrong inference is now a first impression"* cost the same ADR named,
  arriving about an hour after it was written down. The directory now counts
  only when it contains `.py`.
  **A workspace member is not its own project.** Offering each member's
  directory separately produced twenty entries where thirteen were wanted,
  three of them for `mogeung-core`, which is a library and cannot be run at
  all. Skipping members fixed it and immediately hid `desktop/src-tauri` —
  because an **empty `[workspace]`** is how a nested crate detaches from its
  parent, Tauri writes one into every `src-tauri`, and the member-collector
  read it as a workspace whose sole member was itself.
  Worth recording that the second and third are the same lesson from opposite
  directions: detection's failures are not missing entries, they are **confident
  wrong ones**, and the only thing that surfaces them is running it against a
  real tree.
- **The sweep grew a second number, and the pair is the point.** `R-N1`
  reported how many repositories carry no configuration file; it now also
  reports how many detection can offer nothing for, because ADR-0026 moved the
  feature's weight onto detection and *"60% carry no file"* is only half an
  argument. `--detected` prints the list itself, which is the only form in
  which *"why is `cargo run -p mogeung-tray` in my list"* can actually be
  answered.
- **The acceptance list above has not been swept since phase 1 landed**, and it
  is left unticked rather than ticked from the code. Noticed 2026-08-20 while
  marking `R-N2` and `R-N4` built in the roadmap — the same omission, one level
  down. Some of these boxes are answerable from the test suite and some are
  not, and the ones that are not are the interesting half: *output as it
  arrives*, and **"closing the window and reopening it shows the run still
  going"**, which is a claim about a daemon this window may itself be hosting.
  Ticking a box because the code looks like it should pass is the failure this
  file exists to prevent, so the sweep belongs to `R-N13` — the week of use —
  and is named here so it cannot be mistaken for having been done.
