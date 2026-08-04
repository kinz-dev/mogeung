/**
 * The same properties `diff.rs` is pinned by, asserted against the port.
 *
 * Two clients drawing one diff differently is worse than either drawing it
 * badly, so these are deliberately the Rust tests rather than new ones — if the
 * ports ever disagree, one of these fails.
 */

import { describe, expect, it } from "vitest";
import { highlight, pairs, sideBySide, wordDiff, type Span } from "@/lib/diff";

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
    expect(rows).toEqual([
      { left: " ctx", right: " ctx" },
      { left: "-old", right: "+new" },
      { left: " tail", right: " tail" },
    ]);
  });

  it("leaves a blank opposite an unmatched leftover", () => {
    const rows = sideBySide(["-a", "-b", "+c"]);
    expect(rows).toEqual([
      { left: "-a", right: "+c" },
      { left: "-b", right: null },
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
