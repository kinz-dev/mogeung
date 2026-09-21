---
title: More than one repository — the Git window picks
status: active
updated: 2026-09-16
roadmap: [R-D34, R-D35]
depends_on: [A19, A26]
---

# 0044 — More than one repository

Asked 2026-09-16:

> Currently it only detects the claude session folder as a git repo. But from
> time to time I am opening a claude session from a folder "immix-trading-v3"
> […] Hence the mogeung git will only be able to show me the git repo details
> for the immix-trading-v3 project. However […] this gradle will include other
> sibling project. What I really want is a way to add the git repo by: auto
> detect if that is a sibling project defined in gradle; if another folder is
> being added from the "Files" panel, and if that is a git repo folder.

The machine it was asked from:

```
/home/kinz/projects/V3_WORKSPACE
├── immix-agents        ← a git repo
├── immix-gitops        ← a git repo
├── immix-sequencer     ← a git repo, and in the gradle build
└── immix-trading-v3    ← a git repo, and where the session starts
```

## Spec

### Problem

**A session has exactly one repository, everywhere, by construction.**
`session_root` answers `repo_root` else `cwd`, `git_root` refuses a session
that is not in a repo, and all 41 `Git*` messages resolve through that one
answer. So a session started in `immix-trading-v3` can be shown that repository
and no other — while the work is spread over four checkouts that the build
itself composes, and the sibling is one `cd` away in the terminal beside it.

The Files tool already crosses this line: `R-J40` added folders to a session's
workspace and the daemon serves them. What it produced was a **file tree** over
the sibling, not a git surface — the Git tool window never learnt that the
workspace has more than one root in it.

### Assumptions

- `A19` — *the git pane earns commercial-grade reading depth* — `SUPPORTED`.
  IntelliJ's own answer to this is a repository list in the branches popup, so
  this is parity rather than invention.
- `A26` — *the user will commit from mogeung rather than from the terminal* —
  `UNTESTED`, and this makes it **more** answerable rather than less: the
  commonest reason to reach for the terminal in this layout is that the repo
  you want is not the one on screen. No new write verb is added; the existing
  ones learn to name a repository.

### Acceptance

- [x] The Git tool window's header names the repository it is showing, and
      opens a list of the ones this session may show
- [x] A folder added in Files that is a git repository appears in that list
- [x] Picking one points the Log, Local changes, Stash, Console and More tabs
      at it, and the branches popup with them
- [x] Every write verb — stage, commit, discard, branch, switch, stash —
      acts on the **picked** repository, and the `R-D21` live-session warning
      names sessions live in *that* worktree
- [x] A repository the session was never authorised to see is refused by the
      daemon, in words, whatever the client asks for
- [x] The session's own repository is what the Changes pane, the diff base, the
      review marks, collisions and attribution keep using — the picker does not
      move them
- [x] Switching back is one click, and the session's own repo is first in the
      list and marked as its own

### Explicitly out of scope

- **Auto-detecting siblings** — `R-D35`, phase A, deliberately after this. The
  folders already reach the picker through Files; detection only removes the
  hand-adding.
- **Two repositories side by side.** One is picked at a time. IntelliJ shows
  every repository in one popup because its tool window is a tree over all of
  them; mogeung's is a log, a diff and a status list, and three of those have
  no sensible union.
- **Merging, pushing, or anything a second repo makes tempting.** ADR-0012 and
  ADR-0014 are unchanged, and a picker does not widen them.

## Plan

### Approach

**The repository travels on the message, and the fence is the one that already
exists.** Every git command gains an optional `repo`; absent means what it has
always meant — the session's own root. Present, it must resolve inside
`session_roots(id)`, which is *the session's root plus the folders added to its
workspace* — the same whitelist `R-J40` put in front of `list_dir` and
`read_file`, and the same one `resolve_in_workspace` enforces. **Nothing new is
authorised by this feature**: a repository is pickable exactly when its folder
was already readable.

**The client keeps one active repo per session**, in the git slice, and
`lib/gitActions.ts` — the one door every Git-window request already goes
through — attaches it. That is why the client half is small: 17 of the 22 real
send sites are inside that module and need no change at all.

**The four answers the window keys on echo the repo back**, and the client
drops one that is not for the repo it is showing. Not belt-and-braces: a write
verb re-broadcasts `git_local_changes` to *every* window watching that session,
so two windows on two repositories of one session would otherwise feed each
other the wrong status.

### Files touched

| Path | Change |
|---|---|
| `crates/mogeung-core/src/wire.rs` | `repo: Option<String>` on the git commands; `repo` echoed on `GitCommits`, `GitLocalChanges`, `GitRefsInfo`, `GitStashList`; `Workspace` gains `repos` |
| `crates/mogeungd/src/state.rs` | `git_root(id, repo)` + the fence; the flag threaded through the git methods; `workspace()` fills `repos` |
| `crates/mogeungd/src/workspace.rs` | `repos()` — which of a session's roots are repositories |
| `crates/mogeungd/src/api.rs` | destructure and pass; echo the repo into the four answers |
| `desktop/src/wire/types.ts` | mirror |
| `desktop/src/store/index.ts` | `git[id].repo`; answers for another repo dropped; `workspace.repos` |
| `desktop/src/lib/gitActions.ts` | `repoOf(id)` attached to every send; `pickRepo(id, root)` |
| `desktop/src/panes/GitPane.tsx` | the header names the repo and opens the list |
| `desktop/src/ui/BranchesPopup.tsx` | the repository line becomes the list |

### Test strategy

- Rust: the fence — a repo inside the workspace resolves, one outside is
  refused by name; a message with no `repo` still means the session's own.
- Client: `gitActions` attaches the active repo to every verb; a switch clears
  and re-asks; an answer for another repo is dropped; the header lists the
  session's repo first and marked.

### Risks

- **The line between *this session's work* and *this window's repository*.**
  The diff base, Changes, review marks, collisions and attribution are the
  session's own repo and must not follow the picker. Pinned by leaving those
  call sites outside `gitActions`' injection.
- **Stale answers on switch.** One socket, handled in order, so the fetches
  issued by the switch land last; the echo check is what makes that certain
  rather than probable.

## Notes

Phase B built 2026-09-16, the day it was planned.

**The estimate held, and the shape of the cost was as predicted**: the daemon
was mechanical and the client was small because of `gitActions`. What the
numbers looked like in the end:

| | |
|---|---|
| `Git*` messages that gained `repo` | 25 (`GitBlame` deliberately not) |
| `state.rs` methods threaded | 25, plus `write_target` and `git_root` |
| `api.rs` arms | 25 patterns, 25 calls, 2 re-broadcast helpers |
| Client send sites that needed touching | **8** — the other 17 are inside `gitActions` |

**Three things the plan did not foresee.**

**A session in no repository at all had nowhere to fall back to.** The pane
computes its root as *the pick, else the session's own* — and in the layout
this feature was asked from, `claude` starts in the folder *holding* the four
checkouts, so both are empty and the window said "not in a git repo" beside a
picker full of repositories. It now **adopts the first** repository it may
show, as a real pick: a fallback that only changed what was drawn would leave
`repoOf` answering `null` and every request resolving to a root that is not a
repository. Found by a test written from the acceptance list, before the
browser.

**The REST surface stays single-repo.** `/api/...` names a session and has
nowhere to put a repository, so those handlers pass `None` — the session's own,
which is what every one of them has always answered.

**The picker had to be a real pick to go home.** Choosing the session's own
repository writes `null` rather than its path, so a session that never picked
anything and one that picked its way back send the same message — and the
daemon's "no repo named" path stays the only one either of them takes.

**Verified against the real repositories**, not a fixture: a throwaway daemon
on a spare port over the machine's own sessions, the `V3_WORKSPACE`
`immix-trading-v3` session — which already had `immix-sequencer` in its
workspace — and the Git window switched to *Git: immix-sequencer*, drawing the
sequencer's branches (`feat/channel-term-knobs`, `chore/gcp-perf-test`) and its
log (`release: v0.37.0-rc.1`) while the session stayed where it was.

**The known limit, stated because it is a decision and not an oversight**: the
answers that are *not* among the four echoed ones — a commit's diff, a file
diff, the reflog, the worktree list — are still keyed by session alone. Two
windows on two repositories of one session can therefore show each other's
*diff*, though never each other's log, status, branches or stashes. Echoing the
repo on those four covered every answer the window keys its own state on; the
rest are answers to a question this window just asked, where the ordering of
one socket is enough.
