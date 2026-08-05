---
title: Watching a remote machine
status: active
updated: 2026-08-05
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

The top of that window is where you are, not where you could go: the daemon
you are connected to, whether the socket is up, and — from the identity it
publishes (`R-I5`) — which `~/.claude` it is watching, on which host, at which
version. Read that line before trusting anything below it. The URL there has
any token blanked, because this window is the one people share when asking for
help.

**`LOCAL` is always the first row, and always where a launch starts.** It names
the daemon on this machine on the default port, it is never saved to the file,
and it has no Edit or Forget: the destination you need when a remote is
unreachable is the one that must not be losable. Starting mogeung binds that
port and hosts a daemon if nothing is there, exactly as it does with no
connections saved at all.

Saved daemons live in the window's own storage, alongside your other settings.
Each has a name, a URL and an optional token. The row you last
connected to is marked *last used*, but **no launch dials a remote for you** —
you pick it, per session. `--url` on the command line still points that run
wherever you say.

> This changed on 2026-07-31. Until then the last-used connection was reopened
> automatically, which was sticky in both directions: a window started at home
> kept dialling a dev box that was off, and — because the remembered URL was
> applied before the local-port check — it also stopped hosting a local daemon,
> so the machine in front of you was not being watched and nothing said so.

**On this network** lists daemons advertising nearby, and keeps listening for
as long as the window is open — like a wifi picker rather than a search box.
Rows accumulate; they are not cleared between rounds.

That matters because of how mDNS actually behaves: a host answers piecemeal,
per interface and per address family, over several seconds. Expect a machine to
appear with one address and gain others a moment later. Give it a few seconds
before concluding nothing is there — the panel says *listening…* while it is
still too early to tell.

A daemon only appears if it was started with `--advertise`, which is off by
default: the broadcast tells everything on the segment that this machine is
watching Claude Code sessions and where to reach it, and that is not a thing to
do to someone on guest wifi without being asked.

Finding a daemon connects to nothing. **Add** fills in the new-daemon form under
**Saved**, you supply the token, you press Save — and then Connect on the row it
made. A daemon can only advertise from a non-loopback bind, which already
requires a token, so anything you find here will want one.

**What you hid, pinned, labelled, tagged or bookmarked belongs to the machine**,
not to the window: it is filed under the daemon's machine id, so two windows
watching two daemons no longer overwrite each other's, which they did until
2026-07-31. Your terminal tabs are in there too, and swap with the daemon: a
tab rooted in a worktree on the dev box does not follow you to the laptop that
happens to have the same path. The shells it leaves behind are detached, not
killed, and come back with their tabs when you switch back. Upgrading migrates
everything you already had into this machine's file.

**Switching keeps the window and drops the daemon.** Your layout, keymap and
prefs describe *this window* and survive. Everything the old daemon said —
sessions, diffs, repos, open files — goes, because it describes a different
machine. Terminal tabs detach rather than close: tmux keeps the shell running
over there, and switching back re-attaches.

## Requirements

`mogeungd` installed and Claude Code running on the remote box; the mogeung
window on your laptop. Both should be the same build — the wire protocol
tolerates a version skew in either direction for optional fields, but not for
new messages.

**The window takes no command-line flags.** It is a Tauri application, and where
it dials is a setting rather than an argument: `Alt+D`, then Connect. In the
browser (`npm run dev`, or any build served over http) `?url=` on the page does
the same thing for one visit, which is how you point a second tab at a second
daemon. The `mogeung --url …` invocations in older notes were the egui window's,
retired on 2026-08-05
([ADR-0020](../decisions/0020-the-egui-client-is-retired.md)).

---

## Route A — over SSH (recommended)

Nothing listens beyond localhost, nothing needs a token, and everything is
encrypted by ssh. The daemon stays bound to `127.0.0.1` exactly as it is by
default.

**On the dev box**, start a daemon that outlives your shell:

```sh
mogeungd --notify &          # or under systemd, launchd, or a tmux session
```

**On your laptop**, forward the port and open the window:

```sh
ssh -N -L 7717:localhost:7717 devbox &
```

The tunnel puts the dev box's daemon on your `127.0.0.1:7717`, which is the
address the window already dials, so `LOCAL` reaches it and there is nothing to
configure. The window attaches instead of hosting: the port is taken, and taken
is the whole test.

That is also the one thing to watch — if the tunnel is down when you start, the
window finds the port free and hosts a *local* daemon on it, then the tunnel
cannot bind. The connection line says which one you got: read the host it names
before trusting the board.

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

**On your laptop:** `Alt+D` → **Add**, name it, URL `ws://devbox:7717/ws`, paste
the token, Save, Connect.

The token rides the WebSocket URL as `?token=…`, because a browser socket
cannot carry a custom header. Over HTTP the window sends
`Authorization: Bearer …` instead. A wrong token is an immediate 401, not a
hang.

### Put it in the config file instead

Retyping a token is how tokens end up in shell history. `mogeungd` reads
`~/.mogeung/config.toml`, and a flag always beats the file. The window does not
read it — what it needs is one saved connection, which it already remembers:

```toml
# on the dev box
listen = "0.0.0.0:7717"
token  = "…"
# how a client reaches this machine for terminal panes (R-I6)
ssh_target = "dev@devbox"
# announce on the local network so the window's scan finds it (R-I8)
advertise = true
```

Then the dev box is just `mogeungd`. Set the file's permissions to `600` — it
now holds a credential, and so does the saved connection on your laptop.

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

Then point the window at the proxy — `Alt+D` → **Add**, with
`wss://mogeung.example.com/ws` and the token.

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
so nothing needs to be pre-arranged.

Two things worth setting up anyway, because each pane is its own ssh connection:

```
# ~/.ssh/config on the laptop
Host devbox
    User you
    ControlMaster auto
    ControlPath ~/.ssh/cm-%r@%h:%p
    ControlPersist 10m
```

`ControlMaster` reuses one connection for every pane, so you authenticate once
instead of per tab. With `ssh-copy-id` as well, you authenticate not at all.

**The remote command runs through a login shell** — `exec $SHELL -l -c 'tmux …'`
rather than plain `tmux …`. That is not decoration. `ssh host cmd` gets a
non-interactive, non-login shell, and zsh then sources only `.zshenv`, so macOS
never runs `path_helper` and Homebrew's `/opt/homebrew/bin` is absent from
`PATH`. tmux is installed and invisible:

```
zsh:1: command not found: tmux
```

A login shell sources the profile, which is where package managers put their
`PATH`. The cost is that anything your profile prints runs once per pane; tmux
clears the screen on attach, so you will rarely see it.

Sessions still outlive the window — over there. `tmux attach -t <name>` on the
dev box reaches the same shell, and the tab's tooltip names the host so you know
which machine to run that on.

## Troubleshooting

**Scan finds nothing.** In order of likelihood: the daemon was not started with
`--advertise`; its `--listen` is loopback, which refuses to advertise because
nobody could reach it; a firewall is blocking the port (macOS prompts on first
bind — a dismissed prompt looks exactly like a network problem); the two
machines are on different subnets or a guest VLAN; or the network drops
multicast between hosts, which many do.

To tell a silent daemon apart from a silent network, ask the protocol directly
rather than through the UI:

```sh
cargo run -p mogeungd --example browse_probe
```

Same code the Scan button runs, with nothing else in the way.

**"refusing to listen on … with no token."** Working as intended: a bind beyond
loopback needs `--token`. It applies to a daemon the *window* is hosting too —
the check runs on the address actually bound, before anything is served, so a
window cannot become a daemon that `mogeungd` would have refused to be.

**`command not found: tmux` in a terminal pane**, from a machine where tmux is
definitely installed. Fixed on 2026-07-31 by running the remote command through
a login shell — upgrade the **window**, which is the side that builds the
command. If it persists, tmux really is missing there, or it is installed
somewhere your login profile does not add to `PATH`; `ssh devbox 'echo $SHELL;
$SHELL -lc "command -v tmux"'` answers both in one go.

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
