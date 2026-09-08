---
title: The connection list is the client's, and its file is the shell's
status: active
updated: 2026-09-08
decided: 2026-09-08
---

# ADR-0036 — The connection list is the client's, and its file is the shell's

## Context

Two clients wrote two different answers, and the ledger records only one of
them. `R-I7`'s row says the list of daemons is *"saved in
`~/.mogeung/connections.json`, written `0600` because it holds tokens"* — that
was the egui client, retired by [ADR-0020](0020-the-egui-client-is-retired.md).
The TypeScript window that replaced it writes `localStorage`, and
`desktop/src/lib/connections.ts` carries a reasoned argument for doing so:

> Client-side and local to this machine, like the keymap and the layout: which
> daemons *you* watch is not daemon state, and a remote daemon has no business
> holding the list of its peers.

Both halves of that are right, and neither of them is about where the bytes
rest. The roadmap has described a file that has not existed since the egui
client went, so `R-I16` cannot begin without saying which answer stands.

**What forces the decision now is the token.** `R-I16` asks for a connection
manager that carries *"the token `R-I10`'s ladder requires for a non-loopback
bind"*. A token is a shared secret, and secrets have somewhere they may rest
and somewhere they may not.

**The uncomfortable fact that settles it: the secret is already there.** The
daemon accepts a token as `Authorization: Bearer …` **or** `?token=…`
(`crates/mogeungd/src/main.rs`), and the TypeScript window has **no token
handling at all** — no field, no header, nothing. So the only way to reach a
token-gated daemon from the shipped window today is to type the token into the
address:

```
ws://devbox:7717/ws?token=6f1c…
```

which `saveConnections` then writes verbatim into `localStorage`. The choice
is therefore not *whether* to persist a shared secret — that has been
happening since `R-I7` — but whether to keep doing it in the webview's
unencrypted, mode-less key-value store while pretending the question is open.

One more constraint shapes the answer. The list cannot live in a daemon,
because of an ordering problem that has no fix: **you cannot ask a remote
daemon for the list of remotes.** A window pointed at a dev box is not talking
to the local daemon, and the entry it most needs to read is the one that would
get it home.

## Decision

**The list stays the client's. Its home moves from `localStorage` to a file the
Tauri shell owns: `~/.mogeung/connections.json`, mode `0600`, written
atomically.**

Three parts, and the middle one is the point:

1. **It is not daemon state.** `connections.ts`'s argument survives intact —
   no wire family, no daemon code, nothing served. Which daemons you watch is
   client-side taste, exactly like the keymap and the layout.
2. **Client-side is not the same as webview-side.** The client has two halves,
   and the native one can hold a file with the right mode bits. The shell
   already owns local machinery the webview cannot have — it holds the ptys —
   so this is the existing seam, not a new one.
3. **The token becomes its own field and never travels in the URL.** An entry
   is `{id, name, url, token?, note?}`. The dialled URL is composed when
   connecting; what is stored is the address you typed.

A **stable `id`** per entry comes with this, because the URL stops being an
identity the moment it is editable.

When the shell is absent — an ordinary browser tab at `localhost:1420`, which
is a real client and how the UI gets verified — the list falls back to
`localStorage`, read and written as before, and **the panel says so**. A
fallback that silently drops a token would be worse than one that refuses.

Migration is one-way and once: if the file does not exist and `localStorage`
holds a list, it is written to the file and the key is cleared.

## Alternatives

**Keep everything in `localStorage`, token included.** Cheapest, and consistent
with layout, prefs and url, which all live there. It loses on the one axis that
matters here: `localStorage` is unencrypted, carries no mode bits, is readable
by anything executing in that webview, and sits in a profile directory with
directory permissions. `R-I7`'s row already reached the opposite conclusion and
said why — *"written `0600` because it holds tokens"*. Persisting a shared
secret there is a decision nobody has ever actually taken; it is what happened
while no field existed.

**Keep `{name, url}` in `localStorage` and put only the token in an OS
keychain.** Correct about secrets and wrong about cost. It splits one entry
across two stores with different lifetimes, so a forgotten connection leaves a
keychain item behind and a restored profile finds tokens with no entries. It
adds a per-OS dependency (Keychain, libsecret, Credential Manager) to a product
that already owns a private directory on every platform it ships to. Kept as
the **revisit-if** below rather than refused outright.

**Move the list to the daemon, served over the wire.** The most consistent with
*"the daemon is the product"*, and it cannot work. You cannot ask a remote
daemon for the list of remotes, and the local daemon is not necessarily running
or connected when you need the entry that would reach it. It also contradicts
the argument already written in `connections.ts` — a daemon holding the
addresses and tokens of its peers is a lateral-movement store, which is a
strange thing to build into a read-only observer.

**A file the daemon owns rather than the shell.** Same file, wrong writer: it
puts `~/.mogeung/connections.json` on *the daemon's* machine, so connecting to
a dev box would read and write that box's list rather than this one's. The
list is about where **this window** can go.

## Consequences

**Easy.** One home, with the mode bits a secret needs. `R-I7`'s row becomes
true again rather than being quietly wrong. The token leaves the URL, so
`R-I7`'s redaction problem — the dialled URL carrying `?token=` into tooltips
and footers, fixed once in the egui client and never ported — cannot recur in
the same shape, because the address the panel shows no longer contains one.

**Hard.** The window can no longer read its own connection list without asking
the shell, and that ask is asynchronous where `localStorage` was not. The
panel's initial state becomes a load rather than a value.

**The cost worth naming.** A browser tab is a second-class client for this one
panel, and `R-J38`'s standing warning now cuts both ways: the tab keeps
working via the fallback, so a change verified there proves *less* than it did
— the file path, the mode bits and the migration are all invisible from a tab.
Anything about this list has to be checked in the desktop build.

**Ruled out.** Syncing the list between machines; reading it from anything that
is not this client; a daemon that knows its peers.

**Not changed.** Switching still drops the board, settled by `R-I7` and
untouched here. This ADR is about where a list rests, not about what switching
means.

**One thing this ADR deliberately does not fix.** `R-I7`'s row describes a
synthetic `LOCAL` row that every launch starts on, added when reopening the
active connection was *"reverted 2026-07-31"*. **No such row exists in this
client**: `wire/client.ts`'s `defaultUrl` reopens the saved URL on every
launch, so the behaviour that was reverted in the egui client came back with
the TypeScript one. Moving the list does not change that either way, and
changing what the window opens on is a behaviour change with its own verdict to
earn. Named here so the next reader of `R-I7` does not trust the row over the
code.

## Revisit if

A `0600` file proves insufficient for the token — a shared machine, or a
threat model where another process running as the same user is in scope. The
answer then is the OS keychain for the token field alone, which is the
alternative above and is deliberately left costed.

Or: a second client appears that is not this window. The reasoning here leans
on there being exactly one, and [ADR-0020](0020-the-egui-client-is-retired.md)
is what made that true.
