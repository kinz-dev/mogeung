---
title: Local git writes — stage, commit, branch, stash
status: active
updated: 2026-07-31
roadmap: [R-D19, R-D20, R-D21, R-D22, R-D23]
depends_on: [A18, A19, A24, A26]
---

# 0025 — Local git writes

The write half of the Git pane, bounded at the network by
[ADR-0012](../decisions/0012-write-locally-never-publish.md).

Asked for 2026-07-30 — *"Do we support git commit and push in the GIT UI now?
if not, can we plan for this and also plan for other possible missing feature
to support the GIT workflow"* — and scoped, from four offered tiers, to
**local writes only**. `push` was in the question and is deliberately not in
this feature; see [Explicitly out of scope](#explicitly-out-of-scope).

**`R-D19` shipped 2026-07-31**; `R-D20`–`R-D23` are still a plan. See
[Notes](#notes) for what building the first stage changed about the rest.

## Spec

### Problem

The pane renders `git status` with staged and unstaged files distinguished,
per-file diffs, hunks, branch and upstream state, stashes, and a three-way
view of a conflicted file. It can act on none of it.

The concrete moment: you finish reading an agent's twelve changed files in the
pane, decide eight of them are the commit and four are debris, and then switch
to a terminal and retype from memory the list you were just looking at. That
is [ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md)'s "where
review notes go to die", one layer down — except that unlike a prompt, a file
list is not something you should be retyping at all.

Second moment, smaller and more annoying: a conflicted file. `R-D16` shows you
ours, base and theirs side by side and then asks you to go somewhere else to
pick one.

### Assumptions

- **A18** (`SUPPORTED`) — commit history and annotation deserve a pane. The
  write half rests on the same want, no more.
- **A19** (`SUPPORTED`) — the pane earns commercial-grade depth. Note that A19
  was argued *for the reading half specifically*, and its ledger entry names
  this feature as the pressure it expected. It does not support this feature;
  it predicted it.
- **A24** (`UNTESTED`) — a read-only daemon is safe on a trusted network with a
  shared token and no TLS. **This feature changes that sentence's subject.**
  The loopback-or-token guard in ADR-0012 exists so the assumption keeps
  meaning what it says; without the guard this feature voids A24 rather than
  depending on it.
- **A26** (`UNTESTED`) — the user will commit from mogeung rather than from the
  terminal tab open beside it.

> The rule says: if a spec depends on an `UNTESTED` assumption, the work is to
> test the assumption, not to build the feature.

A26 is the one that matters, and it is honest to say the rule is being bent
rather than satisfied. The cheap test — compose the command and let the shell
run it — was offered and declined in favour of the built version, the
A16/A19 shape of a deliberate choice taken over the staged default. So the
build *is* the test, and the failure condition is written down in advance:
if the shell tab keeps winning through a dogfooding week, the verbs come out
and the composed-command path replaces them. An unused write verb is not
decoration, it is liability.

### Acceptance

- [x] Files in Local changes can be staged, unstaged and discarded from the
      pane, individually and in multi-selection, and the list reflects the new
      state without a manual refresh
- [x] Discarding asks first, names every file it will destroy, and says plainly
      that git cannot bring them back
- [ ] A commit can be written and made from the pane — message body, amend of
      the tip commit, and an optional trailer naming the session whose diff it
      came from
- [ ] A commit made from the pane appears in the log below it, and its diff
      arrives already marked read where the hunks were read before committing
- [ ] Branches can be created and switched from the refs list; a switch that
      git refuses reports git's own words, and the pane's state does not move
- [ ] A stash can be pushed, popped and dropped from the stash list
- [ ] A conflicted file can be resolved from the three-way view — take ours,
      take theirs, or mark resolved after editing elsewhere
- [ ] Ahead/behind is never rendered as a bare number: it carries the age of
      the last fetch, or reads as unknown when nothing has fetched
- [x] A write verb arriving on a non-loopback bind without a token is refused,
      and the refusal is tested. **Not a 401**: the write verbs are
      WebSocket-only, so the refusal is a `ServerMsg::Error` on the same
      stream. The 401 is the HTTP token layer's answer and still applies to
      the socket's upgrade request
- [x] Every write failure surfaces git's stderr verbatim rather than a
      paraphrase
- [x] `cargo test --workspace` covers each verb against a temporary repository,
      including one test proving `discard` cannot escape the session root

### Explicitly out of scope

- **`fetch`, `pull`, `push`, and every other remote verb** — ADR-0012 draws the
  line at the network. This is the half of the original question that is not
  being answered; it needs its own ADR and it needs A24 resolved rather than
  assumed. Tracked as `R-D24`, deliberately unstarted.
- **`rebase`, `merge`, `cherry-pick`, `reset --hard`, history rewriting beyond
  amend.** Local, allowed by ADR-0012's principle, and still not built — these
  are the verbs where a wrong click costs an afternoon, and none of them has
  been asked for. Wait for want.
- **Interactive hunk staging** (`git add -p`). The pane has hunks and read
  marks, so it is the natural home for it, and it is the single largest
  addition in the list. File-level first; hunk-level when file-level proves
  used.
- **Any verb that touches a session, prompt or agent.** ADR-0003, permanently.
- **Forge integration** — PRs, reviews, CI status. Not git.

## Plan

*Drafted by an agent, approved by the human before implementation.*

### Approach

Four stages, each shippable and each a roadmap row. The first carries the cost
of the other three.

**`R-D19` — the write path, and the guard.** `git.rs` gains its first write
functions and, more importantly, a different posture for them: a `run_git_write`
sibling to `run_git` that returns git's stderr on failure instead of degrading.
Verbs: `GitStage`, `GitUnstage`, `GitDiscard`. `api.rs` gains a check that
refuses any write verb unless the bind is loopback or a token was presented —
one guard in one place, not a per-verb decision. Every write answers by
re-broadcasting `GitLocalChanges`, so the client never models repo state
locally and the two cannot drift.

**`R-D20` — commit.** `GitCommit { message, amend, trailers }`. The UI is a
message box above Local changes, IntelliJ's layout, which is already the
reference for `R-D18`. The session trailer is the distinctive part and the
reason this is worth building rather than shelling out: mogeung knows which
session produced which hunks, so a commit can record it, and `R-F2` prompt-blame
later reads it back. Read marks survive by construction — hunk anchors are
content hashes, so a committed hunk hashes the same as the uncommitted one did.

**`R-D21` — branches and stashes.** `GitBranchCreate`, `GitSwitch`,
`GitStashPush`, `GitStashPop`, `GitStashDrop`. Mechanically the smallest stage;
the only interesting part is that a switch changes what every other pane is
looking at, so it must invalidate the session's cached diff base rather than
letting the Changes tab quietly compare against a branch that is no longer
checked out.

**`R-D22` — conflict resolution**, on top of `R-D16`'s existing three-way read.
`GitResolve { rel, side }` for ours/theirs, plus a mark-resolved that is just
`git add`. Small because the reading half is done.

**`R-D23` — honest staleness**, and the only row here that is not a write.
`GitRefs` already carries the last-fetch time from `.git/FETCH_HEAD`; the client
shows ahead/behind as bare numbers anyway. Since ADR-0012 keeps `fetch` out,
those numbers are permanently capable of lying, and a number that lies silently
is the failure this project files under "crying wolf" in reverse. Render the
age beside them, or render unknown.

### Files touched

- `crates/mogeungd/src/git.rs` — `run_git_write`, the verb functions, temp-repo
  test fixtures
- `crates/mogeungd/src/api.rs` — the write guard, verb dispatch, re-broadcast
- `crates/mogeungd/src/server.rs` — expose whether the bind was loopback, so
  the guard can ask
- `crates/mogeung-core/src/wire.rs` — the write `ClientMsg` family
- `crates/mogeung-ui/src/gitview.rs` — checkboxes, message box, confirmations,
  error surfacing
- `crates/mogeungd/src/state.rs` — invalidate the pinned diff base on switch
- `docs/design/wire-protocol.md`, `docs/design/architecture.md`,
  `docs/product/concept.md` — all three currently assert read-only

### Risks and unknowns

- **A26 is the whole bet.** The shell tab is one keystroke away and already
  knows git. If it keeps winning, this is a large build that made the product
  worse by widening it. The mitigation is the pre-agreed removal condition
  above, not a hope.
- **`discard` destroys work git never saw.** It is the only verb here with no
  undo. It needs the confirmation, the containment test, and — worth
  considering — a refusal on anything outside the session root even when git
  would happily do it.
- **The stale-remote trap gets worse, not better.** Committing from the pane
  makes it likelier you never visit a terminal, which is exactly where fetching
  happens. `R-D23` is not optional decoration; it is the counterweight.
- **Branch switching under a running agent.** A live session's agent has files
  open and a diff base pinned. Switching branches beneath it is legal git and a
  terrible idea. Unknown whether to refuse it, warn, or allow it silently — this
  is the question most likely to need the user's judgement rather than ours.
- **The guard is a single point of failure.** One missed verb and an
  unauthenticated socket can write to a repo. It belongs in dispatch where every
  verb passes, with a test that enumerates the write family and asserts each is
  refused.

### Test strategy

Temp-repo fixtures (`tempfile`, `git init`, a scripted commit or two) per verb —
a new pattern for this codebase, which has tested git against read-only
fixtures until now. Beyond the happy paths: staging a path containing a leading
dash; discarding a path that tries to escape the root; committing with an empty
message; amending with no commits; switching to a branch with uncommitted
changes in the way; popping a stash that conflicts. Each should produce git's
own refusal, not ours.

Guard tests in `tests/auth.rs` alongside the existing `R-I4` ones: every write
verb, non-loopback, no token, expect 401.

## Notes

### What `R-D19` cost that the plan did not predict (2026-07-31)

The three verbs were the easy part. Two defects in the **read** path had to be
fixed first, both invisible for as long as these strings were only displayed
and both fatal once they became pathspecs handed back to git:

- `git status --porcelain` C-quotes any path it finds unusual — a space is
  enough for surrounding quotes, a non-ASCII byte becomes an octal escape, so
  `café.txt` arrived as `caf\303\251.txt`. `status` now uses `-z`, which
  quotes nothing. That also changes how renames arrive: under `-z` the source
  path is a second record rather than an ` -> ` arrow, so the parser consumes
  it.
- `status` **collapses an untracked directory to a single row**, so a file
  inside a folder an agent has just created never appears in it. `discard`
  partitioned on that listing and classified such a file as tracked. It now
  asks `ls-files`, which answers the question actually being put.

Neither was reachable from the read path's own tests, and both were found by
writing the temp-repo fixtures. That is the argument for the fixtures being
part of this stage's cost rather than a later tidy-up.

### Two things the plan got wrong

- **"Refused with a 401."** The write verbs travel on the WebSocket, and a
  message on an established socket has no status code. The guard answers with
  `ServerMsg::Error` instead. The 401 exists and is the token layer's answer to
  the *upgrade request*, which is a different and earlier check.
- **The guard is less load-bearing than the plan feared**, because `admit`
  (`R-I10`) already refuses to *start* a daemon that is non-loopback with no
  token. It was still built, for a reason worth writing down: `admit` guards
  the binary and this guards the router, and the router is what a test, an
  embedding, or a future entry point constructs.

### Still open, and now more concrete

`R-D21`'s branch-switch question — what to do when a live session has files
open and a diff base pinned — was flagged in the plan as the one most likely to
need a human's judgement. Nothing in `R-D19` answered it, and it should be
asked before that stage starts rather than during it.
