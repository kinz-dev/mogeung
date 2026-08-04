/**
 * The design system's own guard rail.
 *
 * A design system is not a file of tokens — it is the property that nobody is
 * quietly writing `text-[11.5px]` next to it. That property decays silently,
 * one hurried component at a time, and by the time it is visible it costs a
 * sweep to fix. This is the cheapest way to keep it.
 *
 * The counts these assert against are what the sweep left behind: **zero**. If
 * a genuinely new value is needed, add it to the scale in `index.css` — that is
 * the whole point of the failure.
 */

import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

function sources(dir = "src"): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...sources(path));
    else if (/\.tsx?$/.test(path) && !path.endsWith(".test.ts") && !path.endsWith(".test.tsx")) {
      out.push(path);
    }
  }
  return out;
}

/**
 * Every opening `<tag …>` in a source file, attributes and all.
 *
 * Not a regex. `<button onClick={() => …}>` contains a `>` inside an arrow
 * function, so the obvious non-greedy match stops before it ever reaches
 * `className` — and a check that silently truncates its input reports failures
 * that are its own. Brace depth is what distinguishes the tag's `>` from JSX
 * expression content.
 */
function openingTags(src: string, name: string): string[] {
  const out: string[] = [];
  const open = new RegExp(`<${name}\\b`, "g");
  let m: RegExpExecArray | null;
  while ((m = open.exec(src)) !== null) {
    let depth = 0;
    for (let i = m.index; i < src.length; i++) {
      const c = src[i];
      if (c === "{") depth++;
      else if (c === "}") depth--;
      else if (c === ">" && depth === 0) {
        out.push(src.slice(m.index, i + 1));
        break;
      }
    }
  }
  return out;
}

function offenders(pattern: RegExp): string[] {
  const found: string[] = [];
  for (const path of sources()) {
    for (const line of readFileSync(path, "utf8").split("\n")) {
      const m = line.match(pattern);
      if (m) found.push(`${path}: ${m[0]}`);
    }
  }
  return found;
}

describe("the type scale", () => {
  /**
   * Six ad-hoc sizes — 9, 10, 11, 11.5, 12 and 13px — is what this codebase
   * held before there was a scale. `text-[11.5px]` in particular is the tell:
   * a half-pixel nobody chose, arrived at by nudging.
   */
  it("has no ad-hoc font sizes left", () => {
    expect(offenders(/text-\[\d+(\.\d+)?px\]/)).toEqual([]);
  });
});

describe("the radius scale", () => {
  it("has no ad-hoc corner radii left", () => {
    expect(offenders(/rounded-\[\d+px\]/)).toEqual([]);
  });
});

describe("colour", () => {
  /**
   * Every colour goes through `R-J6`'s two hand-tested palettes. A raw hex is
   * a colour that exists in exactly one theme and was checked against neither.
   */
  it("never hard-codes a hex", () => {
    expect(offenders(/(?:bg|text|border|outline|fill|stroke)-\[#[0-9a-fA-F]{3,8}\]/)).toEqual([]);
  });
});

describe("focus", () => {
  /**
   * The rule this whole pass existed for.
   *
   * `A13` — keyboard-first — is `SUPPORTED`, and the app shipped with **zero**
   * `focus-visible` styles. A control you can tab to but cannot see is the
   * product not working, so every `<button>` has to carry a ring: from the
   * shared `interactive` vocabulary, or spelled out.
   */
  it("gives every button a visible focus state", () => {
    const bad: string[] = [];
    for (const path of sources()) {
      const src = readFileSync(path, "utf8");
      for (const tag of openingTags(src, "button")) {
        const styled = /focus-visible/.test(tag) || /\binteractive\b/.test(tag);
        if (!styled) bad.push(`${path}: ${tag.replace(/\s+/g, " ").slice(0, 90)}`);
      }
    }
    expect(bad).toEqual([]);
  });
});
