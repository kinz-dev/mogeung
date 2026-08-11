---
title: Detection is the source of run configurations; a file is a bonus
status: active
updated: 2026-08-09
decided: 2026-08-09
---

# ADR-0026 — Detection is the source of run configurations; a file is a bonus

## Context

[ADR-0025](0025-run-a-process-you-named-never-an-agent.md) decided that a run
request names a configuration rather than carrying a command. This ADR is about
where those configurations come from, and what speaks to a debugger.

The obvious answer was a format of our own — `.mogeung/run.toml`, small, honest,
ours. It was rejected by the person who asked for the feature, in the same
breath as asking for it: *"Instead of mogeung's own format. Which I think no one
care."* That is correct and worth writing down as more than a preference: a
config format is only worth defining if someone will write files in it, and a
tool that watches your agents is not a tool you will author a build config for.

So the configurations had to come from somewhere else, and the plan of record
was to read the IDEs'. **That raised a question worth measuring rather than
assuming**, and it was measured on 2026-08-09 across every git repository on
this machine — 19 of them, the same corpus discipline `R-J28` applies to
transcripts:

| Source | Repos carrying it |
|---|---|
| `.vscode/launch.json` | **1** of 19 — and it is `github/codex`, a third-party clone, not the user's own work |
| `.vscode/tasks.json` | **0** |
| `.idea/runConfigurations/*.xml` | **0** |
| `.run/*.run.xml` | **1** (4 configurations, a Java/Spring project) |
| `.idea/workspace.xml` → `<component name="RunManager">` | **10** |
| **mogeung's own repository** | **none of the above** |

Two readings were available and the first one is wrong.

**The first reading** — the one this ADR originally took — is *"IntelliJ is
where the configurations are, so read IntelliJ."* Ten of nineteen, against one
for VS Code. It implies parsing `.idea/workspace.xml`: per-user, conventionally
gitignored, rewritten live by a running IDE, and carrying an open-ended
plugin-defined `type` namespace (`CargoCommandRunConfiguration`,
`SpringBootApplicationConfigurationType`, `docker-deploy`,
`MAKEFILE_TARGET_RUN_CONFIGURATION` were all found in three repos).

**The second reading**, which is the one taken here after the scope was cut on
2026-08-09 — *"may be just drop the intellij's runConfiguration and the Java
support for now"* — is that the same table says something more useful about
**how many configurations are checked in at all**: five files, in two
repositories, out of nineteen. Whatever the IDE split, **the dominant case is a
repository with no shared run configuration of any kind**, and the second most
common is one whose configurations are private to a machine. A feature that
depends on parsing those is a feature that mostly does nothing.

What every one of those nineteen repositories *does* have is a manifest —
`Cargo.toml`, `package.json`, `pyproject.toml`, a gradle wrapper — that says
exactly how the project is built and tested.

## Decision

**Run configurations are detected from the project's manifests. Reading a
configuration file is a bonus on top, and the only file read is VS Code's.**

- **Detection is the source.** A closed, compiled-in set: `cargo test` /
  `cargo run` / `cargo build` from a `Cargo.toml` (per workspace member),
  `npm` scripts from a `package.json`, `pytest` from a `pyproject.toml` or a
  `tests/` directory, `gradle` / `mvn` from a wrapper. Offered wherever they
  apply, labelled as inferred.
- **`.vscode/launch.json` and `.vscode/tasks.json` are read** when present,
  tolerating comments and trailing commas because VS Code tolerates them. They
  win over a detected entry with the same command, because a human wrote them.
- **IntelliJ's configurations are not read**, and **Java is not supported**, in
  this cut. Both are deferred rather than refused — the evidence above is a
  reason to start elsewhere, not an argument that they are wrong. `R-N12`
  holds them and names what would take them up.
- **Nothing is written.** Not `launch.json`, not anything else. That is
  `~/.claude`'s rule applied to a second set of somebody else's files, and a
  "save this configuration" button would need its own ADR.
- **Debugging speaks the Debug Adapter Protocol, and mogeung ships no
  adapter.** The daemon launches one the user already has — `debugpy`,
  `codelldb` or `lldb-dap`, `vscode-js-debug` — found by a documented search
  order, and when it finds none it says which adapter is missing, where it
  looked, and how to install it, rather than failing to start.

**Classification survives the cut, and matters more than it looks.** Detection
cannot produce an unknown, but `launch.json` can: a `type` we run goes in
`HANDLED`, one we understand and decline goes in `KNOWN_IGNORED`, and anything
else raises a health alert and is **listed in the panel as unrunnable with its
type named** rather than hidden. This is `adapter.rs`'s discipline on a third
private format, and the reason is the same one `R-J28` gives: a configuration
silently missing reads as *"mogeung did not find it"*.

**The sweep keeps measuring what we do not parse.** `R-N1` reports IntelliJ's
sources too. That is the entire mechanism by which `R-N12` would ever be taken
up: a deferral with no measurement behind it is a deferral that becomes
permanent by forgetting.

## Alternatives

**A `.mogeung/run.toml` of our own.** Smallest, most honest, entirely under our
control. Rejected by the asker and correctly: it is a file nobody would write,
so the feature would launch inert everywhere. Worth recording the real
advantage given up — a format we define cannot drift underneath us, and the two
we now read both can.

**Read IntelliJ's configurations, including `.idea/workspace.xml`.** The plan of
record for about an hour, and defensible on the raw counts. Deferred, not
refused, and the reasons are worth keeping: it is a live IDE's private state
with no specification, rewritten while RustRover is open and therefore readable
half-written; its `type` namespace is extended by every plugin, so the
classification list may never converge; and the majority of its configurations
here are marked `temporary="true"`, which is IntelliJ's word for *you ran this
once and I kept it*. Against detection it is more work for a narrower answer.

**VS Code's `launch.json` only, with no detection.** The shape the original ask
implied, and the screenshot's. Rejected on the measurement: one repository of
nineteen, and not one of the user's own. It would ship a panel that is empty in
this repository, which is where it will be developed.

**Vendor the debug adapters.** Would make debugging work on first run with
nothing installed. Rejected: `vscode-js-debug` and `codelldb` are tens of
megabytes each, platform-specific, separately licensed, and would put mogeung in
the business of shipping other people's debuggers on their release schedule.
`R-J22` turned down an 83 MB dependency for less.

**Write our own debuggers.** Not seriously considered, and named here only so
the record shows it was not overlooked. DAP exists precisely so that this is
never the answer.

## Consequences

- **The first thing a user sees is a list mogeung inferred.** Detection is no
  longer a safety net under a parser; it is the product surface. A wrong
  inference is now a first impression, and "why is `cargo run -p mogeung-tray`
  in my list" is a question that has to have a good answer.
- **The feature works on day one in this repository**, which the IntelliJ-first
  plan did not — mogeung carries none of the files that plan depended on.
- **Detection has a ceiling and it is low.** No arguments, no environment, no
  attach, no "the way *I* run this service". The first time that ceiling is hit
  is the first honest evidence for `R-N12`, and it should be recorded rather
  than worked around.
- **Java users get nothing**, including the one Java project in this corpus —
  the only one with checked-in shared configurations, which is a small irony
  worth leaving in the record.
- **One private format instead of two.** `A4`'s family grows by `launch.json`
  alone, and `launch.json` is the one of the three that actually has a
  specification.
- **Secrets are still a live concern.** The sweep found a plaintext API key in
  an `<env>` element of a checked-in `.run.xml`. mogeung will not read that file
  in this cut, but `launch.json` has an `env` block of exactly the same kind, so
  environment values are masked by default wherever a configuration is
  displayed, copied or exported, and unmasking is a deliberate per-value act.

## Revisit if

- Detection's ceiling is hit in practice — a run that needs arguments,
  environment, or an attach — which is `R-N12`'s trigger and should arrive as a
  recorded annoyance rather than a memory.
- The sweep finds shared, checked-in configurations becoming normal in the
  repositories actually worked in. The table above is one machine on one day,
  and it should be re-measured rather than remembered.
- Java comes back for a real reason. Note for whoever picks it up: `java-debug`
  resolves its classpath through the Eclipse JDT language server, and the way
  round that is to attach over JDWP rather than launch — the JVM does the
  resolving, and a `.run.xml` already names the main class and module.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
