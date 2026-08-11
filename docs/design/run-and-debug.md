---
title: Run and debug
status: active
updated: 2026-08-11
covers:
  - crates/mogeung-core/src/run.rs
  - crates/mogeungd/src/detect.rs
  - crates/mogeungd/src/runconfig.rs
  - crates/mogeungd/src/bin/runconfigs.rs
---

# Run and debug

Where a run configuration comes from, and what makes one safe to name over a
socket. The *why* is [feature 0035](../features/0035-run-and-debug.md);
[ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md) and
[ADR-0026](../decisions/0026-other-peoples-run-configurations.md) are the
decisions this implements.

**Only the reading half exists.** `R-N1` and `R-N3` are built; nothing here
starts a process yet. `run.rs`, the daemon's ownership of a child, the wire
verbs and DAP are `R-N4` onwards.

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

## Not yet built

`R-N2` (entries become `RunConfig`s, with a health alert for the unclassified
and a human's entry beating a detected one with the same command), `R-N4`
(the daemon owning a process), `R-N5` (the panel), `R-N6` (env masking),
`R-N7` (a run beside the claim), and all of Phase 2.

The ADR-0025 refusals are **written but not yet enforced anywhere**:
`run::is_agent` and `run::agent_refusal` exist and are tested; the verb that has
to call them is `R-N4`.
