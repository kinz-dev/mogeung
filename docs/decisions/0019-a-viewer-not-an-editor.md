---
title: mogeung reads code and never writes it
status: active
updated: 2026-08-04
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

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
