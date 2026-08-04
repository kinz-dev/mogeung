/**
 * The outline is a line scanner and says so, but "helpful" still has to mean
 * something: these are the shapes it must not miss, and the two it must not
 * invent.
 */

import { describe, expect, it } from "vitest";
import { outline } from "@/lib/symbols";

const names = (body: string, lang: string) => outline(body, lang).map((s) => s.name);

describe("the outline", () => {
  it("finds Rust declarations, including impl blocks", () => {
    const body = [
      "pub struct Session {",
      "    id: String,",
      "}",
      "impl Session {",
      "    pub async fn label(&self) -> String {}",
      "}",
      "const MAX: usize = 4;",
      "pub mod store;",
    ].join("\n");
    expect(names(body, "rs")).toEqual(["Session", "Session", "label", "MAX", "store"]);
  });

  it("finds TypeScript types, arrow consts and functions", () => {
    const body = [
      "export interface Props {}",
      "export type Id = string;",
      "export function useThing() {}",
      "const go = async () => {};",
      "const N = 3;",
    ].join("\n");
    expect(names(body, "ts")).toEqual(["Props", "Id", "useThing", "go", "N"]);
  });

  /** `if (x) {` is the shape that a careless method rule swallows. */
  it("does not call a control-flow line a method", () => {
    const body = ["function f() {", "  if (ready) {", "  }", "}"].join("\n");
    expect(names(body, "ts")).toEqual(["f"]);
  });

  it("separates a Go method from a plain function", () => {
    const body = ["func (s *Server) Run() {}", "func New() *Server {}"].join("\n");
    const got = outline(body, "go");
    expect(got.map((s) => [s.name, s.kind])).toEqual([
      ["Run", "method"],
      ["New", "function"],
    ]);
  });

  it("takes Python nesting from its indentation", () => {
    const body = ["class A:", "    def method(self):", "        pass"].join("\n");
    const got = outline(body, "py");
    expect(got.map((s) => [s.name, s.depth])).toEqual([
      ["A", 0],
      ["method", 1],
    ]);
  });

  it("reads Markdown headings at their level", () => {
    const got = outline("# Title\n\n## Part\ntext\n### Bit", "md");
    expect(got.map((s) => [s.name, s.depth])).toEqual([
      ["Title", 1],
      ["Part", 2],
      ["Bit", 3],
    ]);
  });

  /**
   * Documentation is full of shell transcripts, and `# comment` inside a fence
   * is not a heading. Without this the outline of any README is mostly noise.
   */
  it("ignores headings inside a fenced code block", () => {
    const got = outline(["# Real", "```sh", "# not a heading", "```", "## Also real"].join("\n"), "md");
    expect(got.map((s) => s.name)).toEqual(["Real", "Also real"]);
  });

  it("gives an unknown language nothing rather than a guess", () => {
    expect(outline("SELECT 1;", "sql")).toEqual([]);
    expect(outline("anything", "")).toEqual([]);
  });

  it("clips a pathological file instead of hanging", () => {
    const body = Array.from({ length: 5000 }, (_, i) => `fn f${i}() {}`).join("\n");
    expect(outline(body, "rs")).toHaveLength(2000);
  });
});
