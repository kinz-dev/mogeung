# Working on mogeung

mogeung watches the Claude Code sessions the user runs and tells them which one
needs attention. **It never starts, steers or stops an agent.**

## Read first

- [`docs/README.md`](docs/README.md) — the doc system and its rules
- [`docs/product/assumptions.md`](docs/product/assumptions.md) — what we believe
  and have not checked
- [`docs/decisions/0003-observe-do-not-spawn.md`](docs/decisions/0003-observe-do-not-spawn.md)
  — the most important decision here, and why v0.1 was thrown away

## Rules for docs

> **Never create a markdown file at the repository root.**
> New docs go under `docs/` per [`docs/README.md`](docs/README.md).
> Unsure where? It goes in `docs/features/<current-feature>`.

- Every doc needs frontmatter with `status` and `updated`.
- Design docs need `covers:` listing the code paths they describe.
- Use a skeleton from `docs/_templates/`. Do not invent structure.
- **ADRs are immutable.** Supersede, never edit.
- `STATUS.md` is generated. Never hand-edit it.

## Rules for work

- A feature starts as a spec in `docs/features/NNNN-slug.md` that names the
  assumptions it depends on. **If an assumption is `UNTESTED`, the work is to
  test it — not to build the feature.**
- Record durable choices as an ADR while the reasoning is fresh.
- Update `docs/design/` when behaviour changes, and bump `updated`.

## Rules for code

- The daemon is the product; every UI is a client with no local authority.
- Never write to `~/.claude`. Read only.
- Parsers for Claude Code's formats must degrade, never panic — they are
  undocumented and change without warning.
- Prefer a test that would fail today over a test that documents what already
  works.
- No dollar amounts; see
  [ADR-0005](docs/decisions/0005-tokens-not-dollars.md).

## Commands

```sh
cargo build --release
cargo test --workspace      # all free — nothing spawns an agent
./scripts/check-docs.sh     # frontmatter, staleness, orphans
./scripts/gen-status.sh     # rewrite STATUS.md
```
