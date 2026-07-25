---
title: Reviewing changes
status: active
updated: 2026-07-25
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
