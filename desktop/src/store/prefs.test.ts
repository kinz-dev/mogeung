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
import {
  defaultPrefs,
  emptyScoped,
  loadPrefs,
  migrateSuccession,
  savePrefs,
  type ScopedPrefs,
  type SuccessionFact,
} from "@/store/prefs";

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

/**
 * The `/clear` case, reported twice — once against the egui client in July and
 * again here, because this client was ported from `prefs.rs` without the
 * function that fixed it. These mirror the Rust tests in
 * `crates/mogeung-ui/src/prefs.rs` case for case, so the two clients cannot
 * drift on what counts as a successor.
 */
describe("succession after /clear", () => {
  const at = (n: number) => new Date(Date.UTC(2026, 0, 1, 0, 0, n)).toISOString();
  const fact = (
    id: string,
    alive: boolean,
    pid: number | null,
    started: number,
    cwd = "/repo",
  ): SuccessionFact => ({ id, alive, pid, cwd, started_at: at(started) });
  const scopedWith = (patch: Partial<ScopedPrefs>): ScopedPrefs => ({ ...emptyScoped(), ...patch });

  it("carries the label, tag and pin to the successor", () => {
    const scoped = scopedWith({
      labels: { old: "api-work" },
      tags: { old: "red" },
      pinned: ["old"],
    });
    const sessions = [fact("old", false, 4242, 100), fact("new", true, 4242, 200)];
    const patch = migrateSuccession(scoped, sessions);
    expect(patch).not.toBeNull();
    expect(patch?.labels).toEqual({ new: "api-work" });
    expect(patch?.tags).toEqual({ new: "red" });
    expect(patch?.pinned).toEqual(["new"]);
    // Idempotent: a second pass over the same facts moves nothing, which is
    // what makes it safe on every `session_updated`.
    expect(migrateSuccession(scopedWith(patch!), sessions)).toBeNull();
  });

  it("never overwrites what the successor was given by hand", () => {
    const scoped = scopedWith({
      labels: { old: "stale-name", new: "fresh-name" },
      tags: { old: "red", new: "blue" },
    });
    const sessions = [fact("old", false, 1, 100), fact("new", true, 1, 200)];
    // Nothing to move — and the predecessor keeps its own rather than losing it
    // to a move that went nowhere.
    expect(migrateSuccession(scoped, sessions)).toBeNull();
  });

  it("picks the latest predecessor and ignores strangers", () => {
    const scoped = scopedWith({
      labels: { first: "renamed-early", second: "current-name", "other-pid": "unrelated" },
    });
    const patch = migrateSuccession(scoped, [
      fact("first", false, 7, 100),
      fact("second", false, 7, 200),
      fact("other-pid", false, 9, 300),
      fact("third", true, 7, 400),
      fact("no-pid", true, null, 500),
    ]);
    expect(patch?.labels).toEqual({
      third: "current-name",
      first: "renamed-early",
      "other-pid": "unrelated",
    });
  });

  it("inherits nothing from a reused pid in another directory", () => {
    const scoped = scopedWith({ labels: { old: "api-work" } });
    const sessions = [
      fact("old", false, 4242, 100, "/repo-a"),
      fact("new", true, 4242, 200, "/repo-b"),
    ];
    expect(migrateSuccession(scoped, sessions)).toBeNull();
  });

  it("requires a dead predecessor — two live sessions never trade state", () => {
    const scoped = scopedWith({ labels: { a: "mine" }, pinned: ["a"] });
    const sessions = [fact("a", true, 3, 100), fact("b", true, 3, 200)];
    expect(migrateSuccession(scoped, sessions)).toBeNull();
  });
});
