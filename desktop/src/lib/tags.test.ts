/**
 * A tag is stored as an id in a hand-editable preferences file, so the two
 * cases that matter are the unknown one and the absent one — neither may cost
 * the row its rendering.
 */

import { describe, expect, it } from "vitest";
import { TAGS, tagColor, tagLabel } from "@/lib/tags";

describe("colour tags", () => {
  it("resolves a known id to a palette variable, never a raw hex", () => {
    for (const t of TAGS) {
      expect(tagColor(t.id)).toBe(t.color);
      expect(t.color.startsWith("var(--")).toBe(true);
    }
  });

  it("reads an id this build does not know as no tag", () => {
    expect(tagColor("chartreuse")).toBeNull();
    expect(tagLabel("chartreuse")).toBeNull();
  });

  it("reads an absent tag as no tag", () => {
    expect(tagColor(undefined)).toBeNull();
    expect(tagColor("")).toBeNull();
  });

  it("has ids that are distinct, since one overwrites another in the store", () => {
    expect(new Set(TAGS.map((t) => t.id)).size).toBe(TAGS.length);
  });
});
