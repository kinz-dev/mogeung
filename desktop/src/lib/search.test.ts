import { describe, expect, it } from "vitest";
import { best, searchMove, scorePath } from "./search";

describe("searchMove", () => {
  /**
   * The rule that makes one Enter key mean two things safely: while the arrows
   * are in the query box, Enter runs the search; once they have walked into the
   * results, Enter opens the row. So "am I in the list" has to be exactly right
   * at both boundaries.
   *
   * The boundary that matters most is the top. Off the top must **leave** the
   * list rather than wrap, or there is no keyboard way back to the query you
   * are in the middle of refining — you would have to reach for the mouse to
   * edit the thing you are typing.
   */
  it("can always get back to the query", () => {
    expect(searchMove(false, null, 5, true)).toEqual([true, 0]);
    // ...and from a stale mouse selection, still at the top.
    expect(searchMove(false, 3, 5, true)).toEqual([true, 0]);
    expect(searchMove(true, 0, 5, true)).toEqual([true, 1]);
    // The bottom stops rather than wrapping round to the top.
    expect(searchMove(true, 4, 5, true)).toEqual([true, 4]);
    expect(searchMove(true, 2, 5, false)).toEqual([true, 1]);
    // The top hands the arrows back, keeping what was selected.
    expect(searchMove(true, 0, 5, false)).toEqual([false, 0]);
    // Up while already in the box is not a way *into* the list.
    expect(searchMove(false, 2, 5, false)).toEqual([false, 2]);
  });

  /** Never claim to be in a list that is not there, or Enter silently stops
   * running searches. */
  it("is never in an empty list", () => {
    expect(searchMove(true, 0, 0, true)).toEqual([false, null]);
    expect(searchMove(false, null, 0, false)).toEqual([false, null]);
  });
});

describe("best", () => {
  it("names the engine that won, rather than matching silently", () => {
    const hit = best("watch root", "we inject the watch root at startup");
    expect(hit?.engine).toBe("exact");
  });

  it("falls through to fuzzy when nothing contiguous matches", () => {
    const hit = best("wroot", "the watch root");
    expect(hit?.engine).toBe("fuzzy");
  });

  it("returns null rather than a zero score when nothing matches", () => {
    expect(best("zzz", "the watch root")).toBeNull();
    expect(best("   ", "anything")).toBeNull();
  });

  /**
   * ripgrep's smart case, because that is what the hands expect: a lower-case
   * query matches anything, an upper-case letter means you meant it.
   */
  it("is case-insensitive until the query says otherwise", () => {
    expect(best("watch", "WATCH ROOT")).not.toBeNull();
    expect(best("WATCH", "watch root")).toBeNull();
  });
});

describe("scorePath", () => {
  /** The basename is what you are usually typing, so it has to outrank a
   * directory that happens to contain the same letters. */
  it("prefers a match in the file name over one in the directory", () => {
    const inName = scorePath("wire", "crates/core/src/wire.rs")!;
    const inDir = scorePath("wire", "wire/src/something-else.rs")!;
    expect(inName).toBeGreaterThan(inDir);
  });

  it("rejects a path that does not contain the letters in order", () => {
    expect(scorePath("zqx", "crates/core/src/wire.rs")).toBeNull();
  });
});
