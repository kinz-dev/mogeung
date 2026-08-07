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
  exportFilename,
  exportPayload,
  loadPrefs,
  migrateSuccession,
  savePrefs,
  successions,
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
 * The `/clear` case, reported four times — twice against the egui client in
 * July, again here in August because this client was ported from `prefs.rs`
 * without the function that fixed it, and a fourth time on 2026-08-07 when a
 * terminal left open for two days accumulated a *line* of dead ids and the
 * ordering that picks the predecessor turned out to be no ordering at all.
 *
 * These carry the Rust cases forward — that client is gone (ADR-0020), so this
 * is now the only definition of what counts as a successor.
 */
describe("succession after /clear", () => {
  const at = (n: number) => new Date(Date.UTC(2026, 0, 1, 0, 0, n)).toISOString();
  const fact = (
    id: string,
    alive: boolean,
    pid: number | null,
    active: number,
    cwd = "/repo",
  ): SuccessionFact => ({ id, alive, pid, cwd, last_event_at: at(active) });
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

  /**
   * The 2026-08-07 report, and the reason it took four goes. A terminal open
   * since Tuesday has a `/clear` for every topic it has been through, so the
   * live session trails a *line* of dead ids — and the daemon fills
   * `started_at` from the live registry, where it means when the **process**
   * started. Every id in the line therefore claimed the same `started_at`, the
   * comparison meant to find the newest one always tied, and the tie kept the
   * oldest. The label moved onto a session that had ended two clears ago.
   */
  it("follows the line by activity, not by a process clock they all share", () => {
    const scoped = scopedWith({ labels: { second: "api-work" } });
    // What the daemon actually reports: one pid, one cwd, four ids, and a
    // `started_at` that cannot tell them apart — so it is not consulted.
    const patch = migrateSuccession(scoped, [
      fact("first", false, 26514, 100),
      fact("second", false, 26514, 300),
      fact("third", false, 26514, 200),
      fact("live", true, 26514, 400),
    ]);
    expect(patch?.labels).toEqual({ live: "api-work" });
  });

  it("reaches past a hop no window was open to make", () => {
    // The label is two ids back: this window was closed for the `/clear` that
    // ended `first`, so nothing moved it onto `second` at the time. Insisting
    // on the immediate predecessor stranded it for good.
    const scoped = scopedWith({ labels: { first: "api-work" }, pinned: ["first"] });
    const patch = migrateSuccession(scoped, [
      fact("first", false, 9, 100),
      fact("second", false, 9, 200),
      fact("live", true, 9, 300),
    ]);
    expect(patch?.labels).toEqual({ live: "api-work" });
    expect(patch?.pinned).toEqual(["live"]);
  });

  it("leaves one pin on the line's live head, not one per /clear", () => {
    const scoped = scopedWith({ pinned: ["first", "second", "elsewhere"] });
    const patch = migrateSuccession(scoped, [
      fact("first", false, 9, 100),
      fact("second", false, 9, 200),
      fact("live", true, 9, 300),
    ]);
    expect(patch?.pinned).toEqual(["elsewhere", "live"]);
  });

  it("maps the whole line to its heir, for the callers that move a view", () => {
    const heirs = successions([
      fact("first", false, 9, 100),
      fact("second", false, 9, 200),
      fact("live", true, 9, 300),
      fact("stranger", false, 11, 400),
      fact("elsewhere", true, 9, 500, "/other"),
    ]);
    expect(Object.fromEntries(heirs)).toEqual({ first: "live", second: "live" });
  });
});

/**
 * A backup for the things you wrote. `ADR-0023`.
 *
 * That ADR kept the judgements in the client and named what it accepted: they
 * live in a webview's `localStorage`, so clearing it loses every label, tag and
 * pin silently. This is the cheap half of the answer, and the property worth
 * pinning is that the file is *complete* — a backup that restores some settings
 * and quietly not others is a worse promise than either whole answer.
 */
describe("exporting your preferences", () => {
  const when = new Date("2026-08-07T09:30:00.000Z");

  it("carries the whole preferences object, judgements included", () => {
    const prefs = defaultPrefs();
    prefs.scoped = {
      "machine-a": { ...emptyScoped(), labels: { s1: "the migration" }, tags: { s1: "red" }, pinned: ["s1"] },
    };
    const payload = exportPayload(prefs, when);

    expect(payload.prefs.scoped["machine-a"].labels.s1).toBe("the migration");
    expect(payload.prefs.scoped["machine-a"].tags.s1).toBe("red");
    expect(payload.prefs.scoped["machine-a"].pinned).toEqual(["s1"]);
    // ...and the rest of the object, not a hand-picked subset.
    expect(payload.prefs.theme).toBe(prefs.theme);
    expect(payload.prefs.keymap).toEqual(prefs.keymap);
  });

  /** So an importer — which does not exist yet — can refuse a file that is not this. */
  it("says what it is and which shape it is in", () => {
    const payload = exportPayload(defaultPrefs(), when);
    expect(payload.kind).toBe("mogeung.prefs");
    expect(payload.version).toBe(1);
    expect(payload.exported).toBe("2026-08-07T09:30:00.000Z");
  });

  it("round-trips through JSON, which is how it will actually travel", () => {
    const prefs = defaultPrefs();
    prefs.scoped = { m: { ...emptyScoped(), labels: { s: "a label" } } };
    const back = JSON.parse(JSON.stringify(exportPayload(prefs, when)));
    expect(back.prefs.scoped.m.labels.s).toBe("a label");
  });

  it("names the file by the day, so a folder of them sorts", () => {
    expect(exportFilename(when)).toBe("mogeung-preferences-2026-08-07.json");
  });
});
