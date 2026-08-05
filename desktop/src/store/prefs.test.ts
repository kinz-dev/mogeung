/**
 * A preferences file is older than the build that reads it, always.
 *
 * Adding `tags` to the scoped preferences blanked the window for anyone who had
 * a file from yesterday: `scoped()` returns the stored object *itself* — it has
 * to, or the selector re-renders for ever — so the new field was `undefined` at
 * every read, the first `scoped.tags[id]` threw, and React unwound the tree.
 *
 * These pin the rule that prevents the whole class: a stored entry is completed
 * against the current shape when it is read from disk, once, at the boundary.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { defaultPrefs, emptyScoped, loadPrefs, savePrefs } from "@/store/prefs";

describe("loading preferences written by an older build", () => {
  beforeEach(() => localStorage.clear());

  it("fills a scoped field the file has never heard of", () => {
    // A scoped entry as it was written before `tags` existed.
    localStorage.setItem(
      "mogeung.prefs",
      JSON.stringify({
        scoped: { machine: { hidden: [], pinned: [], labels: {}, editorWrap: [], bookmarks: [] } },
      }),
    );
    const scoped = loadPrefs().scoped.machine;
    expect(scoped.tags).toEqual({});
    // Not merely present — every key of the current shape is there, which is
    // the property that stops the next added field repeating this.
    for (const key of Object.keys(emptyScoped())) {
      expect(scoped).toHaveProperty(key);
    }
  });

  it("keeps what the file did say", () => {
    localStorage.setItem(
      "mogeung.prefs",
      JSON.stringify({ scoped: { machine: { labels: { s1: "mine" }, tags: { s1: "red" } } } }),
    );
    const scoped = loadPrefs().scoped.machine;
    expect(scoped.labels).toEqual({ s1: "mine" });
    expect(scoped.tags).toEqual({ s1: "red" });
  });

  it("fills a top-level field the file has never heard of", () => {
    localStorage.setItem("mogeung.prefs", JSON.stringify({ theme: "light" }));
    const prefs = loadPrefs();
    expect(prefs.theme).toBe("light");
    expect(prefs.notify).toBe(defaultPrefs().notify);
  });

  it("round-trips what it wrote", () => {
    const p = defaultPrefs();
    p.scoped.machine = { ...emptyScoped(), tags: { s1: "blue" } };
    savePrefs(p);
    expect(loadPrefs().scoped.machine.tags).toEqual({ s1: "blue" });
  });

  it("falls back to the defaults on a file that is not JSON", () => {
    localStorage.setItem("mogeung.prefs", "{{{");
    expect(loadPrefs()).toEqual(defaultPrefs());
  });
});
