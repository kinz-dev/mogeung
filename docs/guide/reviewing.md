---
title: Reviewing changes
status: active
updated: 2026-09-11
---

# Reviewing changes

Open a session and choose **Changes**.

## Read in the order given

Files are sorted by risk, not alphabetically. Auth, secrets, migrations, money,
infra, CI and dependency manifests rise; lockfiles, generated code and fixtures
sink and are hidden by default. A file scores as its riskiest hunk.

**This is keyword matching, not analysis.** Treat it as reading order. It will
flag a variable named `password_field` and miss a subtle bug in a boring file.

## Tick as you read

Each hunk has a **read** checkbox. Marking it records a hash of the hunk's
*content* — not its position.

So when the agent edits the file again:

- untouched hunks stay **read**
- rewritten or new hunks come back **unread**

You never read the same code twice. **hide read** narrows the view to what is
actually new.

Caveat: reformatting changes the content, so a purely cosmetic edit makes a hunk
unread again.

## What the diff includes

Committed work since the session started, uncommitted edits, and **untracked new
files** — which plain `git diff` misses and which is exactly what an agent
creating new modules produces.

Two limits worth knowing:

- The base is repo HEAD *when mogeung first saw the session*, so sessions that
  predate the daemon show only uncommitted work.
- Two sessions editing the same file both show it. Git cannot separate them.

## Finishing

**Mark all read** clears the session out of the queue. Nothing is written to
your repo — review state lives only in mogeung's database.

## Handing off

**Terminal**, **IntelliJ**, **VS Code** and **Finder** open the session's working
directory. Editing properly is not mogeung's job and is not planned to be.


## The transcript

Agent replies render as Markdown — headings, lists, tables, inline code and
fenced code blocks. 46% of assistant messages in a real corpus contain at least
one of those, so as one flat string a transcript is markedly harder to read than
what the agent actually wrote.

**Tool output is not rendered as Markdown**, deliberately. A stack trace, a log
or a diff is literal text: Markdown would eat its `*`, turn a leading `#` into a
heading and collapse its line breaks. The rule is *prose the model wrote is
Markdown; output a program emitted is monospace.*

- **markdown** turns rendering off if you want the raw text.
- **thinking** hides reasoning blocks entirely.
- The clipboard button on any message copies it — egui labels are not
  selectable, so this is how you get text out.

Only the most recent 150 events are drawn, with a **show earlier** button.
Markdown is parsed per visible event per frame, so an unbounded transcript would
tie the frame rate to how long the session has been running.

## The file list

One line per file: `●` risk-coloured when unread, green `✓` when fully read,
then the path and its churn. Hover any row for the full path, risk level, flags
and how many hunks remain.

Paths are shortened from the left (`…/src/state.rs`) because the filename is
what tells files apart.

## Reading the diff

**syntax** approximates highlighting with a tokenizer — no grammars, no language
detection. It mis-colours things occasionally; it never alters the text.

**words** highlights only the part of a line that actually moved, for lines that
look like replacements. Turn it off if a hunk is mostly rewritten, where
everything is a change and the emphasis stops meaning anything.

**≡ / ⇹** switches unified and side-by-side. Side-by-side pairs a removed line
with the addition that replaced it, which is what makes the word diff readable.

## Reformatting no longer resurrects hunks

Anchors ignore indentation and internal whitespace, so re-indenting a file does
not bring back hunks you already read.

Normalisation stops at whitespace on purpose. String contents and case still
count, because the failure to avoid at all costs is marking code you have *not*
read as read.

## Flagging and the follow-up prompt

`✎ flag` on a hunk collects it. The prompt window turns everything you flagged
into text — file, hunk header, the actual changed lines, plus any note you add —
and offers one button: **Copy to clipboard.**

**mogeung does not send it.** You paste it into that session's terminal
yourself. That friction is deliberate and permanent: a supervision layer that
starts putting words into agents is the thing that made v0.1 worse than a plain
terminal ([ADR-0008](../decisions/0008-build-the-prompt-never-send-it.md)).

Quoting the real diff lines matters — an agent handed the hunk does better than
one handed "fix the error handling in state.rs".

## Blast radius

**⌁ blast radius** on a file finds the symbols its diff declares or changes, then
searches the repo for other references. Test files are listed first, because
"did anything test this?" is the question with teeth.

**It is `git grep`, not a compiler.** It over-reports common names and misses
anything dynamic. Treat it as "these places mention it, you may want to look".

## Debt

The **Debt** tab answers "how much of what agents wrote in this repo has nobody
read?", counted in hunks and listed riskiest-first. Clicking a file jumps
straight to it.

It only covers sessions mogeung has seen — work from before it was watching is
not in the number.

## The Git tool window

**Git** in the bottom dock (`Alt+9`) is the repository the selected session
is in — IntelliJ's tool window, in mogeung's colours.

**Log** is three panes. On the left, the branches: HEAD, Local, one node per
remote, Tags, grouped on `/`, with a search box and a star for favourites. A
click **scopes the log to that ref and checks nothing out**. In the middle,
one row per commit — the graph, the subject with its refs as chips, author,
date — and at the end of a row the two marks that are mogeung's: a blue dot
for a commit that probably came from this session, and *read* once every hunk
of it has been read. Above the rows, the filter bar: *text or hash*, then
Branch, User, Date and Paths. Enter runs the query; a hash selects that
commit; `author:`, `path:` and `find:` in the box reach the same fields the
dropdowns do. What is in force is named beneath the bar, with *clear* beside
it. The default is every branch, which is what the graph needs; any text
filter hides the graph, because lanes drawn over a subset would join dots
that are not adjacent.

On the right, the selected commit: its files as a tree with counts, and the
details beneath — message, hash, author and email, date, the signature state,
and the branches that contain it. **Click a file and its diff replaces the
details**, read marks and all; *all files* is the root row; *details* brings
the message back. `n` and `p` step through the hunks and across files. The
strip's ⧉ — or `Enter` on the file you already have selected — opens that
diff as a pane in the centre, where a long read has the window's height and
can pop out into a window of its own; it closes like a file.

**Local changes** is the working tree, grouped Conflicted · Staged · Unstaged
· Untracked, with a *this session* toggle and the selected file's diff — or
its three sides when it is conflicted. **Stash** lists what is shelved and
shows one through the same inspector. **Console** is every git question this
window asked, in order, with what came back — git's refusals in its own
words, and the fetch report in full. **More** holds the reflog, the worktrees
and the submodules.

Three panes at the dock's usual height is a cramped IntelliJ, and IntelliJ
knows it: **maximise** the dock with the button on its strip, `Alt+Shift+9`,
or a double-click on the open tool's tab, and the centre folds away until
you restore it.

Right-click a commit for copy sha, copy subject, copy as patch, a link to the
commit on GitHub or GitLab, and *mark* — two marked ends make a range diff.
Right-click a branch for *compare with the current branch*, which shows what
merging it would bring, from the merge base.

The keyboard: `/` is the search box; `↑` `↓` or `j` `k` move the log; `Enter`
steps into the file tree and `Esc` back; `n` `p` walk hunks; `Ctrl+T`
fetches, and the header says when the last fetch was, because ahead/behind is
only as true as that.

**And it acts.** In Local changes the checkbox is the index: tick a file to
stage it, untick to take it out of the next commit; *stage all* and *unstage
all* sit on each group's header. The commit box on the right takes a message,
*amend*, and *name this session in a trailer* — a `Mogeung-Session:` line
that prompt-blame reads back — and commits only what is staged, never `-a`.
Right-click a file for *discard*, which asks first and names every file,
because git keeps no copy. A conflicted file's three-way view offers *take
ours*, *take theirs* and *mark resolved*. Right-click a branch to *check it
out*; if an agent is running in that worktree the window names it and asks,
because git cannot see an agent reading files that silently change. The
branch pane's `+` makes a new branch from HEAD. Stash has *Stash all*, and
*pop* and *drop* on each entry. Every refusal comes back in git's own words.

Nothing here reaches a remote but fetch. Pull and push are not in this
window, by decision.
