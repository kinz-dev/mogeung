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

## What is here, and what is not

| | state |
|---|---|
| Attention queue — scopes, filters, pins, snooze, hide, collapse to a strip | done |
| Transcript — virtualised, markdown, find (`R-B36`), mark a turn | done |
| Code — Monaco, read-only, tabs, split, preview/pinned tabs, per-file wrap | done |
| Changes — hunks, risk flags, read marks, hide-read, hide-noise | done |
| Git — log, filters, commit diff, local changes, refs, stashes, fetch | done |
| Insight — **redesigned**: charts for analytics and burn, plus all eight views | done |
| Rail — Files, Search, Notes, Bookmarks (`R-B40`, `R-B41`, `R-F13`) | done |
| Command palette — actions and go-to-file, Tab switches | done |
| Keyboard — the egui bindings, kept | done |
| **Agent pane** (tmux attach) | built — Tauri only, and the Rust needs webkit's headers to compile |
| **Terminal panel** (your shells) | built — same |
| Health / keymap / connections windows | not built |
| Blame gutter, symbol outline, markdown preview, bookmarks-in-file | not built |
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
