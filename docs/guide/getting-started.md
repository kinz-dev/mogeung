---
title: Getting started
status: active
updated: 2026-07-25
---

# Getting started

## What mogeung does

It watches the Claude Code sessions **you** run and tells you which one needs
you. It never starts, steers or stops an agent — you keep using `claude` exactly
as you do today.

## Requirements

Rust 1.85+, `git`, Claude Code 2.1.x on `PATH`, macOS.

## Run it

```sh
cargo build --release

./target/release/mogeungd    # terminal 1 — the daemon
./target/release/mogeung     # terminal 2 — the window
```

There is nothing to configure and no repos to register. If you have run `claude`
anywhere in the last 14 days, it appears.

Closing the window stops nothing. The UI reconnects on its own, so restarting
the daemon underneath it is fine.

## Options

```
mogeungd --listen 127.0.0.1:7717 --db <path> --poll-ms 1500
mogeung  --url ws://127.0.0.1:7717/ws
```

State lives in `~/.mogeung/mogeung.db`. Nothing is ever written to `~/.claude`.

## First five minutes

1. Open two or three terminals and start `claude` in different repos.
2. Watch the left panel. Whichever session is waiting for you sorts to the top.
3. Click one, open **Changes**, and read top-down — risk order is reading order.
4. Tick hunks as you read them.

The tool is worth very little with one session open. It is built for three or
four. See [the-queue.md](the-queue.md).
