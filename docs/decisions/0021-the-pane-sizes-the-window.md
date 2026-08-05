---
title: The Agent pane sizes the tmux window, and lives with the repaint
status: active
updated: 2026-08-06
decided: 2026-08-06
---

# ADR-0021 — The Agent pane sizes the tmux window, and lives with the repaint

## Context

The Agent pane is a second tmux client on a session that a real terminal is
often attached to as well ([ADR-0010](0010-attach-a-terminal-never-own-one.md)).
tmux gives a session **one** screen, so it must pick a geometry from among its
clients; `scripts/yolomo` sets `window-size latest`, which means whichever
client most recently sent input.

Two clients of different sizes therefore resize the window every time attention
moves between them — and mouse motion counts as input, so crossing the pane with
the pointer is enough.

That would be harmless if the program redrew cleanly. Claude Code does not:
across 476KB of captured pane output there is **no `ESC[2J`** — it paints deltas
and never clears the screen. So a frame that lands at a different offset after a
resize leaves the previous frame's rows behind, permanently. Reported
2026-08-05 as *"every time I change the session, the AGENT window will insert a
new newline"*: measured, the pane's input box was drawn with three blank rows
that no amount of Backspace removed, while Claude Code's own cursor never left
the row it thought the prompt was on.

The mogeung side was cleared by measurement, which is what made this a decision
about sizing rather than a bug hunt: 1,695 pty writes were traced across two
sessions of use and **not one was a newline**; tmux was shown to inject nothing
into a pane on attach, detach or resize, and to consume xterm.js's query
replies.

## Decision

The pane attaches plainly — `tmux attach-session -t =<target>` — and takes its
full part in tmux's sizing. It sizes the window to itself when it is the latest
client, and the repaint artefacts that follow are accepted.

The mitigation offered to the user is **one head per session** (`yolomo -d`, or
detaching the terminal client), not a change in how the pane attaches.

## Alternatives

- **`attach-session -f ignore-size`** — takes the pane out of tmux's sizing
  decision entirely. Built and run on 2026-08-05. It does stop the corruption,
  and it also stops the pane fitting: the agent's TUI is then drawn for the
  *other* client's geometry, so the pane shows a clipped or floating view of a
  window sized for someone else. Rejected within the hour of trying it, in the
  user's words *"a bigger problem than the empty line"* — a pane you cannot
  read is worse than a pane with stale rows in it.
- **Setting `window-size largest` or `smallest` on the session.** Both are
  writes to the user's tmux configuration for the convenience of one client,
  and neither removes the resize — `smallest` merely moves which client is
  squeezed, and still resizes on every attach and detach.
- **Detaching the other client on attach (`attach -d`).** This is mogeung
  reaching into a session to evict a terminal the user chose to have open. The
  product does not do that to an agent's environment.
- **Forcing a full repaint after attaching.** There is nothing to force it
  *with*: the stale rows are in tmux's grid, tmux redraws them faithfully, and
  only the program can overwrite them. Making it repaint means sending it keys,
  which is steering ([ADR-0003](0003-observe-do-not-spawn.md)).

## Consequences

- The pane is readable at whatever size it happens to be, which is the property
  it exists for.
- Sessions watched from mogeung **and** an attached terminal will accumulate
  stale rows in the agent's prompt box. They are cosmetic — no keystroke was
  sent and no text exists — and any repaint clears them.
- The product's answer to this is a workflow, not code: a session with one head
  never resizes, so `yolomo -d` sessions do not show the artefact at all. That
  puts a documented cost on the two-head workflow the default `yolomo` produces.
- We are carrying a defect we did not cause and cannot fix from here. That is
  stated rather than hidden, and it is the price of observing a TUI we do not
  own.

## Revisit if

Claude Code starts clearing the screen on resize (the artefact disappears on its
own, and none of this matters); or a tmux release offers a per-client flag that
excludes a client from sizing *while still drawing it at its own size*, which is
the option that does not exist today; or `yolomo -d` becomes the default, at
which point the two-head case is rare enough to reconsider `ignore-size` for the
sessions that remain.

---
*ADRs are immutable. To change this decision, write a new ADR that supersedes
it and set `status: superseded` plus `superseded_by:` here.*
