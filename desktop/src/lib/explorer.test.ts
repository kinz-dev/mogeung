import { describe, expect, it } from "vitest";
import { fileFilter } from "@/lib/explorer";

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
