/**
 * Searching notes has to say *why* something matched.
 *
 * The filter reads the whole body, so a note can match on its fourth line while
 * its first says something else — and a list that always previews line one then
 * looks like it matched at random.
 */

import { describe, expect, it } from "vitest";
import { previewOf } from "./NotesTool";

describe("previewOf", () => {
  const body = "a title line\nsomething else\negress vs ingress definition\ntrailing";

  it("shows the first line when nothing is being searched", () => {
    expect(previewOf(body, "")).toBe("a title line");
  });

  it("shows the line that matched, not the first", () => {
    expect(previewOf(body, "ingress")).toBe("egress vs ingress definition");
  });

  it("is case-insensitive, like the filter that produced the row", () => {
    expect(previewOf(body, "EGRESS")).toBe("egress vs ingress definition");
  });

  /** A row is only in the list because it matched somewhere, but the preview
   * must still be something rather than blank if that ever stops holding. */
  it("falls back to the first line rather than showing nothing", () => {
    expect(previewOf(body, "nowhere")).toBe("a title line");
    expect(previewOf("", "x")).toBe("");
  });
});
