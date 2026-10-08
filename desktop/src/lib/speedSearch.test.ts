/**
 * Type-to-search in the Files tree. `R-J122`.
 *
 * What is pinned is the part a plain filter got wrong: the answer is a **tree**
 * drawn from the whole walk, not the rows that happened to be expanded, and a
 * name query is not allowed to match its directories.
 */

import { describe, expect, it } from "vitest";
import { fuzzyMarks, speedSearch } from "@/lib/speedSearch";

const TREE = [
  "Cargo.toml",
  "desktop/src/lib/explorer.ts",
  "desktop/src/lib/search.ts",
  "desktop/src/ui/tools/FilesTool.tsx",
  "desktop/src/ui/tools/SearchTool.tsx",
  "docs/README.md",
];

const shape = (r: ReturnType<typeof speedSearch>) =>
  r.rows.map((row) => `${"  ".repeat(row.depth)}${row.name}${row.isDir ? "/" : ""}`);

describe("fuzzyMarks", () => {
  it("prefers the contiguous run you typed over a scattered one", () => {
    // Greedy would light `F`, the `i` of `Fil`, then the first `t` — not `Tool`.
    expect(fuzzyMarks("tool", "FilesTool.tsx")).toEqual([5, 6, 7, 8]);
  });

  it("falls back to a subsequence, in order", () => {
    expect(fuzzyMarks("ftl", "FilesTool.tsx")).toEqual([0, 5, 8]);
    expect(fuzzyMarks("zz", "FilesTool.tsx")).toBeNull();
  });

  /** Smart case: an uppercase letter means you meant it. */
  it("ignores case only for a lowercase query", () => {
    expect(fuzzyMarks("files", "FilesTool.tsx")).not.toBeNull();
    expect(fuzzyMarks("FILES", "FilesTool.tsx")).toBeNull();
  });
});

describe("speedSearch", () => {
  it("draws the matching files as a tree, every folder above them open", () => {
    const r = speedSearch(TREE, "tool");
    expect(shape(r)).toEqual([
      "desktop/",
      "  src/",
      "    ui/",
      "      tools/",
      "        FilesTool.tsx",
      "        SearchTool.tsx",
    ]);
    expect(r.files).toEqual(["desktop/src/ui/tools/FilesTool.tsx", "desktop/src/ui/tools/SearchTool.tsx"]);
  });

  /**
   * **The name, not the path.** `tools` is a directory on the way to both
   * hits; matching the path would have found every file under it by `too`.
   */
  it("does not match a file by the directory it is in", () => {
    const r = speedSearch(TREE, "lib");
    expect(r.files).toEqual([]);
    expect(r.total).toBe(0);
  });

  it("matches the whole path once the query has a slash in it", () => {
    const r = speedSearch(TREE, "lib/se");
    expect(r.files).toEqual(["desktop/src/lib/search.ts"]);
    const lib = r.rows.find((row) => row.name === "lib");
    expect(lib?.marks).toEqual([0, 1, 2]);
  });

  it("marks the matched letters on the file row", () => {
    const r = speedSearch(TREE, "srch");
    const row = r.rows.find((x) => x.path === "desktop/src/lib/search.ts");
    expect(row?.marks).toEqual([0, 3, 4, 5]);
    // The folders above it matched nothing — a name query never marks them.
    expect(r.rows.find((x) => x.name === "lib")?.marks).toEqual([]);
  });

  it("puts the cursor on the best match, not the first", () => {
    // `SearchTool.tsx` begins with it; `search.ts` comes first in the tree but
    // only begins with `search`, so `searchtool` has to go fuzzy there — and fails.
    expect(speedSearch(TREE, "searchtool").best).toBe("desktop/src/ui/tools/SearchTool.tsx");
    expect(speedSearch(TREE, "read").best).toBe("docs/README.md");
  });

  it("lists folders before files, as the unfiltered tree does", () => {
    const r = speedSearch(["a.ts", "a/b/a.ts"], "a.ts");
    expect(shape(r)).toEqual(["a/", "  b/", "    a.ts", "a.ts"]);
  });

  /**
   * An added folder is one row named for itself, after the session's own
   * files — not a row per component of an absolute path.
   */
  it("draws an added folder's files under that folder's own row", () => {
    const r = speedSearch(["src/main.rs", "/home/k/other/src/main.rs"], "main", ["/home/k/other"]);
    expect(shape(r)).toEqual(["src/", "  main.rs", "other/", "  src/", "    main.rs"]);
    expect(r.rows[2].path).toBe("/home/k/other");
    expect(r.files[1]).toBe("/home/k/other/src/main.rs");
  });

  /** The first 300 of a monorepo in name order are all under `a`. */
  it("keeps the best matches past the cap, not the first", () => {
    const many = Array.from({ length: 50 }, (_, i) => `a/x${i}_q_u_e_r_y.ts`);
    const r = speedSearch([...many, "z/query.ts"], "query", [], 10);
    expect(r.total).toBe(51);
    expect(r.files).toHaveLength(10);
    expect(r.files).toContain("z/query.ts");
  });

  it("answers nothing for an empty query", () => {
    expect(speedSearch(TREE, "  ")).toEqual({ rows: [], files: [], best: null, total: 0 });
  });
});
