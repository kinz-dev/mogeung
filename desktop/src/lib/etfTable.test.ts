/**
 * Whole ETF games, played from a seed through the moves the screen makes —
 * no clock, no DOM, time passed in by hand. What is pinned is the race: who
 * is first, when the bots act, when a round ends, that a pass is free and a
 * balance is the rounds' nets and nothing else.
 */

import { describe, expect, it } from "vitest";
import { DEFAULT_ETF, rightSide, seeded, type EtfSettings } from "@/lib/etfGame";
import { YOU, market, nextRound, openRound, outcome, roundWinner, startGame, stopGame, tick, yourTrade, type EtfTable } from "@/lib/etfTable";
import { etfStanding, parseEtfSession, toEtfSession } from "@/lib/etfStats";

/** Open a round at `at`, and trade the right side with `units` after `after` seconds. */
function playRound(t: EtfTable, rand: () => number, at: number, after: number, units = 10): EtfTable {
  t = openRound(t, at, rand);
  // Bots that were quicker than you go first.
  t = tick(t, at + after * 1000, rand);
  if (t.stage === "active") t = yourTrade(t, rightSide(market(t)) ?? "pass", units, at + after * 1000, rand);
  // And the rest, hurried, within two seconds.
  if (t.stage === "active") t = tick(t, at + after * 1000 + 2000, rand);
  return t;
}

function playGame(settings: EtfSettings, seed: number, after = 1): EtfTable {
  const rand = seeded(seed);
  let t = startGame(settings, rand);
  let at = 0;
  while (t.stage !== "over") {
    t = nextRound(playRound(t, rand, at, after));
    at += 60_000;
  }
  return t;
}

describe("a game", () => {
  it("plays every round, and each balance is its rounds' nets and nothing else", () => {
    for (let seed = 1; seed <= 30; seed++) {
      const t = playGame({ ...DEFAULT_ETF, level: "medium", rounds: 8, events: true }, seed);
      expect(t.ending).toBe("complete");
      expect(t.history).toHaveLength(8);
      for (const p of t.players) {
        expect(p.pnl).toBe(t.history.reduce((s, r) => s + r.acts[p.id].score.net, 0));
      }
    }
  });

  it("the three bots are the level's personalities, and never you", () => {
    const t = startGame({ ...DEFAULT_ETF, level: "hard" }, seeded(1));
    expect(t.players.map((p) => p.style)).toEqual([null, "quant", "sniper", "whale"]);
    expect(new Set(t.players.map((p) => p.name)).size).toBe(4);
  });

  it("the right side at full size, first, every round, is a winning game on easy", () => {
    const t = playGame({ ...DEFAULT_ETF, level: "easy" }, 5, 0.2);
    const o = outcome(t);
    expect(o.right).toBe(o.trades);
    expect(o.pnl).toBeGreaterThan(0);
    expect(o.firsts).toBeGreaterThan(0);
  });
});

describe("the race", () => {
  it("the first trade of a round takes the bonus; a later one does not", () => {
    const rand = seeded(2);
    let t = openRound(startGame({ ...DEFAULT_ETF, seconds: 20 }, rand), 0, rand);
    t = yourTrade(t, rightSide(market(t))!, 20, 100, rand);
    expect(t.acts[YOU].first).toBe(true);
    expect(t.acts[YOU].score.bonus).toBeGreaterThan(0);
    t = tick(t, 5_000, rand);
    const later = Object.entries(t.history.at(-1)!.acts).filter(([id]) => Number(id) !== YOU);
    expect(later.every(([, a]) => !a.first)).toBe(true);
  });

  it("each bot acts when its delay is up — sooner once you have traded", () => {
    const rand = seeded(4);
    let t = openRound(startGame({ ...DEFAULT_ETF, level: "easy", seconds: 60 }, rand), 0, rand);
    // Easy bots take five seconds at the least.
    expect(Object.values(t.due).every((d) => d.at >= 5000)).toBe(true);
    t = tick(t, 4_900, rand);
    expect(Object.keys(t.acts)).toHaveLength(0);
    t = yourTrade(t, "pass", 0, 4_900, rand);
    expect(Object.values(t.due).every((d) => d.at >= 4_900 + 500 && d.at <= 4_900 + 2000)).toBe(true);
    t = tick(t, 7_000, rand);
    expect(t.stage).toBe("summary");
  });

  it("a bot slower than the clock passes half a second before the end", () => {
    const rand = seeded(6);
    let t = openRound(startGame({ ...DEFAULT_ETF, level: "easy", seconds: 3 }, rand), 0, rand);
    expect(Object.values(t.due).every((d) => d.pass && d.at === 2_500)).toBe(true);
    t = tick(t, 2_600, rand);
    expect(Object.values(t.acts).every((a) => a.action === "pass" && !a.timedOut)).toBe(true);
    t = tick(t, 3_000, rand);
    expect(t.stage).toBe("summary");
    expect(t.history[0].acts[YOU]).toMatchObject({ action: "pass", timedOut: true, secs: null });
  });

  it("with no clock the round waits for you, however long, and the bots still trade", () => {
    const rand = seeded(7);
    let t = openRound(startGame({ ...DEFAULT_ETF, seconds: null }, rand), 0, rand);
    expect(t.deadline).toBeNull();
    t = tick(t, 3_600_000, rand);
    expect(t.stage).toBe("active");
    expect(Object.keys(t.acts)).toHaveLength(3);
    t = yourTrade(t, "buy", 5, 3_600_000, rand);
    expect(t.stage).toBe("summary");
    expect(t.history[0].acts[YOU].secs).toBe(3600);
  });

  it("an order outside 1–20 whole units is refused in words, and nothing is traded", () => {
    const rand = seeded(8);
    const t = openRound(startGame(DEFAULT_ETF, rand), 0, rand);
    for (const units of [0, 21, 2.5]) {
      const again = yourTrade(t, "buy", units, 100, rand);
      expect(again.acts[YOU]).toBeUndefined();
      expect(again.problem).toMatch(/1 to 20/);
    }
  });
});

describe("the end", () => {
  it("stopped mid-round, the round in play is taken back out of every balance", () => {
    const rand = seeded(9);
    let t = startGame(DEFAULT_ETF, rand);
    t = nextRound(playRound(t, rand, 0, 1));
    const after1 = t.players.map((p) => p.pnl);
    t = openRound(t, 60_000, rand);
    t = yourTrade(t, "buy", 20, 60_500, rand);
    t = stopGame(t);
    expect(t).toMatchObject({ stage: "over", ending: "stopped" });
    expect(t.history).toHaveLength(1);
    expect(t.players.map((p) => p.pnl)).toEqual(after1);
  });

  it("a round is won by the best profit, and by nobody when nobody made one", () => {
    const won = { acts: { 0: { score: { net: 500 } }, 1: { score: { net: 900 } }, 2: { score: { net: -100 } } } } as never;
    expect(roundWinner(won)).toBe(1);
    const lost = { acts: { 0: { score: { net: 0 } }, 1: { score: { net: -900 } } } } as never;
    expect(roundWinner(lost)).toBeNull();
  });

  it("a session survives the round trip through the file, and counts toward the statistics", () => {
    const t = playGame({ ...DEFAULT_ETF, rounds: 5, seconds: 25 }, 11);
    const record = toEtfSession(t, new Date("2026-09-24T10:00:00Z"))!;
    expect(record.rounds).toHaveLength(5);
    expect(record.settings.seconds).toBe(25);
    expect(record.table.map((p) => p.pnl)).toEqual(t.players.map((p) => p.pnl));
    const back = parseEtfSession(JSON.stringify(record))!;
    expect(back).toEqual(record);
    expect(parseEtfSession("{nope")).toBeNull();
    const st = etfStanding([back, { ...back, pnl: back.pnl + 100, place: 1 }, { ...back, ending: "stopped", pnl: 99_999 }]);
    expect(st.scored).toHaveLength(2);
    expect(st.best).toBe(back.pnl + 100);
    expect(st.accuracy).toBe(100);
    expect(st.speed).toBeCloseTo(1);
  });

  it("a game stopped before a round was summed up is not a session", () => {
    const rand = seeded(1);
    expect(toEtfSession(stopGame(openRound(startGame(DEFAULT_ETF, rand), 0, rand)))).toBeNull();
  });
});
