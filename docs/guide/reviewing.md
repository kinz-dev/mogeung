---
title: Reviewing changes
status: active
updated: 2026-10-20
---

# Reviewing changes

Open a session and choose **Changes** (`Alt+2`). The repository itself —
log, branches, staging, commits — is the Git tool window, on its own page:
[git.md](git.md).

## Read in the order given

Files are sorted by risk, not alphabetically. Auth, secrets, migrations, money,
infra, CI and dependency manifests rise; lockfiles, generated code and fixtures
sink and are hidden by default. A file scores as its riskiest hunk.

**This is keyword matching, not analysis.** Treat it as reading order. It will
flag a variable named `password_field` and miss a subtle bug in a boring file.
With a local model configured, **reading guide** asks it for an order of its
own, shown beside the keyword order and never blended into it
([ADR-0031](../decisions/0031-consent-to-a-named-host.md)).

## Tick as you read

Each hunk has a **read** checkbox. Marking it records a hash of the hunk's
*content* — not its position.

So when the agent edits the file again:

- untouched hunks stay **read**
- rewritten or new hunks come back **unread**

You never read the same code twice. **hide read** narrows the view to what is
actually new.

Anchors ignore indentation and internal whitespace, so re-indenting a file does
not bring back hunks you already read. Normalisation stops at whitespace on
purpose: string contents and case still count, because the failure to avoid at
all costs is marking code you have *not* read as read.

## What the diff includes

Committed work since the base, uncommitted edits, and **untracked new files** —
which plain `git diff` misses and which is exactly what an agent creating new
modules produces.

The base is **the last commit made before the session started**, so a session
that predates the daemon still diffs against the right commit. The base is
pinned the first time a session is seen and cleared when the branch is switched
([review-checkpointing.md](../design/review-checkpointing.md)).

Two sessions editing the same file both show it. Git cannot separate them, and
the queue's `⚠ COLLISION` is the warning.

## Finishing

**mark every hunk read** clears the session out of the queue. Nothing is written
to your repo — review state lives only in mogeung's database.

## Verification

Under the session's **Info**, mogeung lists the build, test and typecheck
commands the agent ran and how each ended, and binds a "tests pass"-shaped
claim in its prose to the nearest run — or marks it *unverified* when nothing
was run. A per-repo **signal** command (a test suite, a typecheck) runs only
when you press it and its result sits beside the agent's claims, never merged
with them; with lcov output it shows coverage over the changed lines. The
**Run** dock tool (`Alt+4`) starts a configuration the repository itself
contains, and a run you started is shown beside the claim the same way
([ADR-0025](../decisions/0025-run-a-process-you-named-never-an-agent.md)).

## Handing off

Three buttons beside the anchor in an Agent pane's header hand the session's
folder to another program, on the machine the daemon runs on: the **file
manager** (Finder on a Mac, your desktop's handler elsewhere), a **terminal**
in that folder, and **IntelliJ IDEA**. Which terminal is the `terminal` key
in that machine's `~/.mogeung/config.toml` — the Configuration window edits
it, and a save takes effect at once — and until it is set, terminator on
Linux and iTerm2 on a Mac. It opens a shell in the folder, not an attach to
the session's tmux; the `i` beside it says how to attach by hand. The
IntelliJ button is live when the project has a `.idea` folder and a launcher
was found there; otherwise it is dead and its hover says which. Editing
properly is not mogeung's job and is not planned to be.

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
- The clipboard button on any message copies it; the transcript's own button
  exports the whole conversation as a Markdown file.
- Find (`Ctrl+F` in the pane) searches it, substring and regex side by side.

The pane is virtualised, so a long session costs nothing to scroll; it opens at
the newest turn.

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
A large diff opens collapsed; `Ctrl+D` on a file opens its diff as a pane in
the centre, where it has the window's height and can pop out.

## Flagging and the follow-up prompt

`✎ flag` on a hunk collects it. The prompt window turns everything you flagged
into text — file, hunk header, the actual changed lines, plus any note you add —
and offers two actions: **copy**, and **send**.

**Copy** is always there. **Send** puts exactly the text on screen into that
one session's tmux pane, after a confirmation that names the session and shows
the first line, once; mogeung never reads the reply and never sends on its own
([ADR-0003](../decisions/0003-observe-do-not-spawn.md), amended 2026-08-29).
The confirmation cannot see what the session's screen is showing, so a menu
waiting there would take the Enter — read the pane first. With a local model
configured, **draft** writes the prompt for you from the flagged hunks; the
raw concatenation stays one click away, and drafting never sends.

Quoting the real diff lines matters — an agent handed the hunk does better than
one handed "fix the error handling in state.rs".

## Blast radius

**⌁ blast radius** on a file finds the symbols its diff declares or changes, then
searches the repo for other references. Test files are listed first, because
"did anything test this?" is the question with teeth.

**It is `git grep`, not a compiler.** It over-reports common names and misses
anything dynamic. Treat it as "these places mention it, you may want to look".

## Debt

The **Debt** tool (`Alt+5`) answers "how much of what agents wrote in this repo
has nobody read?", counted in hunks and listed riskiest-first. Clicking a file
jumps straight to it.

It only covers sessions mogeung has seen — work from before it was watching is
not in the number.
