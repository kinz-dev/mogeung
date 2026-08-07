/**
 * `dirTail` — a directory shortened to fit a header without becoming a
 * different directory. Added with the folder path on the pane header,
 * 2026-08-07.
 */

import { describe, expect, it } from "vitest";
import { dirTail } from "@/lib/format";

describe("dirTail", () => {
  it("leaves a path that already fits completely alone", () => {
    expect(dirTail("/home/kinz/mogeung")).toBe("/home/kinz/mogeung");
  });

  /** The end identifies it; a head-truncated path identifies nothing. */
  it("drops leading segments, keeping the end", () => {
    const out = dirTail("/Users/someone/work/clients/acme/services/api");
    expect(out.startsWith("…/")).toBe(true);
    expect(out.endsWith("services/api")).toBe(true);
    expect(out.length).toBeLessThanOrEqual(34);
  });

  /**
   * Segments come off whole. A half-cut name reads as a directory that exists,
   * which is worse than an ellipsis.
   */
  it("never cuts a directory name in half", () => {
    const out = dirTail("/aaa/bbbbbbbb/ccccccccc/ddddddddd/eeeeeeeee");
    for (const part of out.replace("…/", "").split("/")) {
      expect("/aaa/bbbbbbbb/ccccccccc/ddddddddd/eeeeeeeee".split("/")).toContain(part);
    }
  });

  /** Nothing left to drop: the leaf comes back whole and CSS deals with it. */
  it("returns an over-long leaf rather than mangling it", () => {
    const leaf = "a-directory-with-a-truly-unreasonable-name";
    expect(dirTail(`/home/kinz/${leaf}`)).toBe(`…/${leaf}`);
  });
});
