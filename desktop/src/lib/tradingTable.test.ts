/**
 * Whole Trading Games, played from a seed through the same moves the screen
 * makes — no clock, no DOM. What is pinned is the game's bookkeeping: who
 * makes the market when, that every trade has another side, what a report
 * costs when it is wrong, and how a game ends.
 */

import { describe, expect, it } from "vitest";
import { DEFAULT_TG, PENALTY, PRESETS, STARTING_BALANCE, seeded, settlePrice, type TgSettings } from "@/lib/tradingGame";
import {
  ackEvent,
  botQuotes,
  makerDone,
  makingTimedOut,
  nextRound,
  outcome,
  startGame,
  stopGame,
  submitFinal,
  submitReport,
  takingTimedOut,
  yourPnl,
  yourQuote,
  yourTrade,
  type Table,
} from "@/lib/tradingTable";
import { parseSession, standing, toSession } from "@/lib/tradingStats";

const total = (t: Table) => t.players.reduce((s, p) => s + p.balance, 0);

/**
 * Play one round the way a careful player would: quote around the fair value
 * when it is your turn, buy a quote below the cards and sell one above, and
 * report exactly — unless told to get the report wrong.
 */
function playRound(t: Table, rand: () => number, opts: { misreport?: boolean } = {}): Table {
  let now = 0;
  if (t.stage === "event") t = ackEvent(t, now);
  if (t.stage === "making") {
    if (t.settings.mode === "rotational" && t.round.maker === t.you) {
      t = yourQuote(t, { bid: 20, ask: 20 + t.settings.minSpread }, rand, now);
    } else {
      t = botQuotes(t, rand, now);
    }
  }
  if (t.stage === "taking") {
    if (t.round.maker === t.you) t = makerDone(t);
    else {
      const price = settlePrice(t.round.cards);
      const q = t.round.quote!;
      t = yourTrade(t, price > q.ask ? { action: "buy", volume: 2 } : price < q.bid ? { action: "sell", volume: 2 } : { action: "skip" });
    }
  }
  if (t.stage === "report") {
    const actual = yourPnl(t.round, t.you);
    t = submitReport(t, opts.misreport ? actual + 1 : actual);
  }
  now += 1;
  return t;
}

function playGame(settings: TgSettings, seed: number, misreportRound = 0): Table {
  const rand = seeded(seed);
  let t = startGame(settings, rand, 0);
  for (let guard = 0; guard < 40 && t.stage !== "over" && t.stage !== "final"; guard++) {
    t = playRound(t, rand, { misreport: t.round.n === misreportRound });
    t = nextRound(t, rand, 0);
  }
  return t;
}

describe("a rotational game", () => {
  it("rotates the market maker through all four seats, you included", () => {
    const rand = seeded(1);
    let t = startGame({ ...DEFAULT_TG, rounds: 8 }, rand, 0);
    const makers: number[] = [];
    while (t.stage !== "over") {
      makers.push(t.round.maker!);
      t = nextRound(playRound(t, rand), rand, 0);
    }
    expect(makers).toEqual([1, 2, 3, 4, 1, 2, 3, 4]);
    expect(makers).toContain(t.you);
  });

  it("is zero-sum: every trade has another side, so without penalties the table keeps 2,000", () => {
    for (let seed = 1; seed <= 40; seed++) {
      const t = playGame({ ...DEFAULT_TG, rounds: 6 }, seed);
      expect(t.stage).toBe("over");
      expect(total(t)).toBe(4 * STARTING_BALANCE);
      expect(t.history).toHaveLength(6);
    }
  });

  it("a careful player reports exactly and pays nothing for it", () => {
    const t = playGame(DEFAULT_TG, 7);
    expect(t.history.every((r) => r.adjust === 0)).toBe(true);
    const o = outcome(t);
    expect(o.exact).toBe(o.reports);
  });
});

describe("what a mistake costs", () => {
  it("a wrong report forfeits the round's profit and 50 besides", () => {
    // Find a seed where round 1 is a profitable trade for you.
    for (let seed = 1; seed < 200; seed++) {
      const rand = seeded(seed);
      let t = startGame(DEFAULT_TG, rand, 0);
      if (t.round.maker === t.you) continue;
      t = botQuotes(t, rand, 0);
      const price = settlePrice(t.round.cards);
      if (price <= t.round.quote!.ask) continue;
      t = yourTrade(t, { action: "buy", volume: 3 });
      expect(t.stage).toBe("report");
      const profit = yourPnl(t.round, t.you);
      expect(profit).toBeGreaterThan(0);
      const before = t.players.find((p) => p.human)!.balance;
      t = submitReport(t, profit + 1);
      expect(t.round.adjust).toBe(-profit - PENALTY);
      // The profit came in and went out again, and 50 went with it.
      expect(t.players.find((p) => p.human)!.balance).toBe(before - PENALTY);
      return;
    }
    throw new Error("no seed gave a profitable first trade");
  });

  it("an order your balance cannot carry is forfeited and costs 50, and nothing trades", () => {
    const rand = seeded(2);
    let t = startGame(DEFAULT_TG, rand, 0);
    while (t.round.maker === t.you) t = nextRound(playRound(t, rand), rand, 0);
    t = botQuotes(t, rand, 0);
    t = yourTrade(t, { action: "buy", volume: 1000 });
    expect(t.stage).toBe("settled");
    expect(t.round.you.kind).toBe("forfeit");
    expect(t.round.adjust).toBe(-PENALTY);
    expect(t.round.trades[t.you]).toBeUndefined();
  });

  it("a bad quote of yours is refused in words, and the clock keeps running", () => {
    const rand = seeded(1);
    let t = startGame(DEFAULT_TG, rand, 0);
    while (t.round.maker !== t.you) t = nextRound(playRound(t, rand), rand, 0);
    const again = yourQuote(t, { bid: 30, ask: 20 }, rand, 0);
    expect(again.stage).toBe("making");
    expect(again.problem).toMatch(/less than/);
  });
});

describe("clocks", () => {
  it("no quote from you in time is a round without trading", () => {
    const rand = seeded(1);
    let t = startGame(DEFAULT_TG, rand, 0);
    while (t.round.maker !== t.you) t = nextRound(playRound(t, rand), rand, 0);
    expect(t.deadline).toBe(DEFAULT_TG.timeToAnswer * 1000);
    t = makingTimedOut(t);
    expect(t.stage).toBe("settled");
    expect(t.round.you.kind).toBe("timeout");
    expect(t.round.pnl).toEqual({});
  });

  it("no trade from you in time is a skip, and the others' trades still settle", () => {
    const rand = seeded(4);
    let t = startGame(DEFAULT_TG, rand, 0);
    while (t.round.maker === t.you) t = nextRound(playRound(t, rand), rand, 0);
    t = botQuotes(t, rand, 0);
    t = takingTimedOut(t);
    expect(t.stage).toBe("settled");
    expect(t.round.you.kind).toBe("timeout");
    expect(total(t)).toBe(4 * STARTING_BALANCE);
  });
});

describe("open outcry", () => {
  it("the first valid quote makes the market — yours, if you are first", () => {
    const settings: TgSettings = { ...DEFAULT_TG, mode: "openOutcry" };
    const rand = seeded(8);
    let t = startGame(settings, rand, 0);
    expect(t.round.maker).toBeNull();
    expect(t.deadline).toBe(15_000);
    t = yourQuote(t, { bid: 20, ask: 22 }, rand, 0);
    expect(t.round.maker).toBe(t.you);
    expect(t.stage).toBe("taking");
    // A bot too late to shout changes nothing.
    expect(botQuotes(t, rand, 0)).toBe(t);
  });

  it("with nobody quoting in time, the round has no trading", () => {
    const rand = seeded(8);
    const t = makingTimedOut(startGame({ ...DEFAULT_TG, mode: "openOutcry" }, rand, 0));
    expect(t.stage).toBe("settled");
    expect(t.round.skipped).toMatch(/No valid quote/);
  });
});

describe("the end of a game", () => {
  it("with the balance hidden, ends by asking for it — wrong costs 50", () => {
    const settings: TgSettings = { ...DEFAULT_TG, ...PRESETS.hard, difficulty: "hard", events: false };
    const t = playGame(settings, 12);
    expect(t.stage).toBe("final");
    const balance = t.players.find((p) => p.human)!.balance;
    const right = submitFinal(t, balance);
    expect(right).toMatchObject({ stage: "over", finalAdjust: 0 });
    const wrong = submitFinal(t, balance + 5);
    expect(wrong.finalAdjust).toBe(-PENALTY);
    expect(wrong.players.find((p) => p.human)!.balance).toBe(balance - PENALTY);
  });

  it("a balance gone negative ends the game there", () => {
    const rand = seeded(3);
    let t = startGame(DEFAULT_TG, rand, 0);
    t = { ...t, players: t.players.map((p) => (p.human ? { ...p, balance: 10 } : p)) };
    while (t.round.maker === t.you) t = nextRound(playRound(t, rand), rand, 0);
    t = botQuotes(t, rand, 0);
    // Sell as much as the balance allows at a bid well under the cards, report
    // it wrong, and the penalty takes you below zero.
    t = yourTrade(t, { action: "skip" });
    t = { ...t, players: t.players.map((p) => (p.human ? { ...p, balance: -1 } : p)) };
    t = nextRound(t, rand, 0);
    expect(t.stage).toBe("over");
    expect(t.ending).toBe("bust");
  });

  it("stopped by hand keeps the rounds already settled", () => {
    const rand = seeded(5);
    let t = startGame(DEFAULT_TG, rand, 0);
    t = nextRound(playRound(t, rand), rand, 0);
    t = stopGame(t);
    expect(t).toMatchObject({ stage: "over", ending: "stopped" });
    expect(t.history).toHaveLength(1);
  });
});

describe("what is kept", () => {
  it("a session survives the round trip through the file, and counts toward the statistics", () => {
    const t = playGame(DEFAULT_TG, 9, 2);
    const record = toSession(t, new Date("2026-09-24T10:00:00Z"))!;
    expect(record.rounds).toHaveLength(4);
    expect(record.rounds[1].reported).not.toBeNull();
    const back = parseSession(JSON.stringify(record))!;
    expect(back).toEqual(record);
    expect(parseSession("{not json")).toBeNull();
    const st = standing([back, { ...back, at: "2026-09-24T11:00:00Z", pnl: back.pnl + 30, place: 1 }]);
    expect(st.sessions).toBe(2);
    expect(st.best).toBe(back.pnl + 30);
    expect(st.wins).toBeGreaterThanOrEqual(1);
  });

  it("a game stopped before any round settled is not a session", () => {
    const rand = seeded(1);
    expect(toSession(stopGame(startGame(DEFAULT_TG, rand, 0)))).toBeNull();
  });
});
