/**
 * A tag is stored as an id in a hand-editable preferences file, so the two
 * cases that matter are the unknown one and the absent one — neither may cost
 * the row its rendering.
 */

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { TAGS, tagBg, tagColor, tagLabel, compareByTagThenLabel, type Scoped } from "@/lib/tags";

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

describe("the order the queue is asked for", () => {
  const scoped = (over: Partial<Scoped> = {}): Scoped => ({
    pinned: [],
    tags: {},
    labels: {},
    ...over,
  });

  /** Sorting the ids through the real comparator, which is what the queue does
   *  — a list of assertions about pairs would not catch a broken chain. */
  const order = (ids: string[], s: Scoped) => [...ids].sort((a, b) => compareByTagThenLabel(a, b, s));

  it("puts a coloured session above an uncoloured one", () => {
    const s = scoped({ tags: { b: "blue" } });
    expect(order(["a", "b", "c"], s)).toEqual(["b", "a", "c"]);
  });

  it("orders colours by the palette, not by their names", () => {
    // Alphabetically this would be amber, blue, red. The strip down the list
    // has to read the same way every time, so palette order wins.
    const s = scoped({ tags: { x: "blue", y: "red", z: "amber" } });
    expect(order(["x", "y", "z"], s)).toEqual(["y", "z", "x"]);
  });

  it("puts a labelled session above an unlabelled one, under the colours", () => {
    const s = scoped({ tags: { red: "red" }, labels: { lab: "api" } });
    expect(order(["plain", "lab", "red"], s)).toEqual(["red", "lab", "plain"]);
  });

  it("orders two labels alphabetically", () => {
    const s = scoped({ labels: { a: "zebra", b: "apple" } });
    expect(order(["a", "b"], s)).toEqual(["b", "a"]);
  });

  it("keeps a pin above a colour — a pin is the stronger statement", () => {
    const s = scoped({ pinned: ["p"], tags: { r: "red" } });
    expect(order(["r", "p"], s)).toEqual(["p", "r"]);
  });

  /**
   * The cost of this ordering, pinned deliberately.
   *
   * Two sessions that agree on pin, colour and label must compare **equal**, so
   * the daemon's attention rank — the order they arrive in — survives. If this
   * ever returns non-zero, the queue silently stops being ranked by who needs
   * you, which is the product rather than a feature of it.
   */
  it("says nothing about two sessions with no colour and no label", () => {
    expect(compareByTagThenLabel("a", "b", scoped())).toBe(0);
    expect(compareByTagThenLabel("b", "a", scoped())).toBe(0);
  });

  it("says nothing about two sessions wearing the same colour and label", () => {
    const s = scoped({ tags: { a: "red", b: "red" }, labels: { a: "api", b: "api" } });
    expect(compareByTagThenLabel("a", "b", s)).toBe(0);
  });

  it("reads an unknown colour as no colour rather than sorting it anywhere odd", () => {
    const s = scoped({ tags: { known: "green", odd: "chartreuse" } });
    expect(order(["odd", "known"], s)).toEqual(["known", "odd"]);
  });
});
