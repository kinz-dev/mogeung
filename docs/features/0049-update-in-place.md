---
title: Update in place
status: active
updated: 2026-09-24
roadmap: R-J105
depends_on: [A46, A47, A48]
---

# 0049 — Update in place

## Spec

*Owned by the human. What and why, never how.*

### Problem

Asked 2026-09-24: *"Is that possible to build a auto update alert and popup a
screen to ask user to restart mogeung? … Once a changes is detected, we popup a
screen to confirm if user want to update or not and capture the user admin
password and trigger the ~/scripts/install.sh … And once it is done popup a
screen to ask user to restart the application, it is either "OK" to restart it
automatically, or "Later"."*

mogeung is built on the machine it runs on, by agents that commit to `main`
many times a day. The installed window only catches up when someone remembers
to open a terminal, run `./scripts/install.sh`, type a password, wait minutes,
quit the window and start it again. Until then the window in use is older than
the code, and a report made against it may be about something already fixed.
Nothing says how far behind it is, because a build carries no record of the
commit it was built from — every one is `0.1.0`.

### Assumptions

- **A46** `UNTESTED` — restarting the window never stops an agent. Every agent
  pane is a `tmux attach` client, and the hosted daemon resumes from durable
  tail offsets (`R-A6`), so a relaunch is a detach and a reattach. This is what
  makes *Restart now* safe to offer at all, and ADR-0003's line — mogeung never
  stops an agent — rides on it.
- **A47** `UNTESTED` — "a change" means **a new commit on `main`**, not an edit
  in the working tree. Agents edit the tree mid-change; building what is on
  disk would install half-finished work.
- **A48** `UNTESTED` — the operating system's own authentication dialog —
  `pkexec` and the desktop's polkit agent on Linux; none needed on macOS — is
  acceptable in place of a password box in mogeung, and works from the
  installed window with no terminal behind it.

> Per the rule: A46 and A48 are tested **before** anything is built — each is
> an hour's spike, below. A47 is a question for the owner as much as a
> measurement.

### Acceptance

- [x] The window says which commit it is running, and how far behind `main`
      that is.
- [x] Within a minute of a new commit on `main` the status bar says the window
      is behind; once `main` has been still for two minutes, an alert offers
      the update, listing what changed (commit subjects), with **Update** and
      **Not now**. *(Reworded 2026-09-24 when built: the first draft said the
      alert came within a minute, which contradicted its own plan's two-minute
      settle.)*
- [x] **Update** builds that commit where the running window is not disturbed,
      with progress to watch and a way to cancel. A failed build says why and
      leaves the installed version as it was.
- [ ] Installing asks for the password **once**, in the operating system's own
      dialog, for the one step that needs root (`dpkg -i` on Linux) — mogeung
      never sees the password. On macOS nothing is asked. *(Built; unticked
      until A48's spike — the dialog needs a hand at the keyboard.)*
- [ ] When the install is done: **Restart now** relaunches into the new build,
      every agent still running and its pane reattached; **Later** dismisses
      the dialog and leaves a quiet *restart to update* mark in the status bar
      until you do. *(Built and tested to the shell's edge; unticked until
      A46's spike on the installed window.)*
- [x] A daemon this window did not start is not restarted behind your back: it
      is named, with its pid, and restarting it is your click.
- [x] Checking can be turned off, and nothing polls while it is.

### Explicitly out of scope

- Building on every file save, or on a timer. Only a commit triggers anything.
- Installing without asking. Every install is a click.
- A password field in mogeung, or a remembered password. See *Why not a
  password box*, below.
- Updating other machines in the federation. Each machine updates itself, from
  its own checkout.
- A signed release channel (`tauri-plugin-updater`), and Windows (`R-I3`).

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

**Phase 0 — test the assumptions first.**

1. *A48*: from the **installed** window (launched from the dock, no terminal),
   run `pkexec dpkg -i <a .deb>` through a throwaway shell command. Pass: GNOME's
   dialog appears, names the command, and the install completes. Also note what
   happens on *Cancel* and on a wrong password — both have to read as a sentence
   in the window, not as a hung spinner.
2. *A46*: with two agents running (`yolomo`, `qwenmo`) and their panes open,
   call Tauri's `AppHandle::restart()` from the installed window. Pass: both
   tmux sessions survive, both panes reattach, the queue comes back without
   re-reading any transcript whole.

**Phase 1 — know what is running (useful on its own).**

- Stamp every build with its commit: a `build.rs` for `mogeungd` and an addition
  to the shell's, emitting `MOGEUNG_COMMIT` (`git rev-parse HEAD`, with `+dirty`
  when the tree was not clean). `mogeungd --version` prints it, the daemon's
  hello carries it, the window shows it.
- `install.sh` writes `~/.mogeung/install.json` — the commit, the checkout it was
  built from (`$ROOT`), the branch, the time. That file is how the installed
  window finds its source without being told.

**Phase 2 — notice.**

- The shell polls `git -C <source> rev-parse refs/heads/main` every 60 s —
  milliseconds of work, no build. A different commit → an event to the window →
  the alert, with `git log --oneline <running>..<main>` as the list of changes.
- Waits for `main` to have been still for two minutes before alerting, because
  agents commit in bursts and one alert per burst is enough.
- `[update]` in `~/.mogeung/config.toml`: `check = true`, `branch = "main"`,
  `interval = 60`.

**Phase 3 — build and install.**

- A dedicated worktree, `~/.mogeung/update/tree`, checked out *detached* at the
  commit being installed — never the working checkout, which agents are editing
  (A47). It keeps its own `target/`, so every build after the first is
  incremental.
- `install.sh` gains three flags, each useful by hand too:
  - `--build-only` — build everything, install nothing, need no root;
  - `--elevate pkexec` — run the one root step as `pkexec dpkg -i "$DEB"`
    instead of `sudo`, so there is no terminal to type into and the dialog names
    the exact command;
  - `--install-app` (macOS) — copy the `.app` into `/Applications`, the step the
    script today prints and leaves to you.
- The shell runs `--build-only` as you, streaming its output to the alert and to
  `~/.mogeung/update/build.log`, then `--no-build --elevate pkexec` to install.
  The builds get `mogeungd::env::path()`, because a window launched from the dock
  has none of `~/.cargo/bin` or a version-managed `node` on its `PATH` — the
  trap `R-J87` already hit once, with `tmux`.
- The previous `.deb` is kept in `~/.mogeung/update/previous/`, so a build that
  compiles but misbehaves can be rolled back with one click.

**Phase 4 — restart.**

- **Restart now** → `AppHandle::restart()`, which relaunches the new binary at
  the same path. First, anything unsaved is named: a Notes draft is the one
  known case that would be lost.
- **Later** → dismiss; a *restart to update* mark stays in the status bar.
- Attached to a daemon this window did not start → it is named with its pid,
  and the dialog says the running daemon is still the old build. It is not
  stopped for you — `R-J101`'s rule, *never kill what you did not start*.

### Why not a password box

The ask was to capture the admin password in the window and pass it to
`install.sh`. The plan keeps the rest of the ask and replaces that one step.

- **The window renders text it did not write** — agent transcripts, diffs,
  markdown, mermaid, files. A password typed there passes through the page, the
  IPC bridge and a pipe into `sudo -S`, and anything that ever manages to run
  script in the page can read it. The polkit dialog is drawn by another process,
  and mogeung never holds the secret at all.
- **It elevates one command, not a script.** `pkexec dpkg -i <file>` is the
  whole of what runs as root, and the dialog says so. `sudo -S ./install.sh`
  hands root the entire script — builds included — and `install.sh` goes to
  some trouble today to avoid running anything but `dpkg` as root.
- **A password box invites *remember my password***, which would be worse.
- **Longer term, no root at all.** Installing the window per user — an AppImage,
  or the `.deb`'s contents unpacked into `~/.local` with a user desktop entry —
  would make updates need no password on Linux either. That changes how
  `install.sh` installs, so it is an ADR of its own, not part of this.

### Alternatives considered

- **A build every 30 s, or `cargo-watch`.** Refused. A release build of the
  window compiles the daemon twice and takes minutes, so a 30 s loop is a machine
  that never stops compiling; and a file watcher fires on every save, including
  an agent's half-finished edit. Polling a git ref costs milliseconds and fires
  only on a commit.
- **`tauri-plugin-updater`.** Built for signed artefacts served from a release
  endpoint (a manifest plus signatures). Right for handing mogeung to other
  people, wrong for building from source on your own machine. Revisit if
  mogeung is ever distributed.
- **A systemd user timer or launchd agent doing the build.** Would work with the
  window closed. But the ask is an alert in the window, and the window is what
  restarts; start in the shell, and move the build out later if that is wanted.
- **The daemon doing it.** No. The daemon also runs on other machines, has no
  business building or elevating, and is the product; this is the shell's
  concern, like `connections.json` (ADR-0036) and the arcade's files.

### Decisions for the owner

1. **What counts as "a change"?** Recommended: a commit on local `main` (A47).
   Alternatives: a push to `origin/main` (the shell would `git fetch`), or
   another branch.
2. **Build on the click, or ahead of it?** Recommended: on the click. A
   background build makes *Update* instant, at the price of minutes of CPU after
   every burst of commits. Could be a setting, `prebuild = false`.
3. **Run the test suites before offering an install?** `cargo test --workspace`
   and `npm test` would add minutes, and would stop a red `main` from being
   installed. Recommended as a setting, off by default.
4. **The password.** Recommended: the system dialog, as above. If a password box
   in mogeung is still wanted, that is the owner's call, but it should be
   recorded in the ADR with the risks above.

An ADR — *the window may update itself: it builds a committed tip and elevates
through the operating system, never through a password it holds* — is written
when this plan is approved, and `docs/design/architecture.md` gains the shell's
new `~/.mogeung/update/` directory beside the arcade's files.

### Files touched

- `crates/mogeungd/build.rs` (new), `desktop/src-tauri/build.rs` — the commit
  stamp; `crates/mogeungd/src/main.rs` — `--version`; the daemon's hello.
- `scripts/install.sh` — `--build-only`, `--elevate pkexec`, `--install-app`,
  and writing `~/.mogeung/install.json`.
- `desktop/src-tauri/src/update.rs` (new) — the poll, the worktree, the build
  and install runners, the restart; registered in `lib.rs`.
- `desktop/src/ui/UpdateAlert.tsx` (new), the status bar, a store slice for the
  update's state.
- `docs/decisions/00NN-…` (the ADR), `docs/design/architecture.md`,
  `docs/guide/` (how to turn it off).

### Risks and unknowns

- **Build time and disk.** A second `target/` is several GB and the first build
  is cold. To be measured in phase 3; if it is too slow, sharing the checkout's
  `target/` through `CARGO_TARGET_DIR` is the thing to try.
- **A `main` that compiles and misbehaves** gets installed. Mitigated by the
  optional test gate and by keeping the previous `.deb` for a rollback.
- **Replacing a running binary.** `dpkg` replaces `/usr/bin/mogeung-desktop`
  while it runs; Linux keeps the old image for the running process, which is why
  a restart is needed rather than a crash. To confirm on macOS for the `.app`.
- **The beacon blinks.** A restart takes the hosted daemon off the LAN for a
  moment; peers see it go and come back. Acceptable, but it will show.
- **Two routes to a daemon.** A daemon started by `scripts/mogeungd` runs from
  the checkout's own `target/`, not from `~/.local/bin`, so this update does not
  change it at all. The dialog has to say that rather than imply otherwise.

### Test strategy

- Unit, in the shell: the poller against a temporary repository (a commit moves
  it; a burst alerts once; a stopped check polls nothing); the worktree against
  a temporary repository; the command lines built for each platform.
- `install.sh`: the new flags exercised on a throwaway home, as `R-J101` was, and
  run under macOS's `/bin/bash` 3.2.
- The window: the alert's states — available, building, failed, installed,
  restart — against mocked events, including *Later* leaving the mark.
- By hand: phase 0's two spikes, then a real update of this machine from one
  commit to the next with agents running.

## Notes

*Filled during implementation.*

**Built 2026-09-24, at the owner's direction, before phase 0.** The goal was
*"complete this auto update/restart feature"*, and both spikes need a hand at
the installed window — A48 is a password typed into a system dialog, which an
agent must not do, and A46 is a restart of the window the owner is sitting
in. So everything was built and tested up to the edge of those two, and the
first real update is the spike: **install this build once by hand**
(`./scripts/install.sh`), which writes `~/.mogeung/install.json` and puts the
update code in the window; the next commit on `main` is then offered, and
taking it exercises A48 and A46 together.

**The four decisions were taken as recommended**: a commit on local `main` is
the trigger (A47), the build starts on the click, there is no test gate, and
the root step is the system's dialog. Each is a config key or a one-line
change if the owner decides otherwise.

**The restart is not Tauri's.** `AppHandle::restart` spawns the new process
and *then* exits; with the daemon hosted in-process on its port, the new
window would find the port taken, attach to the dying daemon and be left with
nothing. A detached `sh` waits on this pid with `kill -0` and only then starts
the binary (`open -n` the bundle on macOS). Tauri's own `current_exe` is read
for the path — it is captured at start-up, so it still names
`/usr/bin/mogeung-desktop` after `dpkg` has replaced the file underneath.

**The build's `PATH` is the login shell's.** A window from the dock has none
of `~/.cargo/bin` or a version manager's `node`; `mogeungd::env::path()` only
adds the tmux and Homebrew places. The shell asks `$SHELL -ilc` once, reads
the `PATH` between two markers (a profile may print anything), and adds
`~/.cargo/bin` and the daemon's candidates.

**`install.sh` grew three flags and a record**, each usable by hand:
`--build-only`, `--elevate pkexec` (exit 3 when the dialog is dismissed, 4
when not authorised, so *you said no* reads differently from *it broke*) and
`--install-app` on macOS. The record is written from `--git-common-dir`, so an
install from the update worktree still names the checkout it belongs to —
checked from a real linked worktree.

**The build stamp re-runs only when HEAD moves.** `build.rs` names the `HEAD`
file, the branch ref and `packed-refs` — and only those that exist, since cargo
re-runs a build script every time for a missing path. Checked: a second
`cargo build -p mogeungd` reports it `Fresh`.

**Not built**: keeping the previous `.deb` for a one-click rollback, and the
optional test gate. Both are in the plan and neither is in the acceptance;
ADR-0041's *Revisit if* says what would make them necessary.

**A Notes draft is the one thing a restart could lose**, so `NotesTool`
reports an unsaved draft to a small register (`lib/unsaved.ts`) and the
restart dialog names it. Everything else in the window saves as it goes.
