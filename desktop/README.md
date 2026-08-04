# mogeung-desktop

The second client. React + TypeScript + Monaco, packaged with Tauri.

The daemon is unchanged and unaware: this speaks the same WebSocket protocol the
egui window does, so **both can run at once against one daemon**. That is the
whole migration plan — port a pane at a time, keep using the client that works,
retire the old one at parity.

## Running it tomorrow morning

The daemon is already what you run. Start it however you normally do, then:

```sh
cd desktop
npm install          # already done once
npm run dev          # http://localhost:1420
```

Open `http://localhost:1420` in a browser. That is a real client — the daemon
serves WebSocket and REST on localhost and does not care what dialled it.
Everything except the two terminal panes works there.

Pointing at a different daemon: `http://localhost:1420/?url=ws://host:7717/ws`.
It is remembered after the first time.

### The native build, for the terminals

```sh
cd desktop && npm run tauri dev
```

The system dependencies are already installed on this machine, and the shell
builds and links. The two terminal panes are the only thing that needs it — a
browser tab has no pty to hold, so in the browser they say so rather than
showing a black rectangle.

What has *not* happened yet is a pty actually being opened. It builds and
exports its five commands; the first real `tmux attach` is still ahead.

## Building a binary you can hand to someone

```sh
cd desktop
npm install                # once
npm run tauri build        # frontend, then the native shell, then the bundles
```

One command does all three stages: `beforeBuildCommand` runs `tsc --noEmit &&
vite build` into `dist/`, cargo builds the shell in release, and Tauri packages
the result. On this machine that produces, under
`desktop/src-tauri/target/release/`:

| | what it is |
|---|---|
| `mogeung-desktop` | the executable itself, ~32 MB, no installer around it |
| `bundle/deb/mogeung_0.1.0_amd64.deb` | installs to `/usr/bin/mogeung-desktop` with icons and a desktop entry |
| `bundle/rpm/mogeung-0.1.0-1.x86_64.rpm` | the same, for rpm distributions |
| `bundle/appimage/mogeung_0.1.0_amd64.AppImage` | ~90 MB, self-contained — `chmod +x` and run it anywhere |

The AppImage is the one to send to someone who has nothing installed; it carries
webkit and its libraries. The `.deb` and `.rpm` are ~15 MB because they do not,
and expect the system's own.

Useful variants — the flags belong to the tauri CLI, so they go after `--`:

```sh
npm run tauri build -- --no-bundle          # just the executable, skip packaging
npm run tauri build -- --bundles deb        # only the format you want
npm run tauri build -- --debug              # release layout, debug symbols
```

**The version and the name come from `src-tauri/tauri.conf.json`**, not from
`package.json` — `version` names the artefacts and `productName` is what the
window and the desktop entry say. Bump it there or every build overwrites the
last one's files.

### What it needs installed

Linux wants webkit's headers and the packaging tools; on Debian/Ubuntu that is:

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
     libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

macOS needs the Xcode command-line tools and produces `.app` and `.dmg` instead;
the bundle targets are `"all"`, so each platform makes what it can. The first
AppImage build downloads `linuxdeploy` and caches it under `~/.cache/tauri` —
that step, and only that step, needs the network.

The first build is minutes, not seconds: this shell compiles **the daemon too**,
because `mogeungd` is a path dependency. Later builds reuse the cache.

### It is one executable, like the egui one

The built app hosts a daemon on a thread when nothing is already listening, and
attaches when something is — [ADR-0009](../docs/decisions/0009-the-window-may-host-a-daemon.md),
ported faithfully in `src-tauri/src/daemon.rs`. So the binary alone is enough,
and the top bar says `hosting` when it is the one watching. Closing that window
stops the watching; run `mogeungd --notify` separately if you want notifications
to outlive it.

Note that `cargo build --release` at the repo root does **not** build this.
`src-tauri` is its own cargo workspace on purpose — see the comment at the top of
`src-tauri/Cargo.toml` — so a machine without webkit's headers can still run the
`cargo test --workspace` that CLAUDE.md gates on.

## What is here, and what is not

| | state |
|---|---|
| Attention queue — scopes, filters, pins, snooze, hide, collapse to a strip, group by repo (`R-B6`), label (`R-B26`), forget | done |
| Transcript — virtualised, markdown, find (`R-B36`), mark a turn | done |
| Code — Monaco, read-only, tabs, split, per-file wrap, symbol outline (`R-B28`), markdown preview (`R-B29`), blame gutter (`R-D10`), in-file bookmarks | done |
| Changes — hunks, risk flags, read marks, hide-read, hide-noise, word diff (`R-D5`), side-by-side (`R-D6`), syntax colour (`R-D4`), blast radius (`R-D9`) | done |
| Git — log, filters, commit diff, local changes, refs, stashes, fetch | done |
| Insight — **redesigned**: charts for analytics and burn, plus all eight views | done |
| Rail — Files, Search, Notes, Bookmarks (`R-B40`, `R-B41`, `R-F13`) | done |
| Command palette — actions and go-to-file, Tab switches | done |
| Keyboard — the egui bindings, kept | done |
| **Agent pane** (tmux attach) | built — Tauri only, and the Rust needs webkit's headers to compile |
| **Terminal panel** (your shells) | built — same |
| Health (`R-A4`) and keymap rebinding (`R-B12`) windows | done — this row said otherwise for a week |
| Connections window (`R-I7`) — add, name, switch, forget | done — no LAN browsing, which needs multicast the webview cannot do |
| Launch a session, prompt builder, ambient window | not built |
| Verification — observed runs and claims, plus the signal runner and coverage (`R-E2`, `R-E5`) | done — the command runs only when you press it |
| Desktop notification banners (`R-C1`) | done — off until you turn them on, and only from a window that **hosts** its daemon, so a `mogeungd --notify` cannot double up |
| Git write verbs (`R-D19`–`R-D22`) | deliberately absent — see below |

**The Code pane is a viewer and stays one.** Monaco runs with `readOnly: true`
because a read-only editor is a strictly better *viewer* than a hand-rolled one,
not because editing is one flag away. Pillar K, and the daemon offers no write
path for worktree files at all.

**Notes are the one thing that writes**, and that is the line ADR-0015 draws: a
document under `~/.mogeung` is your own writing; a worktree file belongs to the
repository.

## Shape

```
src/
  wire/types.ts     the protocol, hand-mirrored from mogeung-core::wire
  wire/client.ts    the socket: reconnect, re-subscribe, queue sends
  store/            one zustand store; the only place a ServerMsg is handled
  lib/              explorer, search, keymap, formatting, Monaco theme
  ui/               chrome — top bar, queue, rail, status, palette, diff
  ui/tools/         the rail's four tool windows
  panes/            the dockview panes
src-tauri/          the native shell: ptys, global shortcut, window
```

`src-tauri` is **its own cargo workspace**, not a member of the repo's. If it
were a member, a machine without webkit's headers could not run
`cargo test --workspace` for the daemon — and that command is a gate.

## Checks

```sh
npm run check   # tsc --noEmit
npm test        # vitest
npm run build   # tsc + vite build
```

The smoke test mounts the whole window in jsdom with the socket stubbed. It
cannot tell you whether anything *looks* right — but it catches the class of
failure that shows a blank page, and it already earned its keep once: it found
an infinite render loop caused by a store selector that minted a new object on
every call.
