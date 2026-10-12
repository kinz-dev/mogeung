---
title: A seat is configured at launch, never in the user's agent home
status: active
updated: 2026-10-09
decided: 2026-10-09
---

# ADR-0043 — A seat is configured at launch, never in the user's agent home

## Context

[ADR-0003](0003-observe-do-not-spawn.md)'s amendment of this date lets mogeung
start, send to and stop a **seat** — an agent CLI it launched itself, in Gau
Geung mode. A seat needs things an observed session never did: a hook that
tells the daemon when it stops, a status line that reports its context fill,
guidance for the task it was started for, sometimes an MCP server, sometimes a
permission mode. OpenRig, whose orchestration this epic ports, gets all of
those by **writing into the agent's own home and workspace**: hooks and a
status line into `.claude/settings.local.json`, trust and onboarding into
`.claude.json`, guidance into `CLAUDE.md`, hook configuration and trust hashes
into Codex's `config.toml`.

mogeung has had one sentence against exactly that since the start — *"Never
write to `~/.claude`. Read only."* — and when the sentence was put back on the
table it turned out to have a reason that is not about mogeung at all. In the
owner's words, 2026-10-09:

> I don't want mogeung to make any changes under `~/.claude` that could
> surprise a user of a normal session. What if they are not using mogeung's
> feature (the OpenRig one or normal session wrapper), I don't want it affect
> the normal usage of a Claude session.

So the rule is not squeamishness about a directory. It is a promise to the
user's **other** sessions: the `claude` they type in a terminal with mogeung
nowhere near it must behave exactly as it would on a machine that has never
heard of mogeung. A hook that fires in that session because a seat needed it
is the surprise the sentence forbids.

The question that forced this ADR was the owner's own proposal: *"what if we
have a special folder, so when an agent is created in Gau Geung mode it reads
the `~/.claude-geung` files?"* Claude Code supports precisely that
(`CLAUDE_CONFIG_DIR`), and so, with different gaps, do the other three. But it
also supports something narrower, and OpenRig's own history says which to
reach for first: it set `CLAUDE_CONFIG_DIR` on every managed seat once, and
its changelog records the result as *a blank config* — no login, none of the
user's skills, plugins or memory — and the fix was to stop.

What each CLI accepts, checked on 2026-10-09 against the installed binaries
and the published docs:

| CLI | A separate home | Configuration at launch, nothing written | Hooks at launch |
|---|---|---|---|
| Claude Code 2.1.295 | `CLAUDE_CONFIG_DIR` moves every path under `~/.claude`, credentials included; the macOS keychain entry is keyed per directory | `--settings <file-or-json>` overrides any user-settings key for that session, second only to managed settings; `--session-id`, `--name`, `--append-system-prompt-file`, `--mcp-config` with `--strict-mcp-config`, `--permission-mode`, `--add-dir` | yes, as keys in `--settings` — which is how OpenRig passes its `PreToolUse` hook in production |
| Codex 0.154.0 | `CODEX_HOME` | `-c key=value`, `--add-dir`, `-s`, `-a`, `-p` | no: hooks live in `config.toml` with trust hashes |
| Qwen Code 0.23.0 | `QWEN_HOME` moves settings, credentials, memory and skills; `QWEN_RUNTIME_DIR` moves only conversations, logs and todos | `--session-id`, `--approval-mode`, `--allowed-tools`, `--append-system-prompt`, `--include-directories` | no: its 22 hook events read settings files only |
| opencode 1.18.32 | follows XDG, so `XDG_CONFIG_HOME` and `XDG_DATA_HOME` move its config and its database | `OPENCODE_CONFIG_CONTENT` is inline JSON layered above the project config; `OPENCODE_CONFIG` names an extra file; `--agent`, `--model`, `--port` for the server every TUI runs | a plugin file in a directory named by `OPENCODE_CONFIG_DIR`, which **adds** to the user's config rather than replacing it |

## Decision

**mogeung writes nothing under the user's agent homes. A seat is configured by
what its CLI accepts at launch — flags, environment variables, and files under
`~/.mogeung` — and a home mogeung owns is opt-in.**

1. **The user's homes are read-only, all of them.** `~/.claude`,
   `~/.claude.json`, `~/.codex`, `~/.qwen`, `~/.config/opencode` and
   `~/.local/share/opencode`. This is the old sentence, widened to the four
   CLIs it always meant, and it now has its reason attached: a session the
   user starts without mogeung must be unaffected by mogeung's existence.

2. **A seat is configured at launch.** The daemon generates what the seat
   needs under `~/.mogeung` — a settings file, a guidance file, an MCP config,
   an opencode plugin — and hands it to the CLI the way that CLI accepts it:
   `--settings` and `--append-system-prompt-file` for Claude Code, `-c` for
   Codex, Qwen's flags, `OPENCODE_CONFIG_CONTENT` and `OPENCODE_CONFIG_DIR` for
   opencode. The configuration lives for that session and is gone with it.
   Where a CLI has no launch-time way to carry a thing, the seat goes without
   it; mogeung does not reach for the home to make up the difference.

3. **Hooks and a status line travel the same way, and only where they can.**
   A Claude Code seat gets both through `--settings`. An opencode seat gets a
   plugin from a directory mogeung owns, which sees the session's own events.
   A Codex or Qwen seat gets **no hooks** unless clause 5 is taken, and its
   state comes from the files the daemon already reads for every session.
   The hook's receiver is the daemon, on loopback, with a token minted per
   seat; what arrives is data from the agent's process and is treated as
   such.

4. **Guidance is never a file in the worktree.** [ADR-0019](0019-a-viewer-not-an-editor.md)
   stands: no `CLAUDE.md`, `AGENTS.md` or `.claude/settings.local.json` is
   written into a checkout, which is where OpenRig puts them. A seat's
   guidance is a system-prompt append or its CLI's equivalent, or nothing.

5. **A separate home is opt-in, per CLI, per team.** `CLAUDE_CONFIG_DIR`,
   `CODEX_HOME`, `QWEN_HOME` or the XDG pair, pointed under `~/.mogeung`. That
   home is **mogeung's**, so there trust, hooks, a status line, MCP servers and
   guidance may all be written — the rule above is about the user's homes,
   and this is not one. The price is a blank seat: a login of its own, none of
   the user's skills, plugins, memory or user-scope MCP servers, onboarding
   again. The window says so before the opt-in is taken, and an *import from
   your home* is an explicit copy labelled as one that will drift. For Codex
   and Qwen this is the only way to get hooks, which is why it is per CLI.
   The daemon then scans a second home for that CLI, tagged as the seat home.

6. **Trust stays the user's act.** mogeung never pre-accepts a trust dialog in
   the user's home. Claude Code keys trust on the main checkout's root, and a
   worktree falls under it, so a seat in a worktree of a repository the user
   has already opened sees no dialog; a repository they never opened asks
   once, on the seat's own screen, as a row that needs them.

7. **A seat's transcript lands where its CLI puts it.** For a flags-configured
   seat that is the user's home, written by the CLI as it writes every
   session, which is the one kind of write under `~/.claude` this ADR has no
   opinion about: it is not mogeung's. The daemon reads it there, as it does
   today, and identifies the seat by the session id it chose at launch where
   the CLI takes one (`--session-id` for Claude Code and Qwen) and by its
   launch record otherwise.

8. **What mogeung keeps is its own.** Team and seat definitions, the launch
   record that says which sessions are seats, the generated per-seat files,
   and the hook receiver's tokens all live under `~/.mogeung`, daemon-owned,
   per [ADR-0015](0015-markdown-is-the-truth.md)'s split: a definition is the
   user's writing and is mirrored as markdown; a launch record is derived
   state and is not. An OpenRig `rig.yaml` is read as *other people's
   configuration* ([ADR-0026](0026-other-peoples-run-configurations.md)) and
   imported, never adopted as the format.

## Alternatives

**Write into the homes, as OpenRig does.** The straightforward build, and the
one the port would have copied line for line. Rejected for the reason quoted
in Context, which is a promise to sessions mogeung is not part of. OpenRig has
itself retreated from the widest form of this — its changelog records removing
every permission write to `~/.claude/settings.json` and pinning that with a
guard test — which is some evidence that users mind.

**A separate home, always.** The owner's first proposal, and the physically
cleanest: nothing mogeung does can reach the user's home because the seat
never looks there. Rejected as the *default* on OpenRig's recorded experience
of blank seats, and because it makes every seat a second login and a second
corpus for the daemon to read. Kept as clause 5, because for two of the four
CLIs it is the only way to a hook, and because a user who wants seats on a
second account or blind to their skills is asking for exactly this.

**A project-local settings file in the worktree.** OpenRig's actual mechanism
for the status line and hooks on Claude Code, and it does relocate nothing
when `CLAUDE_CONFIG_DIR` is set. Rejected twice over: it is a worktree write,
which ADR-0019 refuses, and a `claude` the user starts by hand in that
worktree would pick it up, which is the surprise this ADR exists to prevent.

**A wrapper script named `claude` on the seat's `PATH`.** Would make any
configuration possible with no file anywhere. Rejected without discussion:
that is wrapping, and wrapping is what ADR-0003 records v0.1 dying of.

**Symlink the user's credentials into the seat home.** Would take the sting
out of clause 5. Rejected: Claude Code rewrites its credentials file on token
refresh, and a rewrite through a symlink either lands in the user's home —
a write this ADR forbids, by proxy — or replaces the link with a file and
silently forks the two logins. A copy that says it is a copy is honest; a
link that pretends to be one file is not.

## Consequences

**Easy.** A session the user starts without mogeung is unaffected by
construction, not by care: there is no code path that writes to a user home,
and a test can assert that over the whole daemon. A seat is reproducible from
its definition, because everything it was given is generated from that
definition at launch. Nothing of mogeung's is ever left behind in a user's
files to clean up, migrate, or apologise for.

**Hard.** Codex and Qwen seats are blinder than Claude Code and opencode
seats unless opted into a home — no stop hook, no status line — and the
panel has to show the difference rather than smooth it over. A Claude Code
seat shows mogeung's status line instead of the user's, for that seat only,
which is a visible difference between a seat and a hand-started session and
is the right one to be visible. A seat's transcript appears in `claude
--resume` inside the seat's worktree, which is the CLI's behaviour and is
left alone.

**Unverified, and said so.** Two facts this ADR rests on are documented
loosely. `--settings` *"can set any key your user settings file can set"*,
and OpenRig passes a hook that way, but the hooks page does not list the flag
among hook locations — [A51](../product/assumptions.md). That a flags-only
launch leaves `~/.claude.json` as a plain launch would is nowhere stated —
[A52](../product/assumptions.md). And the worktree-trust rule in clause 6 is
read from the docs rather than seen — [A53](../product/assumptions.md). The
Gau Geung spec's phase 0 is to test all three before a seat is built; if A51
or A52 fails, clause 5 becomes the default for Claude Code seats and this ADR
says so by amendment rather than by a quiet change of default.

**A new daemon surface.** The hook receiver is the first thing in mogeung
that an agent's own process talks to. It is loopback, per-seat tokened, and
its payloads are data; but it exists, and `A24`'s *read-only* word, already
qualified by [ADR-0025](0025-run-a-process-you-named-never-an-agent.md), is
qualified again.

## Revisit if

- **A51 or A52 is refuted** — then flags are not enough for Claude Code and
  the opt-in home becomes its default, by amendment here.
- **Codex or Qwen gains a launch-time hook flag** — clause 3's exception for
  them should shrink the day it does.
- **The user asks for a write into their home anyway** — a status line they
  want in every session, say. That is a different decision with a different
  reason, and it gets its own ADR rather than a widening of this one; the
  shape `R-B73` already names (mogeung prints the snippet, the user installs
  it, mogeung listens) is the one to start from.
- **A seat is ever found to have changed a hand-started session's behaviour.**
  That is this ADR failing at the one thing it is for, and the response is to
  find the write path and close it, not to document the surprise.

## Amendment — 2026-10-09, later

**Clause 4 is narrowed to worktrees mogeung did not create.**
[ADR-0019's amendment of this date](0019-a-viewer-not-an-editor.md) lets the
daemon write into a **team's** worktree — one it made with `git worktree add`
for a team of seats — so guidance *may* be a file there, in a marked block,
listed on the team's panel, never committed by mogeung. Clause 4's reason
stands exactly where the reason applies: a checkout the user works in by hand
is never written, because a `claude` started there by hand would read the
file, which is the surprise clause 1 forbids. A team worktree is mogeung's
room; a hand-started session in it is the user walking into that room, and
the panel lists what is on the floor. Flags remain the first mechanism
(clause 2). A file is for what a flag cannot carry, or for guidance the user
wants to read in the tree beside the code it is about.

---
*ADRs are immutable. A decision that is **narrowed** changes by an
`## Amendment — YYYY-MM-DD` section appended here, with `updated:` bumped. A
decision genuinely **reversed** is superseded: write a new ADR and set
`status: superseded` plus `superseded_by:` here.*
