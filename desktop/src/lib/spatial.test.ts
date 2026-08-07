/**
 * Which pane is *that way*. `R-B52`.
 *
 * Asked for with two agents up: *"move the focus to the left claude session on
 * screen"*. The wiring is easy; the choice is where this goes wrong, and it
 * goes wrong in a way that looks fine until the layout is not a simple row —
 * a nearest-centre pick will happily jump diagonally past the pane you were
 * pointing at.
 *
 * Coordinates below are a real arrangement each time, drawn in the comment, so
 * a failure says what the layout was rather than which numbers moved.
 */

import { describe, expect, it } from "vitest";
import { pickNeighbour, type Box } from "@/lib/spatial";

const box = (id: string, left: number, top: number, right: number, bottom: number): Box => ({
  id,
  left,
  top,
  right,
  bottom,
});

/**
 * ┌─────────┬─────────┬─────────┐
 * │    a    │    b    │    c    │
 * └─────────┴─────────┴─────────┘
 */
const ROW = [box("a", 0, 0, 100, 100), box("b", 100, 0, 200, 100), box("c", 200, 0, 300, 100)];

/**
 * ┌─────────┬─────────┐
 * │    a    │    b    │
 * ├─────────┼─────────┤
 * │    c    │    d    │
 * └─────────┴─────────┘
 */
const GRID = [
  box("a", 0, 0, 100, 100),
  box("b", 100, 0, 200, 100),
  box("c", 0, 100, 100, 200),
  box("d", 100, 100, 200, 200),
];

describe("moving between panes", () => {
  it("goes one step, not to the end", () => {
    expect(pickNeighbour(ROW, "c", "left")).toBe("b");
    expect(pickNeighbour(ROW, "a", "right")).toBe("b");
  });

  /**
   * **No wrapping**, deliberately. A cursor that jumps from the rightmost pane
   * back to the leftmost is right for a *list* and wrong for a floorplan: the
   * point of moving spatially is that the layout is something you can picture,
   * and a move that teleports across the screen breaks the picture it trades
   * on. Running out should feel like a wall.
   */
  it("stops at the edge rather than wrapping round", () => {
    expect(pickNeighbour(ROW, "a", "left")).toBeNull();
    expect(pickNeighbour(ROW, "c", "right")).toBeNull();
    expect(pickNeighbour(ROW, "a", "up")).toBeNull();
  });

  it("moves up and down as well as across", () => {
    expect(pickNeighbour(GRID, "a", "down")).toBe("c");
    expect(pickNeighbour(GRID, "d", "up")).toBe("b");
    expect(pickNeighbour(GRID, "d", "left")).toBe("c");
    expect(pickNeighbour(GRID, "a", "right")).toBe("b");
  });

  /**
   * The one a nearest-centre pick gets wrong. From `a`, `d` sits diagonally and
   * its centre may be closer than a tall neighbour's — but "right" means the
   * pane beside you, and `b` shares an edge.
   *
   * ┌────┬───────────────┐
   * │ a  │       b       │  b is tall; d is diagonal and compact
   * ├────┼──────┬────────┤
   * │ c  │      │   d    │
   * └────┴──────┴────────┘
   */
  it("prefers the pane sharing an edge over a nearer one on the diagonal", () => {
    const layout = [
      box("a", 0, 0, 50, 100),
      box("b", 50, 0, 300, 100),
      box("c", 0, 100, 50, 200),
      box("d", 60, 100, 160, 200),
    ];
    expect(pickNeighbour(layout, "a", "right")).toBe("b");
  });

  /** With nothing sharing an edge, a diagonal is better than refusing to move. */
  it("takes a diagonal when there is nothing beside you", () => {
    const layout = [box("a", 0, 0, 100, 100), box("b", 200, 200, 300, 300)];
    expect(pickNeighbour(layout, "a", "right")).toBe("b");
  });

  it("picks the nearer of two in the same direction", () => {
    expect(pickNeighbour(ROW, "a", "right")).toBe("b");
    expect(pickNeighbour([ROW[0], ROW[2]], "a", "right")).toBe("c");
  });

  it("does nothing when there is only one pane", () => {
    expect(pickNeighbour([ROW[0]], "a", "right")).toBeNull();
  });

  /** A stale id — a group closed between keystroke and handler — is not a crash. */
  it("does nothing when the active pane is not in the list", () => {
    expect(pickNeighbour(ROW, "gone", "left")).toBeNull();
    expect(pickNeighbour([], "a", "left")).toBeNull();
  });

  /**
   * Tabbed panes share a rectangle exactly. Neither is *left of* the other, and
   * answering either way would be a coin flip the user cannot predict.
   */
  it("refuses to choose between panes stacked in one place", () => {
    const stacked = [box("a", 0, 0, 100, 100), box("b", 0, 0, 100, 100)];
    expect(pickNeighbour(stacked, "a", "left")).toBeNull();
    expect(pickNeighbour(stacked, "a", "right")).toBeNull();
  });
});
