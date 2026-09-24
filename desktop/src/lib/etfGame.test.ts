/**
 * The ETF game's rules, pinned to the reference's numbers — how weights are
 * dealt, how far the ETF strays from its NAV, what a trade pays and what the
 * bots are like — so a session today means what one last week did.
 */

import { describe, expect, it } from "vitest";
import {
  BASKETS,
  COST,
  DEFAULT_ETF,
  LEVELS,
  PERSONALITIES,
  botDecision,
  makeMarkets,
  makeWeights,
  navOf,
  rightSide,
  score,
  seeded,
  signedUsd,
  usd,
  type EtfSettings,
} from "@/lib/etfGame";

describe("the basket", () => {
  it("weights come in whole steps, at least one each, and sum to one", () => {
    const rand = seeded(3);
    for (let i = 0; i < 500; i++) {
      for (const step of [0.1, 0.01]) {
        const w = makeWeights(4, step, rand);
        expect(w).toHaveLength(4);
        expect(w.every((x) => x >= step)).toBe(true);
        expect(w.every((x) => Math.abs(Math.round(x / step) * step - x) < 1e-9)).toBe(true);
        expect(w.reduce((a, b) => a + b, 0)).toBeCloseTo(1, 9);
      }
    }
  });

  it("every basket is four stocks, and the NAV is Σ weight × price to the cent", () => {
    expect(BASKETS).toHaveLength(19);
    expect(BASKETS.every((b) => b.stocks.length === 4)).toBe(true);
    expect(navOf([{ weight: 0.25, price: 200 }, { weight: 0.25, price: 400 }, { weight: 0.25, price: 150 }, { weight: 0.25, price: 200 }])).toBe(237.5);
    expect(navOf([{ weight: 0.33, price: 10.01 }, { weight: 0.67, price: 3 }])).toBe(5.31);
  });
});

describe("a game's markets", () => {
  const play = (s: Partial<EtfSettings>, seed: number) => makeMarkets({ ...DEFAULT_ETF, ...s }, seeded(seed));

  it("one basket and one set of weights for the whole game, and a round per market", () => {
    const ms = play({ rounds: 9 }, 1);
    expect(ms).toHaveLength(9);
    expect(new Set(ms.map((m) => m.basket)).size).toBe(1);
    expect(new Set(ms.map((m) => m.stocks.map((s) => s.weight).join()))).toHaveProperty("size", 1);
    expect(ms[0].stocks.every((s) => s.prev === null)).toBe(true);
    expect(ms[1].stocks.map((s) => s.prev)).toEqual(ms[0].stocks.map((s) => s.price));
  });

  it("easy prices are whole dollars and weights in tenths; harder ones go to the cent", () => {
    for (let seed = 1; seed <= 30; seed++) {
      for (const m of play({ level: "easy", events: true }, seed)) {
        expect(m.stocks.every((s) => Number.isInteger(s.price))).toBe(true);
        expect(m.stocks.every((s) => Number.isInteger(Math.round(s.weight * 100) / 10))).toBe(true);
      }
    }
    const hard = play({ level: "hard" }, 2).flatMap((m) => m.stocks.map((s) => s.price));
    expect(hard.some((p) => !Number.isInteger(p))).toBe(true);
  });

  it("the ETF sits off its NAV by the level's range, either way, five hundredths of a percent wide each side", () => {
    for (const level of ["easy", "medium", "hard"] as const) {
      const [lo, hi] = LEVELS[level].premium;
      let over = 0;
      let under = 0;
      for (let seed = 1; seed <= 60; seed++) {
        for (const m of play({ level, rounds: 5 }, seed)) {
          const off = Math.abs(m.premiumPct);
          expect(off).toBeGreaterThanOrEqual(lo - 0.01);
          expect(off).toBeLessThanOrEqual(hi + 0.01);
          const mid = (m.bid + m.ask) / 2;
          expect((m.ask - m.bid) / mid).toBeCloseTo(0.001, 3);
          // Always a side to take: the stray is wider than the spread.
          expect(rightSide(m)).toBe(m.premiumPct > 0 ? "sell" : "buy");
          if (m.premiumPct > 0) over++;
          else under++;
        }
      }
      expect(over).toBeGreaterThan(100);
      expect(under).toBeGreaterThan(100);
    }
  });

  it("with events off there is no news; on, every round after the first has a headline", () => {
    expect(play({ events: false, rounds: 8 }, 4).every((m) => m.headline === null)).toBe(true);
    const on = play({ events: true, rounds: 8 }, 4);
    expect(on[0].headline).toBeNull();
    expect(on.slice(1).every((m) => typeof m.headline === "string" && m.sentiment !== null)).toBe(true);
  });

  it("news moves prices the way it reads, most of the time", () => {
    let agree = 0;
    let total = 0;
    for (let seed = 1; seed <= 80; seed++) {
      const ms = play({ level: "medium", events: true, rounds: 6 }, seed);
      for (const m of ms.slice(1)) {
        if (m.sentiment !== "++" && m.sentiment !== "--") continue;
        const up = m.stocks.every((s) => s.price > s.prev!);
        const down = m.stocks.every((s) => s.price < s.prev!);
        total++;
        if ((m.sentiment === "++" && up) || (m.sentiment === "--" && down)) agree++;
      }
    }
    expect(total).toBeGreaterThan(20);
    expect(agree).toBe(total);
  });
});

describe("what a trade pays", () => {
  const m = { nav: 237.5, bid: 233.2, ask: 233.4 };

  it("the reference's own example: 10 bought at 233.40 on a NAV of 237.50, first, nets $37.20", () => {
    expect(score("buy", 10, m, true)).toEqual({ gross: 4100, cost: COST, bonus: 620, net: 3720 });
    expect(score("buy", 10, m, false).net).toBe(3100);
  });

  it("the wrong way loses the edge and pays the cost; a first loss earns no bonus", () => {
    expect(score("sell", 10, m, true)).toEqual({ gross: -4300, cost: COST, bonus: 0, net: -5300 });
    expect(score("pass", 10, m, true)).toEqual({ gross: 0, cost: 0, bonus: 0, net: 0 });
    // A thin edge does not cover the $10.
    expect(score("buy", 1, m, true).net).toBe(410 - COST);
  });

  it("says money the way a trader would", () => {
    expect(usd(123450)).toBe("$1,234.50");
    expect(signedUsd(3720)).toBe("+$37.20");
    expect(signedUsd(-1000)).toBe("−$10.00");
  });
});

describe("the bots", () => {
  const m = { nav: 237.5, bid: 233.2, ask: 233.4 };

  it("a sniper is never wrong, a rookie often is, and nobody trades outside 1–20", () => {
    const rand = seeded(9);
    const tally = (style: keyof typeof PERSONALITIES) => {
      let right = 0;
      let traded = 0;
      for (let i = 0; i < 2000; i++) {
        const d = botDecision(style, m, rand);
        if (d.action === "pass") continue;
        expect(d.units).toBeGreaterThanOrEqual(1);
        expect(d.units).toBeLessThanOrEqual(20);
        traded++;
        if (d.action === "buy") right++;
      }
      return right / traded;
    };
    expect(tally("sniper")).toBe(1);
    expect(tally("rookie")).toBeGreaterThan(0.55);
    expect(tally("rookie")).toBeLessThan(0.65);
    expect(tally("whale")).toBeGreaterThan(0.75);
  });

  it("a picky bot passes an edge too thin to be worth its while", () => {
    const thin = { nav: 100.6, bid: 99.95, ask: 100.05 }; // 0.55 a unit
    // Patient's middling size is 4: 4 × 0.55 − 10 < 12.
    expect(botDecision("patient", thin, seeded(1)).action).toBe("pass");
    // Its edge on the example is 4.10 a unit: 16.40 − 10 = 6.40 is still under 12.
    expect(botDecision("patient", m, seeded(1)).action).toBe("pass");
    expect(botDecision("patient", { nav: 250, bid: 239.9, ask: 240.1 }, seeded(1)).action).toBe("buy");
  });
});
