---
title: Away from the desk
status: active
updated: 2026-10-08
---

# Away from the desk

mogeung's window is not the only way to be told a session needs you.

> **Close the window and notifications stop — unless the daemon is separate.**
>
> The window on its own hosts the daemon inside its own process, so closing it
> stops watching. Everything on this page assumes a daemon that outlives the
> window:
>
> ```sh
> ./scripts/mogeungd start -- --notify   # from a checkout: builds, starts, keeps a pid and a log
> mogeungd --notify                      # or the installed binary, under systemd, launchd or tmux
> ```
>
> Then open the window as usual — it finds the port taken and attaches, and
> closing it changes nothing. The window shows `hosting` next to the connection
> dot when it is the one running the daemon.

## Desktop notifications

```sh
mogeungd --notify          # or notify = true in ~/.mogeung/config.toml
```

Posts a desktop banner — `osascript` on macOS, `notify-send` on Linux — when a
session **starts** needing you: waiting, blocked on approval, failed,
rate-limited, stalled, or leaving unread changes.

It notifies on the *transition*, once. A session that has been waiting for an
hour is not re-announced every 1.5 seconds, because a notifier that repeats
itself is one you turn off — and then you miss the one that mattered.

Off unless you ask for it. The window has its own **banners** toggle in the
top bar, which speaks only while the window is hosting its daemon, so a
separate `mogeungd --notify` and a window never double up.

## Push to a phone

```sh
mogeungd --notify --push-url https://ntfy.sh/your-private-topic
```

POSTs the same message body to any URL that accepts one — ntfy, Pushover, a
Slack incoming webhook, your own endpoint. Uses `curl`, so whatever works there
works here. `push_url` in `config.toml` is the same setting.

Pick an unguessable topic name: anyone who knows it can read your session
titles.

## The tray

`mogeung-tray` is a menu-bar item with no window: the count of sessions
waiting for you, which daemon it is counting for, a menu that lists them, and
one click to raise the window. It counts the sessions of every machine you
follow, each labelled *on HOST*.

## Ambient mode

**Ambient** in the top bar opens a large-text board showing only what needs you,
sized to be readable across a room; the `×` closes it. Pair it with **follow**
in the queue panel and a second monitor becomes a status board you never touch.

## Acting on it from elsewhere

A notification tells you; acting on it needs the window. There used to be a
small web client served at `/` for triaging from a phone — it was removed on
2026-07-30, unused (`R-C3`).

What replaced it is the daemon itself: run `mogeungd` on the machine doing the
work and attach the window from wherever you are, or **follow** that daemon
from the window on your laptop and its sessions join your own queue, opened in
place. See [Watching a remote machine](remote.md), which covers the ssh route,
the shared token and following. Notifications fire on the daemon's machine, and
`--push-url` forwards them anywhere; a followed machine's waiting session
notifies here too, labelled *— on HOST*.

The REST API is still there if you want to build something — `/api/queue`,
`/api/health` and the rest are plain JSON over `curl`, and `mogeung queue
--json` wraps the common ones.
