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
- Roadmap items are `R-A1`; assumptions are bare `A1`. Different namespaces —
  they used to collide.

### Always run `./scripts/check-docs.sh` after touching docs

Not optional, and not only when you think you changed something structural.
Renaming a file, moving a doc, or editing a `covers:` list all break things
silently. It is fast and needs no network.

It fails on: markdown at the repo root · missing `status:`/`updated:` ·
unresolvable relative links · a `covers:` path that no longer exists · an `R-…`
id with no matching roadmap row.

It warns on: a design doc whose covered code has commits newer than its
`updated:` date · an ADR nothing links to.

**Treat a staleness warning as work, not noise.** It means the doc now describes
code that has moved. Fix the doc and bump `updated:` — never bump the date alone
to silence it. That converts a true warning into a false clean bill of health,
which is worse than no check.

Run `./scripts/gen-status.sh` too if you changed features, assumptions or tests.
Never edit its output.

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
- Dollars appear on the Analytics view only, labelled *equivalent API cost*
  and dated; every other surface stays tokens. See
  [ADR-0024](docs/decisions/0024-equivalent-cost-in-dollars.md), which
  supersedes ADR-0005.

## Commands

```sh
./scripts/start.sh          # build + run daemon and window together
mprocs                      # same two, side by side, with test/docs on a key

cargo build --release
cargo test --workspace      # all free — nothing spawns an agent
cd desktop && npm test      # the window's own suite; cargo does not run it
./scripts/check-docs.sh     # REQUIRED after any doc change — see above
./scripts/gen-status.sh     # rewrite STATUS.md

cargo run -q -p mogeungd --bin sweep    # what the formats look like today
cargo run -q -p mogeungd --bin runconfigs   # what run configurations exist here
```

### Run the sweep after a Claude Code or Codex upgrade

`A4` says the formats are undocumented and move without warning, and the
canary proves it — but the canary only speaks from a **running daemon**, so
`R-J12`'s fourteenth event type was found by a hand-written script over a
corpus that had grown by 80 files since anyone looked.

`--bin sweep` is that script, kept: it reads the same `HANDLED` /
`KNOWN_IGNORED` / `KNOWN_ITEMS` the parser uses, over whatever corpus is on
this machine, and **exits non-zero if anything is unclassified**. It also
inventories the shapes *below* the type — models, usage keys, content blocks —
because no list exists to check those against and a new one appearing is how
prompt caching arrived.

An unclassified shape is a decision, not a chore: it goes in `HANDLED` if it
carries something, `KNOWN_IGNORED` if it does not, and in
`tests/fixtures/corpus.jsonl` either way so the choice is pinned.

`start.sh --fresh` uses a throwaway database. Worth reaching for when a diff
looks wrong: sessions pin their diff base the first time they are seen, so a
database carried over from an older build can mislead you.

**Shell scripts here must run on bash 3.2**, which is what macOS ships. No
`mapfile`, and expanding an empty array under `set -u` is an error.

Before handing work back: `cargo test --workspace`, `npm test` **and
`npm run check`** in `desktop/`, **and** `./scripts/check-docs.sh` must all
pass. The window is TypeScript since
[ADR-0020](docs/decisions/0020-the-egui-client-is-retired.md), so cargo alone
no longer tests the client at all.

**`npm run check` is `tsc --noEmit`, and it is on this list because `npm test`
does not typecheck.** Vitest transpiles and never checks types, so a type error
— including one in a test file — passes every command above and then fails
`npm run build`, which is `tsc --noEmit && vite build` and is what
`beforeBuildCommand` runs. The first anyone hears of it is a broken
`./scripts/install.sh`, which is the slowest possible place to find out.
Learnt on 2026-08-28, from a guard in a test that `tsc` correctly called
always-true.

## Testing

## Agent skills

### Issue tracker

Work is tracked in `docs/product/roadmap.md` and `docs/features/`, not GitHub
Issues. See [`docs/agents/issue-tracker.md`](docs/agents/issue-tracker.md).

### Triage labels

The five canonical role names, written as a `triage:` frontmatter key. See
[`docs/agents/triage-labels.md`](docs/agents/triage-labels.md).

### Domain docs

Single-context; ADRs live in `docs/decisions/`. See
[`docs/agents/domain.md`](docs/agents/domain.md).

<!-- code-review-graph MCP tools -->
## MCP Tools: code-review-graph

**IMPORTANT: This project has a knowledge graph. ALWAYS use the
code-review-graph MCP tools BEFORE using Grep/Glob/Read to explore
the codebase.** The graph is faster, cheaper (fewer tokens), and gives
you structural context (callers, dependents, test coverage) that file
scanning cannot.

### When to use graph tools FIRST

- **Exploring code**: `semantic_search_nodes_tool` or `query_graph_tool` instead of Grep
- **Understanding impact**: `get_impact_radius_tool` instead of manually tracing imports
- **Code review**: `detect_changes_tool` + `get_review_context_tool` instead of reading entire files
- **Finding relationships**: `query_graph_tool` with callers_of/callees_of/imports_of/tests_for
- **Architecture questions**: `get_architecture_overview_tool` + `list_communities_tool`

Fall back to Grep/Glob/Read **only** when the graph doesn't cover what you need.

### Key Tools

| Tool | Use when |
| ------ | ---------- |
| `detect_changes_tool` | Reviewing code changes — gives risk-scored analysis |
| `get_review_context_tool` | Need source snippets for review — token-efficient |
| `get_impact_radius_tool` | Understanding blast radius of a change |
| `get_affected_flows_tool` | Finding which execution paths are impacted |
| `query_graph_tool` | Tracing callers, callees, imports, tests, dependencies |
| `semantic_search_nodes_tool` | Finding functions/classes by name or keyword |
| `get_architecture_overview_tool` | Understanding high-level codebase structure |
| `refactor_tool` | Planning renames, finding dead code |

### Workflow

1. The graph auto-updates on file changes (via hooks).
2. Use `detect_changes_tool` for code review.
3. Use `get_affected_flows_tool` to understand impact.
4. Use `query_graph_tool` pattern="tests_for" to check coverage.
