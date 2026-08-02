---
title: Notes belong to the daemon, and markdown is the truth
status: active
updated: 2026-08-02
decided: 2026-08-02
---

# ADR-0015 — Notes belong to the daemon, and markdown is the truth

## Context

Pillar `L` (`R-L1`–`R-L3`) adds the first thing in mogeung that is **the user's
own writing**. Everything else it shows is derived: sessions, diffs, commits,
transcripts, doc inventories — all produced by an agent or by git and merely
rendered here. If mogeung loses a diff it recomputes it. If it loses a note,
the note is gone.

That difference forces two questions the rest of the product never had to ask,
and they were decided together on 2026-08-02.

**Who owns it?** There is already a split in this codebase, and it is not
arbitrary. Client-side (`prefs.json`, `layout.json`, `state/<machine>.json`)
holds *your view*: pins, hidden, labels, bookmarks, layout. Daemon-side
(`reviewed`, `signals`) holds things you authored *about the work*: which hunks
you have read, which command tests a repo. Review marks are the closest
existing relative of a note — user-authored, daemon-owned, reachable from any
client.

**What is a task?** Free markdown with checkboxes is the cheapest possible
answer and cannot be outgrown badly. A separate task model is more capable and
is how a checklist becomes a project manager nobody asked for. Both were on the
table.

## Decision

**Notes and documents are daemon state. Markdown is the source of truth, and
everything structured about a task is derived from it.**

Three parts, and the third is what makes the first two safe:

1. **The daemon owns them**, in its SQLite store beside `reviewed` and
   `signals`, served over the same wire. Any client sees them; a second window
   is not a second set of notes. Notes about a machine live *with* that machine.
2. **A document is markdown text.** A task is a line matching a checkbox
   (`- [ ]` / `- [x]`). There is no task that exists outside a document, and no
   field on a task that is not written in the document.
3. **The structured half is a derived cache and an append-only history, never
   a second source of truth.** mogeung keeps status transitions with timestamps
   so it can say "three closed today" — a thing the markdown cannot answer,
   because a checkbox has no memory. If the cache and the document ever
   disagree, **the document wins and the cache is rebuilt from it**. It must be
   safe to delete the entire derived table and lose nothing but history.

Two constraints travel with this and are not separable from it:

- **A one-way mirror to disk.** Every write also lands as a `.md` file under
  `~/.mogeung/notes/`. mogeung does not read these back — they are not an input
  and editing one does not change anything — but they mean your writing is
  never trapped in a database only mogeung can open. `grep`, a backup tool and
  any editor all work on them. This is the mitigation for the one real cost of
  daemon ownership, and it is a constraint rather than a nicety because without
  it that cost is unacceptable.
- **Bindings are tags, not locations.** A note may name a session or a repo,
  and naming one does not put the note inside it. A note outlives the session
  it was written during, survives a repo moving, and does not disappear when a
  session is forgotten.

## Alternatives

**Markdown files as the store, client-side.** The strongest alternative, and
what a lone developer would reach for first: real files, no database, mogeung
one reader among many. Rejected on what remote reach turned it into — with
`R-I4`–`R-I7` a window can watch any machine, and files-on-the-client means the
notes you see belong to whichever machine the *window* runs on rather than the
machine you are looking at. Notes about the dev box would live on the laptop
that happened to open it. The one-way mirror above keeps most of what this
alternative offered.

**Markdown files inside the repository** (`.mogeung/`). Notes travel with the
code and can be committed. Rejected because mogeung would start writing files
into repositories it is supposed to be observing, which needs a gitignore
decision from every user, and because it makes private thinking accidentally
shareable — the wrong default for a scratchpad.

**A real task model, tasks first.** Ids, statuses, due dates, links, tasks that
exist without a document. Rejected as the shape most likely to be built and
unused: the roadmap named it before it was designed, and nothing in the request
asked for scheduling. The derived cache above buys the useful half — counting
and history — without the half that needs maintaining.

**Documents only, no derived table at all.** Genuinely tempting, and the
version with no drift risk whatsoever. Rejected because "what did I finish
today" is a real question and a checkbox has no memory of when it was ticked.
The rebuild rule is what keeps this from being a second truth.

## Consequences

Easy: one set of notes however many windows are open. Notes reachable over the
REST API like everything else, so they are greppable without a UI. The derived
table can be dropped and rebuilt at any time, which makes schema changes cheap.

Hard, and a direct consequence of daemon ownership: **the notes you see are the
watched machine's.** Watching the dev box shows the dev box's notes, and your
laptop scratchpad is not there. That is correct — a note about a session on
that machine belongs with it — and it will still be surprising the first time.
It is the mirror image of what `R-I11` decided for view state, which went the
other way for good reasons, and the two will need explaining together.

Also hard: two representations exist, which is the thing this codebase has been
most careful to avoid. The rebuild rule is the whole defence. If it is ever
convenient to write something into the cache that is not in the document, that
is the moment this decision has been abandoned.

Ruled out: tasks that are not lines in a document; notes stored in a repo;
anything that reads the disk mirror back in.

## Revisit if

The mirror turns out to be what people actually use — editing the `.md` files
directly and expecting mogeung to notice. That would say the store and the
truth are the wrong way round, and the honest response is to invert them rather
than to add a file watcher and call it two-way.

Also revisit if the derived cache is ever the reason a bug is hard to fix. It
exists to answer one question; if it is carrying anything else by then, it has
become the task model this ADR declined.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
