/**
 * The ordering rules for the scratch tree, stated without rendering anything.
 * `R-L9`.
 *
 * The rule worth pinning is the one that is not obvious: **files keep the
 * daemon's newest-first order, folders do not.** A tree sorted throughout by
 * modification time jumps about while you type, because saving a file moves the
 * folder holding it — so folders are alphabetical and only their contents keep
 * the order `R-L5` asked for.
 */

import { describe, expect, it } from "vitest";
import { movedInto, moveTargets, treeRows } from "@/lib/scratchTree";

const none = new Set<string>();

describe("laying the scratch folder out as rows", () => {
  it("puts folders before files, at each level", () => {
    const rows = treeRows(["root.txt", "sql/a.sql"], ["sql"], none);
    expect(rows.map((r) => `${r.kind}:${r.path}`)).toEqual([
      "folder:sql",
      "file:sql/a.sql",
      "file:root.txt",
    ]);
  });

  it("orders folders alphabetically and files newest-first", () => {
    // The daemon sends files newest-first; `zebra` is newer than `apple` here.
    const rows = treeRows(["zebra.txt", "apple.txt"], ["b", "a"], none);
    expect(rows.map((r) => r.path)).toEqual(["a", "b", "zebra.txt", "apple.txt"]);
  });

  it("indents by depth", () => {
    const rows = treeRows(["a/b/deep.txt"], ["a", "a/b"], none);
    expect(rows.map((r) => [r.path, r.depth])).toEqual([
      ["a", 0],
      ["a/b", 1],
      ["a/b/deep.txt", 2],
    ]);
  });

  /** A folder with nothing in it still has to be visible, or you cannot move
   *  a file into the one you just made. */
  it("shows a folder that holds no files", () => {
    const rows = treeRows([], ["empty"], none);
    expect(rows).toEqual([{ kind: "folder", path: "empty", name: "empty", depth: 0 }]);
  });

  it("hides what is inside a collapsed folder, but not the folder", () => {
    const rows = treeRows(["sql/a.sql"], ["sql", "sql/deep"], new Set(["sql"]));
    expect(rows.map((r) => r.path)).toEqual(["sql"]);
  });

  it("shows a name rather than a path on the row", () => {
    const rows = treeRows(["sql/query.sql"], ["sql"], none);
    expect(rows.map((r) => r.name)).toEqual(["sql", "query.sql"]);
  });
});

describe("moving a file between folders", () => {
  it("keeps the file name and changes the folder", () => {
    expect(movedInto("a.txt", "sql")).toBe("sql/a.txt");
    expect(movedInto("sql/a.txt", "notes")).toBe("notes/a.txt");
  });

  it("moves back to the root", () => {
    expect(movedInto("sql/a.txt", "")).toBe("a.txt");
  });

  /** So the caller can leave the wire alone rather than renaming onto itself. */
  it("answers null when the move would change nothing", () => {
    expect(movedInto("sql/a.txt", "sql")).toBeNull();
    expect(movedInto("a.txt", "")).toBeNull();
  });

  it("offers every folder but the one the file is already in", () => {
    expect(moveTargets(["sql", "notes"], "sql/a.txt")).toEqual(["", "notes"]);
    expect(moveTargets(["sql", "notes"], "a.txt")).toEqual(["sql", "notes"]);
  });
});
