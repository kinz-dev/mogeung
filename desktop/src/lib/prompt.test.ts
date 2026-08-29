/**
 * The prompt is text a human pastes, so its shape is its contract: a numbered
 * list, the file and hunk header on the row, your own note under it, and the
 * changed lines fenced as `diff` so the agent reading it knows what it is
 * looking at.
 */

import { describe, expect, it } from "vitest";
import {
  buildPrompt,
  changedLines,
  draftAsk,
  draftLinesPerHunk,
  MIN_DRAFT_LINES_PER_HUNK,
  TOTAL_DRAFT_LINES,
  type FlaggedHunk,
} from "@/lib/prompt";

const hunk = (patch: Partial<FlaggedHunk> = {}): FlaggedHunk => ({
  sessionId: "s",
  path: "src/auth.rs",
  header: "@@ -10,7 +10,9 @@",
  note: "",
  body: [],
  ...patch,
});

describe("the follow-up prompt", () => {
  it("numbers the flags and names each file", () => {
    const text = buildPrompt("", [hunk(), hunk({ path: "src/db.rs" })]);
    expect(text).toContain("1. `src/auth.rs` @@ -10,7 +10,9 @@");
    expect(text).toContain("2. `src/db.rs`");
  });

  it("leads with your own words when you wrote any", () => {
    const text = buildPrompt("  these need error handling  ", [hunk()]);
    expect(text.startsWith("these need error handling\n\n")).toBe(true);
  });

  it("says nothing where a note is blank rather than leaving an empty line", () => {
    expect(buildPrompt("", [hunk({ note: "   " })])).not.toContain("\n   \n");
  });

  it("fences the quoted lines as a diff", () => {
    const text = buildPrompt("", [hunk({ body: ["-old", "+new"] })]);
    expect(text).toContain("```diff\n-old\n+new\n```");
  });

  /** Context lines are lookup-able; the changed ones are the question. */
  it("quotes only what changed", () => {
    expect(changedLines([" ctx", "-old", "+new", " tail"])).toEqual(["-old", "+new"]);
  });
});

describe("what the model is asked, when it drafts", () => {
  it("carries your words verbatim and each flag's own note", () => {
    const ask = draftAsk("no new deps", [hunk({ note: "this leaks on the error path" })]);
    expect(ask).toContain("no new deps");
    expect(ask).toContain("their note: this leaks on the error path");
    expect(ask).toContain("src/auth.rs @@ -10,7 +10,9 @@");
  });

  /**
   * The answer is pasted into somebody's terminal. A model that opens with
   * "Here is a draft:" has put a sentence into their session that they did not
   * write, so the output contract is the load-bearing half of this prompt.
   */
  it("asks for the instruction and nothing around it", () => {
    const ask = draftAsk("", [hunk()]);
    expect(ask).toContain("Answer with the instruction itself and nothing else");
    expect(ask).toContain("Do not invent work they did not");
  });

  /**
   * `R-O3` paid 78 seconds and an empty answer for this: a per-item cap is not
   * a bound while the item count is unbounded.
   */
  it("bounds the whole ask rather than only each hunk", () => {
    const long = Array.from({ length: 200 }, (_, i) => `+line ${i}`);
    const many = Array.from({ length: 40 }, () => hunk({ body: long }));
    const quoted = draftAsk("", many)
      .split("\n")
      .filter((l) => l.startsWith("+line ")).length;
    expect(quoted).toBeLessThanOrEqual(TOTAL_DRAFT_LINES);
    // …and never so little that a hunk says nothing at all.
    expect(draftLinesPerHunk(500)).toBe(MIN_DRAFT_LINES_PER_HUNK);
  });

  /** What was cut has to be visible as a cut, or the draft is written from a
   *  hunk the model believes it saw whole. */
  it("says how much of a hunk it left out", () => {
    const ask = draftAsk("", [hunk({ body: Array.from({ length: 50 }, (_, i) => `+${i}`) })]);
    expect(ask).toContain("more changed line(s)");
  });
});
