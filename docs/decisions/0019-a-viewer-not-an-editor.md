---
title: mogeung reads code and never writes it
status: active
updated: 2026-10-09
decided: 2026-08-04
---

# ADR-0019 — mogeung reads code and never writes it

This closes `R-L4`, which was filed as an open question on 2026-08-02 and
explicitly left undecided: *"let's keep it for next phase, nothing decided
yet."*

## Context

[Pillar K](../product/roadmap.md#k-explicitly-not) has said "an editor —
explicitly not" since the roadmap was numbered, and `R-B24` shipped a viewer on
that basis. But the position had never been *argued*; it was inherited. `R-L4`
recorded the doubt honestly rather than assuming the answer.

On 2026-08-04 the question was reopened deliberately, and for a while the answer
was going to be yes: a full editor, CodeMirror or Monaco, as part of the client
rewrite. What changed it was working out what editing would actually cost, and
then a re-read of how the tool is really used — the agent does the editing, and
what the human does is *read*.

The costs are not in the UI. They are:

- **A write verb in the protocol.** There is none today, and its absence is
  structural: `architecture.md` says *"the roadmap's 'an editor — explicitly
  not' is a property of the protocol, not just the UI."*
- **An ADR superseding pillar K**, and a re-scoping of
  [ADR-0012](0012-write-locally-never-publish.md)'s guard to cover arbitrary
  file writes rather than only git verbs.
- **A24 voided or rewritten.** Its evidence line says *"the word 'read-only' in
  this row is load-bearing"* — it is the assumption that a tokenless LAN daemon
  is safe. A daemon that writes arbitrary worktree files is a different
  proposition entirely.
- **The concurrent writer**, which is the one with no good answer. The agent is
  editing these files continuously. Two writers on one file with no coordination
  leaves a choice between refusing to save, silently reloading under the user,
  and merging. `R-D21` met the same class of question about switching branches
  under a running agent and answered it by *warning* rather than guessing —
  which works for an occasional event and not for a permanent condition.

## Decision

**mogeung reads code. It never writes it.** The Code pane is a viewer,
permanently, and the daemon gains no verb that writes a worktree file.

Concretely:

- Monaco runs with `readOnly: true`, and its refusal message says why rather
  than merely declining the keystroke. A read-only editor is chosen because it
  is a strictly better *viewer* than a hand-rolled one — find, folding,
  go-to-line, minimap, breadcrumbs, bracket matching — **not** because editing
  is one flag away.
- The escape hatch is the one pillar K always named and that is already in use:
  hand off to the editor open beside mogeung.
- **Notes are not an exception to this**, they are a different thing. A document
  under `~/.mogeung` is the user's own writing and nothing else can regenerate
  it ([ADR-0015](0015-markdown-is-the-truth.md)); a worktree file belongs to the
  repository. If that distinction is not held, it erodes a feature at a time.

## Alternatives

- **A full editor.** Genuinely considered, and briefly chosen. Lost on the
  concurrent writer: it is structural rather than occasional, and every
  resolution is bad. The rest of the cost — verb, ADR, A24 — is merely large.
- **Editing behind a flag**, off by default. Worse than either answer: it keeps
  the whole cost (verb, guard, A24 rewrite, concurrency) and adds a setting that
  makes the product's own rule conditional.
- **Editing only files no live session has touched.** Sounds targeted, and the
  attribution it depends on is A8, which is `AT RISK` and *cannot separate two
  sessions editing one file*. Building a safety rule on a known-shaky heuristic
  is worse than not building it.
- **Keeping `R-L4` open.** The status quo, and rejected because the pressure is
  real and one-directional: the better the viewer gets, the more reasonable
  "just let me fix this one line" will sound. An open question is not a defence.

## Consequences

- **The TypeScript port needs no daemon work at all.** Every pane is fed by an
  endpoint that already exists. This is the largest consequence and it is a
  good one: [ADR-0018](0018-a-second-client-in-typescript.md) becomes strictly a
  client rewrite against a frozen protocol.
- A24 keeps its evidence intact. Pillar K stands unamended.
- The concurrent-writer problem never has to be answered.
- **Fixing a typo means leaving mogeung.** That is the cost, it is paid every
  time, and it will be irritating. The mitigation is that the editor is already
  open — but this is a real regression against the version of the product that
  edits, and it is accepted knowingly.
- Everything needed to *become* an editor later is still in place: `readOnly` is
  a flag, and the work would be the daemon half, which is unchanged by waiting.

## Revisit if

- You find yourself routing trivial edits through the agent because the editor
  is too far away. That is the signal, and it is behavioural — not an opinion
  about whether editing would be nice.
- The concurrent-writer problem gets a real answer: file locks the daemon can
  hold, or a session that can be asked to pause. Then the objection this
  decision rests on has gone, and the rest is merely work.

## Amendment — 2026-09-03: a scratch file is the second thing that is not an exception

The window's editor now writes one kind of file: a **scratch file** under
`~/.mogeung/scratch` (`R-L5`,
[ADR-0035](0035-the-editor-writes-scratch-files-and-nothing-else.md)). This
narrows nothing above and is recorded here so that a reader of *"mogeung reads
code and never writes it"* is not surprised by a writable Monaco.

The clause it sits beside is the third bullet of the Decision — *notes are not
an exception to this, they are a different thing* — and the argument is the
same one: a scratch file is under `~/.mogeung`, is the user's own writing, and
belongs to no repository. The fences ADR-0035 adds are what keep it that way:
the daemon mints the name, refuses anything that is not a bare file name in
that directory, and does the write. `readOnly: true` and its message stand for
every worktree file, and the daemon still has no verb that writes one.

## Amendment — 2026-10-09: a team's worktree is written, in Gau Geung mode

**The decision is narrowed to every worktree that is not a team's.** In Gau
Geung mode —
[ADR-0003's amendment of this date](0003-observe-do-not-spawn.md#amendment--2026-10-09-mogeung-may-own-a-seat-it-started)
— the daemon creates worktrees for a team of seats, and in those worktrees,
and only those, mogeung writes: files it generates for the team, and files a
human edits in the Code pane. Asked 2026-10-09, in the walkthrough of what
still blocked the OpenRig port: *"No, I don't think we should keep it as 'a
viewer not an editor' anymore, at least for the gau-geung mode."*

**Why the objection this ADR rests on no longer holds there.** The decisive
cost above was the concurrent writer, *"structural rather than occasional"*,
and the Revisit condition named its cure: *"a session that can be asked to
pause."* A seat is a session the daemon started and owns, in a worktree the
daemon made for it. For the first time the daemon knows the writers: which
seats live in the worktree, and whether each is busy or idle from the CLI's
own registry — the fact v0.1 could not see and this product was rebuilt to
read. The problem is not solved in general, and *"editing only files no live
session has touched"* stays rejected for the reason given, `A8`. It is solved
for a worktree whose only writers are seats, which is what a team worktree
is.

### The fences

1. **Only a team's worktree.** One the daemon created with `git worktree add`
   for a team, in Gau Geung mode. The user's own checkout is never written,
   and a team set up to work in one — which the spec may allow — gets no file
   writes there: guidance goes by system-prompt append only
   ([ADR-0043](0043-a-seat-is-configured-at-launch-never-in-your-home.md),
   clause 2). The Code pane stays `readOnly: true`, message and all, for every
   file outside a team worktree, and everywhere outside Gau Geung mode.
2. **Two kinds of write, both the daemon's.** *Generated files* — guidance
   (`CLAUDE.md`, `AGENTS.md`, `QWEN.md` or their local variants), the team's
   own documents (its spec, scopes, plan), and workspace-level settings a CLI
   reads only from the workspace — written inside marked blocks, listed on the
   team's panel as files mogeung wrote, never committed by mogeung, and removed
   with the team where the block is the whole file. *A human's edit in the
   Code pane* — the first write verb in the protocol, carrying the team, the
   path, the content, and the hash of the content it was opened from.
3. **The save refuses rather than guesses.** It refuses when the file on disk
   no longer matches the hash it was opened from — an agent wrote under you —
   and shows the difference. It refuses while any seat in that worktree is
   busy, unless you say otherwise on that save. `R-D21` warned; this refuses.
   Reloading under the user and merging, the two bad resolutions named above,
   are not built.
4. **Judged like the seat verbs.** Per connection, as
   [ADR-0025's amendment of this date](0025-run-a-process-you-named-never-an-agent.md):
   a loopback client may write, any other needs `--allow-run` and a token, and
   a relayed write is the owning daemon's to refuse. `A24` is rewritten again
   by this: a daemon with Gau Geung mode on is not read-only, and the ledger
   must say so.
5. **Scratch and notes are unchanged**, and the third bullet's distinction
   holds with one more case: a note is yours and belongs to no repository; a
   generated file belongs to the team and goes with it; a worktree edit
   belongs to the repository and is made by you.

### What it costs

[Pillar K](../product/roadmap.md#k-explicitly-not)'s *"an editor — explicitly
not"* is narrowed to the same scope, and says so. A write verb exists, and
every cost the Context counted is paid: the protocol has one,
[ADR-0012](0012-write-locally-never-publish.md)'s guard has to cover it, `A24`
moves. The product now has two Code panes that look the same and behave
differently — the *two panes look like terminals* problem of
[ADR-0011](0011-own-a-shell-never-an-agent.md), one pane over — and the panel
has to say which it is showing.

**Revisit if** a team worktree is ever found with a writer that is not a seat
— a hand-started agent, an editor, a script — in which case fence 3's busy
check was reading the wrong thing and the write should have refused. Or if
fence 3's refusal is overridden routinely: then it is decoration, and the
honest response is to take the override away rather than to soften the check.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
