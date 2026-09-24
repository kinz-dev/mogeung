/**
 * The Fermi game's scoring, pinned to the table the reference publishes, the
 * way it reads numbers typed the way people type them, and whole runs.
 */

import { describe, expect, it } from "vitest";
import { BANK, DEFAULT_FERMI, drawQuestions, formatAmount, parseAmount, rangeProblem, scoreRange } from "@/lib/fermi";
import { current, fermiOutcome, nextQuestion, showHint, skip, startRun, stopRun, submit, timedOut } from "@/lib/fermiRun";
import { fermiStanding, parseFermiSession, toFermiSession } from "@/lib/fermiStats";
import { seeded } from "@/lib/tradingGame";

describe("scoring, by the reference's table", () => {
  it("a tight range on the answer is the most there is: 50 + 100 × 1.5 = 200", () => {
    expect(scoreRange(95, 105, 100)).toMatchObject({ hit: true, points: 200 });
    // Exactly the answer, as a range of one point.
    expect(scoreRange(100, 100, 100).points).toBe(200);
  });

  it("the midpoint is geometric, so ×2 low and ×2 high score alike", () => {
    const low = scoreRange(40, 60, 100);
    const high = scoreRange(160, 240, 100);
    expect(low.hit).toBe(false);
    expect(low.distance).toBeCloseTo(high.distance, 1);
    // 10 to 1,000 has its midpoint at 100, not 505.
    expect(scoreRange(10, 1000, 100).mid).toBeCloseTo(100, 6);
  });

  it("a hit's base points fall with distance, and a wide range earns less of them", () => {
    // Midpoint 120 on 100: log 0.079 → 80; range 100–144 is ×1.44, log 0.158 → ×1.5.
    expect(scoreRange(100, 144, 100).points).toBe(50 + 80 * 1.5);
    // Midpoint about 173: log 0.24 → 40; range 100–300, log 0.48 → ×1.2.
    expect(scoreRange(100, 300, 100).points).toBe(50 + 40 * 1.2);
    // 1 to 10,000: midpoint 100 → 100, but log 4 wide → ×0.8.
    expect(scoreRange(1, 10_000, 100).points).toBe(50 + 100 * 0.8);
  });

  it("a miss keeps nothing when tight, more the wider it was", () => {
    expect(scoreRange(200, 220, 100).points).toBe(0);
    expect(scoreRange(200, 500, 100).points).toBe(20); // log 0.40 wide
    expect(scoreRange(200, 1500, 100).points).toBe(35); // log 0.88 wide
    expect(scoreRange(200, 5000, 100).points).toBe(40); // log 1.4 wide
  });

  it("a range must be two positive numbers, low first", () => {
    expect(rangeProblem(null, 5)).toMatch(/both/);
    expect(rangeProblem(0, 5)).toMatch(/above zero/);
    expect(rangeProblem(9, 5)).toMatch(/above the high/);
    expect(rangeProblem(1, 5)).toBeNull();
  });
});

describe("numbers as people type them", () => {
  it("reads plain, grouped, scientific and worded amounts", () => {
    expect(parseAmount("86400")).toBe(86_400);
    expect(parseAmount("86,400")).toBe(86_400);
    expect(parseAmount("8.1e9")).toBe(8.1e9);
    expect(parseAmount("8.1 billion")).toBeCloseTo(8.1e9);
    expect(parseAmount("8.1bn")).toBeCloseTo(8.1e9);
    expect(parseAmount("3.5k")).toBe(3500);
    expect(parseAmount("2M")).toBe(2e6);
    expect(parseAmount("2x10^6")).toBe(2e6);
    expect(parseAmount("2×10^6")).toBe(2e6);
    expect(parseAmount("10^24")).toBe(1e24);
    expect(parseAmount(".5")).toBe(0.5);
    for (const bad of ["", "lots", "3 apples", "1..2", "e9"]) expect(parseAmount(bad)).toBeNull();
  });

  it("writes them back the way a person would say them", () => {
    expect(formatAmount(86_400)).toBe("86,400");
    expect(formatAmount(8.1e9)).toBe("8.1 billion");
    expect(formatAmount(3.4e8)).toBe("340 million");
    expect(formatAmount(2.74e13)).toBe("27.4 trillion");
    expect(formatAmount(5.97e24)).toBe("5.97 × 10²⁴");
    expect(formatAmount(12.345)).toBe("12.3");
    expect(formatAmount(0.004)).toBe("4 × 10⁻³");
  });
});

describe("the bank and a game's questions", () => {
  it("every question has a unit, a positive answer, a hint, and a unique id", () => {
    expect(BANK.length).toBeGreaterThanOrEqual(60);
    expect(new Set(BANK.map((q) => q.id)).size).toBe(BANK.length);
    for (const q of BANK) {
      expect(q.unit.length).toBeGreaterThan(0);
      expect(q.answer).toBeGreaterThan(0);
      expect(q.hints.length).toBeGreaterThan(0);
    }
    for (const level of ["easy", "medium", "hard"] as const) expect(BANK.filter((q) => q.level === level).length).toBeGreaterThanOrEqual(15);
  });

  it("draws without repeats, from the pool asked for", () => {
    const qs = drawQuestions({ pool: "hard", questions: 15 }, seeded(3));
    expect(qs).toHaveLength(15);
    expect(new Set(qs.map((q) => q.id)).size).toBe(15);
    expect(qs.every((q) => q.level === "hard")).toBe(true);
    const mixed = drawQuestions({ pool: "mixed", questions: 15 }, seeded(4));
    expect(new Set(mixed.map((q) => q.level)).size).toBeGreaterThan(1);
  });
});

describe("a run", () => {
  it("scores each answer, keeps skips and timeouts at 0, and ends after the last", () => {
    let r = startRun({ ...DEFAULT_FERMI, questions: 5 }, seeded(8), 0);
    const exact = current(r).answer;
    r = submit(r, String(exact), String(exact), 4_000);
    expect(r.stage).toBe("reveal");
    expect(r.answers[0]).toMatchObject({ outcome: "answered", points: 200, ms: 4_000 });
    r = nextQuestion(r, 5_000);
    expect(r.deadline).toBe(5_000 + 60_000);
    r = nextQuestion(skip(r, 6_000), 6_000);
    r = nextQuestion(timedOut(r), 7_000);
    r = nextQuestion(submit(r, "1", "10^30", 8_000), 8_000);
    r = submit(r, "1", "2", 9_000);
    r = nextQuestion(r, 9_000);
    expect(r).toMatchObject({ stage: "over", ending: "complete" });
    const o = fermiOutcome(r);
    expect(o.max).toBe(1000);
    expect(o.answered).toBe(3);
    expect(r.answers.map((a) => a.outcome)).toEqual(["answered", "skipped", "timeout", "answered", "answered"]);
  });

  it("a range that does not read is refused in words, and nothing is answered", () => {
    const r = startRun(DEFAULT_FERMI, seeded(1), 0);
    const again = submit(r, "lots", "10", 100);
    expect(again.stage).toBe("asking");
    expect(again.problem).toMatch(/does not read/);
    expect(submit(r, "10", "", 100).problem).toMatch(/both/);
  });

  it("hints show one at a time, only when on, and the count is kept", () => {
    let r = startRun({ ...DEFAULT_FERMI, hints: true }, seeded(2), 0);
    r = showHint(showHint(r));
    expect(r.hintsShown).toBe(current(r).hints.length);
    r = skip(r, 10);
    expect(r.answers[0].hints).toBe(current(r).hints.length);
    const off = showHint(startRun({ ...DEFAULT_FERMI, hints: false }, seeded(2), 0));
    expect(off.hintsShown).toBe(0);
  });

  it("with no clock there is no deadline", () => {
    expect(startRun({ ...DEFAULT_FERMI, seconds: null }, seeded(1), 0).deadline).toBeNull();
  });

  it("a session survives the file, and the statistics say which way you miss", () => {
    let r = startRun({ ...DEFAULT_FERMI, questions: 5 }, seeded(5), 0);
    // Answer every question with a range a hundredth of the answer: every miss is low.
    for (let i = 0; i < 5; i++) {
      const a = current(r).answer;
      r = nextQuestion(submit(r, String(a / 120), String(a / 80), 1000), 1000);
    }
    const rec = toFermiSession(r, new Date("2026-09-24T12:00:00Z"))!;
    expect(parseFermiSession(JSON.stringify(rec))).toEqual(rec);
    const st = fermiStanding([rec]);
    expect(st.hitRate).toBe(0);
    expect(st.tooLow).toBe(100);
    expect(st.typicalFactor).toBeCloseTo(100, -1);
    expect(toFermiSession(stopRun(startRun(DEFAULT_FERMI, seeded(1), 0)))).toBeNull();
  });
});
