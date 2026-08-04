/**
 * The prompt is text a human pastes, so its shape is its contract: a numbered
 * list, the file and hunk header on the row, your own note under it, and the
 * changed lines fenced as `diff` so the agent reading it knows what it is
 * looking at.
 */

import { describe, expect, it } from "vitest";
import { buildPrompt, changedLines, type FlaggedHunk } from "@/lib/prompt";

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
