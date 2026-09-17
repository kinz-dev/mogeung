---
title: Run configurations — IntelliJ's Run tool window, its Edit dialog and its popup
status: draft
updated: 2026-09-17
roadmap: [R-N9, R-N10, R-N11, R-N15, R-N16, R-N17, R-N18, R-N19, R-N20, R-N21, R-N22, R-N23, R-N24, R-N25, R-N26]
depends_on: [A13, A32, A33, A34, A42, A43]
---

# 0045 — Run configurations

Asked 2026-09-17, with two screenshots of IntelliJ — the **Run** popup
(`Alt+Shift+F10`) and the **Run/Debug Configurations** dialog — open on a real
project:

> Redesign the Run configuration windows and a run configuration popup (by
> shortcut Alt+Shift+F10), with reference to the IntelliJ Run Configuration and
> the Alt+4 Run panel. The Run panel should be like a terminal that shows the
> output of the process being run, multiple and tabbed. The run configuration
> allows us to config the process to be run. The Run config popup is a shortcut
> to select what to be run or edit the run config. […] The app should support
> running Java (Gradle project) and Rust (Cargo project) applications. I think
> we will need some plugin mechanism for supporting other types of app in
> future.

Three things were asked for — a gap analysis, a plan, and *no implementation*
— and this file is the first two. It is also **`R-N15`'s deliverable**: the
design session that row said had to come before the two build rows under it,
with its three questions answered in [The design](#the-design).

## Spec

*Owned by the human. What and why, never how.*

### Problem

The Run panel `R-N5` built is one column that does everything: the list,
the buttons, the output, the exit code. It proved the pillar and it is now
what a month of use argues with. Four things are wrong with it, and one of
them is the daemon's.

**It is not a terminal.** `run.rs` spawns on pipes with `stdin` closed.
Gradle sees no tty and drops to `--console=plain`; cargo drops its colour and
its progress bar; a process that asks a question hangs. The output is a stack
of text rows in a `div`, one run at a time, under the list.

**It offers what a manifest implies and nothing a human decided.** Detection
is the source (ADR-0026) and its ceiling is low by design — *"no arguments,
no environment, no 'the way I actually run this service'"*. The ADR said the
first time that ceiling was hit should *"arrive as a recorded annoyance rather
than a memory"*. **This is that record.** In the repository the ask came from,
the panel offers `./gradlew test` and `./gradlew build`, and the thing that is
run all day — *Media Driver (Single)*, *Stack (3 node)*, `:tools:orders-cli:run
--args="seed local-single.properties"` — is described in 78 IntelliJ files
mogeung counts and does not read.

**Nothing can be authored.** ADR-0026 rejected a format of our own because
*"no one cares"* — nobody would write the file. That was about a file. The
ask now is for a **form**, which is a different bet and is written down as
one ([A42](../product/assumptions.md)).

**It has one key.** `Alt+4` opens the dock tool. IntelliJ users arrive with
`Alt+Shift+F10` (choose and run), `Shift+F10` (run again), `Ctrl+F2` (stop)
in their hands, and `A13` says this user is one of them.

### What was measured, 2026-09-17

The repository the ask came from (`immix-trading-v3`, a Gradle build of 23
applications and 12 tools, four sibling checkouts under one workspace —
[feature 0044](0044-more-than-one-repository.md)'s corpus), read by a script
rather than by eye:

| `.idea/runConfigurations/*.xml` | 78 files |
|---|---|
| `type="Application"` — a Java main class plus an IntelliJ module | **74** |
| `type="CompoundRunConfigurationType"` — *Stack (3 node)* and two more, naming Applications by name | 3 |
| `type="ShConfigurationType"` — *Local Seed*, `/bin/bash tools/local-demo/seed.sh` | 1 |
| carrying an `<env>` block | 2 |
| `folderName`: Apps 58 · Sequencer 8 · Media Driver 4 · Snapshot Store 4 · Tools 1 · none 3 | — |
| *before launch*: **Make** | 74 of 74 |

**Every one of the 74 `Application` configurations maps to a Gradle
subproject that declares the same main class.** The module name
`immix-trading-v3.applications.media-driver.main` is a path; that path's
`build.gradle.kts` applies the `application` plugin and sets
`mainClass.set("xyz.immix.trading.mediadriver.MediaDriverMain")`; the XML's
`MAIN_CLASS_NAME` is that string. **74 of 74 agree, 0 mismatch, 0 without a
build file, 0 without a `mainClass`.** So `./gradlew :applications:media-driver:run
--args="local-single.properties"` starts what IntelliJ starts — *verified*,
not inferred, which is the distinction
[ADR-0028](../decisions/0028-intellij-when-there-is-a-debugger.md) rejected
the inference on.

Three things that would make that command *adjacent to* the IntelliJ launch
were checked, because ADR-0028 named exactly that failure:

- **JVM flags.** The XML passes `--add-opens=java.base/sun.nio.ch=ALL-UNNAMED …`.
  No application's build file carries them — the convention plugin sets
  `applicationDefaultJvmArgs = jdkAccessFlags()`, and the root build applies
  the same list to every `JavaExec` task (`tasks.withType<JavaExec>()
  .configureEach { jvmArgs(jdkAccessFlags) }`). Gradle's `run` gets the flags
  IntelliJ's XML gets, and so does the start script `installDist` writes,
  which carries `applicationDefaultJvmArgs` as its `DEFAULT_JVM_OPTS`. Under
  the launch this plan chooses — [the start script](#how-a-java-process-is-started)
  — the XML's options go in through `JAVA_OPTS` beside them, so the process
  gets both, as IntelliJ's does.
- **Working directory.** IntelliJ runs with `$PROJECT_DIR$`; Gradle's `run`
  task runs in the *subproject*, and a `JavaExec`'s `workingDir` is not the
  caller's to set. Harmless here — every `local-*.properties` is a classpath
  resource under `src/main/resources` — and wrong for a project that reads a
  relative path. **This is one of the reasons the plan launches the JVM
  directly** through the start script, where the working directory is
  mogeung's to choose.
- **Before launch: Make.** Gradle's `run` depends on `classes` and
  `installDist` on the jar. Either way the step is Gradle's, implicit, and
  never skipped.
- **What IntelliJ itself does here.** `.idea/gradle.xml` says
  `delegatedBuild=false` on `corretto-25`: IntelliJ compiles with its own
  compiler and launches `java -cp <its own resolved jars> MainClass`
  directly, which is the shape the start-script launch reproduces. No
  `build/install` output exists on disk — nobody has run `installDist` —
  and no build file sets `standardInput`, so under `./gradlew :x:run` the
  application would get no stdin.

**ADR-0028's own trigger has fired.** It wrote: *"a checked-in `.run.xml`
carrying a directly spawnable type — a Gradle task, a shell script, a Maven
goal — needs no debugger at all. None exists on this machine today. One would
make the reader worth building before `R-N9`."* *Local Seed* is one. So
`R-N12` comes forward from behind the debugger, by amendment — see
[Decisions](#decisions-this-plan-asks-for).

Two more shapes were read for the forms, not for running. `workspace.xml`
in the same repository holds five `GradleRunConfiguration` entries — four of
them `temporary="true"` — whose whole content is `taskNames: [":tools:orders-cli:run",
"--args=\"seed local-single.properties\""]`. That *is* the Gradle kind's form.
mogeung's own `workspace.xml` holds four `CargoCommandRunConfiguration`
entries: `command`, `workingDirectory`, `envs`, `emulateTerminal`, `backtrace`,
`channel`, `requiredFeatures`, `isRedirectInput`. That is the Cargo kind's
form. Neither file is read (ADR-0028 keeps `workspace.xml` deferred on the
grounds that still hold); both say what the fields are.

### Gap analysis

The reference is the two screenshots plus IntelliJ's Run tool window, which
the ask names by its key. *Wire* says whether the daemon already answers;
*takes* says where the work is and which row carries it.

**Screen 1 — the Run tool window (`Alt+4`)**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| A **tab per process**, named for its configuration, with a running/passed/failed glyph | one list; one output box under it; one at a time | ✓ `runs`, `run_output` | client — `R-N16` |
| Re-run **replaces** the tab; *allow multiple instances* makes a second one | a re-run is another row in `runs`; nothing is replaced | ✗ `singleton` | daemon field, client — `R-N16` |
| A **console**: ANSI colour, cursor movement, progress bars — the process has a tty | pipes; `cargo` drops colour, `gradle` falls to `--console=plain`; lines drawn as text rows | ✗ | **daemon — `R-N18`**: a pty per run, bytes on the wire, xterm.js renders |
| Type into the process | `stdin(Stdio::null())` | ✗ `run_input` | daemon — `R-N18`, **opt-in per configuration** |
| Left toolbar: re-run · stop · pin · close · scroll to end · soft-wrap · clear · filter · find | play and stop on the row | — | client — `R-N16` |
| *Process finished with exit code N* as the console's last line | a verdict chip on the row | ✓ `run_ended` | client |
| **`File.java:12` in a stack trace is a link** to the file | nothing | ✓ `fetch_file`, `showFilePane` | client — a regex over the buffer, Java · Rust · TS shapes |
| Detail: configuration · source · cwd · pid · started · duration · exit | name and command | partly — no `pid`, no `cwd` | daemon fields on `Run` — `R-N16` |
| The tab stays after exit, and the last few runs of a configuration are kept | `runs` keeps **every** run for the daemon's life, rings included | — | daemon: keep the last three ended per configuration, say what was dropped — `R-N16` |
| Float, or window | a dock tool is chrome ([ADR-0017](../decisions/0017-the-rail-is-chrome.md)) | — | a `run:<id>` pane kind, then [ADR-0037](../decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md)'s pop-out for free — `R-N16` |
| A Gradle **task tree** beside the console | — | — | **not doing** — it is the Tooling API, which is a JVM; the console carries Gradle's own `> Task :x` lines |
| Services tool window, Run dashboard | — | — | not doing |

**Screen 2 — Run/Debug Configurations**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| A tree: **type → folder → configuration**; `+` `−` copy · new folder · sort | a flat list | ✗ `kind`, `folder` | `R-N17` the tree; `R-N25` the dialog |
| `+` lists the configuration **types** | — | ✗ | `R-N19` — the kinds, each with a form schema |
| Name · folder · *allow multiple instances* · *store as project file* | — | ✗ | `R-N24` — the daemon's store; **never** *as project file* (ADR-0026: nothing is written into the repository) |
| **Application**: main class · module · program args · VM options · working dir · env · *Make* before launch | — | ✗ | read from IntelliJ's file and **launched as a JVM through Gradle's start script** — `R-N20`, `R-N23`: `./gradlew :module:installDist`, then `build/install/<name>/bin/<name> <args>` in `$PROJECT_DIR$` with the XML's VM options in `JAVA_OPTS`; *Make* is the `installDist` step, never skipped |
| **Gradle**: tasks and args · Gradle project · env · VM options · *run as test* | detection: `./gradlew test`, `./gradlew build` | ✗ | `R-N20` |
| **Cargo Command**: command · working dir · env · backtrace · channel · features · *emulate terminal* · redirect input | detection: `cargo test`, `cargo build`, `cargo run -p <bin>` | ✗ | `R-N21` |
| **Shell Script**: script path *or* text · options · interpreter · working dir · env · *execute in terminal* | — | ✗ | `R-N22` |
| **Compound**: the configurations to run together | — | ✗ | `R-N23` |
| **npm**: script · args | detection: every script | — | `R-N19` gives it a form for nothing |
| An env table; values masked | keys shown, values masked, one revealed at a time (`R-N6`) | ✓ | kept; **the form edits a user configuration's env only** — IntelliJ's and `launch.json`'s stay read-only and masked |
| *Edit configuration templates* | — | — | not doing |
| Run · Apply · OK · Cancel | — | — | `R-N25` |
| — | *(mogeung's)* a configuration from a manifest or somebody else's file is **read-only**, with *duplicate as your own* | — | `R-N25` |

**Screen 3 — the Run popup (`Alt+Shift+F10`)**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| `0` Edit Configurations… | — | — | `R-N26` → the dialog |
| Folders, `▸` opening a submenu; a folder's digit runs its last-run member, and the row says which (*mnemonic is to …*) | — | — | `R-N26` |
| Top-level configurations, then **temporary** ones greyed | — | — | `R-N26` — *recently run* replaces *temporary*; mogeung keeps no temporaries |
| Digits `1`–`9`; `↑↓` `Enter`; typing filters | `GitOpsPopup` is the precedent (`R-D31`) | — | `R-N26` |
| *Hold Shift to Debug* | — | — | drawn **disabled and naming `R-N9`–`R-N11` until they land**; then it starts the same configuration under the debugger — see [Debugging inside mogeung](#debugging-inside-mogeung) |
| `F4` edits the selected; `Delete` forgets a temporary | — | — | `R-N26` — `F4` and `Shift+Enter` open the dialog on that row |
| `Shift+F10` run · `Ctrl+F2` stop · `Alt+Shift+F9` debug popup | `Alt+4` only | — | `R-N26` — the keymap; Mac `⌃R` · `⌘F2` · `⌃⌥R` |
| The toolbar **run widget** — configuration `▾` · ▶ · 🐞 · ⏹ | — | — | named, not rowed: cheap once the popup exists, and the top bar is dense |

**Underneath all three**

| IntelliJ | mogeung today | wire | takes |
|---|---|---|---|
| Configuration types are **plugins** | `detect.rs` is a flat function per manifest; `runconfig.rs` reads VS Code's files | — | `R-N19` — a `RunKind` seam, compiled in |
| `.run/*.run.xml` and `.idea/runConfigurations/*.xml` | counted by the sweep, read by nothing (ADR-0028) | ✗ | `R-N23` — the reader, by amendment |
| A configuration belongs to a **project**, and a workspace holds several | one repository per session, by construction | ✗ `repo` | `R-N24` — [R-D34](../product/roadmap.md)'s rule: a `repo` that resolves inside `session_roots` |

### `R-N15`'s three questions, answered

1. **What is a process here?** A **record**. A tab is a run; it stays with
   its exit code until you close it or re-run its configuration; the daemon
   keeps the last three ended runs per configuration with their buffers and
   drops older ones, saying so. A run is never only a live row — `R-N7`
   needs the record, because a claim is checked against what happened.
2. **Does the tree follow the source or the folder?** **Kind, then folder,
   then configuration** — IntelliJ's own tree, which is what the screenshot
   shows. The source (*inferred* · `launch.json` · IntelliJ · yours) is a
   **badge on the row and the reason a form is read-only**, not a level in
   the tree. ADR-0026's *"a wrong inference is a first impression"* is
   honoured by the badge, which is where a reader's eye lands, rather than
   by a grouping that would put `cargo test` under *inferred* and the same
   command from `tasks.json` under *launch.json*. A detected entry gets the
   folder of the manifest it came from (`desktop/`, the root), so a monorepo
   reads as its directories.
3. **Where does the log go when there are three of them?** **One tab per
   process, in the Run tool window** — IntelliJ's answer and `R-B49`'s
   answer for agents. Two side by side is a pop-out: a tab becomes a
   `run:<id>` pane in the centre and, from there, a window under ADR-0037.

### Assumptions

| Id | Claim | Status |
|---|---|---|
| [A13](../product/assumptions.md) | Drives by keyboard, reaches for a palette before a menu | `SUPPORTED` — the popup is the whole reason for a key |
| [A32](../product/assumptions.md) | Reading the IDEs' configurations gives us enough to run | `AT RISK` on arrival, and **this measurement is its strongest support yet**: 74 of 74 checked-in configurations in the repository worked in are spawnable through a verified mapping. Recorded in the ledger; the status moves when the reader ships and is used |
| [A33](../product/assumptions.md) | The user runs here rather than in the IDE beside it | `UNTESTED` — and the ask is the first evidence: a panel argued with in this detail is a panel that is being used. The removal condition stands unchanged |
| [A34](../product/assumptions.md) | The run belongs beside the claim | `UNTESTED` — untouched by this spec; `R-N7`'s binding is kept as it is |
| [A42](../product/assumptions.md) | A configuration authored in a **form** gets saved, where a file in mogeung's own format would not | **`UNTESTED`**, new — the bet under `R-N24` and `R-N25` |
| [A43](../product/assumptions.md) | Gradle's `application` plugin — `run`, or the start script `installDist` writes — starts what IntelliJ's `Application` configuration starts | **`SUPPORTED`**, new — by the measurement above, on one repository; the sweep re-measures it |

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is
> to test it. `A42` carries the authoring half and nothing else, so **the
> store and the dialog are the last stage** and are the test; every stage
> before them rests on `SUPPORTED` rows or on the measurement. `A33` is the
> pillar's standing bet and this spec adds no weight to it — every row here
> makes the panel *more* what the IDE is, which is exactly the position
> `A33` says is a losing one to defend forever. If `R-N13`'s week ever says
> the panel is opened and abandoned, this whole file comes out with it.

### The design

**Mockup:** [Run configurations — design mockup](https://claude.ai/artifact/TepnvDrNn73k9X6Bdd8RWD),
drawn in the window's own tokens over `immix-trading-v3`'s real
configurations and this repository's detected ones. Its tabs, tree rows and
the popup's digits are clickable so the selection behaviour can be felt rather
than described; the console text is example output.

The Run tool stays in the **bottom dock** on `Alt+4` — chrome, one tool at
a time, following the selected session. What changes is that it becomes a
window in its own right: a strip of process tabs, a console that is a
terminal, and a pinned first tab that is the configurations tree.

```
┌ RUN · immix-trading-v3 ▾ ── ☰ Configurations · ● Media Driver (Single) · ✓ Local Seed · ✗ cargo test ─────── ⤢ ↻ ┐
│ ▶ │ Media Driver (Single)   IntelliJ · Application → gradle   pid 41822   started 11:02:14   running 00:03:41   │
│ ■ │ applications/media-driver/build/install/media-driver/bin/media-driver local-single.properties   cwd .        │
│ ⇲ │ ─────────────────────────────────────────────────────────────────────────────────────────────────────────── │
│ ↧ │ > Task :applications:media-driver:compileJava UP-TO-DATE                                                    │
│ ⤶ │ > Task :applications:media-driver:installDist UP-TO-DATE                                                    │
│ ⌫ │ 11:02:17.204 INFO  MediaDriverMain - aeron dir /dev/shm/aeron-kinz                                          │
│ ⌕ │ 11:02:17.211 WARN  DriverConductor - low disk space on /dev/shm                                             │
│   │ ▮                                                                                                           │
│   │ JAVA_OPTS from IntelliJ's VM options: --add-opens=java.base/sun.nio.ch=ALL-UNNAMED …  · installDist 1.8s    │
└───┴─────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**The header** is the repository picker (`R-D34`'s, reused), the tab strip,
and on the right maximise and reload. A tab is a **process**: its
configuration's name, a glyph for running · exited 0 · exited N · stopped,
and a close. Re-running a configuration whose run is still going asks
*stop and re-run?* unless the configuration allows multiple instances, in
which case it opens a second tab named *(2)*. The first tab, **☰
Configurations**, is pinned and is `R-N17`: the tree, with ▶ on every
runnable row, the source badge, and *edit* opening the dialog on that row.

**The console** is xterm.js fed from the daemon's byte stream, not a pty of
its own — the process runs on the daemon's machine and its bytes travel the
websocket the way every other event does. Colour, cursor movement and
progress bars work because the process has a pty on the daemon's side.
Typing into it is allowed only when the configuration says so (*allow
input*), and the console says *read-only* in its corner when it is not.
`File.java:12` and `src/x.rs:12:3` are links that open the Code pane at the
line. The last line after exit is IntelliJ's own sentence: *Process finished
with exit code 0*. Above it, a **detail strip** — source and kind, pid,
started, duration, cwd, and the command as it was spawned. Below it, one
line for anything the kind wants to say, such as the VM-options note above.

**The left toolbar**, top to bottom: re-run · stop · pop out (a pane, then a
window) · scroll to end · soft-wrap · clear · find. Every one of them is on
the keyboard too.

**The Run/Debug Configurations dialog** is the screenshot: a tree on the
left under `+` `−` copy · folder · sort, a form on the right, and Run ·
Cancel · Apply at the foot.

```
┌ Run/Debug Configurations ────────────────────────────────────────────────────────────── × ┐
│ + − ⧉ 🗀 ⇅          │ Name  Media Driver (Single)              Folder  Media Driver ▾      │
│ ▾ Gradle             │ ─────────────────────────────────────────────────────────────────── │
│   ▾ Apps        (58) │ ⓘ Read from .idea/runConfigurations/Media_Driver__Single_.xml — an  │
│   ▾ Media Driver (4) │   IntelliJ Application configuration, run here as Gradle. Edit it   │
│       Media Driver (0/2)│ in IntelliJ, or  [ duplicate as your own ]                       │
│       Media Driver (1/2)│                                                                  │
│       Media Driver (2/2)│ Gradle project   immix-trading-v3  (./gradlew)                   │
│     ● Media Driver (Single)│ Launch        start script (installDist) ▾  or gradle run     │
│   ▸ Sequencer    (8) │ Arguments        local-single.properties                             │
│   ▸ Snapshot Store(4)│ Working directory  .   $PROJECT_DIR$ as IntelliJ; mogeung sets it    │
│   ▸ Tools        (1) │ Environment      (none)                                              │
│ ▾ Compound       (3) │ VM options       --add-opens=java.base/sun.nio.ch=ALL-UNNAMED …     │
│     Stack (3 node)   │                   via JAVA_OPTS; the script adds the build's own     │
│ ▾ Shell Script   (1) │ ☑ Emulate terminal   ☐ Allow input   ☐ Allow multiple instances     │
│     Local Seed       │                                                                     │
│ ▾ Cargo   (inferred) │                                                                     │
│     cargo test …     │                                                                     │
│ Edit templates… (no) │                                          [ Run ]  [ Cancel ] [Apply]│
└──────────────────────┴─────────────────────────────────────────────────────────────────────┘
```

The form is **rendered from the kind's schema**, so this dialog knows no
kind by name: a kind declares its fields — text, arguments, a path, a
directory, an env table, a toggle, a choice, a list of configurations — and
the dialog draws them. A configuration read from a manifest or from
somebody else's file is shown **read-only** with its origin named and
*duplicate as your own* beside it; a duplicate is a user configuration
in the daemon's store, badged *yours*, and it diverges from the file it came
from on purpose. Env values are masked here exactly as in the panel, and a
user configuration's own env is the one table that can be typed into.

**The popup** is `GitOpsPopup`'s construction over a different list:

```
┌ Run ─────────────────────────────────────────────┐
│ 0  ✎ Edit Configurations…                        │
│ ─────────────────────────────────────────────── │
│ 1  🗀 Apps        (mnemonic is to “Orders (Single)”) ▸ │
│ 2  🗀 Media Driver                              ▸ │
│ 3  🗀 Sequencer                                 ▸ │
│ 4  🗀 Snapshot Store                            ▸ │
│ ─────────────────────────────────────────────── │
│ 5  ⧉ Stack (3 node)                              │
│ 6  ⧉ Stack (Single)                              │
│ 7  ⧉ Trading (Single)                            │
│ ─────────────────────────────────────────────── │
│    ⟳ cargo test --workspace         inferred · 2m│
│    ⟳ Local Seed                    IntelliJ · 1h │
│ ─────────────────────────────────────────────── │
│ 8  🗀 Tools                                     ▸ │
│ Hold Shift to debug — not yet (R-N9)  ·  F4 edit │
└──────────────────────────────────────────────────┘
```

Folders first, then top-level configurations, then the **recently run**
greyed where IntelliJ greys its temporaries, then *Tools*. A folder's digit
runs the member it ran last and says which; `→` or `Enter` opens the folder
in place. Typing filters every level, as the branches popup does. `F4` and
`Shift+Enter` open the dialog on the row. `Escape` closes.

**Keyboard**, because `A13`: `Alt+Shift+F10` the popup · `Shift+F10` re-run
the selected configuration · `Ctrl+F2` stop the active tab's run · `Alt+4`
the tool · `Ctrl+Shift+F4` close the tab · `Alt+←`/`→` between tabs. On a
Mac, `⌃⌥R` · `⌃R` · `⌘F2` — IntelliJ's own. All of them are chords, so an
Agent pane keeps its bare keys.

**The plugin seam** is a Rust trait and a schema, and nothing dynamic:

```rust
pub trait RunKind: Sync {
    fn id(&self) -> &'static str;               // "gradle"
    fn schema(&self) -> KindSchema;              // fields the dialog draws
    fn detect(&self, repo: &Path) -> Vec<RunConfig>;
    fn resolve(&self, params: &Params, repo: &Path) -> Result<Resolved, String>;
    fn from_intellij(&self, xml: &IntellijConfig, repo: &Path) -> Option<Params>;
}
pub const KINDS: &[&dyn RunKind] = &[&Gradle, &Cargo, &Npm, &Python, &Maven, &Shell, &Compound];
```

`resolve` is the only place an argv is composed, and it is compiled in — a
kind turns *parameters* into a command, a client never sends one, and
ADR-0025 clause 1 holds as it does today. Adding a kind is one file, a
corpus fixture and a test; the dialog and the popup do not change. **No
dynamic loading, no plugin directory, no manifest language**, for two
reasons that are ADR-0029's shape with the argument inverted: a loaded
plugin that composes argv is a command supplier with a nicer name, which is
the exact thing clause 1 refuses; and a kind's value is the set of decisions
it carries — a closed argument shape, a verified mapping, an unrunnable
reason — which are worth a test against a real corpus and worthless as a
declarative file that goes stale on its own schedule.

**Three things that are mogeung's, not IntelliJ's.** The source badge on
every row and the read-only form it implies; a run beside the claim it bears
on (`R-N7`, unchanged); and a console that is a view of a process on the
daemon's machine, so it works against a remote daemon and survives the
window closing (ADR-0025 clause 3).

**Three things IntelliJ has that this does not**, each named on the surface
rather than absent: the Gradle task tree; *store as project file* (mogeung
never writes into a repository — the form says where a configuration lives);
and expression evaluation and conditional breakpoints in the debugger, which
need a compiler and are where IntelliJ wins. Debugging itself is in this plan,
as its last stage.

#### How a Java process is started

Asked 2026-09-17, after the mockup: *"how does it work with the java process,
how does it load a gradlew project and resolve the java classpath?"* —
and then, *"is there a difference in running gradle as compared to running a
java process?"* The answers are the same answer. **mogeung never loads the
Gradle project model and never resolves a classpath. Gradle does both**, in
its own daemon, from the build scripts, the version catalog and the Artifact
Registry. What mogeung decides is *who stands in front of the process*, and
the plan uses both shapes, chosen by what a configuration *is*:

- **A Gradle task** — a `GradleRunConfiguration`, a detected `test` or
  `build`, a user's own list of tasks — runs as `./gradlew <tasks>` on a pty,
  Gradle in front of whatever it forks. One command, and it cannot run stale
  code. The costs are Gradle's: the exit code is Gradle's and the
  application's is a line of text; stop is indirect (the client dies, the
  daemon cancels the build, the build destroys the child); stdin is not
  forwarded; the working directory is the task's.
- **An application** — IntelliJ's `Application`, or a user's own *Java
  application* — is two steps the kind owns. `./gradlew :sub:installDist`
  runs first, in the same tab, and a non-zero exit **refuses the launch**
  with the tail of the build as the reason. Then
  `build/install/<name>/bin/<name> <args>` is spawned as a **direct child**
  in the directory the configuration names, `$PROJECT_DIR$` by default, with
  the XML's VM options in `JAVA_OPTS` beside the build's own in the script's
  `DEFAULT_JVM_OPTS`. Gradle's start script `exec`s `java`, so the JVM is
  mogeung's child: the exit code is the application's, stop is a `SIGTERM`
  mogeung sends and can escalate on its own terms — which matters to a media
  driver whose shutdown hook cleans `/dev/shm` — stdin works, the working
  directory is IntelliJ's, and a debugger attaches through `JAVA_OPTS` too.
  The console shows the build step, then the application alone.

| | Gradle in front | The JVM directly |
|---|---|---|
| Processes | client JVM, daemon, then the app | the app |
| Console | Gradle's progress bar and task lines mixed with app output | the build, then only the app |
| Exit code | Gradle's | the app's own |
| Stop | via build cancellation | a direct signal |
| stdin | not forwarded | works |
| Working directory | the task's | the configuration's |
| Stale code | impossible | impossible only because `installDist` is never skipped |

Both need the same precondition — the `application` plugin on the module,
which all 74 configurations in the corpus satisfy — and neither puts a line
of Gradle's project model into mogeung. `<name>` is `applicationName`, the
project name by default; the kind reads it from the build file when it is
set and checks the script exists after `installDist`, listing the
configuration unrunnable with the reason when it does not.

**The process inherits `mogeungd`'s environment, not your shell's.**
`JAVA_HOME`, `PATH` and `gcloud`'s credentials can all differ when the daemon
was started from the tray. The form's env table covers it per configuration,
and the Gradle kind's form carries a *run through login shell* toggle —
`$SHELL -lc 'exec "$0" "$@"' <argv…>`, so the argv is still mogeung's and
only the environment is the shell's.

#### Debugging inside mogeung

Asked 2026-09-17: *"in case we want to run debug inside mogeung, how are we
going to support it?"* The shape was decided in `R-N9`–`R-N11` and this is
how it meets the design above. **A debug session is a run plus a debugger
attached to it.** The tab, the pty, the console and the exit code come from
the Run tool window; what is added lives in the daemon, because the process
runs there, so a remote daemon debugs a process on its own machine and the
window only draws.

1. **The daemon speaks the Debug Adapter Protocol** to an adapter over
   stdio, forwards its events to the window as `debug_*` messages, and
   answers the adapter's `runInTerminal` request with the run's own pty from
   `R-N18`, so the debuggee runs in the same console a plain run does. Where
   an adapter lacks `runInTerminal`, its `output` events feed the console.
2. **Breakpoints are set in the Code pane's gutter** — Monaco's glyph
   margin already knows the file and the line — and sent to the adapter
   before the process is resumed. When it pauses, the pane opens the frame's
   file at the line.
3. **A Debug dock tool** beside Run shows threads and frames, the selected
   frame's locals and fields, watches where the adapter can evaluate, and
   the stepping toolbar.
4. **The keys are IntelliJ's**: F8 over, F7 into, Shift+F8 out, F9 resume,
   Ctrl+F8 toggle a breakpoint. They are bare keys, so `focusOwns`'s rule
   applies: a focused console keeps them for the process, and the Code pane
   or the Debug tool steps.

**The kind supplies the debug launch.** A Java application is the same
two-step launch with one flag added —
`JAVA_OPTS=-agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=127.0.0.1:0`
— after which the JVM starts suspended and prints `Listening for transport
dt_socket at address: 43117` on stdout. The daemon is already reading that
pty, takes the port from the line and attaches; IntelliJ's own Gradle
debugging works the same way. A Gradle task is `--debug-jvm` on a fixed
port. A Rust binary is `cargo build`, the kind's build step, then the
adapter's `launch` with the binary, its arguments, cwd and env.

**Rust is cheap.** Three adapters already exist on a machine like this one:
`lldb-dap` from LLVM, `gdb -i dap` on GDB 14 and later, and `codelldb` where
VS Code is installed. Each is adapter configuration, found by `R-N9`'s
documented search order, plus rustup's pretty-printers loaded through the
adapter's init commands, or a `String` renders as a raw struct.

**Java is the hard part, and there is no adapter to borrow.** Every Java
debugger speaks JDWP, the wire protocol the JVM itself exposes; the only DAP
adapter for it runs inside the Eclipse JDT language server, which
ADR-0026 priced as a dependency the size of the pillar and which an IntelliJ
user does not have.

| Route | What it costs | What it gives |
|---|---|---|
| jdtls with `java-debug` | ~100 MB install, a JDK to run it, and a Gradle import of all 35 modules before the first breakpoint | expression evaluation, conditional breakpoints, hot swap |
| A JDWP client in the daemon, exposed inside as one more adapter | multi-day work in Rust, no external dependency, every JVM speaks it | breakpoints by file and line, step over/into/out, threads, frames, locals and fields one level deep, strings |

**The second is what this plan proposes.** It is not writing a debugger in
the ptrace-and-DWARF sense ADR-0026 rightly refused. It is a client for the
JVM's own documented debug API, the one `jdb` and IntelliJ use, and it sits
behind the daemon's DAP seam so the window treats Java like any other
adapter. Breakpoints are deferred on `CLASS_PREPARE` and set when the class
loads, the line-to-index mapping is `Method.LineTable`, and the first cut
evaluates no expressions and takes no conditions, because both need a
compiler and are where IntelliJ wins.

**Source lookup** is the one piece that needs the workspace. A JVM reports a
class name and a source file name, not a path; the daemon searches
`src/main/java/<package path>/<File>.java` across the session's roots,
which since `R-D34` includes the sibling checkouts the build composes. A
frame in a library jar says *no source* and offers step-out. Rust needs none
of this: DWARF carries full paths.

**When.** Stage 7, after the run stages have been used and `R-N13` has a
verdict on them. IntelliJ's debugger is the strongest incumbent in this
product and a half-debugger is what would send someone back to it. The
removal condition is the pillar's own: a breakpoint set here and re-set
there.

### Decisions this plan asks for

Written as ADRs and amendments **when this plan is approved**, before the
first line of code — the way ADR-0025 and ADR-0026 preceded feature 0035.
The claims, so they can be argued with now:

- **Amend ADR-0028** — *the reader comes forward, on verification rather
  than inference.* The alternative it rejected was *infer `./gradlew
  :module:run` from the module and main class*; what ships instead **checks**
  the module's build file declares that main class and lists the
  configuration unrunnable, naming `R-N9`, when it does not. The trigger it
  wrote down — a checked-in shell-script configuration — has fired. The
  launch is the JVM directly, through the start script `installDist` writes,
  and the debug path is JDWP through `JAVA_OPTS` on that same launch — no
  classpath work in mogeung in either case. `workspace.xml` stays deferred,
  unchanged.
- **Amend ADR-0026, the debugger half** — *"write our own debuggers — not
  seriously considered; DAP exists precisely so that this is never the
  answer"* is narrowed by one case: Java, for which no standalone adapter
  exists. A JDWP client is a client for the JVM's own documented debug API,
  not a native debugger, and it is built behind the daemon's DAP seam so the
  window never knows. *mogeung ships no adapter* still holds for every
  language that has one.
- **Amend ADR-0026** — *"nothing is written" is narrowed to "nothing is
  written into a repository".* A configuration authored in the dialog lives
  in the daemon's store, under `~/.mogeung`, keyed by repository — the same
  fence [ADR-0035](../decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)
  drew for scratch files. *"A format of our own that no one would write"*
  becomes a form, and `A42` is the bet that a form is different. Clause 1's
  argument moves by one step and the amendment says where: on loopback a
  client can now define a command and run it, which is the terminal panel's
  trust and ADR-0025's own argument for loopback; beyond loopback the save
  needs the token (`writes_allowed`) and the start needs `--allow-run` —
  two deliberate acts, as before.
- **ADR-0039 — a run kind is compiled in, and its form is its schema.** The
  seam above, its registry, and why there is no plugin loader.
- **ADR-0040 — a run may have a pty, its bytes travel the wire, and input is
  opt-in per configuration.** ADR-0011's *"mogeung still never writes to a
  pty"* is about a shell a human drives and the agent that could be trapped
  in it; a run is a process mogeung spawned, owns and can never make an
  agent (clause 2). The fences: input only when the configuration says so;
  never on a timer or from anything but a keystroke in that console; a
  read-only console says it is one.
- **Clause 2 for text.** A user-authored Shell or Command configuration
  whose text names an agent CLI as a word (`run::AGENTS`) is refused at save
  and at spawn. ADR-0025 said a script that goes on to call `claude` walks
  past the program check and that if it happened in practice the ADR needed
  a successor rather than a quiet widening; a form of our own is where it
  would happen first, and a word match on our own text is the cheap fence
  that keeps the successor unwritten.

### Acceptance

`R-N18` — a pty for a run:

- [ ] A configuration with *emulate terminal* on runs `cargo test` in colour
      and `./gradlew` with its rich console; off, both fall back to plain
      output exactly as today
- [ ] With *allow input* on, a process that reads a line gets what is typed
      in the console; with it off, the console says *read-only* and the
      daemon refuses `run_input` in words
- [ ] The byte buffer is bounded, a reconnecting window gets the tail, and
      the amount dropped is stated
- [ ] The line log used beside a claim (`R-N7`) is ANSI-stripped and
      searchable, unchanged in shape
- [ ] Stop still stops the process group; the exit code still lands after
      the last byte

`R-N16` — the Run tool window:

- [ ] A tab per run, named for its configuration, with a state glyph; a
      pinned *Configurations* first tab
- [ ] Re-running a configuration replaces its tab after a confirmation when
      the run is still going; *allow multiple instances* opens a second
- [ ] The console shows *Process finished with exit code N*; the detail
      strip shows source, kind, pid, cwd, started, duration and the spawned
      command
- [ ] `File.java:12` and `path.rs:12:3` in the console open the Code pane
      at the line
- [ ] Re-run, stop, pop-out, scroll-to-end, wrap, clear and find are on the
      toolbar and on keys; a tab pops out to a pane and to a window
- [ ] The daemon keeps the last three ended runs per configuration and the
      tab strip says when an older one was dropped

`R-N19` — the kinds:

- [ ] `detect.rs`'s offers are unchanged in content and order after the move
      into kinds — the existing acceptance test against this repository is
      the proof
- [ ] Every kind has a schema, and a schema field type nobody handles fails
      the client's type check rather than drawing nothing
- [ ] Adding a kind touches no client file — asserted by the sixth kind
      being added in a Rust file and a fixture only

`R-N20` — Gradle:

- [ ] A Gradle root offers a *Java application* entry for every subproject
      whose build file declares `application { mainClass }` and is listed by
      a literal `include(...)`; `test` and `build` as today
- [ ] The form has project, tasks or application, arguments, environment,
      working directory, VM options, *run as test* and *run through login
      shell*; `--args=` is one argv element and no shell is involved
- [ ] An application launches in two steps — `./gradlew :sub:installDist`,
      then `build/install/<name>/bin/<name>` as a direct child — the second
      never running when the first fails and the first never skipped; the
      working directory is the configuration's, `$PROJECT_DIR$` by default;
      the VM options travel in `JAVA_OPTS`
- [ ] A task configuration launches through `./gradlew <tasks>` on a pty, and
      its row says the working directory is the task's and the exit code is
      Gradle's
- [ ] The row says whether the wrapper or the machine's `gradle` runs

`R-N21` — Cargo · `R-N22` — Shell:

- [ ] The Cargo form is IntelliJ's: command, package, bin, features,
      release, backtrace, channel, arguments, environment, working dir,
      emulate terminal, allow input
- [ ] A Shell configuration runs a script path or inline text under a named
      interpreter; text naming an agent CLI is refused by name at save and
      at spawn

`R-N23` — IntelliJ's files:

- [ ] `.run/*.run.xml` and `.idea/runConfigurations/*.xml` are read;
      `Application`, `ShConfigurationType`, `CompoundRunConfigurationType`,
      `GradleRunConfiguration` and `CargoCommandRunConfiguration` run; every
      other type is listed unrunnable with its type named and fails the
      sweep until classified
- [ ] An `Application` runs as Gradle **only** when the module's build file
      declares its main class; otherwise it is listed with the reason and
      `R-N9` named
- [ ] An `<env>` value is masked everywhere it was before, in the new file
- [ ] A compound starts every member in its own tab and stops them together
- [ ] In `immix-trading-v3`, 74 Applications, 3 compounds and 1 script are
      runnable and the panel is not 183 dead rows

`R-N24` — the store · `R-N25` — the dialog:

- [ ] A configuration saved in the dialog survives a daemon restart and is
      offered to every window watching that repository
- [ ] A saved configuration's program is checked against `run::AGENTS` at
      save and at spawn; a save from beyond loopback without the token is
      refused in words
- [ ] A read-only origin has no Save; *duplicate as your own* makes a user
      configuration badged *yours*
- [ ] Every message the dialog sends carries parameters and an id, never a
      command line — asserted the way `RunPane.test.tsx` asserts it today
- [ ] The tree groups kind → folder → configuration, and `R-N17`'s pinned
      tab is the same tree with ▶ on each row

`R-N9` — the DAP client:

- [ ] The daemon finds an adapter by a documented search order and, when
      none is found, says which, where it looked and how to install it
- [ ] A debug session is a run: it has a tab, a pty console and an exit
      code, with the adapter attached; `runInTerminal` is answered with the
      run's own pty
- [ ] A Java application started under the debugger prints its JDWP port to
      the console and the daemon attaches before the first line of `main`

`R-N10` — the gutter and the Debug tool:

- [ ] A click in the Code pane's gutter sets a breakpoint that is hit; the
      pane opens the frame's file at the line when the process pauses
- [ ] Threads, frames and the selected frame's locals show in the Debug
      tool; F8, F7, Shift+F8 and F9 step and resume; Ctrl+F8 toggles a
      breakpoint; a focused console keeps all five for the process
- [ ] A frame with no source says so, and step-out is offered

`R-N11` — Rust, then Java:

- [ ] `cargo run -p <bin>` debugs through whichever of `lldb-dap`,
      `gdb -i dap` or `codelldb` is installed, with rustup's pretty-printers
      loaded so a `String` reads as text
- [ ] A Java application debugs through the daemon's own JDWP adapter:
      breakpoints by file and line, step over/into/out, threads, frames,
      locals and fields, strings
- [ ] Expression evaluation and conditional breakpoints are absent, and the
      Debug tool says so rather than drawing a box that does nothing

`R-N26` — the popup and the keys:

- [ ] `Alt+Shift+F10` opens the popup; a digit runs its row and closes; `0`
      opens the dialog; `F4` opens the dialog on the row; typing filters
- [ ] A folder's digit runs the member it ran last, and the row says which
- [ ] `Shift+F10` re-runs the selected configuration, `Ctrl+F2` stops the
      active tab's run, and both work on a Mac under their own spellings
- [ ] *Hold Shift to debug* is drawn disabled and names `R-N9`

### Explicitly out of scope

- **Expression evaluation, conditional breakpoints, hot swap, and Python and
  Node debugging.** Debugging is in this plan as its last stage; these are
  what it leaves out — the first three because they need a compiler and are
  where IntelliJ wins, the last two because the ask names Java and Rust.
- **A Gradle task tree**, coverage, a profiler, the test-runner tab. Each is
  a feature the size of this one, and the console carries Gradle's own task
  lines.
- **`.idea/workspace.xml`.** Still deferred, on ADR-0028's grounds; still
  counted by the sweep.
- **Writing `.run.xml` or `launch.json`.** ADR-0026: nothing is written into
  a repository. A user configuration lives in the daemon's store, and if a
  shareable file is ever wanted it is an *export*, which is its own ask.
- **Configuration templates**, the Services tool window, *Run Anything*.
- **Docker, compose, Kubernetes.** `R-N14` and `R-I14`, unchanged.
- **A build system.** mogeung runs what the project builds with; Gradle's
  `run` builds first because Gradle says so, not because mogeung does.
- **The toolbar run widget.** Named above; not rowed until the popup has been
  used for a week.

## Plan

*Drafted by an agent, 2026-09-17. Awaiting approval; nothing built.*

### Approach

Six stages, in this order, each shippable and each usable before the next
exists. The daemon's pty comes second because it is what the ask means by
*like a terminal* and it lights up every configuration that exists today;
the authoring half comes last because it is the only stage resting on an
`UNTESTED` row.

**1. `R-N19` — the seam, with no behaviour change.** `crates/mogeungd/src/kinds/`
gains a `RunKind` trait and one file per kind — `gradle.rs`, `cargo.rs`,
`npm.rs`, `python.rs`, `maven.rs`, `shell.rs`, `compound.rs` — and
`detect.rs`'s bodies move into each kind's `detect`. `mogeung-core::run`
gains `KindSchema { id, label, fields: Vec<Field> }` and `Field { key,
label, kind: Text | Args | Path | Dir | Env | Bool | Choice(..) | Configs,
hint }`, and `RunConfig` gains `kind`, `params`, `folder`, `singleton`,
`pty`, `stdin`, `members` — all `#[serde(default)]`, so an older client
reads what it read before. `Origin` gains `IntelliJ` and `User`. The wire
gains `fetch_run_kinds → run_kinds { kinds }`, sent once on connect. The
acceptance test that asserts this repository's thirteen offers is the proof
the refactor changed nothing. **A public `mogeung-core` type changes, so
`cargo check --manifest-path desktop/src-tauri/Cargo.toml` is on this
stage's list.**

**2. `R-N18` + `R-N16` — a pty, and the tool window.** `run.rs` grows a
second branch of `start`: when `cfg.pty`, `portable-pty` opens a pair, the
child gets the slave as its stdio and its own process group as now, and a
reader task pumps the master into a **byte ring** (1 MiB, `dropped_bytes`
stated) broadcast as `run_bytes { run_id, seq, data }` — base64, since the
wire is JSON. The line ring stays and is fed from the same bytes with ANSI
stripped, so `R-N7` and search see what they see today. `run_input` writes
the master only when `cfg.stdin`; `run_resize` sets the winsize. `Run`
gains `pid`, `cwd`, `kind`, `origin`, `group`. `Runs` evicts: three ended
runs per configuration, oldest first, and `run_started` carries what was
dropped. `singleton` is checked at start: a running singleton is refused
with *stop it first*, and the client turns that into the confirmation. The
window's `RunPane.tsx` becomes `desktop/src/panes/run/` — `RunPane` (the
strip and the picker), `ProcessTab`, `RunConsole` (xterm.js with no pty:
`term.write(bytes)`, links via a registered link provider, the read-only
corner), `DetailStrip`, `Toolbar` — and `lib/runLinks.ts` holds the
`file:line` patterns as a pure module. A `run:<id>` pane kind joins `file:`
and `diff:` in `panes.ts`, never restored by a saved layout, so ADR-0037's
pop-out is a registration rather than a feature.

**3. `R-N26` — the popup and the keys**, over the configurations that exist
after stage 2. `lib/runOps.ts` is the list as data — folders, digits, the
last-run mnemonic, the recent section — over a context of `{ configs, runs,
recents }`, with `GitOpsPopup`'s rules: digits stay stable, a disabled row
answers. `RunPopup.tsx` draws it; `keymap.ts` gains `run.popup`, `run.run`,
`run.stop`, `run.edit`, spelled by physical key (`F10` composes nothing, but
`Shift+F10` is GTK's context-menu key — verified in the real window before
the row is marked, `R-J38`'s lesson) with `MAC_KEYS` rows. `prefs` gains
`runSelected[repo]` and `runRecents[repo]`, machine-scoped.

**4. `R-N20` + `R-N23` — Gradle, and IntelliJ's files.** The Gradle kind's
`detect` reads `settings.gradle(.kts)` for **literal** `include("…")` lines
— `R-D35`'s lesson, a settings script is code and what is not literal is
not offered — and each subproject's build file for `mainClass.set("…")`,
offering a *Java application* entry beside `test` and `build`. `resolve`
has two shapes. A **task** configuration composes `[./gradlew, <tasks>…]`
with `--console=rich` under a pty and `plain` otherwise. An **application**
configuration composes a two-step launch — `[./gradlew, :sub:installDist]`,
then `[<sub>/build/install/<name>/bin/<name>, <args>…]` with `JAVA_OPTS`
carrying the VM options and the working directory the configuration's — and
`run.rs` gains a **build step** before spawn for it: the first command runs
to completion in the same tab, and a non-zero exit refuses the launch with
the tail of the build as the reason. `runconfig.rs` gains the IntelliJ reader: a
substring-tolerant XML read of `.run/*.run.xml` and
`.idea/runConfigurations/*.xml` (the sweep's `xml_config_types` grown into
one that reads options), classified against `INTELLIJ_HANDLED` /
`INTELLIJ_KNOWN_IGNORED`, with `Application` handed to the Gradle kind's
`from_intellij`, which resolves the module name to a path, reads the build
file, and returns params only when the main class agrees — otherwise
`cannot_run("IntelliJ resolves this main class through its own project
model, and applications/x declares a different one (or none); starting it
would need IntelliJ's own project model")`. `Sh` goes to the Shell kind, `Compound` to
Compound, `GradleRunConfiguration` and `CargoCommandRunConfiguration` to
theirs. The env mask covers the new file from the first commit. The sweep
exits non-zero on an unclassified **checked-in** IntelliJ type from this
stage on, and keeps `workspace.xml` as inventory. The ADR-0028 amendment
lands in the same commit as the reader.

**5. `R-N21` + `R-N22` + `R-N24` + `R-N25` — the forms, the store, the
dialog.** The Cargo and Shell kinds get their full schemas (npm and Python
already have trivial ones from stage 1). `store.rs` gains `run_configs
(repo, id, json, updated)`; `run_config_save { session_id, repo?, config }`
validates through the kind (`resolve` must succeed, the program must not be
an agent, shell text must not name one), mints `user:<uuid>` for a new id,
writes, and re-broadcasts `run_configs` to every window on that repository;
`run_config_delete` likewise. Both are gated by `writes_allowed`, the git
writes' gate. `RunConfigWindow.tsx` is the dialog: the tree from
`lib/runTree.ts` (pure — kind → folder → configuration, counts, sort), a
form drawn from the schema by `lib/runForm.ts` (pure — schema + params →
fields, validation, dirty state), the read-only banner for non-user origins
with *duplicate as your own*, and Run · Cancel · Apply. The ADR-0026
amendment and `A42` land here.

**6. `R-N17` — the Configurations tab.** The same `runTree` as a pinned
first tab in the tool window, with ▶ on each row, the badge, and *edit*
opening the dialog. Last because it is the cheapest and the popup and the
dialog already reach everything it lists.

**7. `R-N9` + `R-N10` + `R-N11` — debugging, after the run stages have
been used.** `dap.rs` is the client: adapter discovery by a documented
search order, the protocol over stdio, `runInTerminal` answered with the
run's pty, events forwarded as `debug_*` messages. `jdwp/` is the Java
adapter, in-process behind the same seam: the wire protocol, deferred
breakpoints on `CLASS_PREPARE`, `Method.LineTable` for line to index,
frames, locals and fields one level deep, strings. The window gets a
`DebugPane` dock tool, the gutter in `FilePane`, and the keys. Rust first
through an external adapter, because it proves the client with no adapter
code of ours; Java second. **Not started until `R-N13` has a verdict on the
run stages** — IntelliJ's debugger is the strongest incumbent in the product
and a half-debugger is what would send someone back to it.

**Repository scoping** rides stage 5's wire change: every message in the
run family gains `repo: Option<String>` under `R-D34`'s rule — it must
resolve inside `session_roots`, or the daemon refuses in words — and the
tool window's header is the picker the Git window already has. Until then
a session's own repository is what it has always been.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/run.rs` | `RunConfig` gains `kind`, `params`, `folder`, `singleton`, `pty`, `stdin`, `members`; `Origin::{IntelliJ, User}`; `KindSchema`, `Field`; `Run` gains `pid`, `cwd`, `kind`, `origin`, `group`, `dropped_bytes` |
| `crates/mogeung-core/src/wire.rs` | `repo` on the run family; `RunInput`, `RunResize`, `RunConfigSave`, `RunConfigDelete`, `FetchRunKinds`; `RunBytes`, `RunKinds` |
| `crates/mogeungd/src/kinds/{mod,gradle,cargo,npm,python,maven,shell,compound}.rs` | **new** — the seam and every kind |
| `crates/mogeungd/src/detect.rs` | bodies move into the kinds; the acceptance test stays |
| `crates/mogeungd/src/runconfig.rs` | the IntelliJ reader and its two lists; merge order user › files › detected |
| `crates/mogeungd/src/run.rs` | the pty branch, byte ring, input, resize, singleton, eviction |
| `crates/mogeungd/src/store.rs` | `run_configs` table and its three functions |
| `crates/mogeungd/src/state.rs`, `api.rs` | `repo` resolution through `session_roots`; the new verbs and their gates |
| `crates/mogeungd/src/bin/runconfigs.rs` | checked-in IntelliJ types classified and failing; `--detected` shows kinds |
| `crates/mogeungd/Cargo.toml` | `portable-pty`, `base64` |
| `crates/mogeungd/src/dap.rs` | **new**, stage 7 — adapter discovery, the protocol over stdio, `runInTerminal` onto the run's pty |
| `crates/mogeungd/src/jdwp/` | **new**, stage 7 — the Java adapter behind the DAP seam |
| `desktop/src/panes/DebugPane.tsx`, `panes/FilePane.tsx` | **new** dock tool; the breakpoint gutter |
| `desktop/src-tauri/` | **`cargo check`** after every core change |
| `desktop/src/wire/types.ts` | the mirror |
| `desktop/src/store/index.ts`, `store/prefs.ts` | byte buffers per run, `runTab`, `runPopup`, `runEdit`; `runSelected`, `runRecents` machine-scoped |
| `desktop/src/lib/runOps.ts`, `runTree.ts`, `runForm.ts`, `runLinks.ts` (+ tests) | the pure parts |
| `desktop/src/panes/run/*.tsx` | `RunPane`, `ProcessTab`, `RunConsole`, `DetailStrip`, `Toolbar`, `ConfigTree` |
| `desktop/src/ui/RunConfigWindow.tsx`, `ui/RunPopup.tsx` | the dialog; the popup |
| `desktop/src/lib/keymap.ts`, `lib/panes.ts`, `App.tsx` | keys; the `run:` pane kind; mounts |
| `docs/decisions/0039-*.md`, `0040-*.md`; amendments to `0026`, `0028` | the decisions above |
| `docs/design/run-and-debug.md` | rewritten; `covers:` grows by `kinds/`, `store.rs`, the new client files |
| `docs/design/wire-protocol.md`, `architecture.md` | the verbs; the tool window |
| `docs/guide/running.md` | **new** — the popup, the dialog, the console, the keys |
| `docs/product/roadmap.md`, `assumptions.md` | rows `R-N18`–`R-N26`, `R-N15`–`R-N17` re-sized; `A42`, `A43` — done with this spec |

### Test strategy

Pure functions first, where the port can be wrong quietly:

- **Every kind's `resolve`** on a table of params: Gradle's `--args=` is one
  element and contains no shell; Cargo's `--` separates program args; Shell
  text naming `claude` is refused. Each kind's `detect` against a fixture
  tree under `crates/mogeungd/tests/fixtures/kinds/`.
- **The IntelliJ reader** against fixtures copied from the corpus — *Media
  Driver (Single)*, *Stack (3 node)*, *Local Seed*, a `GradleRunConfiguration`
  from `workspace.xml`'s shape, one with an `<env>` holding a fake key, and
  one of a type nobody handles — asserting the verified mapping, the
  unrunnable reason when the build file disagrees, the compound's members,
  the masked value absent from the payload, and the sweep's exit code both
  ways. **A test that would fail today**: a fixture Gradle project with a
  settings file, two subprojects and the XML for one, asserting exactly one
  runnable `:a:run` and one unrunnable row naming `R-N9`.
- **The pty branch** against `/bin/sh -c 'printf "\033[31mx\033[0m"; read a;
  echo got:$a'`: the escape arrives intact, input echoes when allowed and is
  refused when not, the exit code lands after the last byte (`R-J68`'s
  drain, repeated twenty times), the byte ring bounds and states what it
  dropped. No language toolchain in the suite.
- **The store**: save → broadcast; save naming an agent → refused at save
  and, with the store edited underneath, at spawn; save beyond loopback
  without a token → refused; delete; a `singleton` start refused while
  running.
- **Client, in `GitPane.test.tsx`'s shape** — a fake socket, the exact
  `ClientMsg`: the popup's digit sends `run_start` with an id and **no
  message this window can send carries a command line** (today's assertion,
  kept and widened to the dialog: `run_config_save` carries params); `0`
  opens the dialog; `F4` opens it on the row; a folder's digit runs its
  last-run member; the dialog draws every `Field` kind and refuses to save an
  invalid form; a read-only origin has no Save; the tool window opens a tab
  per run, replaces on re-run, opens a second under multiple instances,
  shows the exit line, and a `File.java:12` click sends `fetch_file` and
  shows the pane; `runLinks` on Java, Rust, TypeScript and Gradle shapes.
- **The two-step launch**: the second command never runs when the first
  exits non-zero — asserted with `/bin/false` as the build step — and the
  refusal carries the build's tail.
- **The DAP client** against a fake adapter: a script speaking DAP over
  stdio with canned `initialize`, `setBreakpoints`, `stopped` and
  `runInTerminal`, so no toolchain is in the suite. The port-scrape regex
  and source lookup are pure and tested.
- **The JDWP adapter** against a real JVM, marked `#[ignore]`, run by hand,
  and **skipping loudly** when `javac` is absent — feature 0035's rule that a
  suite which silently skips is worse than one that says it skipped.
- `cargo test --workspace`, `npm test`, `npm run check`,
  `./scripts/check-docs.sh`, and the Tauri shell's `cargo check` at every
  stage.

### Risks and unknowns

- **`Shift+F10` is GTK's context-menu key and `F10` focuses a menubar in
  some toolkits.** The chords are spelled by physical key and verified in the
  real window before any row is marked — `R-J38` is the standing warning
  about a browser tab proving the wiring and not the trigger. `⌃⌥R` is the
  fallback spelling on every platform if `Alt+Shift+F10` does not reach the
  page.
- **Two buffers per run.** A byte ring for the terminal and a line ring for
  the claim; 1 MiB and 2,000 lines, times the runs kept. Eviction keeps it
  bounded and the numbers are stated in the payload — a log that quietly
  loses its middle is worse than one that admits it.
- **A Gradle daemon outlives the run.** Stop signals the process group;
  Gradle's daemon is its own group by design and survives, which is Gradle's
  contract and not a leak. `--no-daemon` is a field, off by default, and the
  row does not promise what it cannot do.
- **A task configuration's cwd and exit code are Gradle's.** Harmless for
  `test` and `build`, and for anything else the application launch exists.
  The row says which shape a configuration has.
- **`installDist` on every launch.** Incremental and a second or two with a
  warm daemon, and never skipped — a stale start script run silently is the
  failure this shape invites, and the `UP-TO-DATE` lines in the console are
  the proof it ran.
- **The JDWP port scrape rests on one stdout line.** `Listening for
  transport dt_socket at address: N` has been the JVM's wording for two
  decades and IntelliJ relies on it too; if it ever moves, a port mogeung
  picks in `address=` is the fallback.
- **A half-debugger is what sends someone back to IntelliJ.** No expressions
  and no conditions in the first cut, and stage 7 waits for `R-N13`'s
  verdict on the six before it.
- **A duplicated IntelliJ configuration diverges silently** from the XML it
  came from. The badge is the mitigation; a *re-read from IntelliJ* button
  is a later ask if it happens.
- **Clause 1's argument moves.** A user configuration means a client can
  define what runs — on loopback the terminal panel's trust, beyond it two
  gates. The amendment writes it down; `A24`'s row gets a sentence.
- **`A33` is still the pillar's bet** and every stage here makes the panel
  more like the IDE beside it. The removal condition is unchanged and the
  stage resting on `A42` is last, so five stages are not held hostage to
  the sixth.
- **Scope gravity.** A test runner tab, a Gradle tree, coverage, templates.
  The out-of-scope list is the fence, and each was named rather than
  omitted.

## Notes

*Filled during implementation. Surprises, dead ends, things the plan got wrong.*

- **2026-09-17, before any code.** The measurement in the spec was taken
  while writing it, by a script over `immix-trading-v3`, and it reversed the
  sequencing ADR-0028 chose four weeks earlier: the blocker was *"not one
  names a spawnable command"*, and 74 of 74 do, once *verified against the
  build file* replaces *inferred from the module name*. The difference is one
  file read per configuration. Worth recording that the ADR's own alternative
  — *infer `./gradlew :module:run`* — was rightly rejected and is not what is
  proposed: a mismatch is listed as unrunnable, not started.
- **The mockup** was published the same day at the link under
  [The design](#the-design). It was drawn, not looked at in a browser by the
  agent that drew it — the live page is the review surface, and anything
  clipped or wrong there is a one-line fix rather than a design question.
- **2026-09-17, later: two questions changed two things.** *How does it
  resolve the classpath* — it never does, Gradle does, and the question
  exposed that the plan had chosen the weaker of Gradle's two launch shapes
  for IntelliJ's `Application` configurations: `run` puts Gradle in front of
  the process and takes the exit code, the stop, stdin and the working
  directory with it, where the start script `installDist` writes gives them
  all back and matches what IntelliJ itself does (`delegatedBuild=false`).
  *How would debugging work* — the `R-N9`–`R-N11` shape stands, Rust turns
  out to be adapter configuration over adapters already on the machine, and
  Java has no adapter to borrow, so the Java adapter is the one piece of
  debugger mogeung would build, behind the DAP seam. Both are in the plan as
  stage 4's launch shape and stage 7, and stage 7 waits for the week of use.
