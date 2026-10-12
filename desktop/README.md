# mogeung-desktop

The window. React + TypeScript + Monaco, packaged with Tauri.

The daemon is unchanged and unaware, which is the whole point: this speaks the
WebSocket protocol that existed before it did. It was built beside the egui
window against the same daemon and on 2026-08-05 became the only client
([ADR-0020](../docs/decisions/0020-the-egui-client-is-retired.md)). What it
shows and what it can do is the roadmap's business
([docs/product/roadmap.md](../docs/product/roadmap.md)); this page is how to
run and build it.

## Running it

The daemon is already what you run. Start it however you normally do, then:

```sh
cd desktop
npm install
npm run dev          # http://localhost:1420
```

Open `http://localhost:1420` in a browser. That is a real client — the daemon
serves WebSocket and REST on localhost and does not care what dialled it.
Everything except the terminal panes, the LAN listener and the connection file
works there, and each of those says so rather than failing silently.

Pointing at a different daemon: `http://localhost:1420/?url=ws://host:7717/ws`.
It is remembered after the first time.

### The native build, for the terminals

```sh
cd desktop && npm run tauri dev
```

The terminal panes (the Agent pane's `tmux attach` and your own shells), the
UDP listener behind *Found on your LAN*, the `0600` connection file and the
updater live in the Tauri shell — a browser tab has no pty to hold and no
socket to listen on.

Vite is told **not** to watch `src-tauri/`. That directory holds ~17k cargo
fingerprint files once the Rust half has been built, and watching them exhausts
Linux's inotify handles — which surfaces as `ENOSPC` from `node:fs/watchers`,
reads as "disk full", and is nothing of the kind. If you still hit the limit
with other watchers running, `sysctl fs.inotify.max_user_watches` is the knob.

## Building a binary you can hand to someone

```sh
cd desktop
npm install                # once
npm run tauri build        # frontend, then the native shell, then the bundles
```

To build *and install it here* — the daemon too, and the `.deb` through `dpkg`
— use [`../scripts/install.sh`](../scripts/install.sh) instead; it wraps this
command and defaults to `--bundles deb`, which is the only one an install on
this machine needs. What follows is the bundler itself.

One command does all three stages: `beforeBuildCommand` runs `tsc --noEmit &&
vite build` into `dist/`, cargo builds the shell in release, and Tauri packages
the result. On Linux that produces, under `desktop/src-tauri/target/release/`:

| | what it is |
|---|---|
| `mogeung-desktop` | the executable itself, ~32 MB, no installer around it |
| `bundle/deb/mogeung_<version>_amd64.deb` | installs to `/usr/bin/mogeung-desktop` with icons and a desktop entry |
| `bundle/rpm/mogeung-<version>-1.x86_64.rpm` | the same, for rpm distributions |
| `bundle/appimage/mogeung_<version>_amd64.AppImage` | ~90 MB, self-contained — `chmod +x` and run it anywhere |

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

Linux wants webkit's headers, the packaging tools and `mold`, the linker the
repository's `.cargo/config.toml` asks for; on Debian/Ubuntu that is:

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
     libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev mold
```

macOS needs the Xcode command-line tools and produces `.app` and `.dmg` instead;
the bundle targets are `"all"`, so each platform makes what it can. The first
AppImage build downloads `linuxdeploy` and caches it under `~/.cache/tauri` —
that step, and only that step, needs the network.

The first build is minutes, not seconds: this shell compiles **the daemon too**,
because `mogeungd` is a path dependency. Later builds reuse the cache.

### It is one executable

The built app hosts a daemon on a thread when nothing is already listening, and
attaches when something is — [ADR-0009](../docs/decisions/0009-the-window-may-host-a-daemon.md),
ported faithfully in `src-tauri/src/daemon.rs`. The hosted daemon reads
`~/.mogeung/config.toml` for its bind, token, ssh target and beacon, exactly as
`mogeungd` would. So the binary alone is enough, and the top bar says `hosting`
when it is the one watching. Closing that window stops the watching; run
`mogeungd --notify` separately if you want notifications to outlive it.

Note that `cargo build --release` at the repo root does **not** build this.
`src-tauri` is its own cargo workspace on purpose — see the comment at the top of
`src-tauri/Cargo.toml` — so a machine without webkit's headers can still run the
`cargo test --workspace` that CLAUDE.md gates on. The flip side: a change to a
public `mogeung-core` type compiles everywhere that command looks and can still
break this shell, so `cargo check --manifest-path src-tauri/Cargo.toml` is the
check to run after one.

## What writes, and what never does

Monaco runs with `readOnly: true` on worktree files because a read-only editor
is a strictly better *viewer* than a hand-rolled one, not because editing is
one flag away: pillar K, and the daemon offers no write path for worktree files
at all ([ADR-0019](../docs/decisions/0019-a-viewer-not-an-editor.md)). Notes,
tasks and scratch files are the things that write, and they write under
`~/.mogeung` only ([ADR-0015](../docs/decisions/0015-markdown-is-the-truth.md),
[ADR-0035](../docs/decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)).
Git writes stay local and are gated by the daemon.

## Shape

```
src/
  wire/types.ts     the protocol, hand-mirrored from mogeung-core::wire
  wire/client.ts    the socket: reconnect, re-subscribe, queue sends
  store/            one zustand store; the only place a ServerMsg is handled
  lib/              keymap (the bindings are data), explorer, search, git, federation, formatting, Monaco theme
  ui/               chrome — top bar, queue, rail, status, palette, diff, the windows and popups
  ui/tools/         the rail's tool windows: Files, Search, Scratch, Notes, Tasks, Bookmarks, Ask Mogeung
  panes/            the dockview panes: Agent, file, diff, scratch, and the dock tools
src-tauri/          the native shell: ptys, global shortcut, the connection file, the LAN listener, popouts, the updater
```

`src-tauri` is **its own cargo workspace**, not a member of the repo's. If it
were a member, a machine without webkit's headers could not run
`cargo test --workspace` for the daemon — and that command is a gate.

## Checks

```sh
npm run check   # tsc --noEmit — vitest does not typecheck, so this is not optional
npm test        # vitest
npm run build   # tsc + vite build, which is what the bundler runs
```

The smoke test mounts the whole window in jsdom with the socket stubbed. It
cannot tell you whether anything *looks* right — but it catches the class of
failure that shows a blank page, and it already earned its keep once: it found
an infinite render loop caused by a store selector that minted a new object on
every call.
