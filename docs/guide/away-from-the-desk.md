---
title: Away from the desk
status: active
updated: 2026-07-25
---

# Away from the desk

mogeung's window is not the only way to be told a session needs you.

## Desktop notifications

```sh
mogeungd --notify
```

Posts a macOS banner when a session **starts** needing you — waiting, blocked on
approval, failed, stalled, or leaving unread changes.

It notifies on the *transition*, once. A session that has been waiting for an
hour is not re-announced every 1.5 seconds, because a notifier that repeats
itself is one you turn off — and then you miss the one that mattered.

Off unless you ask for it.

## Push to a phone

```sh
mogeungd --notify --push-url https://ntfy.sh/your-private-topic
```

POSTs the same message body to any URL that accepts one — ntfy, Pushover, a
webhook, your own endpoint. Uses `curl`, so whatever works there works here.

Pick an unguessable topic name: anyone who knows it can read your session
titles.

## The web client

The daemon serves a small client at `/`:

```sh
open http://127.0.0.1:7717/
```

Queue, diffs, mark-as-read and snooze — enough to triage from the sofa. Anything
wanting a keyboard stays in the desktop window.

### Using it from a phone

You have to bind beyond localhost:

```sh
mogeungd --listen 0.0.0.0:7717 --notify
```

> **There is no authentication.** Anyone who can reach that port can read every
> transcript on this machine and open terminals on it. The daemon logs a warning
> when you do this.
>
> A trusted home network is the most this is suitable for. On anything else use
> a VPN or an SSH tunnel:
>
> ```sh
> ssh -L 7717:127.0.0.1:7717 your-mac
> ```

## Ambient mode

**Ambient** in the top bar opens a large-text board showing only what needs you,
sized to be readable across a room. `esc` closes it.

Pair it with **follow** in the queue panel and a second monitor becomes a status
board you never touch.
