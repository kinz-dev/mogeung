---
title: Getting started
status: active
updated: 2026-07-25
---

# Getting started

Running it is one command — the window starts a daemon if none is watching and
attaches to one if there is. If you want watching to continue with no window
open, start `mogeungd` yourself first and the window will attach to it.

**Developing on mogeung?** `./scripts/start.sh` builds and runs both processes
with one command, or `mprocs` puts them side by side. The rest of this page is
the manual route.

## What mogeung does

It watches the Claude Code sessions **you** run and tells you which one needs
you. It never starts, steers or stops an agent — you keep using `claude` exactly
as you do today.

## Requirements

Rust 1.85+, `git`, Claude Code 2.1.x on `PATH`. macOS or Linux.

The window additionally needs node 20+ and the system webview
(`libwebkit2gtk-4.1-dev` on Linux) to build, since it is a Tauri application —
see [desktop/README.md](../../desktop/README.md). The daemon needs neither.

## Run it

```sh
cargo build --release
./target/release/mogeungd            # terminal 1 — the daemon

cd desktop && npm install
npm run tauri dev                    # terminal 2 — the window
```

`npm run tauri build` produces a real binary plus a `.deb`, `.rpm` and an
AppImage, and after that the window is just an application you launch.

There is nothing to configure and no repos to register. If you have run `claude`
anywhere in the last 14 days, it appears.

Closing the window stops nothing that outlives it. The window reconnects on its
own, so restarting the daemon underneath it is fine.

## Options

```
mogeungd --listen 127.0.0.1:7717 --db <path> --poll-ms 1500
```

The window takes no flags: which daemon it talks to is a setting, `Alt+D`. In a
browser (`npm run dev`) `?url=ws://host:7717/ws` points one visit somewhere else.

State lives in `~/.mogeung/mogeung.db`. Nothing is ever written to `~/.claude`.

## First five minutes

1. Open two or three terminals and start `claude` in different repos.
2. Watch the left panel. Whichever session is waiting for you sorts to the top.
3. Click one, open **Changes**, and read top-down — risk order is reading order.
4. Tick hunks as you read them.

The tool is worth very little with one session open. It is built for three or
four. See [the-queue.md](the-queue.md).
