/**
 * The mental-math history: what is kept per game and what is made of it.
 * Pinned because a trend line is only as honest as the numbers under it, and
 * a history file is only useful if a line written today still reads tomorrow.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { answer, endRun, giveUp, startRun, tick, type Settings } from "@/lib/mentalMath";
import {
  againstRecent,
  appendHistory,
  byOperation,
  filterRecords,
  fitLine,
  hardest,
  loadHistory,
  metrics,
  overview,
  parseRecords,
  toRecord,
  type GameRecord,
} from "@/lib/mentalMathStats";

const all = { add: true, sub: true, mul: true, div: true };
const timed: Settings = { difficulty: "easy", ops: all, duration: 60, question: "auto" };

/** A seeded stream, so the questions are the same every run. */
function seeded(seed: number): () => number {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function game(at: string, correct: number, ms: number, extra: Partial<GameRecord> = {}): GameRecord {
  return {
    v: 1,
    at,
    difficulty: "easy",
    ops: "+−×÷",
    duration: 60,
    question: "auto",
    early: false,
    ms,
    correct,
    wrong: 0,
    skipped: 0,
    bestStreak: correct,
    log: Array.from({ length: correct }, (_, i) => ({ text: `${i} + 1`, kind: "add" as const, ms: 2000, how: "right" as const })),
    ...extra,
  };
}

describe("what a game keeps", () => {
  it("every question, with its time and how it went", () => {
    let r = startRun(timed, 0, seeded(1));
    const first = r.problem.text;
    r = answer(r, String(r.problem.answer), 1500, seeded(2))!;
    r = giveUp(r, "", 4000, seeded(3));
    r = tick(r, 60_000);
    const rec = toRecord(r)!;
    expect(rec).toMatchObject({ correct: 1, skipped: 1, ms: 60_000, early: false, ops: "+−×÷", duration: 60 });
    expect(rec.log).toEqual([
      expect.objectContaining({ text: first, ms: 1500, how: "right" }),
      expect.objectContaining({ ms: 2500, how: "skipped" }),
    ]);
  });

  it("nothing, for a game in which nothing was asked", () => {
    expect(toRecord(endRun(startRun(timed, 0, seeded(1)), 10))).toBeNull();
  });

  it("reads back what it wrote, skips what it cannot read, and puts the file in order", async () => {
    localStorage.clear();
    await appendHistory(game("2026-09-24T10:00:00.000Z", 5, 60_000));
    localStorage.setItem("mogeung.arcade.mental-math", `${localStorage.getItem("mogeung.arcade.mental-math")}{broken\n`);
    await appendHistory(game("2026-09-23T10:00:00.000Z", 3, 60_000));
    const back = await loadHistory();
    expect(back.map((r) => r.correct)).toEqual([3, 5]);
    expect(parseRecords('{"at":"nope"}\n')).toEqual([]);
  });
});

describe("what is made of it", () => {
  beforeEach(() => localStorage.clear());

  it("per game: accuracy, answers a minute, and the average time on the ones you got", () => {
    const m = metrics(game("2026-09-24T10:00:00Z", 10, 120_000, { wrong: 1, skipped: 1 }));
    expect(m).toEqual({ asked: 12, accuracy: 83, perMinute: 5, avgMs: 2000 });
  });

  it("a least-squares line — flat through a constant, and the slope of a straight rise", () => {
    expect(fitLine([4])).toBeNull();
    expect(fitLine([3, 3, 3])).toEqual({ slope: 0, intercept: 3 });
    const up = fitLine([1, 3, 5, 7])!;
    expect(up.slope).toBeCloseTo(2);
    expect(up.intercept).toBeCloseTo(1);
  });

  it("filters by difficulty and by mode", () => {
    const rs = [game("2026-09-24T10:00:00Z", 5, 60_000), game("2026-09-24T11:00:00Z", 5, 60_000, { difficulty: "hard" as never, duration: null })];
    expect(filterRecords(rs, { difficulty: "easy", mode: "all" })).toHaveLength(1);
    expect(filterRecords(rs, { difficulty: "all", mode: "endless" })).toHaveLength(1);
  });

  it("an overview, with the days played in a row counted back from the latest", () => {
    const rs = [
      game("2026-09-21T10:00:00", 4, 60_000),
      game("2026-09-23T10:00:00", 6, 60_000),
      game("2026-09-24T09:00:00", 8, 60_000),
      game("2026-09-24T20:00:00", 2, 60_000, { early: true }),
    ];
    const o = overview(rs);
    expect(o).toMatchObject({ games: 4, correct: 20, asked: 20, accuracy: 100, bestPerMinute: 8, dayStreak: 2 });
  });

  it("each operation's speed, and whether it is moving", () => {
    const slow = (ms: number) => ({ text: "7 × 8", kind: "mul" as const, ms, how: "right" as const });
    const rs = [
      game("2026-09-21T10:00:00Z", 0, 60_000, { log: [slow(6000), slow(6000)] }),
      game("2026-09-22T10:00:00Z", 0, 60_000, { log: [slow(5000)] }),
      game("2026-09-23T10:00:00Z", 0, 60_000, { log: [slow(3000)] }),
      game("2026-09-24T10:00:00Z", 0, 60_000, { log: [slow(2000)] }),
    ];
    const [mul] = byOperation(rs);
    expect(mul).toMatchObject({ kind: "mul", solved: 5, missed: 0, series: [6, 5, 3, 2], earlier: 5.5, recent: 2.5 });
  });

  it("the hardest questions: missed ones first, then the slowest", () => {
    const rs = [
      game("2026-09-24T10:00:00Z", 0, 60_000, {
        log: [
          { text: "7 × 8", kind: "mul", ms: 9000, how: "wrong" },
          { text: "12 × 13", kind: "mul", ms: 8000, how: "right" },
          { text: "2 + 2", kind: "add", ms: 900, how: "right" },
          { text: "7 × 8", kind: "mul", ms: 4000, how: "right" },
        ],
      }),
    ];
    const h = hardest(rs, 2);
    expect(h[0]).toMatchObject({ text: "7 × 8", seen: 2, missed: 1, avgMs: 4000 });
    expect(h[1]).toMatchObject({ text: "12 × 13", missed: 0 });
  });

  it("a new game against the recent ones at the same setting — and only those", () => {
    const rs = [
      game("2026-09-22T10:00:00Z", 6, 60_000),
      game("2026-09-23T10:00:00Z", 8, 60_000),
      game("2026-09-23T11:00:00Z", 30, 60_000, { duration: 120 }),
      game("2026-09-23T12:00:00Z", 30, 30_000, { early: true }),
    ];
    const now = game("2026-09-24T10:00:00Z", 9, 60_000);
    expect(againstRecent([...rs, now], now)).toEqual({ games: 2, avgPerMinute: 7, bestPerMinute: 8 });
    expect(againstRecent([now], now)).toBeNull();
  });
});
