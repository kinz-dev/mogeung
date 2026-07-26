---
title: Structured transcript instead of an embedded terminal
status: superseded
updated: 2026-07-26
decided: 2026-07-25
superseded_by: ADR-0010
---

# ADR-0002 — Structured transcript instead of an embedded terminal

## Context

The original concept listed an integrated terminal as P0, on the reasoning that
everything bottoms out in a shell. Separately, VT100 emulation is egui's weakest
case ([ADR-0001](0001-rust-core-with-egui-ui.md)).

## Decision

**No embedded terminal.** A session is rendered as typed events — prompts, tool
calls with one-line summaries, tool results, errors — not as emulated scrollback.
"Open my real terminal here" covers the rest.

## Rationale

The constraint pointed at the better design. Terminal output is an undifferentiated
character stream: it cannot be searched by tool, linked to a diff hunk, or diffed
between runs. Typed events can be all three.

Choosing this because egui made the alternative hard would have been a bad
reason. It is right on its own merits, and the concept doc was revised to say so.

## Consequences

- Transcripts are searchable and linkable; the corpus becomes queryable
  (roadmap section F).
- Anything relying on raw terminal rendering — progress bars, TUI output,
  ANSI art — is lost. Acceptable.
- Multi-line commands must be collapsed for single-line display. A real bug was
  found here when a heredoc leaked newlines into the session headline.
