---
title: A connection manager
status: shipped
updated: 2026-09-08
roadmap: [R-I16]
depends_on: [A24]
---

# 0040 — A connection manager

## Spec

### Problem

`R-I7` gave the window add, name, switch and forget, and stopped there. What it
did not give is **edit**, and the gap shows up the first time you get something
wrong: a typo in an address is forget-and-add, a dev box that gets renamed is
forget-and-add, and a port that moves is forget-and-add. Asked 2026-09-03:

> enhance remote connection (remote connect manager panel that can CRUD a list
> of remote connection setting)

Three more things are wrong underneath that ask:

- **A `Connection` is `{name, url}`**, so there is nowhere to put the token
  `R-I10`'s ladder requires for a non-loopback bind. The window has no token
  handling at all, which means the only way to reach a token-gated daemon is to
  type `?token=…` into the address — putting a shared secret into
  `localStorage` by the back door.
- **The list has no order you control.** Entries land in the order they were
  added, and the one you use every day sinks.
- **The URL is the identity.** `ConnectionsWindow` keys its rows on `c.url`,
  which is exactly why editing one was never possible.

### Assumptions

[A24](../product/assumptions.md) — a read-only daemon is safe to reach over a
trusted network with a shared token, without TLS — is **`UNTESTED`**.

> If any is `UNTESTED`, the work is to test it — not to build this.

**This row does not rest on that bet, and the rule is answered rather than
waived.** A24 is a claim about what is safe to do *on a network*. `R-I10`
already shipped the whole ladder on it and is ✅: a non-loopback bind with no
token refuses to start, and `wss://` is available through a reverse proxy. This
row adds **no network posture whatsoever** — it does not open a connection type
that was closed, does not relax `server::admit`, and does not make a remote
daemon reachable that was not reachable this morning.

What it changes is **where a secret rests on this disk**, and it changes it in
the direction of less exposure: from the webview's unencrypted `localStorage`,
where it arrives today smuggled inside a URL, to a `0600` file. Building this
makes A24's bet *cheaper to lose*, never dearer.

A24 stays `UNTESTED` and this feature must not be read as evidence about it.
The row that would test it is `R-I10`'s, and the test is use over a real
network rather than anything here.

### Acceptance

- [ ] I can edit an entry's **address** in place, and the change survives a
      restart of the window.
- [x] I can give an entry a **token**, and it is never shown in, or composed
      into, the address the panel displays.
- [ ] I can **reorder** entries, and the order survives a restart.
- [ ] I can add and forget entries, as before, and I cannot forget the one I am
      connected to.
- [x] Two entries may hold the **same URL** by two routes, and editing one does
      not disturb the other.
- [x] The list is at `~/.mogeung/connections.json`, mode `0600`, on **this**
      machine — not on the daemon's.
- [x] A list previously kept in `localStorage` appears in the file the first
      time the window starts, and the `localStorage` key is gone afterwards.
- [ ] In a browser tab, the panel still works against `localStorage` and
      **says** that it is not using the file.

### Explicitly out of scope

- **Opening the tunnel.** `R-I4`'s route is `ssh -L` run by you. A panel that
  ran `ssh` would be a window with a shell verb, which is the line
  [ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md) drew. An
  entry may **record** the command as a note so the panel can show it beside a
  dead connection, and the panel must never execute it.
- Testing whether an address is reachable before you switch to it.
- Browsing the LAN for daemons. It needs multicast, which a webview cannot do.
- Syncing the list between machines.
- Any change to what switching means: the board still drops. What the window
  **opens on** is also untouched, and see the finding under Risks — it is not
  what `R-I7`'s row claims.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

[ADR-0036](../decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md)
is the shape: the list stays the client's, and its file becomes the shell's.

A `Connection` gains a stable `id` (the identity the URL used to be), an
optional `token`, and an optional `note` for the tunnel command. The shell gets
two commands — `connections_load` and `connections_save` — that read and write
`~/.mogeung/connections.json` at `0600`, writing through a temporary file in
the same directory so a crash cannot truncate the list.

`lib/connections.ts` keeps its shape and changes its backing store: `load` and
`save` become async, dispatch on `isTauri()`, and fall back to `localStorage`
in a browser tab. Migration happens inside `load`, once, when the file is
absent and the key is not.

The panel gains an edit row per entry, up/down buttons, and a token field
rendered as a password input that is never echoed into the address line.

### Files touched

| Path | Change |
| ---- | ------ |
| `desktop/src-tauri/src/lib.rs` | `connections_load` / `connections_save`, `0600`, atomic |
| `desktop/src-tauri/capabilities/default.json` | unchanged — these are our own commands |
| `desktop/src/lib/connections.ts` | `id`/`token`/`note`, async load/save, migration, fallback |
| `desktop/src/ui/ConnectionsWindow.tsx` | edit, reorder, token, note, the fallback notice |
| `docs/design/architecture.md` | the connection list's home |

### Risks and unknowns

- **The async load changes the panel's first paint.** It opens with no rows for
  a tick where it used to open with all of them. Rendering an empty list and
  then filling it reads as a lost list; it must render a loading state.
- **Migration runs once and deletes.** If it half-runs — file written, key not
  cleared — the next start migrates again over a list the user has since
  edited. The key is cleared only after the write is acknowledged.
- **`0600` on a file that already exists** is not set by writing to it. The
  mode is applied on create, and re-applied on every write, because a file
  restored from a backup may arrive `0644`.
- **`R-I7`'s "synthetic `LOCAL` row" does not exist in this client, and that is
  a finding rather than a risk.** That row says reopening the active connection
  next launch was *"reverted 2026-07-31"* because a sticky default survived
  leaving the machine and silently disabled ADR-0009. The reversion was in the
  egui client; `wire/client.ts`'s `defaultUrl` reopens
  `localStorage["mogeung.url"]` on every launch, so the TypeScript window
  reintroduced exactly what was reverted. **Deliberately out of scope here** —
  this row is about where a list rests, and changing what the window opens on
  is a behaviour change that deserves its own row and its own verdict. Filed so
  it is not discovered a third time.

### Test strategy

Rust: a round trip through a temporary `HOME`; the mode is `0600` after a write
and after a **re**write over a `0644` file; a malformed file yields an empty
list rather than an error, matching the parser posture the rest of the project
takes.

TypeScript: migration moves a `localStorage` list into the shell exactly once
and clears the key; the browser fallback reads and writes `localStorage`
without touching the shell; editing one of two entries that share a URL leaves
the other alone; the token never appears in the composed address.

## Notes

**Built 2026-09-08.** The shell gained `connections.rs` (its own module — `lib.rs`
was already at 784 lines and `daemon.rs` is the precedent), `Input` gained a
`secret` prop, and `ConnectionsWindow` gained an editable address, a token, a
tunnel-command note, and up/down ordering.

- **The unticked boxes are unticked on purpose.** Four criteria — edit surviving
  a restart, reorder surviving a restart, the forget button being refused on the
  current connection, and the browser-tab notice — are **covered by tests at the
  persistence and helper layer but have not been seen in the running window**.
  This session did not launch the desktop build.
- **And a browser tab cannot settle them**, which is
  [ADR-0036](../decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md)'s
  own warning arriving immediately: a tab exercises the `localStorage` fallback,
  so the file, its mode bits and the migration are exactly the things it cannot
  show. `R-J38` is the standing lesson and it applies here more sharply than
  usual. The verification is: open the desktop window, edit an address, restart,
  and `ls -l ~/.mogeung/connections.json`.
- **The name of an unnamed row is its URL, not `defaultName`.** Preserving that
  cost a line of explanation in `coerce`, and it is preserved because a storage
  migration quietly renaming rows would be a second change wearing the first
  one's clothes.
- **`R-I7`'s row is wrong about the `LOCAL` row** — see Risks. Found while
  writing the acceptance criteria, which is the argument for writing them before
  the code.
