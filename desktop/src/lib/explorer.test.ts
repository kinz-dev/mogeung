import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";
import { setDock } from "@/lib/panes";
import { closeFile, fileFilter, openFile } from "@/lib/explorer";

/**
 * The Files filter. Written against the two things that made the old one
 * useless — it only saw the rows that happened to be expanded, and it could
 * only match a literal — so every case here would have failed before.
 */
describe("the files filter", () => {
  const tree = [
    "src/ui/tools/FilesTool.tsx",
    "src/ui/tools/SearchTool.tsx",
    "src/lib/explorer.ts",
    "src/store/index.ts",
    "crates/mogeungd/src/state.rs",
    "crates/mogeung-core/src/session.rs",
    "README.md",
  ];
  const matching = (q: string) => tree.filter((p) => fileFilter(q).test(p));

  it("anchors against the name, not only the path", () => {
    // `^state` cannot match the path — it starts with `crates/` — so a filter
    // that tested the path alone would answer nothing here.
    expect(matching("^state")).toEqual(["crates/mogeungd/src/state.rs"]);
  });

  it("anchors against the path too, so a subtree can be asked for", () => {
    expect(matching("^src/lib")).toEqual(["src/lib/explorer.ts"]);
  });

  it("takes an extension as the regex it looks like", () => {
    expect(matching("\\.rs$")).toEqual([
      "crates/mogeungd/src/state.rs",
      "crates/mogeung-core/src/session.rs",
    ]);
  });

  it("is smart-cased: lowercase ignores case, an uppercase letter does not", () => {
    expect(matching("filestool")).toEqual(["src/ui/tools/FilesTool.tsx"]);
    expect(matching("Filestool")).toEqual([]);
  });

  it("reads an unfinished pattern literally rather than answering nothing", () => {
    // Mid-keystroke on the way to `explorer(x)`. A thrown SyntaxError, or an
    // empty result, would both read as "no such file".
    const f = fileFilter("explorer(");
    expect(f.regex).toBe(false);
    expect(f.test("src/lib/explorer(1).ts")).toBe(true);
    expect(f.test("src/lib/explorer.ts")).toBe(false);
  });

  it("is empty when nothing is typed, and matches everything", () => {
    const f = fileFilter("   ");
    expect(f.empty).toBe(true);
    expect(matching("   ")).toEqual(tree);
  });

  it("matches every file with a bare dot-star, which is the cap's job to survive", () => {
    expect(matching(".*")).toEqual(tree);
  });
});

/**
 * One open file, one pane. `R-B53`.
 *
 * The window ran two tab systems until 2026-08-07 — dockview's, and the Code
 * pane's own strip with its own two-way split. These pin the state half of
 * collapsing them: what `open` means now that it no longer carries a side or
 * an active index.
 */
describe("opening files, one pane each", () => {
  beforeEach(() => {
    useStore.setState({ explorer: {} });
    setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  });

  it("keeps one entry per (path, revision)", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s1", "a.rs", { pin: true });
    expect(useStore.getState().explorer.s1.open).toHaveLength(1);

    // The worktree twin of a path and a revision of it are different files.
    openFile("s1", "a.rs", { pin: true, rev: "abc1234" });
    expect(useStore.getState().explorer.s1.open).toHaveLength(2);
  });

  /** The IntelliJ rule, now one preview per session rather than per side. */
  it("reuses the unpinned preview and keeps the pinned ones", () => {
    openFile("s1", "pinned.rs", { pin: true });
    openFile("s1", "browsed-a.rs");
    openFile("s1", "browsed-b.rs");

    const paths = useStore.getState().explorer.s1.open.map((t) => t.path);
    expect(paths).toEqual(["pinned.rs", "browsed-b.rs"]);
  });

  it("promotes a preview to pinned rather than opening it twice", () => {
    openFile("s1", "a.rs");
    openFile("s1", "a.rs", { pin: true });
    const open = useStore.getState().explorer.s1.open;
    expect(open).toHaveLength(1);
    expect(open[0].pinned).toBe(true);
  });

  it("carries a pending gotoLine onto a file already open", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s1", "a.rs", { line: 42 });
    expect(useStore.getState().explorer.s1.open[0].gotoLine).toBe(42);
  });

  it("closes by identity, not by index", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s1", "b.rs", { pin: true });
    closeFile("s1", "a.rs", null);
    expect(useStore.getState().explorer.s1.open.map((t) => t.path)).toEqual(["b.rs"]);
  });

  /** Each session keeps its own files — that is what "bound to its session" is. */
  it("keeps one session's files out of another's", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s2", "b.rs", { pin: true });
    expect(useStore.getState().explorer.s1.open.map((t) => t.path)).toEqual(["a.rs"]);
    expect(useStore.getState().explorer.s2.open.map((t) => t.path)).toEqual(["b.rs"]);
  });
});
