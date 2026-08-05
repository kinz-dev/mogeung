/**
 * A tag is stored as an id in a hand-editable preferences file, so the two
 * cases that matter are the unknown one and the absent one — neither may cost
 * the row its rendering.
 */

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { TAGS, tagBg, tagColor, tagLabel } from "@/lib/tags";

describe("colour tags", () => {
  it("resolves a known id to a palette variable, never a raw hex", () => {
    for (const t of TAGS) {
      expect(tagColor(t.id)).toBe(t.color);
      expect(tagBg(t.id)).toBe(t.bg);
      expect(t.color.startsWith("var(--")).toBe(true);
      expect(t.bg.startsWith("var(--")).toBe(true);
    }
  });

  /**
   * A tint named here but absent from the stylesheet renders as *no
   * background at all* — the row simply looks untagged, which is precisely
   * the complaint this was built to answer and would say nothing while
   * failing.
   */
  it("names a surface both palettes actually define", () => {
    const css = readFileSync("src/index.css", "utf8");
    const dark = css.slice(css.indexOf(':root[data-theme="dark"]'), css.indexOf(':root[data-theme="light"] {\n  --bg:'));
    const light = css.slice(css.indexOf(':root[data-theme="light"] {\n  --bg:'));
    for (const t of TAGS) {
      const name = t.bg.replace(/^var\(|\)$/g, "");
      expect(`${name} in dark: ${dark.includes(`${name}:`)}`).toBe(`${name} in dark: true`);
      expect(`${name} in light: ${light.includes(`${name}:`)}`).toBe(`${name} in light: true`);
    }
  });

  it("reads an id this build does not know as no tag", () => {
    expect(tagColor("chartreuse")).toBeNull();
    expect(tagBg("chartreuse")).toBeNull();
    expect(tagLabel("chartreuse")).toBeNull();
  });

  it("reads an absent tag as no tag", () => {
    expect(tagColor(undefined)).toBeNull();
    expect(tagColor("")).toBeNull();
    expect(tagBg(undefined)).toBeNull();
    expect(tagBg("")).toBeNull();
  });

  it("has ids that are distinct, since one overwrites another in the store", () => {
    expect(new Set(TAGS.map((t) => t.id)).size).toBe(TAGS.length);
  });
});
