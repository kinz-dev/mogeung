/**
 * One query, several engines, and the winner says its name. `R-F10`.
 *
 * The row asked for substring, regex, `rg` and fuzzy running together with the
 * best answer winning. Two of those questions were settled while `R-B36` was
 * built — no external binaries, one scale — and what these pin is the part that
 * is easy to get subtly wrong: that an unsearched list keeps the order the
 * daemon gave it, and that a searched one is ordered by the score rather than
 * by luck.
 */

import { describe, expect, it } from "vitest";
import { rank, winner } from "@/lib/search";

const items = ["commit the changes", "q-commit helper", "read the transcript", "commit"];
const id = (s: string) => s;

describe("ranking a list", () => {
  /**
   * The trap this exists to avoid: scoring an unqueried list gives every item
   * the same number, and the sort then throws away an order the daemon computed
   * from counts or recency.
   */
  it("returns everything in its original order when nothing is being searched", () => {
    expect(rank(items, "", id).map((r) => r.item)).toEqual(items);
    expect(rank(items, "   ", id).every((r) => r.hit === null)).toBe(true);
  });

  it("drops what does not match", () => {
    expect(rank(items, "transcript", id).map((r) => r.item)).toEqual(["read the transcript"]);
  });

  /**
   * Position, not length. `exactScore` rewards an early match and a word
   * boundary and is deliberately blind to how much text follows — so `commit`
   * and `commit the changes` tie, and the daemon's order breaks the tie. What
   * is asserted is the part that is actually decided here: a hit at the start
   * of a string beats the same hit buried in the middle of another.
   */
  it("puts an early match above a buried one", () => {
    const ranked = rank(["please go and commit that", "commit that now"], "commit", id);
    expect(ranked[0].item).toBe("commit that now");
  });

  /** A subsequence match is the reason a plain `includes` was not enough. */
  it("finds a hyphenated name from the letters you typed", () => {
    const hits = rank(items, "qcommit", id);
    expect(hits.map((r) => r.item)).toContain("q-commit helper");
    expect(hits[0].hit?.engine).toBe("fuzzy");
  });

  /**
   * Which engine wins here is `best`'s business and is not asserted — for a
   * long query the subsequence scorer usually outscores the all-words one, and
   * pinning that would be this test owning someone else's ranking. What matters
   * to `R-F10` is that the row is *found at all*, which the `.includes()` filter
   * this replaced could not do.
   */
  it("finds two words that are both there but not adjacent", () => {
    const ranked = rank(["the changes were committed"], "changes committed", id);
    expect(ranked).toHaveLength(1);
    expect(ranked[0].hit?.engine).toBeTruthy();
  });

  it("names the engine that decided the order", () => {
    expect(winner(rank(items, "commit", id))).toBe("exact");
    expect(winner(rank(items, "zzzz", id))).toBeNull();
    expect(winner(rank(items, "", id))).toBeNull();
  });

  it("searches whatever the accessor concatenates", () => {
    const rows = [{ name: "a", desc: "about egress" }];
    expect(rank(rows, "egress", (r) => `${r.name} ${r.desc}`)).toHaveLength(1);
    expect(rank(rows, "egress", (r) => r.name)).toHaveLength(0);
  });

  it("survives an empty list without inventing a winner", () => {
    expect(rank([], "anything", id)).toEqual([]);
    expect(winner([])).toBeNull();
  });
});
