---
title: Run and debug
status: active
updated: 2026-08-20
covers:
  - crates/mogeung-core/src/run.rs
  - crates/mogeungd/src/detect.rs
  - crates/mogeungd/src/runconfig.rs
  - crates/mogeungd/src/bin/runconfigs.rs
  - crates/mogeungd/src/run.rs
  - desktop/src/panes/RunPane.tsx
---

# Run and debug

Where a run configuration comes from, and what makes one safe to name over a
socket. The *why* is [feature 0035](../features/0035-run-and-debug.md);
[ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md) and
[ADR-0026](../decisions/0026-other-peoples-run-configurations.md) are the
decisions this implements.

**The panel lives in the bottom dock**, on `Alt+4`, beside Changes and
Transcript — it is something you consult against the centre rather than the
thing you are doing. It moved there hours after it first shipped in the centre.

**Phase 1 is built.** `R-N1`–`R-N8`: the sweep, the reader, detection, process
ownership, the panel, masking, the corroboration type and mogeung's own
checked-in configuration. Phase 2 — the DAP client, breakpoints, per-language
adapters — is not started.

## Two sources, and one of them is the source

```
manifests ──> detect.rs ────┐
                            ├──> RunConfig ──> (R-N4: a process)
.vscode/*.json ─> runconfig.rs ─┘
```

`detect.rs` **is the source** and `runconfig.rs` is a bonus on top. That is
ADR-0026's decision, taken on a measurement rather than a preference, and
`R-N1`'s sweep is what keeps the measurement current: on this machine 35 of 58
repositories carry no run configuration file of any kind, and mogeung's own is
one of them.

The practical consequence is that **the panel is never empty because a project
did not adopt anything**. It is empty only when a project has no manifest we
understand, which the sweep also counts.

## `RunConfig`, and why the id is the security boundary

ADR-0025 clause 1 is *named, not supplied*: the wire carries a configuration
**id**, never a command string, so reaching the port lets you run this
repository's own test suite rather than giving you a shell.

That makes [`RunConfig::id`](../../crates/mogeung-core/src/run.rs) load-bearing
rather than convenient, with two properties:

- **Stable across restarts.** A client holds an id and asks for it later.
- **Derived from origin, directory and command** — never from a counter. A
  counter would silently re-point at a different configuration when the list
  changed underneath a client holding one, which is the failure that turns an
  id indirection into a worse version of passing the command.

It is readable (`detected:desktop:npm-test`) because an id appears in a wire
payload, a log line and a bug report, and *"which configuration is `a3f19c`"*
is a question with no cheap answer.

`Origin` is carried to the surface so the panel can say which entries were
inferred — ADR-0026 named *"a wrong inference is now a first impression"* as
the price of promoting detection, and an unlabelled guess is that price
unpaid.

## Detection is a closed set

Every command in `detect.rs` is compiled in. A manifest supplies a **name** — a
workspace member, an npm script — and never a program or an argument shape. So
ADR-0025 clause 1 holds by construction: there is no file whose contents become
a command line.

| Manifest | Offered |
|---|---|
| `Cargo.toml` with `[workspace]` | `cargo test --workspace`, `cargo build` |
| `Cargo.toml` with `[package]` | `cargo test`, `cargo build` |
| a member **with a binary** | `cargo run -p <name>` |
| `package.json` | every script, as `npm test` / `npm start` / `npm run <x>` |
| a Python project | `python -m pytest` |
| `gradlew` / `mvnw`, else `build.gradle` / `pom.xml` | `test` and `build` targets |

Ordering is tests, then builds, then anything long-running, because the first
three lines are what the panel is judged by and `R-N7` exists to put a run in
front of a claim about tests.

Three rules exist because their absence produced a confidently wrong list, and
each has a test named after the failure:

- **`cargo run -p X` only where there is a binary.** This is the good answer to
  ADR-0026's *"why is `cargo run -p mogeung-tray` in my list"*, and why
  `mogeung-core` is absent.
- **A workspace member is not its own project.** `cargo test` at the root
  already covers it; offering both produced twenty entries where thirteen were
  wanted.
- **An empty `[workspace]` is a crate detaching from its parent**, not a
  workspace whose sole member is itself. Tauri writes one into every
  `src-tauri`, and misreading it made that crate exclude itself.

**A `tests/` directory alone is not a Python project.** ADR-0026's wording
allows it and every Rust crate here has one, so the first run offered
`python -m pytest` for `mogeungd`. It counts only when it holds `.py`.

## Classification, for the file we do read

`runconfig.rs` reads `.vscode/launch.json` and `tasks.json`, tolerating comments
and trailing commas because VS Code does. `HANDLED` and `KNOWN_IGNORED` are
**decisions, not a schema** — the `type` field is an open namespace extended by
every debug extension — and anything in neither is `Unclassified`: listed with
its type named, never hidden, because a configuration silently missing reads as
*"mogeung did not find it"*.

The key is **`type/request`**, not `type`. ADR-0026 says *"a `type` we run goes
in `HANDLED`"* and the corpus is one word short of that: the only real
`launch.json` on either machine carries `lldb` twice, once `launch` and once
`attach`, and only one of them is a thing this cut can honour. The compound is
reported the way [`codex.rs`](../../crates/mogeungd/src/codex.rs)'s nested
taxonomy already is.

The two files classify against **separate lists**, because they share no
vocabulary — a `type` of `process` means nothing in `launch.json` — and one
list answering for both would let a decision about tasks quietly make a debug
type runnable.

## What is measured and never parsed

IntelliJ's `.run/*.run.xml`, `.idea/runConfigurations/*.xml` and
`.idea/workspace.xml` are counted by the sweep and read by nothing. ADR-0026
deferred them; `R-N12` holds the condition to take them up; and the sweep keeps
counting because *"a deferral with no measurement behind it becomes permanent by
forgetting."*

**That is what happened, on 2026-08-20** — the sweep's own numbers took the
deferral up. 243 checked-in configurations across 13 of 59 repositories, all in
the user's own projects, against one `launch.json` belonging to a clone, and a
namespace of **three** types where `workspace.xml` has nine.
[ADR-0028](../decisions/0028-intellij-when-there-is-a-debugger.md) reads the two
halves apart: the checked-in files are taken up, `workspace.xml` stays deferred
on the namespace it is actually about.

Still read by nothing, and that is now a **sequence** rather than a doubt. Not
one of the 243 names a command that can be spawned — 194 are a Java main class
plus a module whose classpath IntelliJ resolves, 49 are composites of those — so
the reader waits for `R-N9`, whose JDWP attach is the thing that makes them
start. The sweep must keep counting `workspace.xml` even now that a decision not
to read it exists, or the deferral loses the evidence that keeps it a decision.

They are an **inventory, never a verdict** — nine distinct types appear on this
machine, including `docker-deploy` — so they can never fail the sweep. A check
that can never pass is a check people stop running, and stopping would cost the
measurement `R-N12` depends on.

## The sweep is the instrument

`cargo run -q -p mogeungd --bin runconfigs` reads the same `HANDLED` /
`KNOWN_IGNORED` the parser uses, over whatever repositories are on this machine,
and **exits non-zero when a VS Code type is unclassified**. It reports two
numbers that only mean something together:

- how many repositories carry **no configuration file** — the population a
  parser cannot help, and ADR-0026's argument for detection;
- how many **detection** can offer nothing for — whether the replacement
  actually reaches them.

`--detected` prints the entries themselves, which is the only form in which a
wrong inference can be seen rather than reported.

## Owning a process

`run.rs` holds a `Runs` per daemon: a `Run` and a bounded output ring per
child. Four things are load-bearing.

**All four ADR-0025 clauses are enforced where a process is actually spawned**,
not where a request arrives — a check on the *configuration* could be walked
around by anything that rewrites one:

1. `start()` takes a **configuration id** and looks it up in what the
   repository produced. An id that is not there is refused, naming the clause.
2. `is_agent()` is checked on the program about to be spawned. The limit is
   known and ADR-0025 states it: a script in the repository that goes on to
   call `claude` walks past this.
3. Runs live on `AppState`, so one survives the window closing.
4. `runs_allowed(bind, --allow-run)` mirrors `writes_allowed` exactly — one
   place computes *is this safe*, so the start-up refusal and the per-request
   gate cannot come to disagree.

**Children get their own process group** (`setsid`), and stopping signals the
group. A `cargo test` that leaves its test binary running is a stop button that
stops nothing.

**Stopped is not Exited.** *"You stopped it"* and *"it failed"* are different
answers to **did the tests pass**, and `R-N7` shows this beside a claim, so
`Run::passed()` returns `None` for a stopped run.

**The output ring is bounded and says what it dropped.** A log that quietly
loses its middle is worse than one that admits it.

## Secrets never travel

`RunConfig` carries `env_keys: Vec<String>` — **names, and there is nowhere to
put a value**. That is deliberate: a shape that *could* carry values is one
somebody fills in later, and ADR-0026 warned that a mask added afterwards is a
mask that was missing. Values are read from disk at the moment of spawning, and
revealing one is a separate verb taking a **single key**.

The test serialises a configuration whose `env` holds a fake key and asserts the
secret is absent from the payload — the thing that actually travels, so a field
added tomorrow still fails it.

## A run beside a claim, never merged

`Corroboration` puts what the agent said next to the runs you did, and has **no
field that merges them**. A run mogeung executed must not be able to launder a
claim the agent made; `R-E3`'s standard holds — *unverified means no completed
check, not no passing check*.

## Not yet built

All of Phase 2: `dap.rs`, breakpoints in the file pane, the Debug panel, and
per-adapter language support (`R-N9`–`R-N11`). Also `R-N14` (Docker and
compose), and `R-N12` — IntelliJ and Java, **taken up in principle on
2026-08-20 and sequenced behind `R-N9`** rather than deferred on evidence any
more (ADR-0028).
