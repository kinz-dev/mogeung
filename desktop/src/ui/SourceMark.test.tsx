/**
 * The mark that says which CLI a session is. `R-J49`.
 *
 * Two things worth pinning, and neither is "it renders an icon". The mark is
 * **wordless** — that was the second ask on the day, and a helpful hand adding
 * the label back is exactly the kind of change that looks like an improvement
 * in a diff. And an **unknown** source names itself rather than falling through
 * to Claude's glyph and Claude's colour: the daemon can be newer than the
 * window, and a Gemini session drawn as a Claude one is a lie the queue would
 * tell silently.
 */

import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { SourceMark, sourceIcon } from "@/ui/SourceMark";

describe("the source mark", () => {
  it("carries no text — the name is on hover", () => {
    const { container } = render(<SourceMark source="qwen_code" />);
    expect(container.textContent).toBe("");
    expect(screen.getByTitle("a qwen session")).toBeInTheDocument();
  });

  it("gives each CLI its own glyph, so colour is not doing the work alone", () => {
    const glyphs = ["claude_code", "codex", "qwen_code"].map(sourceIcon);
    expect(new Set(glyphs).size).toBe(3);
  });

  /** A CLI this build has never heard of says what it is. */
  it("does not draw an unknown source as Claude", () => {
    expect(sourceIcon("something_new")).not.toBe(sourceIcon("claude_code"));
    render(<SourceMark source={"something_new" as never} />);
    expect(screen.getByTitle("a something_new session")).toBeInTheDocument();
  });
});
