/**
 * The same properties `diff.rs` is pinned by, asserted against the port.
 *
 * Two clients drawing one diff differently is worse than either drawing it
 * badly, so these are deliberately the Rust tests rather than new ones — if the
 * ports ever disagree, one of these fails.
 */

import { describe, expect, it } from "vitest";
import { highlight, hunkStart, pairs, sideBySide, wordDiff, type Span } from "@/lib/diff";

const toks = (line: string) => highlight(line).map((p) => p.tok);
const changed = (spans: Span[]) => spans.filter((s) => s.changed).map((s) => s.text).join("");
const rebuilt = (spans: Span[]) => spans.map((s) => s.text).join("");

describe("highlighting", () => {
  /**
   * The single property that matters: colouring must never change the text. A
   * renderer that drops or duplicates characters is worse than no highlighting.
   */
  it("is lossless", () => {
    for (const line of [
      'let x = "hi"; // note',
      "fn main() { let n = 0xFF_u8; }",
      "  # python comment",
      "s = 'it\\'s escaped'",
      "",
      "   ",
      "➡ unicode ✔ and emoji 🎉",
      'let s = "unterminated',
    ]) {
      expect(highlight(line).map((p) => p.text).join("")).toBe(line);
    }
  });

  it("recognises the obvious tokens", () => {
    expect(toks("let x = 1")).toContain("keyword");
    expect(toks('x = "str"')).toContain("string");
    expect(toks("x = 42")).toContain("number");
    expect(toks("// hi")).toContain("comment");
    expect(toks("let s = Session::new()")).toContain("type");
  });

  it("does not read a # inside a string as a comment", () => {
    expect(toks('let url = "http://x/#frag";')).not.toContain("comment");
  });
});

describe("the word diff", () => {
  it("marks only what moved", () => {
    const [old, now] = wordDiff("-let timeout = 30;", "+let timeout = 60;");
    expect(changed(old)).toBe("30");
    expect(changed(now)).toBe("60");
  });

  it("is lossless in both directions", () => {
    for (const [a, b] of [
      ["-abc", "+abd"],
      ["-", "+"],
      ["-same", "+same"],
      ["-totally different", "+nothing alike here"],
      ["-a(b, c)", "+a(b, c, d)"],
    ]) {
      const [l, r] = wordDiff(a, b);
      expect(rebuilt(l)).toBe(a);
      expect(rebuilt(r)).toBe(b);
    }
  });

  it("has nothing changed when only the marker differs", () => {
    const [l, r] = wordDiff("-x", "+x");
    expect(changed(l)).toBe("");
    expect(changed(r)).toBe("");
  });

  /**
   * Regression carried over from the Rust: the first implementation compared
   * the `-`/`+` markers as content, so the common-prefix scan stopped at
   * position 0 and every replacement lit the whole line — no better than having
   * no word diff at all.
   */
  it("never widens the highlight because of the marker", () => {
    const [old, now] = wordDiff("-    self.timeout = 30;", "+    self.timeout = 60;");
    expect(changed(old)).toBe("30");
    expect(changed(now)).toBe("60");
  });
});

describe("side-by-side pairing", () => {
  it("puts a modified line opposite the one it replaced", () => {
    const rows = sideBySide([" ctx", "-old", "+new", " tail"]);
    expect(rows.map((r) => [r.left, r.right])).toEqual([
      [" ctx", " ctx"],
      ["-old", "+new"],
      [" tail", " tail"],
    ]);
  });

  it("leaves a blank opposite an unmatched leftover", () => {
    const rows = sideBySide(["-a", "-b", "+c"]);
    expect(rows.map((r) => [r.left, r.right])).toEqual([
      ["-a", "+c"],
      ["-b", null],
    ]);
  });

  /** No header, no numbers — never a guess. */
  it("numbers nothing when it was given no starting point", () => {
    const rows = sideBySide([" ctx", "-old", "+new"]);
    expect(rows.every((r) => r.leftNo === null && r.rightNo === null)).toBe(true);
  });

  it("reads the two starting lines off a hunk header", () => {
    expect(hunkStart("@@ -6,7 +6,9 @@ fn main() {")).toEqual({ old: 6, new: 6 });
    expect(hunkStart("@@ -0,0 +1,2 @@")).toEqual({ old: 0, new: 1 });
    // One-line sides drop the count entirely.
    expect(hunkStart("@@ -12 +14 @@")).toEqual({ old: 12, new: 14 });
    // A merge's combined header describes three sides, not two.
    expect(hunkStart("@@@ -1,2 -3,4 +5,6 @@@")).toBeNull();
    expect(hunkStart("not a header")).toBeNull();
  });

  /**
   * The lopsided run is the case worth pinning: four lines removed against
   * two added, and the side that ran out must not go on counting. Getting this
   * wrong puts every number after the hunk's first change off by two, which is
   * the kind of wrong you only notice after opening the file.
   */
  it("advances each side only on the lines that side has", () => {
    const rows = sideBySide([" a", "-b", "-c", "-d", "+B", " e"], { old: 10, new: 20 });
    expect(rows.map((r) => [r.leftNo, r.rightNo])).toEqual([
      [10, 20], // " a"
      [11, 21], // "-b" / "+B"
      [12, null], // "-c" against a blank
      [13, null], // "-d" against a blank
      [14, 22], // " e" — the right side skipped the two blanks
    ]);
  });

  /** The unified view needs the same pairing, as a lookup both ways. */
  it("gives the unified view the same pairs, indexed", () => {
    const p = pairs([" ctx", "-old", "+new", " tail"]);
    expect(p.get(1)).toBe(2);
    expect(p.get(2)).toBe(1);
    expect(p.has(0)).toBe(false);
  });

  /** A run broken by a context line must not pair across the break. */
  it("does not pair across a context line", () => {
    const p = pairs(["-a", " ctx", "+b"]);
    expect(p.size).toBe(0);
  });
});
