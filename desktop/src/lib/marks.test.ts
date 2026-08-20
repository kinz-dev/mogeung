/**
 * Numbered bookmarks. `R-J37`.
 *
 * Asked for in IntelliJ's own terms, so the rules here are IntelliJ's — with
 * one deliberate softening, in the third test.
 */

import { beforeEach, describe, expect, it } from "vitest";
import {
  cursorIn,
  forgetCursor,
  markFor,
  MNEMONICS,
  noteCursor,
  setMnemonic,
  toggleMark,
  type Mark,
} from "@/lib/marks";

const m = (line: number, digit?: string): Mark =>
  digit ? ["s1", "src/a.ts", line, digit] : ["s1", "src/a.ts", line];

describe("a bookmark's digit", () => {
  it("lands on a line that had no mark at all", () => {
    expect(setMnemonic([], "s1", "src/a.ts", 12, "3")).toEqual([m(12, "3")]);
  });

  it("is given to a line that was already marked, rather than marking it twice", () => {
    const out = setMnemonic([m(12)], "s1", "src/a.ts", 12, "3");
    expect(out).toEqual([m(12, "3")]);
  });

  /**
   * The softening. IntelliJ drops the whole bookmark when a mnemonic moves
   * away; here the line keeps its plain mark, because that mark may have been
   * made by hand in the margin long before — and a chord that silently deletes
   * something you did not ask it to touch is the worse surprise.
   */
  it("moves, and the line it leaves keeps its mark", () => {
    const out = setMnemonic([m(12, "3"), m(40)], "s1", "src/a.ts", 40, "3");
    expect(out).toEqual([m(12), m(40, "3")]);
  });

  it("comes off when the same digit is set on the same line again", () => {
    const out = setMnemonic([m(12, "3")], "s1", "src/a.ts", 12, "3");
    expect(out).toEqual([m(12)]);
    expect(markFor(out, "3")).toBeNull();
  });

  it("never names two lines at once", () => {
    let marks: Mark[] = [];
    for (const line of [10, 20, 30]) marks = setMnemonic(marks, "s1", "src/a.ts", line, "7");
    expect(marks.filter((x) => x[3] === "7")).toHaveLength(1);
    expect(markFor(marks, "7")).toEqual(m(30, "7"));
  });

  it("leaves other digits alone", () => {
    const out = setMnemonic([m(1, "1"), m(2, "2")], "s1", "src/a.ts", 9, "3");
    expect(markFor(out, "1")).toEqual(m(1, "1"));
    expect(markFor(out, "2")).toEqual(m(2, "2"));
  });

  /** Nine, not ten: `$mod+0` and `$mod+Shift+0` are the two zoom resets. */
  it("stops at nine", () => {
    expect(MNEMONICS).toEqual(["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    expect(MNEMONICS).not.toContain("0");
  });
});

describe("a plain mark in the margin", () => {
  it("toggles on and off, and keeps the shape a digit can be added to", () => {
    const on = toggleMark([], "s1", "src/a.ts", 5);
    expect(on).toEqual([m(5)]);
    expect(toggleMark(on, "s1", "src/a.ts", 5)).toEqual([]);
  });

  it("does not disturb a numbered mark on another line", () => {
    const out = toggleMark([m(5, "4")], "s1", "src/a.ts", 9);
    expect(markFor(out, "4")).toEqual(m(5, "4"));
  });
});

/**
 * The cursor the set-chord acts on. A chord fires wherever the keyboard is —
 * including over a terminal — so the position is only usable while its own
 * pane is the active one.
 */
describe("where the cursor is", () => {
  beforeEach(() => forgetCursor("file:s1::src/a.ts"));

  it("answers only for the pane the keyboard is in", () => {
    noteCursor("file:s1::src/a.ts", "s1", "src/a.ts", 42);
    expect(cursorIn("file:s1::src/a.ts")?.line).toBe(42);
    expect(cursorIn("agent")).toBeNull();
    expect(cursorIn(null)).toBeNull();
  });

  it("is forgotten when its pane goes, so a closed file cannot be bookmarked", () => {
    noteCursor("file:s1::src/a.ts", "s1", "src/a.ts", 42);
    forgetCursor("file:s1::src/a.ts");
    expect(cursorIn("file:s1::src/a.ts")).toBeNull();
  });
});
