/**
 * Searching the preview means searching what the preview *says*. `R-B38`.
 *
 * Every case here is one the source-side search got wrong: you typed the words
 * and the syntax was in the way. The line number is the other half — a hit you
 * cannot go and edit is a hit that made you scroll.
 */

import { describe, expect, it } from "vitest";
import { renderedLines } from "@/lib/markdown";
import { best } from "@/lib/search";

const find = (src: string, q: string) =>
  renderedLines(src).filter((l) => best(q, l.text) !== null);

describe("what a Markdown source reads as", () => {
  it("drops heading marks and keeps the line they were on", () => {
    expect(renderedLines("# A heading")).toEqual([{ text: "A heading", line: 1 }]);
  });

  it("finds emphasised words the source spells with asterisks", () => {
    expect(find("this is **bold** text", "bold")).toEqual([
      { text: "this is bold text", line: 1 },
    ]);
  });

  it("finds a phrase a table splits with pipes", () => {
    const src = "| Env | Shape |\n|---|---|\n| prod | wide |";
    // "prod wide" is a phrase only once the cell walls are gone.
    expect(find(src, "prod wide")).toEqual([{ text: "prod wide", line: 3 }]);
  });

  it("drops a table's alignment row rather than emitting punctuation", () => {
    const src = "| a | b |\n|---|---|\n| c | d |";
    expect(renderedLines(src).map((l) => l.line)).toEqual([1, 3]);
  });

  it("keeps a link's words and loses its URL", () => {
    const src = "see [the design doc](https://example.com/very/long/path)";
    expect(find(src, "design doc")).toHaveLength(1);
    expect(find(src, "example.com")).toHaveLength(0);
  });

  it("counts the line a hit is really on, blank lines and all", () => {
    const src = "# Title\n\nsome prose\n\n- a bullet about egress";
    expect(find(src, "egress")).toEqual([{ text: "a bullet about egress", line: 5 }]);
  });

  /**
   * A fence is shown verbatim, so its asterisks really are on the screen.
   * Stripping them would break the search on the text people paste most.
   */
  it("leaves a fenced block exactly as it is shown", () => {
    const src = "prose\n\n```rust\nlet x = **not_emphasis;\n```";
    expect(find(src, "**not_emphasis")).toEqual([
      { text: "let x = **not_emphasis;", line: 4 },
    ]);
  });

  it("does not emit the fence markers themselves", () => {
    expect(renderedLines("```\ncode\n```").map((l) => l.text)).toEqual(["code"]);
  });

  it("strips list markers, task boxes and quotes", () => {
    const src = "- [ ] write the thing\n> a quotation\n1. first";
    expect(renderedLines(src).map((l) => l.text)).toEqual([
      "write the thing",
      "a quotation",
      "first",
    ]);
  });

  it("emits nothing for a horizontal rule or a blank line", () => {
    expect(renderedLines("\n---\n\n***\n")).toEqual([]);
  });

  it("survives an empty document without inventing a line", () => {
    expect(renderedLines("")).toEqual([]);
  });

  /** Emphasis inside a link label needs the link taken off first. */
  it("unwraps a link whose label is emphasised", () => {
    expect(renderedLines("[**bold link**](x)")).toEqual([
      { text: "bold link", line: 1 },
    ]);
  });

  it("keeps a bare underscore in an identifier", () => {
    expect(renderedLines("call some_function now")).toEqual([
      { text: "call some_function now", line: 1 },
    ]);
  });
});
