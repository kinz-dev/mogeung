---
title: Getting started
status: active
updated: 2026-10-12
---

# Getting started

Running it is one command — the window starts a daemon if none is watching and
attaches to one if there is. If you want watching to continue with no window
open, start `mogeungd` yourself first and the window will attach to it.

**Developing on mogeung?** `./scripts/start.sh` builds and runs both processes
with one command, or `mprocs` puts them side by side. The rest of this page is
the manual route.

## What mogeung does

It watches the agent sessions **you** run — Claude Code, Codex, Qwen Code,
opencode — and tells you which one needs you. It never starts, steers or stops
an agent — you keep using the CLI exactly as you do today.

## Requirements

Rust 1.85+, `git`, and at least one of the four CLIs on `PATH`; `tmux`, so a
session can be hosted in a pane. macOS or Linux. On Linux, `mold` as well
(`sudo apt install mold`): the repository's `.cargo/config.toml` links with it,
and a build without it stops at the first link with *cannot find 'ld'*.

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

That is the development pair. For a daemon that keeps running after you close
the terminal — a box you only reach from another machine, say —
`./scripts/mogeungd start` builds it and starts it in the background, and
`stop`, `restart`, `status` and `logs` manage it (`-h` for the rest).

To **install it on this machine** instead — so
mogeung is an application in the launcher rather than two terminals — one
script does the lot:

```sh
./scripts/install.sh
```

It builds both halves, puts `mogeungd`, `yolomo`, `yolomop`, `qwenmo`,
`codexmo` and `opencodemo` in `~/.local/bin`, and
installs the window's `.deb` with `dpkg -i`, asking for `sudo` at that step and
no other. `./scripts/install.sh --uninstall` takes it all back off. See
[desktop/README.md](../../desktop/README.md) for the bundles themselves — the
`.rpm` and the AppImage, and what each is for.

**After that, it keeps itself up to date.** An installed window looks at the
checkout it was installed from once a minute, and when a new commit lands on
`main` and has been still for two minutes it offers the update: *Update*
builds it in `~/.mogeung/update/tree` while you keep working, installs it —
asking for your password in the system's own dialog, never in mogeung — and
then asks *Restart now* or *Later*. Agents keep running through a restart. The
status bar shows the commit you are running and how far behind `main` it is.
To stop the looking, click *stop checking* in the offer, or put
`update_check = false` in `~/.mogeung/config.toml`; `update_branch`,
`update_interval` (seconds) and `update_source` (the checkout) are there too.
See [ADR-0041](../decisions/0041-the-window-updates-itself-through-the-os.md).

There is nothing to configure and no repos to register. If you have run `claude`,
`codex`, `qwen` or `opencode` anywhere in the last 14 days, it appears.

## Seen, or hosted

An agent started in an ordinary terminal can be **seen** — it is in the queue
with its diff, its tokens and its status — but it cannot be **hosted**: a pty
has exactly one master, and that terminal owns it, so mogeung's Agent pane can
only point you at it. Start it under tmux instead and the same live session can
be in your terminal and in a mogeung pane at once ([ADR-0010](../decisions/0010-attach-a-terminal-never-own-one.md)).

That is all `yolomo`, `yolomop`, `qwenmo`, `codexmo` and `opencodemo` do:

```sh
yolomo          # claude, under tmux
yolomop         # the same claude session, through `llmproxy --claude`
qwenmo          # qwen, under tmux
codexmo         # codex, under tmux
opencodemo      # opencode, under tmux
codexmo -d      # ...headless: mogeung's pane becomes its only terminal
```

Run one in the directory you want the agent working in. Plain `claude`, `qwen`,
`codex` or `opencode` still works and is still watched — you just get a pane that points
rather than one that attaches, which is the usual reason an agent shows up
correctly and the Terminal pane stays empty.

The five are siblings on purpose rather than one script with a flag
([ADR-0029](../decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)): they differ in the
binary and in how each CLI is told to stop asking, and that is easier to read
than to abstract. Each skips approval prompts, because an agent blocked on a
prompt it never wrote to disk reads as *working* while it waits for you.
`codexmo` is the one that stops short of its CLI's most dangerous flag: it uses
`--ask-for-approval never --sandbox workspace-write` rather than
`--dangerously-bypass-approvals-and-sandbox`, because that flag also turns off a
**sandbox**, which none of the other CLIs has to give up. You get your
usual Codex session minus the prompts. Ask for the full bypass and it wins:

```sh
codexmo -- --dangerously-bypass-approvals-and-sandbox
```

`opencodemo` passes opencode's own `--auto`, which approves everything your
opencode config does not explicitly deny. opencode is the one CLI here that
*does* write a blocked prompt down, so an opencode session started without
`--auto` still reads correctly: it ranks **APPROVE** — *needs approval for
bash: …* — while it waits for you, and a question the agent asks you with its
`question` tool reads as **WAITING** with the question as its activity.

mogeung never runs these for you ([ADR-0003](../decisions/0003-observe-do-not-spawn.md)).
You start the agent; mogeung notices.

Closing the window stops nothing that outlives it. The window reconnects on its
own, so restarting the daemon underneath it is fine.

## Options

```
mogeungd --listen 127.0.0.1:7717 --db <path> --poll-ms 1500   # mogeungd --help for the rest
```

`--notify`, `--push-url`, `--token`, `--ssh-target`, `--no-beacon`,
`--allow-run` and the model flags are the ones you will meet; every flag has a
`config.toml` twin except `--allow-run`. The window takes no flags: which
daemon it talks to is a setting, the connection dot in the top bar. In a
browser (`npm run dev`) `?url=ws://host:7717/ws` points one visit somewhere else.

State lives in `~/.mogeung/mogeung.db`. Nothing is ever written to `~/.claude`.

## First five minutes

1. Open two or three terminals and start `yolomo` (or `codexmo`, `qwenmo`,
   `opencodemo`) in different repos.
2. Watch the Attention list. Whichever session is waiting for you sorts to the
   top.
3. Click one, open **Changes**, and read top-down — risk order is reading order.
4. Tick hunks as you read them.

The tool is worth very little with one session open. It is built for three or
four. See [the-queue.md](the-queue.md).
