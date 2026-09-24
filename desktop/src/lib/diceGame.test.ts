/**
 * The dice game's rules, pinned to the reference's numbers — faces an event
 * leaves, how the maker guesses, what an order may risk — and the one thing
 * asked to differ from it: a clock that can be any second, or none.
 */

import { describe, expect, it } from "vitest";
import { CLOCK_STOPS, clockFrom, clockText, stepClock } from "@/lib/arcadeClock";
import { orderFits, rightCall, tradePnl } from "@/lib/takerTable";
import {
  EVENTS,
  LIMITS,
  PRESETS,
  eventText,
  faces,
  fairValue,
  makerQuote,
  maxWorth,
  meanWorth,
  minWorth,
  pickEvent,
  roll,
  seeded,
  total,
  worstCase,
  type Die,
} from "@/lib/diceGame";

const die = (worth: number, open: boolean): Die => ({ show: worth, worth, open });

describe("the dice", () => {
  it("each event leaves the faces it says, each equally likely", () => {
    expect(faces(null).map((f) => f.worth)).toEqual([1, 2, 3, 4, 5, 6]);
    expect(faces("high").map((f) => f.show)).toEqual([5, 5, 5, 6, 6, 6]);
    expect(faces("low").map((f) => f.show)).toEqual([1, 1, 1, 2, 2, 2]);
    expect(faces("ends").map((f) => f.show)).toEqual([1, 1, 1, 6, 6, 6]);
    expect(faces("even").map((f) => f.show)).toEqual([2, 2, 4, 4, 6, 6]);
    expect(faces("odd").map((f) => f.show)).toEqual([1, 1, 3, 3, 5, 5]);
    // A 1 still shows 1; it is worth 7.
    expect(faces("oneIsSeven")[0]).toEqual({ show: 1, worth: 7 });
    for (const e of EVENTS) expect(eventText(e)).toMatch(/this round$/);
  });

  it("an event moves the range a hidden die can take, and its average", () => {
    expect([minWorth(null), maxWorth(null), meanWorth(null)]).toEqual([1, 6, 3.5]);
    expect([minWorth("oneIsSeven"), maxWorth("oneIsSeven")]).toEqual([2, 7]);
    expect(meanWorth("oneIsSeven")).toBeCloseTo(27 / 6);
    expect([minWorth("low"), maxWorth("low"), meanWorth("low")]).toEqual([1, 2, 1.5]);
    expect(maxWorth("odd")).toBe(5);
  });

  it("rolls one to all dice face down, the first ones, and every face in reach", () => {
    const rand = seeded(4);
    const hiddenCounts = new Set<number>();
    const shows = new Set<number>();
    for (let i = 0; i < 3000; i++) {
      const dice = roll(5, null, rand);
      const hidden = dice.filter((d) => !d.open).length;
      expect(hidden).toBeGreaterThanOrEqual(1);
      // Face down first, then face up — no face-up die before a hidden one.
      expect(dice.slice(0, hidden).every((d) => !d.open)).toBe(true);
      hiddenCounts.add(hidden);
      dice.forEach((d) => shows.add(d.show));
    }
    expect([...hiddenCounts].sort()).toEqual([1, 2, 3, 4, 5]);
    expect([...shows].sort()).toEqual([1, 2, 3, 4, 5, 6]);
    expect(roll(4, "even", rand).every((d) => d.show % 2 === 0)).toBe(true);
  });

  it("an event comes about twice a game", () => {
    const rand = seeded(8);
    let fired = 0;
    for (let i = 0; i < 8000; i++) if (pickEvent(4, rand)) fired++;
    expect(fired / 8000).toBeGreaterThan(0.46);
    expect(fired / 8000).toBeLessThan(0.54);
  });
});

describe("the market maker", () => {
  it("counts what is face up and guesses each hidden die within its range, one either side", () => {
    const dice = [die(4, false), die(6, false), die(2, true), die(3, true)];
    const rand = seeded(2);
    const mids = new Set<number>();
    for (let i = 0; i < 2000; i++) {
      const q = makerQuote(dice, null, rand);
      expect(q.ask - q.bid).toBe(2);
      const mid = q.bid + 1;
      // 5 face up, two hidden guessed at 1–6 each.
      expect(mid).toBeGreaterThanOrEqual(5 + 2);
      expect(mid).toBeLessThanOrEqual(5 + 12);
      mids.add(mid);
    }
    expect(mids.size).toBe(11);
  });

  it("the fair value is the face-up dice and 3.5 for each hidden one", () => {
    const dice = [die(4, false), die(6, false), die(2, true), die(3, true)];
    expect(fairValue(dice, null)).toBe(12);
    expect(fairValue(dice, "high")).toBe(5 + 2 * 5.5);
    expect(worstCase(dice, null)).toBe(17);
    expect(total(dice)).toBe(15);
  });
});

describe("orders", () => {
  it("a buy must be affordable, and a sale must survive every hidden die at its highest", () => {
    const dice = [die(1, false), die(1, false), die(5, true)]; // worst case 17
    const q = { bid: 9, ask: 11 };
    const worst = worstCase(dice, null);
    expect(worst).toBe(17);
    expect(orderFits("buy", 45, 500, q, worst)).toBe(true);
    expect(orderFits("buy", 46, 500, q, worst)).toBe(false);
    // Sold at 9, a unit can lose 17 − 9 = 8.
    expect(orderFits("sell", 62, 500, q, worst)).toBe(true);
    expect(orderFits("sell", 63, 500, q, worst)).toBe(false);
    // With the dice limited to 1 or 2 the worst is 9: a sale at 9 risks nothing.
    expect(orderFits("sell", 10_000, 500, q, worstCase(dice, "low"))).toBe(true);
  });

  it("pays a buyer what the dice came to above the ask, a seller what they fell below the bid", () => {
    const q = { bid: 9, ask: 11 };
    expect(tradePnl("buy", 3, q, 15)).toBe(12);
    expect(tradePnl("sell", 3, q, 15)).toBe(-18);
    expect(tradePnl("sell", 2, q, 4)).toBe(10);
  });

  it("the right call: buy below the fair value, sell above it, leave it inside", () => {
    const q = { bid: 9, ask: 11 };
    expect(rightCall(q, 12)).toBe("buy");
    expect(rightCall(q, 8.5)).toBe("sell");
    expect(rightCall(q, 10)).toBe("skip");
    expect(rightCall(q, 11)).toBe("skip");
  });
});

describe("the clock — the one thing asked to be looser than the reference", () => {
  it("presets keep the reference's times, and a clock can be any whole second from 1 to 600", () => {
    expect([PRESETS.easy.tradeSeconds, PRESETS.medium.tradeSeconds, PRESETS.hard.tradeSeconds]).toEqual([60, 20, 5]);
    expect(LIMITS.seconds).toEqual([1, 600]);
    expect(clockFrom(7, 20)).toBe(7);
    expect(clockFrom(13.4, 20)).toBe(13);
    expect(clockFrom(0, 20)).toBe(1);
    expect(clockFrom(9999, 20)).toBe(600);
    expect(clockFrom(null, 20)).toBeNull();
    expect(clockFrom("soon", 20)).toBe(20);
  });

  it("− and + walk a ladder that is fine where a second matters, and stop at the ends", () => {
    expect(CLOCK_STOPS.slice(0, 10)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(stepClock(5, 1)).toBe(6);
    expect(stepClock(60, 1)).toBe(75);
    // From a typed value between stops, to the next stop either way.
    expect(stepClock(13, 1)).toBe(15);
    expect(stepClock(13, -1)).toBe(12);
    expect(stepClock(1, -1)).toBe(1);
    expect(stepClock(600, 1)).toBe(600);
  });

  it("says a clock the way a person would", () => {
    expect(clockText(null)).toBe("no limit");
    expect(clockText(5)).toBe("5s");
    expect(clockText(90)).toBe("90s");
    expect(clockText(600)).toBe("10m");
  });
});
