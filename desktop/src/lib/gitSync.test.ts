/**
 * The arithmetic of a sync's balloon. `R-J109`. What it draws, and how a run
 * starts and ends through the store, is `SyncBalloon.test.tsx`.
 */

import { describe, expect, it } from "vitest";
import {
  NEWS_MS,
  QUIET_MS,
  STALE_MS,
  TROUBLE_MS,
  beginRun,
  expiresAt,
  failRun,
  finishRun,
  headline,
  moved,
  repoName,
  runningFor,
  splitFetchLines,
  standing,
  type SyncRun,
} from "@/lib/gitSync";

const T = 1_800_000_000_000;
const answer = (over: Partial<Parameters<typeof finishRun>[1]> = {}) => ({
  session_id: "s1",
  updates: [],
  upstream: "origin/main",
  ahead: 0,
  behind: 0,
  ...over,
});
const done = (over: Partial<SyncRun> = {}): SyncRun => ({
  ...finishRun(beginRun([], { session: "s1", name: "mogeung", at: T }), answer(), T + 1000)[0],
  ...over,
});

describe("a run", () => {
  it("one per repository: a new sync of the same session replaces the old, and a few stand at most", () => {
    let runs = beginRun([], { session: "s1", name: "a", at: T });
    const first = runs[0].id;
    runs = beginRun(runs, { session: "s1", name: "a", at: T + 5 });
    expect(runs).toHaveLength(1);
    expect(runs[0].id).not.toBe(first);
    for (const s of ["s2", "s3", "s4"]) runs = beginRun(runs, { session: s, name: s, at: T });
    expect(runs.map((r) => r.session)).toEqual(["s2", "s3", "s4"]);
  });

  it("finishes only a sync this window started — git_fetched is broadcast to every window", () => {
    const runs = beginRun([], { session: "s1", name: "a", at: T });
    expect(finishRun(runs, answer({ session_id: "s2" }), T + 1)).toEqual(runs);
    expect(finishRun([], answer(), T + 1)).toEqual([]);
    const [r] = finishRun(runs, answer({ behind: 2 }), T + 1);
    expect(r).toMatchObject({ phase: "done", endedAt: T + 1, behind: 2, upstream: "origin/main" });
  });

  it("fails with git's own words", () => {
    const [r] = failRun(beginRun([], { session: "s1", name: "a", at: T }), "s1", "fatal: could not read", T + 1);
    expect(r).toMatchObject({ phase: "failed", error: "fatal: could not read" });
  });

  it("a run whose answer went astray stops blocking the next press", () => {
    const runs = beginRun([], { session: "s1", name: "a", at: T });
    expect(runningFor(runs, "s1", T + 1000)).not.toBeNull();
    expect(runningFor(runs, "s1", T + STALE_MS)).toBeNull();
    expect(runningFor(runs, "s2", T)).toBeNull();
  });
});

describe("what git said", () => {
  it("git's own prefixes mark what was not a ref update", () => {
    const { updates, warnings } = splitFetchLines([
      "   1a2b3c4..5d6e7f8  main       -> origin/main",
      "warning: redirecting to https://example.test/repo.git/",
      " * [new branch]      feature    -> origin/feature",
      "error: some local refs could not be updated",
    ]);
    expect(updates).toHaveLength(2);
    expect(warnings).toEqual([
      "warning: redirecting to https://example.test/repo.git/",
      "error: some local refs could not be updated",
    ]);
  });
});

describe("the balloon's words", () => {
  it("names the phase and the repository", () => {
    expect(headline(beginRun([], { session: "s1", name: "mogeung", at: T })[0])).toBe("Syncing mogeung…");
    expect(headline(done())).toBe("Sync complete — mogeung");
    expect(headline(done({ warnings: ["warning: x"] }))).toBe("Sync complete, with warnings — mogeung");
    expect(headline(done({ phase: "failed" }))).toBe("Sync failed — mogeung");
  });

  it("says where the branch stands, which is what a fetch was for", () => {
    expect(standing(done())).toBe("up to date with origin/main");
    expect(standing(done({ behind: 3 }))).toBe("3 behind origin/main");
    expect(standing(done({ ahead: 2 }))).toBe("2 ahead of origin/main");
    expect(standing(done({ behind: 3, ahead: 2 }))).toBe("3 behind, 2 ahead of origin/main");
    expect(standing(done({ upstream: null }))).toBe("this branch has no upstream to compare with");
  });

  it("says nothing new out loud rather than saying nothing", () => {
    expect(moved(done())).toBe("the remotes had nothing new");
    expect(moved(done({ updates: ["a"] }))).toBe("1 ref updated");
    expect(moved(done({ updates: ["a", "b"] }))).toBe("2 refs updated");
  });

  it("names a repository by its directory", () => {
    expect(repoName("/home/k/projects/mogeung")).toBe("mogeung");
    expect(repoName("/home/k/projects/mogeung/")).toBe("mogeung");
    expect(repoName("C:\\src\\thing")).toBe("thing");
    expect(repoName(null)).toBe("this repository");
  });
});

describe("how long it stays", () => {
  it("while it runs, and longer the more there is to read", () => {
    expect(expiresAt(beginRun([], { session: "s1", name: "a", at: T })[0])).toBe(Infinity);
    const end = T + 1000;
    expect(expiresAt(done())).toBe(end + QUIET_MS);
    expect(expiresAt(done({ updates: ["a"] }))).toBe(end + NEWS_MS);
    expect(expiresAt(done({ warnings: ["warning: a"] }))).toBe(end + TROUBLE_MS);
    expect(expiresAt(done({ phase: "failed" }))).toBe(end + TROUBLE_MS);
    expect(QUIET_MS).toBeLessThan(NEWS_MS);
    expect(NEWS_MS).toBeLessThan(TROUBLE_MS);
  });
});
