/**
 * The Event Contracts game: the odds-form update the reference teaches, a
 * flow that behaves as it describes — noise pays a spread and leaves a wide
 * one, the informed take a wrong centre — both scores, and whole runs.
 */

import { describe, expect, it } from "vitest";
import {
  DEFAULT_EVENTS,
  EVENTS,
  LEVELS,
  balancedWidth,
  calibrationPoints,
  expectedPnl,
  fillValue,
  flow,
  inventory,
  makeContracts,
  modelQuote,
  oddsText,
  pnlPoints,
  posterior,
  quoteProblem,
  verdict,
  type Contract,
} from "@/lib/eventContracts";
import { contract, eventOutcome, nextRound, quote, skip, startRun, stopRun, timedOut } from "@/lib/eventRun";
import { eventStanding, parseEventSession, toEventSession } from "@/lib/eventStats";
import { seeded } from "@/lib/tradingGame";

const at = (truth: number, implied = truth, traders = 12): Contract => ({
  brief: { title: "t", prior: 50, direction: "none", ratio: null, evidence: null, band: 4, maxWidth: 12, traders },
  implied,
  truth,
  happened: true,
});

describe("the update", () => {
  it("is the reference's worked example: base 25%, 3× for → 50", () => {
    expect(posterior(25, 3, "for")).toBeCloseTo(50, 9);
    expect(posterior(50, 3, "against")).toBeCloseTo(25, 9);
    expect(posterior(40, null, "none")).toBe(40);
    expect(oddsText(25)).toBe("1:3");
    expect(oddsText(60)).toBe("3:2");
  });

  it("a game's contracts are different events, the truth within the band of the update", () => {
    for (const level of ["easy", "medium", "hard"] as const) {
      const cs = makeContracts({ ...DEFAULT_EVENTS, level, rounds: 12 }, seeded(4));
      expect(new Set(cs.map((c) => c.brief.title)).size).toBe(12);
      for (const c of cs) {
        expect(Math.abs(c.truth - c.implied)).toBeLessThanOrEqual(LEVELS[level].band + 1e-9);
        expect(c.implied).toBeCloseTo(posterior(c.brief.prior, c.brief.ratio, c.brief.direction), 9);
        expect(c.brief.maxWidth).toBe(LEVELS[level].maxWidth);
      }
    }
    expect(EVENTS.length).toBeGreaterThanOrEqual(12);
  });
});

describe("the flow", () => {
  it("the informed deal only when the truth is outside your quote", () => {
    const rand = seeded(1);
    for (let i = 0; i < 200; i++) {
      const inside = flow({ bid: 55, ask: 65 }, at(60), { informed: 1 }, rand);
      expect(inside).toEqual([]);
      const cheap = flow({ bid: 40, ask: 45 }, at(60), { informed: 1 }, rand);
      expect(cheap.every((f) => f.side === "sold" && f.informed && f.price === 45)).toBe(true);
    }
  });

  it("noise pays the spread, and walks away from a wide one", () => {
    const rand = seeded(2);
    let tight = 0;
    let wide = 0;
    for (let i = 0; i < 300; i++) {
      tight += flow({ bid: 58, ask: 62 }, at(60), { informed: 0 }, rand).length;
      wide += flow({ bid: 50, ask: 70 }, at(60), { informed: 0 }, rand).length;
    }
    expect(tight / 300).toBeGreaterThan(8);
    expect(wide).toBe(0);
  });

  it("fills are marked at the truth, and what is held at the close counts too", () => {
    const fills = [
      { side: "sold" as const, size: 3, price: 55, informed: false },
      { side: "bought" as const, size: 1, price: 45, informed: false },
    ];
    expect(fillValue(fills[0], 50)).toBe(15);
    expect(fillValue(fills[1], 50)).toBe(5);
    expect(inventory(fills)).toBe(-2);
    expect(pnlPoints(fills, 50)).toBe(10);
  });

  it("on every level the balanced width sits strictly inside the widest allowed", () => {
    for (const level of ["easy", "medium", "hard"] as const) {
      const rules = LEVELS[level];
      const w = balancedWidth(rules);
      expect(w).toBeGreaterThan(1);
      expect(w).toBeLessThan(rules.maxWidth);
      expect(expectedPnl(w, rules)).toBeGreaterThan(expectedPnl(rules.maxWidth, rules));
      expect(expectedPnl(w, rules)).toBeGreaterThan(expectedPnl(2, rules));
    }
  });
});

describe("scoring and the review", () => {
  it("calibration: nothing at 50, the most on the truth, a loss further off than a coin", () => {
    expect(calibrationPoints(50, 80)).toBe(0);
    expect(calibrationPoints(80, 80)).toBe(14.4);
    expect(calibrationPoints(20, 80)).toBeLessThan(0);
  });

  it("the verdicts: a wrong centre first, then width, then praise", () => {
    const rules = LEVELS.medium;
    const w = balancedWidth(rules);
    expect(verdict({ bid: 40, ask: 48 }, at(60), rules, []).tone).toBe("bad");
    expect(verdict({ bid: 54, ask: 66 }, at(60), rules, []).text).toMatch(/Too wide/);
    const informed = [{ side: "sold" as const, size: 3, price: 61, informed: true }];
    expect(verdict({ bid: 59, ask: 61 }, at(60), rules, informed).text).toMatch(/Too tight.*1 informed trader took/);
    const lo = 60 - Math.floor(w / 2);
    expect(verdict({ bid: lo, ask: lo + w }, at(60), rules, []).tone).toBe("good");
  });

  it("a quote must be whole cents, ordered, and within the round's width", () => {
    expect(quoteProblem({ bid: 48, ask: 53 }, 10)).toBeNull();
    expect(quoteProblem({ bid: 53, ask: 48 }, 10)).toMatch(/below/);
    expect(quoteProblem({ bid: 40, ask: 60 }, 10)).toMatch(/widest quote is 10/);
    expect(quoteProblem({ bid: 48.5, ask: 53 }, 10)).toMatch(/whole/);
    expect(quoteProblem({ bid: 95, ask: 101 }, 10)).toMatch(/0¢ to 100¢/);
  });
});

describe("a run", () => {
  it("quotes, skips and timeouts, reviewed each, scored, and kept", () => {
    const rand = seeded(7);
    let r = startRun({ ...DEFAULT_EVENTS, rounds: 4 }, rand, 0);
    expect(r.deadline).toBe(60_000);
    const c = contract(r);
    const m = Math.round(c.implied);
    r = quote(r, { bid: m - 3, ask: m + 3 }, rand, 12_000);
    expect(r.stage).toBe("review");
    expect(r.rounds[0]).toMatchObject({ outcome: "quoted", ms: 12_000 });
    expect(r.rounds[0].score).toBeCloseTo(r.rounds[0].pnl + r.rounds[0].calibration, 5);
    r = nextRound(skip(nextRound(r, 13_000), 14_000), 14_000);
    r = nextRound(timedOut(r), 80_000);
    const refused = quote(r, { bid: 10, ask: 90 }, rand, 81_000);
    expect(refused.problem).toMatch(/widest quote/);
    r = nextRound(quote(r, { bid: 45, ask: 50 }, rand, 81_000), 82_000);
    expect(r).toMatchObject({ stage: "over", ending: "complete" });
    expect(r.rounds.map((x) => x.outcome)).toEqual(["quoted", "skipped", "timeout", "quoted"]);
    expect(r.rounds[1].score + r.rounds[2].score).toBe(0);

    const rec = toEventSession(r, new Date("2026-09-24T12:00:00Z"))!;
    expect(parseEventSession(JSON.stringify(rec))).toEqual(rec);
    const st = eventStanding([rec]);
    expect(st.sessions).toBe(1);
    expect(st.best).toBeCloseTo(eventOutcome(r).score / 4, 0);
    expect(toEventSession(stopRun(startRun(DEFAULT_EVENTS, seeded(1), 0)))).toBeNull();
  });

  it("with no clock there is no deadline", () => {
    expect(startRun({ ...DEFAULT_EVENTS, seconds: null }, seeded(1), 0).deadline).toBeNull();
  });

  it("the balanced quote, every round, is a winning game over many", () => {
    let total = 0;
    for (let seed = 1; seed <= 30; seed++) {
      const rand = seeded(seed);
      let r = startRun({ ...DEFAULT_EVENTS, level: "medium", rounds: 8 }, rand, 0);
      for (let guard = 0; guard < 20 && r.stage !== "over"; guard++) {
        r = nextRound(quote(r, modelQuote(contract(r), LEVELS.medium), rand, 0), 0);
      }
      expect(r.stage).toBe("over");
      total += eventOutcome(r).pnl;
    }
    expect(total).toBeGreaterThan(0);
  });
});
