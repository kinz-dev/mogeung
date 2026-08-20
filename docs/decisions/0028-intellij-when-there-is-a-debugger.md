---
title: IntelliJ's checked-in configurations are taken up, and wait for the debugger
status: active
updated: 2026-08-20
decided: 2026-08-20
---

# ADR-0028 — IntelliJ's checked-in configurations are taken up, and wait for the debugger

## Context

[ADR-0026](0026-other-peoples-run-configurations.md) made detection the source
of run configurations and read one file, VS Code's. It deferred IntelliJ and
Java *"rather than refused"*, and named two things that would bring them back:
the ceiling being hit in practice, and — the one that fired —

> The sweep finds shared, checked-in configurations becoming normal in the
> repositories actually worked in. The table above is one machine on one day,
> and it should be re-measured rather than remembered.

It has been re-measured twice since, and this ADR is the outcome of the
revisit that ADR-0026 planned for rather than a reversal of it. (`R-N1`'s
sweep is what made the revisit possible at all: *"a deferral with no
measurement behind it is a deferral that becomes permanent by forgetting."*)

**2026-08-20, `cargo run -q -p mogeungd --bin runconfigs`, 59 repositories on
the Linux machine** — and this time counted by *configurations* as well as by
repositories, because the count of files was what made ADR-0026's decisive
second reading:

| Source | Repos | Configurations |
|---|---|---|
| `.idea/runConfigurations/*.xml` | 5 | **229** (183 `Application`, 46 compound) |
| `.run/*.run.xml` | 8 | **14** (11 `Application`, 3 `Multirun`) |
| `.idea/workspace.xml` → `RunManager` | 22 | 163, across **9** distinct types |
| `.vscode/launch.json` | 2 | one is a third-party clone, one is ours (`R-N8`) |
| `.vscode/tasks.json` | 1 | ours |

So the checked-in IntelliJ sources are **13 of 59 repositories and 243
configurations, all of them in the user's own projects**, against one
`launch.json` that belongs to somebody else. ADR-0026 counted five checked-in
files across nineteen repositories and concluded the dominant case was a
repository with no shared configuration at all. That is still true — 36 of 59
carry nothing — but the second most common case is no longer *private to a
machine*: it is **checked in, and it is IntelliJ's**.

**A second fact decides the shape**, and it was not visible on the first
machine. ADR-0026 deferred IntelliJ partly because *"its `type` namespace is
extended by every plugin, so the classification list may never converge."*
Measured, that is a statement about **`workspace.xml`** and not about the
checked-in files:

- checked in: **three** types — `Application`, `CompoundRunConfigurationType`,
  `Multirun` — two of which are composites of the first;
- `workspace.xml`: **nine**, including `docker-deploy`,
  `JetRunConfigurationType`, `KotlinStandaloneScriptRunConfigurationType` and
  `CargoCommandRunConfiguration`.

**And a third fact blocks the build.** Not one of the 243 checked-in
configurations names a command that can be spawned. 194 carry
`MAIN_CLASS_NAME` with an IntelliJ `<module>` — a Java main class whose
classpath IntelliJ resolves from its own project model — and the other 49 are
composites naming other configurations by name. There is nothing in the corpus
that a reader could hand to `run.rs` today.

## Decision

**The checked-in sources are taken up. The reader is sequenced behind
[`R-N9`](../product/roadmap.md), the DAP client, because that is what makes
them runnable.**

- **`.run/*.run.xml` and `.idea/runConfigurations/*.xml` will be read**, on the
  same terms as `launch.json`: classification with a `HANDLED` /
  `KNOWN_IGNORED` / listed-and-named split, a human's entry beating a detected
  one, environment values masked, and nothing written back.
- **`.idea/workspace.xml` stays deferred**, now on sharper grounds than *not
  yet*: it is the file whose namespace does not converge, whose entries are
  mostly `temporary="true"` — IntelliJ's word for *you ran this once* — and
  which a running IDE rewrites underneath the reader.
- **Nothing is built until `R-N9` lands.** This is sequencing, not appetite.
  Every configuration in the corpus is a Java main class, so a reader shipped
  today produces a list nobody can start; the way to start one is
  ADR-0026's own note — **attach over JDWP rather than launch**, letting the
  JVM resolve what IntelliJ would have — and that needs the debugger.
- **Composites are a listing problem, not a running one.** `Compound` and
  `Multirun` name other configurations; they mean something only once their
  members can run, so they arrive with them.
- **No inferred Gradle command.** See the alternatives.

## Alternatives

- **Read and list them now, marked unrunnable.** Cheap, and it would have put
  183 named-but-dead rows in one repository's panel. ADR-0026's *listed as
  unrunnable with its type named* exists so that a configuration is not
  **silently missing** — it is a rule about honesty in a short list, not a
  licence to fill a panel with things that cannot be started. Rejected on that
  distinction, which is worth keeping because the two look identical from the
  code's side.
- **Infer `./gradlew :module:run` from the module and main class.** This is the
  confident-wrong-inference failure ADR-0026 was written against, and the
  corpus says why it would be wrong rather than merely risky: a subproject need
  not apply the `application` plugin, and its `mainClass` need not be the
  `MAIN_CLASS_NAME` in the XML. A run panel that starts *something adjacent to*
  what you asked for is worse than one that starts nothing.
- **Resolve the classpath ourselves**, through Gradle's tooling API or the
  Eclipse JDT language server. ADR-0026 already priced this: a dependency the
  size of the pillar, for one language.
- **Keep deferring, with no successor named.** Rejected as the failure
  ADR-0026 named in its own text — permanent by forgetting. The difference
  between that and this decision is one word in the roadmap row: `R-N12` now
  has a *predecessor* rather than a condition nobody is watching.
- **Supersede ADR-0026.** Not needed and would misrepresent it: that ADR
  deferred these *rather than refusing* them and wrote down what would bring
  them back. This is that mechanism working, the same way
  [ADR-0027](0027-the-rail-stacks.md) took up ADR-0017's revisit trigger
  without overturning the rule.

## Consequences

- **`R-N12` stops being open-ended.** It cannot start before `R-N9`, and when
  `R-N9` lands it has a corpus, a three-type namespace and a shape waiting for
  it rather than an argument to have again.
- **Java users still get nothing, deliberately, for another cut.** ADR-0026
  left a small irony in the record — the one Java project in that corpus was
  the only one with checked-in configurations. It is thirteen repositories now
  and the irony is bigger, which is the honest price of sequencing this behind
  a debugger that has not been started.
- **The sweep must keep counting `workspace.xml`** even though this ADR has
  decided not to read it. The moment it stops, the deferral loses the evidence
  that keeps it a decision rather than a habit.
- **`R-N6`'s mask has a new file to cover before the reader ships.** The
  plaintext API key ADR-0026 found lives in an `<env>` element of a checked-in
  `.run.xml` — the very files this takes up. Noted now, while it is free, so it
  is not discovered by shipping.
- **A cheap trigger to bring the reader forward**: a checked-in `.run.xml`
  carrying a directly spawnable type — a Gradle task, a shell script, a Maven
  goal — is runnable with no debugger at all. None exists on this machine
  today. One would make the reader worth building before `R-N9`.

## Revisit if

- `R-N9` lands. Then this is a build, and its first subject is `immix-*`.
- A checked-in configuration appears that can be spawned without a classpath —
  the trigger above.
- Detection's ceiling is hit as a recorded annoyance, which is ADR-0026's other
  trigger and argues for arguments and environment in **detection**. That is a
  different row, and confusing it with this one would spend a debugger's worth
  of work on a smaller problem.
- The next re-measurement finds the checked-in count falling. It has moved a
  long way in eleven days on one machine, and one machine on one day is exactly
  what ADR-0026 warned against trusting.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
