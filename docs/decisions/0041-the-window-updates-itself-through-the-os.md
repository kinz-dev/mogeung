---
title: The window updates itself — from a committed tip, elevated by the operating system, never by a password it holds
status: active
updated: 2026-09-24
decided: 2026-09-24
---

# ADR-0041 — The window updates itself: it builds a committed tip and elevates through the operating system, never through a password it holds

## Context

Asked 2026-09-24: *"build a auto update alert and popup a screen … a scheduled
job that trigger a build regularily (may be every 30s) … or cargo-watch … Once
a changes is detected, we popup a screen to confirm if user want to update or
not and capture the user admin password and trigger the ~/scripts/install.sh
with the sudo passwd passed to the script … popup a screen to ask user to
restart the application … "OK" … or "Later"."*

mogeung is built on the machine it runs on, by agents committing to `main`
many times a day, and installed by `./scripts/install.sh` — which on Linux
needs root for exactly one step, `dpkg -i`. The installed window lags the code
until someone runs the script by hand, and nothing said by how much: every
build was `0.1.0`. Three things make this a decision rather than a feature:

- **What to build.** The working tree is edited by agents mid-change; a build
  of *what is on disk* installs half-finished work.
- **How to get root.** The window renders text it did not write — agent
  transcripts, diffs, markdown, mermaid, files. A password typed into it passes
  through the page, the IPC bridge and a pipe into `sudo -S`.
- **How to restart.** The window usually hosts the daemon in-process
  (ADR-0009), bound to its port; agents live in tmux (ADR-0010), and ADR-0003
  says mogeung never stops one.

## Decision

The shell may update the window, in four steps the window asks for one at a
time ([feature 0049](../archive/features/0049-update-in-place.md), `R-J105`):

1. **An update is a commit.** The shell reads the branch the build follows
   (`main` by default) in the checkout `install.sh` recorded, once a minute, and
   offers a new commit once the branch has been still for two minutes. Nothing
   is built until you say *Update*.
2. **It is built from that commit, not from the working tree** — in a worktree
   of its own, `~/.mogeung/update/tree`, detached, with its own `target/`.
3. **Root is asked for by the operating system.** `install.sh --elevate
   pkexec` runs `dpkg -i` through polkit, whose dialog is drawn by another
   process and names the command. **mogeung has no password field and never
   holds one.** On macOS nothing needs root.
4. **The restart waits for this process to exit** before starting the new one,
   so the port the hosted daemon held is free for the next window to bind.
   Agents are untouched: an agent pane is a `tmux attach` client, and a
   restart is a detach and a reattach. A daemon this window did not start is
   named and left running.

## Alternatives

- **A build every 30 s, or a file watcher (`cargo-watch`).** A release build
  of the window compiles the daemon twice and takes minutes, so a 30 s loop is
  a machine that never stops compiling; a watcher fires on every save,
  including an agent's half-finished edit. Reading a git ref costs milliseconds
  and fires only on a commit.
- **A password box in the window, handed to `sudo -S`.** The ask. Lost on the
  page's exposure: anything that ever manages to run script in a webview that
  renders agent output could read it, and `sudo -S ./install.sh` would run the
  whole script — builds included — as root, which `install.sh` goes to some
  trouble today to avoid. It also invites *remember my password*.
- **Tauri's `AppHandle::restart`.** Spawns the new process *before* exiting,
  so the new window finds the port still held, attaches to a daemon about to
  die, and is left with nothing.
- **`tauri-plugin-updater`.** Built for signed artefacts served from a release
  endpoint. Right for distributing mogeung, wrong for building from source on
  your own machine.
- **The daemon doing it.** The daemon also runs on other machines, is the
  product, and has no business building or elevating. The shell already owns
  window-side files (ADR-0036, the arcade's history).

## Consequences

- The window says which commit it runs and how far behind the branch that is,
  from any installed build.
- An update is two clicks and one system dialog, and a failed build changes
  nothing that is installed.
- A second `target/` lives under `~/.mogeung/update/tree` — gigabytes, and a
  cold first build. Deleting the directory is safe; the next update makes it
  again.
- Only a build that `install.sh` installed can find its source; one installed
  before `install.json` existed says so and asks for one more install by hand.
- A `main` that compiles and misbehaves is installed as readily as a good one.
  There is no test gate and no rollback yet.
- A daemon started with `scripts/mogeungd` runs from the checkout's own
  `target/`, not `~/.local/bin`, and this does not update it at all.

## Revisit if

- mogeung is handed to other people: a signed release channel replaces
  building from source.
- The window is installed per user (an AppImage, or files under `~/.local`),
  which removes the root step altogether.
- A bad `main` gets installed and costs real time: the test gate and a
  one-click rollback stop being optional.
