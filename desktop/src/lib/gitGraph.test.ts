import { describe, expect, it } from "vitest";
import { graphCell, lanes, MAX_LANES } from "@/lib/gitGraph";
import type { CommitInfo } from "@/wire/types";

const c = (sha: string, parents: string[]): CommitInfo => ({
  sha: sha + "0000000000000000000000000000000000",
  short: sha,
  author: "k",
  epoch: 0,
  summary: sha,
  refs: [],
  parents,
  touches_session: false,
});

describe("lanes", () => {
  it("draws a linear history in one lane", () => {
    const { rows, width } = lanes([c("a", ["b"]), c("b", ["c"]), c("c", [])]);
    expect(width).toBe(1);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(rows[0].above).toEqual([]);
    expect(rows[0].below).toEqual([true]);
    expect(rows[2].below, "a root commit ends its lane").toEqual([]);
  });

  it("opens a second lane at a merge and closes it at the fork", () => {
    // m merges f into the main line; f branched off c.
    const { rows, width } = lanes([c("m", ["b", "f"]), c("f", ["c"]), c("b", ["c"]), c("c", [])]);
    expect(width).toBe(2);
    expect(rows[0]).toMatchObject({ lane: 0, merge: true, parents: [0, 1] });
    expect(rows[1].lane, "the branch commit sits in the lane the merge opened").toBe(1);
    expect(rows[2].lane).toBe(0);
    // c is expected by both lanes; the join collapses them.
    expect(rows[3]).toMatchObject({ lane: 0, joins: [1] });
    expect(rows[3].below).toEqual([]);
  });

  it("reuses a freed lane rather than leaking columns rightward", () => {
    // Two separate merges, one after the other: the second must take lane 1
    // again, not lane 2.
    const { rows, width } = lanes([
      c("m2", ["m1", "g"]),
      c("g", ["m1"]),
      c("m1", ["a", "f"]),
      c("f", ["a"]),
      c("a", []),
    ]);
    expect(width).toBe(2);
    expect(rows[1].lane).toBe(1);
    expect(rows[3].lane).toBe(1);
  });

  it("keeps a lane open across what would be a page boundary", () => {
    // The merge's second parent is far below; every row between carries the
    // waiting lane. Computed over one list, so no page can drop it.
    const { rows } = lanes([c("m", ["a", "z"]), c("a", ["b"]), c("b", ["c"]), c("c", ["z"]), c("z", [])]);
    for (const i of [1, 2, 3]) expect(rows[i].above).toEqual([true, true]);
    expect(rows[4]).toMatchObject({ lane: 0, joins: [1] });
  });

  it("matches a full sha against an abbreviated parent", () => {
    const parent = c("b", []);
    const { rows } = lanes([c("a", ["b"]), parent]);
    expect(rows[1].lane).toBe(0);
    expect(rows[1].joins).toEqual([]);
  });

  it("caps the drawn width", () => {
    // An octopus with nine parents wants ten lanes.
    const parents = Array.from({ length: 9 }, (_, i) => `p${i}`);
    const { width } = lanes([c("o", parents)]);
    expect(width).toBe(MAX_LANES);
  });
});

describe("graphCell", () => {
  it("draws a ring for a merge and a dot otherwise", () => {
    const { rows, width } = lanes([c("m", ["a", "f"]), c("f", ["a"]), c("a", [])]);
    expect(graphCell(rows[0], width)).toContain('stroke-width="1.6"');
    expect(graphCell(rows[2], width)).not.toContain('stroke-width="1.6"');
    // The fork's second parent leaves the dot diagonally into lane 1.
    expect(graphCell(rows[0], width)).toContain('x1="6" y1="11" x2="18" y2="22"');
  });
});
