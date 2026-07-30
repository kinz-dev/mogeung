---
title: Watching a remote machine
status: active
updated: 2026-07-30
---

# Watching a remote machine

Your agents run on a dev box. You want the queue on your laptop.

mogeung splits cleanly for this, because it was already split: **the daemon is
the product and every window is a client with no local authority**. "Remote" is
not a mode — it is simply being the window that did not start the daemon.

```
your laptop                        the dev box
┌──────────────┐                   ┌─────────────────────────┐
│ mogeung      │ ── ws://…/ws ──→  │ mogeungd                │
│ (the window) │ ←── snapshot,  ── │   watches ITS ~/.claude  │
│              │     queue, git    │   runs git on ITS repos  │
└──────────────┘                   └─────────────────────────┘
```

Everything the daemon answers — the queue, diffs, git, the file explorer,
insight, health — arrives over the wire and works unchanged. What cannot cross
the wire is anything that touches *your* machine: opening IntelliJ, focusing a
terminal, launching one. Those refuse rather than acting on the wrong box.

> **Read this before you expose a port.**
>
> The daemon has **no TLS**. Its `--token` is a shared secret in clear text, and
> it is **optional** — a daemon started without one accepts anyone who can
> reach the port. Anyone who can is able to read every transcript on that
> machine and open terminals on it.
>
> This is a deliberate, recorded bet ([A24](../product/assumptions.md)): a token
> on a trusted network, TLS only once the bet fails. If your network is not one
> you trust, use **Route A** below, which does not open a port at all.

## Requirements

`mogeungd` installed and Claude Code running on the remote box; `mogeung` on
your laptop. Both should be the same build — the wire protocol tolerates a
version skew in either direction for optional fields, but not for new messages.

---

## Route A — over SSH (recommended)

Nothing listens beyond localhost, nothing needs a token, and everything is
encrypted by ssh. The daemon stays bound to `127.0.0.1` exactly as it is by
default.

**On the dev box**, start a daemon that outlives your shell:

```sh
mogeungd --notify &          # or under systemd, launchd, or a tmux session
```

**On your laptop**, forward the port and attach:

```sh
ssh -N -L 7717:localhost:7717 devbox &
mogeung --url ws://127.0.0.1:7717/ws
```

That is the whole setup. The window attaches instead of starting its own daemon
— that is what `--url` means, and it is the reason to prefer `--url` over
`--addr` here.

> **One rough edge you must know about on this route.** The window decides
> whether a daemon is remote by looking at the address it dialled, and through a
> tunnel that address is `127.0.0.1`. So the window concludes it is local and
> **re-enables the five actions that should refuse** — "Open in IntelliJ" will
> open your laptop's IntelliJ at a path that only exists on the dev box, and
> "Launch terminal" will start a terminal on your laptop.
>
> Nothing is damaged; the actions simply misfire. Avoid them on this route, or
> use the workaround below.

**Workaround — a tunnel that still looks remote.** Forward to a different
loopback address, which the window does not recognise as local, so the refusals
work correctly:

```sh
ssh -N -L 127.0.0.2:7717:localhost:7717 devbox &
mogeung --url ws://127.0.0.2:7717/ws
```

Linux binds all of `127.0.0.0/8` already. On macOS, add the alias first:
`sudo ifconfig lo0 alias 127.0.0.2`.

This exploits how the check is written rather than fixing it, so treat it as a
stopgap.

---

## Route B — a listening daemon on a trusted LAN

Fewer moving parts, and correct refusals in the window. Everything travels in
clear text, so this is for a network you already trust.

**On the dev box:**

```sh
mogeungd --listen 0.0.0.0:7717 --token "$(openssl rand -hex 24)"
```

Copy the token it printed. The daemon will warn loudly at startup that it is
listening beyond localhost — that warning is not boilerplate, and it says
something different depending on whether you set a token. Read it once.

**On your laptop:**

```sh
mogeung --url ws://devbox:7717/ws --token <the-token>
```

The token rides the WebSocket URL as `?token=…`, because a browser socket
cannot carry a custom header. Over HTTP the window sends
`Authorization: Bearer …` instead. A wrong token is an immediate 401, not a
hang.

### Put it in the config file instead

Retyping a token is how tokens end up in shell history. Both binaries read
`~/.mogeung/config.toml`, and a flag always beats the file:

```toml
# on the dev box
listen = "0.0.0.0:7717"
token  = "…"

# on your laptop
url    = "ws://devbox:7717/ws"
token  = "…"
```

Then both sides are just `mogeungd` and `mogeung`. Set the file's permissions
to `600` — it now holds a credential.

---

## Checking that it worked

The window's status line names the daemon it is attached to. Beyond that:

```sh
# from the laptop, against either route
curl -s -H "Authorization: Bearer <token>" http://devbox:7717/api/health
```

`/api/health` is the honest answer to *"is the board empty because nothing is
happening, or because mogeung cannot see?"* — it reports unknown line types,
unreadable lines and the Claude Code version it is reading. A remote daemon that
has gone blind looks exactly like a quiet afternoon, which is the failure this
endpoint exists to make visible.

If `curl` returns nothing at all, no daemon answered. If it returns
`401 missing or wrong token`, the daemon is fine and your token is not.

## What works, and what refuses

**Works unchanged** — the queue, session detail, transcripts, diffs and review
marks, the whole Git pane, the file explorer and go-to-file, content search,
insight, health, notifications (they fire on the *daemon's* machine, and
`--push-url` forwards them anywhere).

**Refuses, on purpose** — these act on a machine, and the machine is not yours:

| Action | What it says |
|---|---|
| Jump to terminal | its terminals are on the other machine |
| Open in IntelliJ / VS Code / Finder | that path lives on the other machine |
| Launch terminal | would open a terminal on the other machine |
| Screenshot / image preview | the image lives on the other machine |

A refusal is a message in the error strip, not a silent no-op.

## Known rough edges

Remote support is built but has not been through a dogfooding week (`R-I4`),
and these are the things that would find you first.

**The in-app terminal panel is not remote-aware.** Unlike the four actions
above, the terminal tabs at the bottom of the window are not guarded. They start
a shell on **your** machine, rooted at a path that exists on the **dev box** —
so you get a local shell in a directory that is not there. Use ssh in a real
terminal for now.

**Tunnel-vs-refusal**, as described in Route A.

## Troubleshooting

**"could not bind — is a daemon already running?"** on the dev box. One already
is. That is the design: whoever wins the bind is the daemon. Attach to it.

**The window started its own daemon instead of attaching.** You used `--addr`
where you wanted `--url`. `--addr` races for the local bind first; `--url` never
starts a daemon, which is what you want when the daemon is elsewhere.

**Connected, but the board is empty.** The daemon warns at startup when the
machine has no transcripts at all — check its log for *"is Claude Code installed
for this user?"*. A daemon watching the wrong `$HOME` (a service account, say)
sees nothing and reports honestly that it sees nothing.

**Everything is stale after the laptop slept.** The window reconnects on its
own; the daemon kept scanning throughout, so the first snapshot after reconnect
is current.

See also [troubleshooting.md](troubleshooting.md) and
[away-from-the-desk.md](away-from-the-desk.md), which covers push notifications
— the natural companion to a daemon you are not sitting in front of.
