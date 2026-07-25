---
title: Rust core with a native egui UI
status: active
updated: 2026-07-25
decided: 2026-07-25
---

# ADR-0001 — Rust core with a native egui UI

## Context

The tool must stay fast on large repos for years, watch the filesystem, drive
git, and render dense live dashboards. The Rust GUI field was surveyed against
this specific application.

## Decision

**Rust core (`mogeungd`) plus a native egui/eframe client.** A web UI, if built,
is a second client over the same API rather than a replacement.

Core: `tokio`, `axum`, shelled-out `git`, `imara-diff`, `notify`, `tree-sitter`,
SQLite via `rusqlite`.

## Alternatives

- **GPUI** — powers Zed, so it clearly can do editors. But it is maintained *for*
  Zed: pre-1.0 with explicit no-API-stability, ~3 publishes in 18 months trailing
  monorepo HEAD, thin standalone docs, ~101k lifetime downloads. A foundation
  that moves underneath us.
- **Iced** — more native-feeling, slower to set up, thinner widget ecosystem.
- **Floem** — same maintained-for-one-app risk as GPUI.
- **Xilem** — best long-term architecture, not production-ready.
- **TypeScript + React** — fastest to build, but the daemon must stay fast for
  years and this is the layer hardest to replace later.

egui was chosen on ~13M downloads (roughly 10× iced and slint combined), the
best documentation, and proof at scale in serious tooling.

## Consequences

- Rust's git/watch/PTY/parsing ecosystem is excellent; that half is low risk.
- egui's weak cases are rich text editing and terminal emulation. This forced
  [ADR-0002](0002-structured-transcript-not-a-terminal.md), which turned out to
  be the better product anyway.
- If an editor is ever built, egui will fight us. Deferred deliberately; see
  roadmap section K.
- egui 0.35 replaced `App::update(ctx)` with `App::ui(&mut Ui)` and collapsed
  the panel types mid-build. Expect churn; pin versions.
