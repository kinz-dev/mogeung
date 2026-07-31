---
title: Watching a remote machine
status: active
updated: 2026-07-31
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
> A daemon binding beyond loopback **must** have a `--token`; without one it
> refuses to start rather than serving openly. That is the floor, not safety:
> the daemon still has **no TLS**, so the token and everything after it travel
> in clear text, and anyone holding it can read every transcript on that machine
> and open terminals on it.
>
> This is a deliberate, recorded bet ([A24](../product/assumptions.md)): a token
> on a trusted network, TLS only once the bet fails. If your network is not one
> you trust, use **Route A** below, which does not open a port at all.

## Choosing a daemon from the window

The flags below are how you reach a daemon the first time. After that, use the
window: click the connection dot in the top bar, or press `Alt+D`.

Saved daemons live in `~/.mogeung/connections.json` (written owner-only, since
it holds tokens). Each has a name, a URL and an optional token, and the one you
last connected to is reopened next launch — a flag on the command line still
overrides it for that run.

**Scan the network** asks over mDNS which daemons are advertising nearby. A
daemon only appears if it was started with `--advertise`, which is off by
default — the broadcast tells everything on the segment that this machine is
watching Claude Code sessions and where to reach it, and that is not a thing to
do to someone on guest wifi without being asked.

Finding a daemon connects to nothing. It fills in the form, you supply the
token, you press Connect. A daemon can only advertise from a non-loopback bind,
which already requires a token — so anything you find here will want one.

**Switching keeps the window and drops the daemon.** Your layout, keymap and
prefs describe *this window* and survive. Everything the old daemon said —
sessions, diffs, repos, open files — goes, because it describes a different
machine. Terminal tabs detach rather than close: tmux keeps the shell running
over there, and switching back re-attaches.

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

The tunnel does not confuse the window about whose machine it is looking at.
The daemon says which machine it is on and the window compares (`R-I5`), so the
local-only actions below refuse correctly even though the address you dialled
is `127.0.0.1`. Hover the connection dot to see who is answering:

```
watching /home/dev/.claude on devbox
mogeungd 0.1.0 · pid 4242
```

> Until 2026-07-31 this was not true — the window guessed from the address
> string, and a tunnel read as local, silently re-enabling actions that then
> opened editors and terminals on the wrong machine. If your window predates
> that, upgrade it; the daemon alone is not enough, since it is the window that
> does the comparing.

---

## Route B — a listening daemon on a trusted LAN

Fewer moving parts, and correct refusals in the window. Everything travels in
clear text, so this is for a network you already trust.

**On the dev box:**

```sh
mogeungd --listen 0.0.0.0:7717 --token "$(openssl rand -hex 24)"
```

Add `--advertise` if you want the window's network scan to find it. Off by
default, deliberately — see "Choosing a daemon from the window" above.

Copy the token it printed. Leave `--token` off and the daemon will not start —
it prints what to do instead and exits. Set it and the daemon still warns at
startup that the token is travelling in clear text; that warning is not
boilerplate.

There is no `--insecure`. If you want a listening daemon with no token, the
answer is Route A: bind loopback and let ssh carry it.

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
# how a client reaches this machine for terminal panes (R-I6)
ssh_target = "dev@devbox"
# announce on the local network so the window's scan finds it (R-I8)
advertise = true

# on your laptop
url    = "ws://devbox:7717/ws"
token  = "…"
```

Then both sides are just `mogeungd` and `mogeung`. Set the file's permissions
to `600` — it now holds a credential.

---

## Route C — behind a TLS reverse proxy

Route B's token travels in clear text. If that is not good enough and you would
rather not tunnel, put a TLS terminator in front of the daemon: the clients can
dial `wss://` as of 2026-07-31.

Keep the daemon on loopback and let the proxy be the only thing listening:

```sh
mogeungd --token "$(openssl rand -hex 24)"     # 127.0.0.1:7717, as by default
```

Caddy needs two lines and gets a certificate by itself:

```
mogeung.example.com {
    reverse_proxy 127.0.0.1:7717
}
```

Then point the window at the proxy:

```sh
mogeung --url wss://mogeung.example.com/ws --token <the-token>
```

Keep the token. The proxy encrypts the path; it does not decide who may use it,
and the daemon behind it will still hand every transcript to whoever asks.

The proxy must forward WebSocket upgrades — Caddy and recent nginx do this
without being asked; older nginx configs need `Upgrade`/`Connection` headers set
explicitly.

**Certificates come from the operating system's trust store**, so a private CA
works if the machine running the window already trusts it. A self-signed
certificate that nothing trusts will be refused, and there is no flag to skip
the check.

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
`--push-url` forwards them anywhere). The terminal panes work too, over ssh —
see below.

**Refuses, on purpose** — these act on a machine, and the machine is not yours:

| Action | What it says |
|---|---|
| Jump to terminal | its terminals are on `devbox` |
| Open in IntelliJ / VS Code / Finder | that path lives on `devbox` |
| Launch terminal | would open a terminal on `devbox` |
| Screenshot / image preview | the image lives on the other machine |

A refusal is a message in the error strip, not a silent no-op, and it names the
machine — a refusal you cannot act on reads as a bug.

## Terminals, when the daemon is elsewhere

The terminal panel and the agent pane both drive tmux. Against a remote daemon
they drive it **over ssh**, on the machine that has the files (`R-I6`), so a
shell tab opens where the work is rather than on your laptop.

For that, the daemon has to say how it is reached:

```sh
mogeungd --ssh-target dev@devbox        # or ssh_target in its config.toml
```

Any destination ssh understands — `user@host`, or a `Host` alias from your
`~/.ssh/config`, which is the tidier option since the alias can carry the port,
the identity file and a `ProxyJump`.

Without it the panel says so and opens nothing:

> terminals run on devbox, and it has not published an ssh target — start its
> daemon with `--ssh-target user@host`

That refusal is deliberate. There is no guess available: the hostname a daemon
reports need not resolve from here, and need not be the name ssh wants.

**Authentication happens in the pane.** A key passphrase or a host-key prompt
appears in the terminal itself and you answer it there — it is a real terminal,
so nothing needs to be pre-arranged. Agent forwarding, `ControlMaster` and the
rest are your ssh config's business; mogeung adds only `-t`, which tmux needs.

Sessions still outlive the window — over there. `tmux attach -t <name>` on the
dev box reaches the same shell, and the tab's tooltip names the host so you know
which machine to run that on.

## Troubleshooting

**"refusing to listen on … with no token."** Working as intended: a bind beyond
loopback needs `--token`. The message prints both ways out. Note that it also
applies to the window — `mogeung --addr 0.0.0.0:7717` hosts a daemon, so it is
refused on the same terms and exits rather than opening one quietly.

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
