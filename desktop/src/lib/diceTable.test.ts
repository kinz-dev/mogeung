/**
 * Whole dice games, played from a seed through the same moves the screen makes
 * — no clock, no DOM. What is pinned is the bookkeeping: that a balance is the
 * trades and the penalties and nothing else, what each mistake costs, that the
 * clocks you set are the clocks you get, and how a game ends.
 */

import { describe, expect, it } from "vitest";
import { DEFAULT_DICE, PRESETS, dealDice, describeDice, seeded, type DiceEvent, type DiceSettings, type Die, type Rand } from "@/lib/diceGame";
import {
  PENALTY,
  STARTING_BALANCE,
  ackEvent,
  landed,
  nextRound,
  outcome,
  reportTimedOut,
  rightCall,
  roundPnl,
  startGame,
  stopGame,
  submitFinal,
  submitReport,
  tradingTimedOut,
  yourOrder,
  type TakerTable,
} from "@/lib/takerTable";
import { parseTakerSession, takerStanding, toTakerSession } from "@/lib/takerStats";

type DiceTable = TakerTable<Die, DiceEvent, DiceSettings>;

/** A dice game, from the shared market-taking rules with the dice dealer. */
const start = (s: DiceSettings, rand: Rand): DiceTable => startGame(s, dealDice, rand);
const next = (t: DiceTable, rand: Rand): DiceTable => nextRound(t, dealDice, rand);

/** Up to the quote: past any event, and the dice landed at `now`. */
function toQuote(t: DiceTable, now = 0): DiceTable {
  return landed(ackEvent(t), now);
}

/** Trade on the fair value — buy below it, sell above it, skip inside — one unit, reported exactly. */
function playRound(t: DiceTable, opts: { misreport?: boolean } = {}): DiceTable {
  t = toQuote(t);
  const call = rightCall(t.round.quote, t.round.fair);
  t = yourOrder(t, call === "skip" ? "skip" : { side: call, units: 1 }, 1500);
  if (t.stage === "report") {
    const pnl = roundPnl(t.round);
    t = submitReport(t, opts.misreport ? pnl + 1 : pnl);
  }
  return t;
}

function playGame(settings: DiceSettings, seed: number, misreportRound = 0): DiceTable {
  const rand = seeded(seed);
  let t = start(settings, rand);
  for (let guard = 0; guard < 20 && t.stage !== "over" && t.stage !== "final"; guard++) {
    t = next(playRound(t, { misreport: t.round.n === misreportRound }), rand);
  }
  return t;
}

describe("a game", () => {
  it("plays every round, and the balance is the trades and the penalties, nothing else", () => {
    for (let seed = 1; seed <= 40; seed++) {
      const t = playGame({ ...PRESETS.medium, difficulty: "medium", reportSeconds: null }, seed, 2);
      expect(t.stage).toBe("over");
      expect(t.history).toHaveLength(4);
      const moved = t.history.reduce((s, r) => s + roundPnl(r) + r.adjust, 0);
      expect(t.balance).toBe(STARTING_BALANCE + moved);
    }
  });

  it("from the second round on, an event comes first when events are on, and never in the first", () => {
    let seen = 0;
    for (let seed = 1; seed <= 60; seed++) {
      const rand = seeded(seed);
      let t = start({ ...DEFAULT_DICE, events: true, rounds: 3 }, rand);
      expect(t.round.event).toBeNull();
      expect(t.stage).toBe("rolling");
      t = next(playRound(t), rand);
      if (t.round.event) {
        seen++;
        expect(t.stage).toBe("event");
        expect(t.round.items.every((d) => d.worth >= 1)).toBe(true);
      }
    }
    expect(seen).toBeGreaterThan(20);
  });

  it("a careful player reports exactly and pays nothing for it", () => {
    const t = playGame(DEFAULT_DICE, 7);
    expect(t.history.every((r) => r.adjust === 0)).toBe(true);
    const o = outcome(t);
    expect(o.exact).toBe(o.reports);
    expect(o.rightSide).toBe(o.edges);
  });
});

describe("what a mistake costs", () => {
  /** A round-one quote where buying one unit makes money. */
  function profitableBuy(): DiceTable {
    for (let seed = 1; seed < 400; seed++) {
      const t = toQuote(start(DEFAULT_DICE, seeded(seed)));
      if (t.round.settle > t.round.quote.ask) return t;
    }
    throw new Error("no seed gave a profitable first buy");
  }

  it("a wrong report forfeits the round's profit and 50 besides", () => {
    let t = yourOrder(profitableBuy(), { side: "buy", units: 3 }, 0);
    expect(t.stage).toBe("report");
    const profit = roundPnl(t.round);
    expect(profit).toBeGreaterThan(0);
    t = submitReport(t, profit + 1);
    expect(t.round.adjust).toBe(-profit - PENALTY);
    expect(t.balance).toBe(STARTING_BALANCE - PENALTY);
  });

  it("an order the balance cannot carry is forfeited at 50, and nothing trades", () => {
    const t = yourOrder(toQuote(start(DEFAULT_DICE, seeded(3))), { side: "buy", units: 1000 }, 0);
    expect(t.stage).toBe("settled");
    expect(t.round.move).toMatchObject({ kind: "forfeit", side: "buy", units: 1000 });
    expect(roundPnl(t.round)).toBe(0);
    expect(t.balance).toBe(STARTING_BALANCE - PENALTY);
  });

  it("half a unit, or none, is refused in words and the clock keeps running", () => {
    const t = toQuote(start(DEFAULT_DICE, seeded(3)), 0);
    for (const units of [0, 1.5, NaN]) {
      const again = yourOrder(t, { side: "sell", units }, 10);
      expect(again.stage).toBe("trading");
      expect(again.problem).toMatch(/whole unit/);
      expect(again.deadline).toBe(t.deadline);
    }
  });
});

describe("the clocks you set are the clocks you get", () => {
  it("time to trade runs from the dice landing, for as long as it was set", () => {
    for (const seconds of [1, 7, 13, 600]) {
      const t = toQuote(start({ ...DEFAULT_DICE, tradeSeconds: seconds }, seeded(1)), 50_000);
      expect(t.deadline).toBe(50_000 + seconds * 1000);
    }
  });

  it("no limit is no deadline at all — and a decision still has its time recorded", () => {
    let t = toQuote(start({ ...DEFAULT_DICE, tradeSeconds: null }, seeded(1)), 1000);
    expect(t.stage).toBe("trading");
    expect(t.deadline).toBeNull();
    t = yourOrder(t, "skip", 1000 + 8 * 60_000);
    expect(t.round.decidedMs).toBe(8 * 60_000);
  });

  it("running out of time to trade skips the round at no cost", () => {
    const t = tradingTimedOut(toQuote(start(DEFAULT_DICE, seeded(1))));
    expect(t.stage).toBe("settled");
    expect(t.round.move).toEqual({ kind: "timeout" });
    expect(t.balance).toBe(STARTING_BALANCE);
  });

  it("the report has no clock unless you set one; set, a late report is a wrong one", () => {
    const quoted = (s: DiceSettings) => toQuote(start(s, seeded(5)));
    const untimed = yourOrder(quoted(DEFAULT_DICE), { side: "buy", units: 1 }, 100);
    expect(untimed.stage).toBe("report");
    expect(untimed.deadline).toBeNull();

    const timed = yourOrder(quoted({ ...DEFAULT_DICE, reportSeconds: 12 }), { side: "buy", units: 1 }, 100);
    expect(timed.deadline).toBe(100 + 12_000);
    const late = reportTimedOut(timed);
    const pnl = roundPnl(late.round);
    expect(late.round).toMatchObject({ reportLate: true, reported: null });
    expect(late.round.adjust).toBe(pnl >= 0 ? -pnl - PENALTY : -PENALTY);
    expect(late.balance).toBe(STARTING_BALANCE + pnl + late.round.adjust);
  });
});

describe("the end of a game", () => {
  it("with the balance hidden, ends by asking for it — wrong costs 50", () => {
    const t = playGame({ ...PRESETS.hard, difficulty: "hard", events: false, reportSeconds: null }, 12);
    expect(t.stage).toBe("final");
    expect(submitFinal(t, t.balance)).toMatchObject({ stage: "over", finalAdjust: 0, balance: t.balance });
    const wrong = submitFinal(t, t.balance + 5);
    expect(wrong.finalAdjust).toBe(-PENALTY);
    expect(wrong.balance).toBe(t.balance - PENALTY);
  });

  it("a balance gone negative ends the game there, with no question after it", () => {
    const rand = seeded(3);
    let t = start({ ...DEFAULT_DICE, showBalance: false }, rand);
    t = { ...t, balance: 20 };
    t = yourOrder(toQuote(t), { side: "buy", units: 1000 }, 0);
    expect(t.balance).toBe(20 - PENALTY);
    t = next(t, rand);
    expect(t).toMatchObject({ stage: "over", ending: "bust" });
  });

  it("stopped by hand keeps the rounds already settled, and not the one in play", () => {
    const rand = seeded(5);
    let t = next(playRound(start(DEFAULT_DICE, rand)), rand);
    t = stopGame(toQuote(t));
    expect(t).toMatchObject({ stage: "over", ending: "stopped" });
    expect(t.history).toHaveLength(1);
  });
});

describe("what is kept", () => {
  it("a session survives the round trip through the file, and counts toward the statistics", () => {
    const t = playGame({ ...DEFAULT_DICE, tradeSeconds: 13 }, 9, 2);
    const record = toTakerSession(t, describeDice, new Date("2026-09-24T10:00:00Z"))!;
    expect(record.rounds).toHaveLength(3);
    expect(record.settings.tradeSeconds).toBe(13);
    expect(record.rounds[0]).toMatchObject({ ms: 1500, total: t.history[0].settle });
    const back = parseTakerSession(JSON.stringify(record))!;
    expect(back).toEqual(record);
    expect(parseTakerSession("{not json")).toBeNull();
    expect(parseTakerSession('{"at":"2026-09-24T10:00:00Z"}')).toBeNull();

    const st = takerStanding([back, { ...back, at: "2026-09-24T11:00:00Z", pnl: back.pnl + 30 }, { ...back, ending: "stopped", pnl: 999 }]);
    expect(st.sessions).toBe(3);
    expect(st.scored).toHaveLength(2);
    expect(st.best).toBe(back.pnl + 30);
    expect(st.decisionMs).toBe(1500);
  });

  it("a game stopped before any round settled is not a session", () => {
    expect(toTakerSession(stopGame(start(DEFAULT_DICE, seeded(1))), describeDice)).toBeNull();
  });
});
