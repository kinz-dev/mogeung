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
