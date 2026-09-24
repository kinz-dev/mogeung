/**
 * The card game's deal, pinned to the reference: mostly hidden hands, a maker
 * that quotes a hand of its own, and the shared market-taking rules played
 * through with cards.
 */

import { describe, expect, it } from "vitest";
import { DEFAULT_CARDS, PRESETS, blindQuote, dealCards, dealHand, describeCards, type CardSettings } from "@/lib/cardGame";
import { ackEvent, landed, nextRound, outcome, rightCall, roundPnl, startGame, submitReport, yourOrder } from "@/lib/takerTable";
import { parseTakerSession, toTakerSession } from "@/lib/takerStats";
import { BASE_WORTH, maxPrice, seeded, settlePrice } from "@/lib/tradingGame";

describe("the deal", () => {
  it("is distinct cards, at most one face up — none about half the time", () => {
    const rand = seeded(4);
    let none = 0;
    for (let i = 0; i < 4000; i++) {
      const hand = dealHand(5, null, rand);
      expect(new Set(hand.map((c) => c.rank + c.suit)).size).toBe(5);
      const open = hand.filter((c) => c.open);
      expect(open.length).toBeLessThanOrEqual(1);
      if (open.length) expect(hand[0].open).toBe(true);
      else none++;
    }
    expect(none / 4000).toBeGreaterThan(0.46);
    expect(none / 4000).toBeLessThan(0.54);
  });

  it("an event changes the deck the hand comes from", () => {
    const rand = seeded(5);
    for (let i = 0; i < 300; i++) {
      expect(dealHand(4, { kind: "high" }, rand).every((c) => c.worth >= 10)).toBe(true);
      expect(dealHand(4, { kind: "red" }, rand).every((c) => c.suit === "♥" || c.suit === "♦")).toBe(true);
    }
  });
});

describe("the maker", () => {
  it("quotes blind: about 8 a card on average, two wide, whatever you hold", () => {
    const rand = seeded(9);
    let sum = 0;
    for (let i = 0; i < 4000; i++) {
      const q = blindQuote(3, null, rand);
      expect(q.ask - q.bid).toBe(2);
      sum += q.bid + 1;
    }
    // A full deck averages 416 / 52 = 8 a card.
    expect(sum / 4000 / 3).toBeGreaterThan(7.8);
    expect(sum / 4000 / 3).toBeLessThan(8.2);
  });

  it("a dealt round carries its settle, fair value and the worst the hidden cards could be", () => {
    const rand = seeded(2);
    for (let n = 1; n <= 200; n++) {
      const h = dealCards(n, { ...DEFAULT_CARDS, count: 4, events: true }, rand);
      expect(h.settle).toBe(settlePrice(h.items));
      expect(h.worst).toBe(maxPrice(h.items, h.event));
      expect(h.worst).toBeGreaterThanOrEqual(h.settle);
      expect(h.fair).toBeGreaterThan(0);
    }
    // With nothing face up, a three-card hand's fair value is three average cards.
    const blind = dealCards(1, DEFAULT_CARDS, () => 0);
    expect(blind.items.every((c) => !c.open)).toBe(true);
    expect(blind.fair).toBeCloseTo(24, 5);
  });

  it("presets are the reference's", () => {
    expect(PRESETS.easy).toMatchObject({ count: 3, rounds: 4, tradeSeconds: 60, showBalance: true, events: false });
    expect(PRESETS.hard).toMatchObject({ count: 5, rounds: 8, tradeSeconds: 5, showBalance: false });
    expect(BASE_WORTH.A).toBe(14);
  });
});

describe("a whole game with cards", () => {
  it("plays through the shared rules, and the record reads its hands as cards", () => {
    const settings: CardSettings = { ...PRESETS.medium, difficulty: "medium", reportSeconds: null };
    const rand = seeded(11);
    let t = startGame(settings, dealCards, rand);
    while (t.stage !== "over") {
      t = landed(ackEvent(t), 0);
      const call = rightCall(t.round.quote, t.round.fair);
      t = yourOrder(t, call === "skip" ? "skip" : { side: call, units: 2 }, 800);
      if (t.stage === "report") t = submitReport(t, roundPnl(t.round));
      t = nextRound(t, dealCards, rand);
    }
    expect(t.history).toHaveLength(6);
    const o = outcome(t);
    expect(o.exact).toBe(o.reports);
    const record = toTakerSession(t, describeCards, new Date("2026-09-24T12:00:00Z"))!;
    expect(record.rounds[0].hand).toMatch(/^(10|[2-9JQKA])[♠♥♦♣]( (10|[2-9JQKA])[♠♥♦♣]){3}$/);
    expect(parseTakerSession(JSON.stringify(record))).toEqual(record);
  });
});
