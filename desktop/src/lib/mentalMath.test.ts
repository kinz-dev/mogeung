/**
 * The drill's rules. The ranges are rankyourbrain.net's, so each one is pinned
 * here: a range that drifts makes a *best* at one difficulty mean something
 * else next week, which is the one thing a score must not do.
 */

import { beforeEach, describe, expect, it } from "vitest";
import {
  DEFAULT_SETTINGS,
  DIFFICULTIES,
  OPS,
  SECRET,
  answer,
  bestOf,
  cleanAnswer,
  endRun,
  generate,
  giveUp,
  isRight,
  loadSaved,
  bestKey,
  questionLeft,
  questionSeconds,
  recordBest,
  save,
  secretStep,
  startRun,
  summarize,
  tick,
  verdict,
  type Difficulty,
  type Op,
  type Settings,
} from "@/lib/mentalMath";

/** mulberry32 — a seeded stream, so a failure names the question it saw. */
function seeded(seed: number): () => number {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** What the question says, worked out independently of how it was made. */
function evaluate(text: string): number {
  const expr = text.replace(/,/g, "").replace(/×/g, "*").replace(/÷/g, "/").replace(/−/g, "-");
  expect(expr).toMatch(/^[\d\s()+\-*/]+$/);
  return Function(`return (${expr})`)() as number;
}

const only = (op: Op): Record<Op, boolean> => ({ add: op === "add", sub: op === "sub", mul: op === "mul", div: op === "div" });
const all: Record<Op, boolean> = { add: true, sub: true, mul: true, div: true };

function operands(text: string): number[] {
  return (text.match(/[\d,]+/g) ?? []).map((t) => Number(t.replace(/,/g, "")));
}

function sample(d: Difficulty, op: Op, count = 3000) {
  const rand = seeded(DIFFICULTIES.indexOf(d) * 10 + OPS.indexOf(op) + 1);
  return Array.from({ length: count }, () => generate({ difficulty: d, ops: only(op) }, rand));
}

const within = (x: number, lo: number, hi: number) => x >= lo && x <= hi;

describe("questions", () => {
  it("every answer is what its question says, and a whole number", () => {
    const rand = seeded(7);
    for (const d of DIFFICULTIES) {
      for (let i = 0; i < 4000; i++) {
        const p = generate({ difficulty: d, ops: all }, rand);
        expect(Number.isInteger(p.answer)).toBe(true);
        expect(evaluate(p.text), p.text).toBe(p.answer);
      }
    }
  });

  it("addition stays in each difficulty's range", () => {
    const ranges: Record<Difficulty, [number, number]> = {
      easy: [2, 100],
      normal: [10, 500],
      advanced: [100, 999],
      expert: [500, 9999],
    };
    for (const d of DIFFICULTIES) {
      for (const p of sample(d, "add")) {
        const [a, b] = operands(p.text);
        expect(within(a, ...ranges[d]) && within(b, ...ranges[d]), `${d}: ${p.text}`).toBe(true);
      }
    }
  });

  it("easy subtraction is addition backwards and never goes below zero; the rest do", () => {
    for (const p of sample("easy", "sub")) {
      const [whole, part] = operands(p.text);
      expect(within(part, 2, 100) && within(p.answer, 2, 100) && whole === part + p.answer, p.text).toBe(true);
    }
    for (const d of ["normal", "advanced", "expert"] as const) {
      expect(sample(d, "sub").some((p) => p.answer < 0), d).toBe(true);
    }
  });

  it("multiplication uses each difficulty's factors, and advanced sometimes squares up to 49", () => {
    const factors: Record<Difficulty, [[number, number], [number, number]]> = {
      easy: [[2, 12], [2, 100]],
      normal: [[2, 19], [2, 50]],
      advanced: [[12, 99], [2, 20]],
      expert: [[15, 99], [15, 99]],
    };
    for (const d of DIFFICULTIES) {
      for (const p of sample(d, "mul")) {
        const [a, b] = operands(p.text);
        const usual = within(a, ...factors[d][0]) && within(b, ...factors[d][1]);
        const squared = d === "advanced" && within(a, 12, 49) && within(b, 12, 49);
        expect(usual || squared, `${d}: ${p.text}`).toBe(true);
      }
    }
    const big = sample("advanced", "mul").filter((p) => operands(p.text)[1] > 20).length / 3000;
    expect(big).toBeGreaterThan(0.12);
    expect(big).toBeLessThan(0.22);
  });

  it("division always comes out whole, within each difficulty's divisors", () => {
    const most: Record<Difficulty, number> = { easy: 12, normal: 19, advanced: 25, expert: 99 };
    for (const d of DIFFICULTIES) {
      for (const p of sample(d, "div")) {
        const [whole, by] = operands(p.text);
        expect(within(by, 2, most[d]) && whole % by === 0 && whole / by === p.answer, `${d}: ${p.text}`).toBe(true);
      }
    }
  });

  it("two-step questions come only above normal, only with two ops on, at the original's rates", () => {
    const share = (d: Difficulty, ops: Record<Op, boolean>) => {
      const rand = seeded(99);
      let mixed = 0;
      for (let i = 0; i < 6000; i++) if (generate({ difficulty: d, ops }, rand).kind === "mixed") mixed++;
      return mixed / 6000;
    };
    expect(share("easy", all)).toBe(0);
    expect(share("normal", all)).toBe(0);
    expect(share("expert", only("mul"))).toBe(0);
    expect(share("advanced", all)).toBeGreaterThan(0.12);
    expect(share("advanced", all)).toBeLessThan(0.18);
    expect(share("expert", all)).toBeGreaterThan(0.36);
    expect(share("expert", all)).toBeLessThan(0.44);
  });

  it("never asks the same thing twice in a row", () => {
    const rand = seeded(3);
    let last = generate({ difficulty: "easy", ops: only("div") }, rand);
    for (let i = 0; i < 2000; i++) {
      const p = generate({ difficulty: "easy", ops: only("div") }, rand, last.text);
      expect(p.text).not.toBe(last.text);
      last = p;
    }
  });
});

describe("answers", () => {
  it("forgives separators, spaces and a typographic minus, and nothing else", () => {
    expect(isRight("1,234", 1234)).toBe(true);
    expect(isRight(" 56 ", 56)).toBe(true);
    expect(isRight("−17", -17)).toBe(true);
    expect(isRight("12a", 12)).toBe(false);
    expect(isRight("", 0)).toBe(false);
    expect(isRight("-", 0)).toBe(false);
    expect(isRight("5", 56)).toBe(false);
  });

  it("the box keeps only digits and one leading minus", () => {
    expect(cleanAnswer("12x3")).toBe("123");
    expect(cleanAnswer("−4-5")).toBe("-45");
    expect(cleanAnswer("4-")).toBe("4");
  });
});

const timed: Settings = { difficulty: "easy", ops: all, duration: 60, question: "auto" };
const endless: Settings = { ...timed, duration: null };

describe("a run", () => {
  it("scores the moment the answer is right, and not before", () => {
    const r = startRun(timed, 0, seeded(1));
    expect(answer(r, String(r.problem.answer).slice(0, -1) || "x", 500)).toBeNull();
    const next = answer(r, String(r.problem.answer), 1200, seeded(2))!;
    expect(next).toMatchObject({ correct: 1, seq: 2, streak: 1, bestStreak: 1, shownAt: 1200, missed: null });
    expect(next.byKind[r.problem.kind]).toEqual({ solved: 1, missed: 0, ms: 1200 });
  });

  it("Enter skips an empty box and counts a wrong answer as wrong, showing what it was", () => {
    const r = startRun(timed, 0, seeded(1));
    const skipped = giveUp(r, "", 100);
    expect(skipped).toMatchObject({ skipped: 1, wrong: 0, streak: 0 });
    expect(skipped.missed).toEqual({ text: r.problem.text, answer: r.problem.answer, typed: null, why: "skipped" });
    const wrong = giveUp(skipped, "7", 200);
    expect(wrong).toMatchObject({ skipped: 1, wrong: 1 });
    expect(wrong.missed?.typed).toBe("7");
  });

  it("a question skips itself after its difficulty's seconds, with no penalty but the streak", () => {
    let r = startRun(timed, 0, seeded(1));
    r = answer(r, String(r.problem.answer), 1000)!;
    expect(questionLeft(r, 1000)).toBe(8000);
    expect(tick(r, 8999)).toBe(r);
    const timedOut = tick(r, 9000);
    expect(timedOut).toMatchObject({ correct: 1, skipped: 1, wrong: 0, streak: 0, bestStreak: 1, seq: 3 });
    expect(timedOut.missed?.why).toBe("timeout");
  });

  it("a timed run ends on its clock and is a score; stopped early it is not", () => {
    const r = startRun(timed, 0, seeded(1));
    const done = tick(r, 60_000);
    expect(done).toMatchObject({ endedAt: 60_000, early: false });
    expect(tick(done, 70_000)).toBe(done);
    expect(bestOf(done)).toBe(0);
    const stopped = endRun(r, 30_000);
    expect(stopped).toMatchObject({ endedAt: 30_000, early: true });
    expect(bestOf(stopped)).toBeNull();
  });

  it("an endless run has no clock of any kind — an hour on one question is still that question", () => {
    const r = startRun(endless, 0, seeded(1));
    expect(r.endsAt).toBeNull();
    expect(questionLeft(r, 3_600_000)).toBeNull();
    expect(tick(r, 3_600_000)).toBe(r);
    const done = endRun(answer(r, String(r.problem.answer), 4000)!, 3_600_000);
    expect(done).toMatchObject({ early: false, endedAt: 3_600_000 });
    expect(bestOf(done)).toBe(1);
  });

  it("sums up by operation, with the average over the ones solved", () => {
    let r = startRun({ ...timed, ops: only("mul") }, 0, seeded(4));
    r = answer(r, String(r.problem.answer), 2000)!;
    r = answer(r, String(r.problem.answer), 6000)!;
    r = giveUp(r, "", 7000);
    const s = summarize(endRun(r, 30_000));
    expect(s).toMatchObject({ score: 2, total: 3, accuracy: 67, ms: 30_000, perMinute: 4, avgMs: 3000 });
    expect(s.kinds).toEqual([{ kind: "mul", solved: 2, missed: 1, avgMs: 3000 }]);
  });

  it("judges every length per two minutes, as the original judges its standard one", () => {
    expect(verdict(50, 120).text).toBe("Outstanding");
    expect(verdict(25, 60).text).toBe("Outstanding");
    expect(verdict(49, 120).text).toBe("Excellent");
    expect(verdict(60, 300).text).toBe("Good effort");
    expect(verdict(0, 120).text).toBe("Keep practising");
  });
});

describe("the question timer", () => {
  it("defaults to the original's rule: the difficulty's seconds when timed, none when endless", () => {
    expect(DEFAULT_SETTINGS.question).toBe("auto");
    const seconds = DIFFICULTIES.map((difficulty) => questionSeconds({ ...timed, difficulty }));
    expect(seconds).toEqual([8, 12, 16, 20]);
    expect(questionSeconds(endless)).toBeNull();
  });

  it("a set timer skips at its own seconds, endless included", () => {
    const quick = startRun({ ...timed, question: 3 }, 0, seeded(1));
    expect(tick(quick, 2999)).toBe(quick);
    expect(tick(quick, 3000)).toMatchObject({ skipped: 1, seq: 2, missed: { why: "timeout" } });

    const training = startRun({ ...endless, question: 5 }, 0, seeded(1));
    const skipped = tick(training, 5000);
    expect(skipped).toMatchObject({ skipped: 1, endedAt: null });
    expect(questionLeft(skipped, 5000)).toBe(5000);
  });

  it("off lets a question wait until the run's own clock runs out", () => {
    const r = startRun({ ...timed, question: "off" }, 0, seeded(1));
    expect(questionLeft(r, 59_999)).toBeNull();
    expect(tick(r, 59_999)).toBe(r);
    expect(tick(r, 60_000)).toMatchObject({ endedAt: 60_000, skipped: 0 });
  });

  it("is part of where a best lives, except auto, which keeps the key bests had before it", () => {
    expect(bestKey({ ...timed, duration: 120 })).toBe("120:easy:+−×÷");
    expect(bestKey({ ...timed, duration: 120, question: 10 })).toBe("120:easy:+−×÷:q10");
    expect(bestKey({ ...timed, question: "off" })).toBe("60:easy:+−×÷:qoff");
    expect(bestKey({ ...endless, question: 5 })).toBe("endless:easy:+−×÷:q5");
  });

  it("an entry saved before it existed comes back as auto", () => {
    localStorage.setItem(
      "mogeung.mentalMath",
      JSON.stringify({ settings: { difficulty: "normal", ops: all, duration: 60 }, bests: {}, found: true }),
    );
    expect(loadSaved().settings.question).toBe("auto");
  });
});

describe("bests", () => {
  const finish = (s: Settings, right: number, at: number) => {
    let r = startRun(s, 0, seeded(5));
    for (let i = 0; i < right; i++) r = answer(r, String(r.problem.answer), i * 100)!;
    return s.duration === null ? endRun(r, at) : tick(r, at);
  };

  it("records a timed best only when it beats the last one, per setting", () => {
    const first = recordBest({}, finish(timed, 3, 60_000));
    expect(first).toMatchObject({ isNew: true, previous: null, bests: { "60:easy:+−×÷": 3 } });
    expect(recordBest(first.bests, finish(timed, 2, 60_000))).toMatchObject({ isNew: false, previous: 3 });
    expect(recordBest(first.bests, finish({ ...timed, duration: 120 }, 1, 120_000)).bests).toEqual({
      "60:easy:+−×÷": 3,
      "120:easy:+−×÷": 1,
    });
  });

  it("an endless best is the longest streak; a run stopped early records nothing", () => {
    expect(recordBest({}, finish(endless, 4, 99_000)).bests).toEqual({ "endless:easy:+−×÷": 4 });
    const early = endRun(startRun(timed, 0, seeded(1)), 1000);
    expect(recordBest({}, early)).toMatchObject({ isNew: false, bests: {} });
  });
});

describe("what it keeps", () => {
  beforeEach(() => localStorage.clear());

  it("comes back as it was saved", () => {
    save({ settings: { difficulty: "expert", ops: only("div"), duration: null, question: 10 }, bests: { a: 3 }, found: true });
    expect(loadSaved()).toEqual({
      settings: { difficulty: "expert", ops: only("div"), duration: null, question: 10 },
      bests: { a: 3 },
      found: true,
    });
  });

  it("survives a damaged entry rather than refusing to open", () => {
    localStorage.setItem("mogeung.mentalMath", "{not json");
    expect(loadSaved().found).toBe(false);
    localStorage.setItem(
      "mogeung.mentalMath",
      JSON.stringify({ settings: { difficulty: "legend", ops: {}, duration: 7, question: 7 }, bests: { x: "9", y: 2 } }),
    );
    expect(loadSaved()).toEqual({
      settings: { difficulty: "easy", ops: all, duration: 120, question: "auto" },
      bests: { y: 2 },
      found: false,
    });
  });
});

describe("the way in", () => {
  const typeAll = (keys: string) => [...keys].reduce(secretStep, "");

  it("is the first six digits of π", () => {
    expect(SECRET).toBe("314159");
    expect(typeAll("314159")).toBe(SECRET);
  });

  it("starts over on a wrong digit, keeping whatever still begins it", () => {
    expect(typeAll("3141")).toBe("3141");
    expect(typeAll("31415")).toBe("31415");
    expect(typeAll("31418")).toBe("");
    expect(typeAll("31314159")).toBe(SECRET);
    expect(typeAll("3141314159")).toBe(SECRET);
    expect(typeAll("271828")).toBe("");
  });
});
