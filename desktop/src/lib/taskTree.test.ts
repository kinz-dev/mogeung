/**
 * The shape of the task panel, stated without rendering anything. `R-L11`.
 *
 * The rule worth pinning hardest is the one that would go wrong quietly: a
 * **done parent stays visible while an open child needs it**. Hiding ticked
 * tasks is what makes a checklist a list of what to do, and hiding a ticked
 * parent would orphan its children — dropping real work off the screen, or
 * floating it at a level nobody wrote.
 */

import { describe, expect, it } from "vitest";
import { groupsOf, keyOf, openUnder, rowsOf, totalUnder } from "@/lib/taskTree";
import type { Task } from "@/wire/types";

const t = (over: Partial<Task> & { ord: number }): Task => ({
  note_id: "n1",
  text: `task ${over.ord}`,
  done: false,
  ...over,
});

const none = new Set<string>();
const paths = (rows: ReturnType<typeof rowsOf>) =>
  rows.map((r) => `${"  ".repeat(r.node.depth)}${r.node.task.text}`);

describe("grouping by heading", () => {
  it("keeps document order and counts what is open", () => {
    const g = groupsOf([
      t({ ord: 0, text: "loose" }),
      t({ ord: 1, text: "milk", group: "Groceries" }),
      t({ ord: 2, text: "bread", group: "Groceries", done: true }),
    ]);

    expect(g.map((x) => x.name)).toEqual([null, "Groceries"]);
    expect(g[1].open).toBe(1);
    expect(g[1].total).toBe(2);
  });

  /** A heading used twice is two sections; merging them moves tasks up the page. */
  it("does not merge two runs of the same heading", () => {
    const g = groupsOf([
      t({ ord: 0, group: "Work" }),
      t({ ord: 1, group: "Home" }),
      t({ ord: 2, group: "Work" }),
    ]);
    expect(g.map((x) => x.name)).toEqual(["Work", "Home", "Work"]);
  });
});

describe("nesting by depth", () => {
  it("makes a child of the task above it", () => {
    const g = groupsOf([
      t({ ord: 0, text: "parent" }),
      t({ ord: 1, text: "child", depth: 1 }),
      t({ ord: 2, text: "sibling" }),
    ]);
    expect(paths(rowsOf(g[0].nodes, { showDone: true, collapsed: none }))).toEqual([
      "parent",
      "  child",
      "sibling",
    ]);
  });

  /**
   * Markdown lets you over-indent. Drawing that as two levels of blank
   * indentation is a gap nobody wrote, so the relationship is kept and the
   * levels are renumbered.
   */
  it("renumbers a jump in depth to a single level", () => {
    const g = groupsOf([t({ ord: 0, text: "parent" }), t({ ord: 1, text: "child", depth: 3 })]);
    expect(paths(rowsOf(g[0].nodes, { showDone: true, collapsed: none }))).toEqual([
      "parent",
      "  child",
    ]);
  });

  it("comes back out to the level it names", () => {
    const g = groupsOf([
      t({ ord: 0, text: "a" }),
      t({ ord: 1, text: "b", depth: 1 }),
      t({ ord: 2, text: "c", depth: 2 }),
      t({ ord: 3, text: "d", depth: 1 }),
    ]);
    expect(paths(rowsOf(g[0].nodes, { showDone: true, collapsed: none }))).toEqual([
      "a",
      "  b",
      "    c",
      "  d",
    ]);
  });

  /** Nesting does not reach across a heading — a new section is a new list. */
  it("does not nest a task under one in another group", () => {
    const g = groupsOf([
      t({ ord: 0, text: "a", group: "One" }),
      t({ ord: 1, text: "b", group: "Two", depth: 1 }),
    ]);
    expect(g[1].nodes[0].depth).toBe(0);
  });
});

describe("hiding what is done", () => {
  it("hides a ticked leaf", () => {
    const g = groupsOf([t({ ord: 0, text: "open" }), t({ ord: 1, text: "shut", done: true })]);
    expect(paths(rowsOf(g[0].nodes, { showDone: false, collapsed: none }))).toEqual(["open"]);
  });

  /** The rule that would go wrong quietly. */
  it("keeps a ticked parent while an open child needs it", () => {
    const g = groupsOf([
      t({ ord: 0, text: "parent", done: true }),
      t({ ord: 1, text: "child", depth: 1 }),
    ]);
    expect(paths(rowsOf(g[0].nodes, { showDone: false, collapsed: none }))).toEqual([
      "parent",
      "  child",
    ]);
  });

  it("hides a ticked parent once its children are ticked too", () => {
    const g = groupsOf([
      t({ ord: 0, text: "parent", done: true }),
      t({ ord: 1, text: "child", depth: 1, done: true }),
    ]);
    expect(rowsOf(g[0].nodes, { showDone: false, collapsed: none })).toEqual([]);
  });
});

describe("collapsing", () => {
  it("hides the children and keeps the parent", () => {
    const g = groupsOf([
      t({ ord: 0, text: "parent" }),
      t({ ord: 1, text: "child", depth: 1 }),
    ]);
    const shut = new Set([keyOf(g[0].nodes[0].task)]);
    const rows = rowsOf(g[0].nodes, { showDone: true, collapsed: shut });

    expect(paths(rows)).toEqual(["parent"]);
    expect(rows[0].collapsed).toBe(true);
  });
});

describe("what a parent covers", () => {
  it("counts itself and everything under it", () => {
    const g = groupsOf([
      t({ ord: 0, text: "parent" }),
      t({ ord: 1, text: "a", depth: 1 }),
      t({ ord: 2, text: "b", depth: 1, done: true }),
    ]);
    const parent = g[0].nodes[0];
    expect(totalUnder(parent)).toBe(3);
    expect(openUnder(parent)).toBe(2);
  });
});
