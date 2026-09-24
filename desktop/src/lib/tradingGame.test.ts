/**
 * The Trading Game's rules, pinned to the reference's numbers — a deck that
 * drifts, or a bot that quotes differently, would make a session today mean
 * something else from one last week.
 */

import { describe, expect, it } from "vitest";
import {
  BASE_WORTH,
  PENALTY,
  PRESETS,
  botQuote,
  botTrade,
  deal,
  deckFor,
  eventText,
  makerFeedback,
  maxPrice,
  maxVolume,
  orderFits,
  outcryQuote,
  pickEvent,
  quoteProblem,
  reportAdjustment,
  seeded,
  settle,
  settlePrice,
  theoreticalValue,
  tradePnl,
  type Card,
} from "@/lib/tradingGame";

const card = (rank: Card["rank"], open: boolean, suit: Card["suit"] = "♠"): Card => ({ rank, suit, worth: BASE_WORTH[rank], open });

describe("the deck", () => {
  it("is 52 cards, and each event leaves the cards it says", () => {
    expect(deckFor(null)).toHaveLength(52);
    expect(deckFor({ kind: "high" }).every((c) => c.worth >= 10)).toBe(true);
    expect(deckFor({ kind: "high" })).toHaveLength(20);
    expect(deckFor({ kind: "low" })).toHaveLength(24);
    expect(deckFor({ kind: "even" }).every((c) => c.worth % 2 === 0)).toBe(true);
    expect(deckFor({ kind: "odd" }).every((c) => c.worth % 2 === 1)).toBe(true);
    expect(deckFor({ kind: "red" })).toHaveLength(26);
    expect(deckFor({ kind: "black" }).every((c) => c.suit === "♠" || c.suit === "♣")).toBe(true);
    const revalued = deckFor({ kind: "worth", rank: "K", worth: 2 });
    expect(revalued.filter((c) => c.rank === "K").every((c) => c.worth === 2)).toBe(true);
    expect(revalued.filter((c) => c.rank === "Q").every((c) => c.worth === 12)).toBe(true);
  });

  it("deals distinct cards and always keeps at least one face down", () => {
    const rand = seeded(11);
    for (let i = 0; i < 2000; i++) {
      const n = 3 + (i % 3);
      const hand = deal(n, null, rand);
      expect(hand).toHaveLength(n);
      expect(new Set(hand.map((c) => c.rank + c.suit)).size).toBe(n);
      expect(hand.filter((c) => c.open).length).toBeLessThan(n);
    }
  });

  it("an event comes about twice a game, and says what it does", () => {
    const rand = seeded(3);
    let fired = 0;
    for (let i = 0; i < 8000; i++) if (pickEvent(8, rand)) fired++;
    expect(fired / 8000).toBeGreaterThan(0.22);
    expect(fired / 8000).toBeLessThan(0.28);
    expect(eventText({ kind: "worth", rank: "Q", worth: 3 })).toBe("Card Q is worth 3 this round");
    expect(eventText({ kind: "red" })).toBe("Only red cards this round");
  });
});

describe("what a hand is worth", () => {
  it("settles at its sum, and its fair value counts the hidden cards at the deck's average", () => {
    const hand = [card("A", true), card("2", true), card("7", false)];
    expect(settlePrice(hand)).toBe(23);
    // Two cards seen; the other 50 average (416 − 16) / 50 = 8.
    expect(theoreticalValue(hand, null)).toBeCloseTo(16 + 8, 5);
  });

  it("its worst case is the face-up cards and the highest cards still unseen", () => {
    const hand = [card("A", true), card("5", false), card("5", false)];
    // Three aces remain: the worst is two of them.
    expect(maxPrice(hand, null)).toBe(14 + 14 + 14);
    expect(maxPrice(hand, { kind: "low" })).toBe(14 + 7 + 7);
  });
});

describe("quotes and orders", () => {
  it("a bot quotes about 8 a card, two wide, blind to the face-up cards", () => {
    const rand = seeded(5);
    for (let i = 0; i < 1000; i++) {
      const q = botQuote({ cards: 4 }, null, rand);
      expect(q.ask - q.bid).toBe(2);
      const mid = (q.bid + q.ask) / 2;
      expect(mid).toBeGreaterThanOrEqual(32 - 5);
      expect(mid).toBeLessThanOrEqual(32 + 4);
    }
    expect((botQuote({ cards: 3 }, { kind: "high" }, () => 0.5).bid + 1) / 3).toBe(12);
    const shouted = outcryQuote({ cards: 3 }, null, seeded(9));
    expect(shouted.bid).toBeLessThan(shouted.ask);
  });

  it("your quote must be whole, ordered and within the spread", () => {
    const s = PRESETS.medium;
    expect(quoteProblem({ bid: 30, ask: 33 }, s)).toBeNull();
    expect(quoteProblem({ bid: 33, ask: 30 }, s)).toMatch(/less than/);
    expect(quoteProblem({ bid: 30, ask: 31 }, s)).toMatch(/2–5/);
    expect(quoteProblem({ bid: 30, ask: 36 }, s)).toMatch(/2–5/);
    expect(quoteProblem({ bid: 30.5, ask: 33 }, s)).toMatch(/whole/);
  });

  it("a buy must be affordable, and a sale must survive the worst case", () => {
    const hand = [card("A", true), card("5", false), card("5", false)];
    const q = { bid: 30, ask: 32 };
    expect(orderFits("buy", 15, 500, q, hand, null)).toBe(true);
    expect(orderFits("buy", 16, 500, q, hand, null)).toBe(false);
    // Worst case 42: a unit sold at 30 can lose 12.
    expect(maxVolume("sell", 500, q, hand, null)).toBe(41);
    expect(orderFits("sell", 41, 500, q, hand, null)).toBe(true);
    expect(orderFits("sell", 42, 500, q, hand, null)).toBe(false);
  });

  it("the market maker takes the other side of every trade", () => {
    const q = { bid: 20, ask: 22 };
    expect(tradePnl("buy", 5, q, 30)).toBe(40);
    expect(tradePnl("sell", 3, q, 30)).toBe(-30);
    const pnl = settle(1, q, { 2: { action: "buy", volume: 5 }, 3: { action: "sell", volume: 3 }, 4: { action: "skip" } }, 30);
    expect(pnl).toEqual({ 1: -10, 2: 40, 3: -30 });
    expect(Object.values(pnl).reduce((a, b) => a + b, 0)).toBe(0);
  });
});

describe("reporting and feedback", () => {
  it("a wrong report forfeits the profit and costs 50; a loss stays, plus the 50", () => {
    expect(reportAdjustment(40, 40)).toBe(0);
    expect(reportAdjustment(40, 39)).toBe(-40 - PENALTY);
    expect(reportAdjustment(-30, -30)).toBe(0);
    expect(reportAdjustment(-30, 30)).toBe(-PENALTY);
  });

  it("judges a maker's quote on centring and on where the market landed", () => {
    const hand = [card("A", true), card("2", true), card("7", false)]; // fair 24
    expect(makerFeedback({ bid: 22, ask: 26 }, hand, null, 23).scenario).toBe("good_good");
    expect(makerFeedback({ bid: 22, ask: 26 }, hand, null, 30).scenario).toBe("good_bad");
    expect(makerFeedback({ bid: 10, ask: 30 }, hand, null, 23).scenario).toBe("good_good");
    expect(makerFeedback({ bid: 14, ask: 16 }, hand, null, 15).scenario).toBe("bad_good");
    expect(makerFeedback({ bid: 14, ask: 16 }, hand, null, 23).scenario).toBe("bad_bad");
  });
});

describe("the bots", () => {
  it("trade whole volumes of 1–10, buy a quote far below the hand, and sell one far above", () => {
    const rand = seeded(21);
    const hand = [card("A", true), card("K", true), card("Q", false)];
    let bought = 0;
    let sold = 0;
    for (let i = 0; i < 500; i++) {
      const cheap = botTrade({ bid: 8, ask: 10 }, hand, rand);
      if (cheap.action !== "skip") {
        expect(Number.isInteger(cheap.volume) && cheap.volume >= 1 && cheap.volume <= 10).toBe(true);
        if (cheap.action === "buy") bought++;
      }
      const dear = botTrade({ bid: 70, ask: 72 }, hand, rand);
      if (dear.action === "sell") sold++;
    }
    expect(bought).toBeGreaterThan(450);
    expect(sold).toBeGreaterThan(450);
  });
});
