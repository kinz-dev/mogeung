/**
 * A market-taking game in progress: a market maker quotes on a hand you can
 * only partly see, and you buy, sell or skip against it, report what you made,
 * and keep count of your balance. Two of the arcade's games are this game with
 * a different hand — the dice game (`lib/diceGame.ts`) and the card game
 * (`lib/cardGame.ts`) — so the round-by-round rules live here once, and each
 * game brings only its **dealer**: what a hand is, what it settles at, its
 * fair value and the worst it could come to.
 *
 * A round goes: **event** (if one fires) → **rolling** (the hand is dealt or
 * rolled; the screen says when it has landed) → **trading** (the quote is up
 * and your clock, if you set one, runs) → **report** (after a trade, you say
 * what you made) → **settled** (the hand turned over, your profit, what the
 * fair value said). Then the next round, until the last — or until your
 * balance goes negative, which ends it: with nothing left, every order is over
 * budget. With the balance hidden, the game ends by asking for it.
 *
 * The rules are the reference's for both games: an order the balance cannot
 * carry is forfeited at 50; running out of time costs nothing; a wrong report
 * forfeits the round's profit and 50 besides; a wrong final balance costs 50.
 *
 * Pure functions over a `TakerTable`, so the screen draws one and says when
 * time has passed, and a test plays a whole game from a seed.
 */

import { PENALTY, STARTING_BALANCE, reportAdjustment, type Quote, type Rand } from "@/lib/tradingGame";

export type Side = "buy" | "sell";

/** What every market-taking game's settings have; each adds its own. */
export interface TakerSettings {
  /** How many dice or cards in a hand. */
  count: number;
  rounds: number;
  /** Seconds to trade once the quote is up; `null` for no clock. */
  tradeSeconds: number | null;
  /** Seconds to report your profit; `null` — the reference's rule — for no clock. */
  reportSeconds: number | null;
  showBalance: boolean;
  events: boolean;
}

/** A dealt hand and everything the rules need to know about it. */
export interface Hand<I, E> {
  event: E | null;
  items: I[];
  quote: Quote;
  /** What the hand settles at: the sum of everything in it. */
  settle: number;
  /** The fair value, from what is face up and the average of what is not. */
  fair: number;
  /** The most it could settle at — what a sale must be able to survive. */
  worst: number;
}

/** A game's dealer: the hand for round `n`, event included. */
export type Dealer<I, E, S extends TakerSettings> = (n: number, settings: S, rand: Rand) => Hand<I, E>;

export type Stage = "event" | "rolling" | "trading" | "report" | "settled" | "final" | "over";

/** What you did with a round's quote. */
export type Move =
  | { kind: "trade"; side: Side; units: number }
  | { kind: "skip" }
  | { kind: "timeout" }
  /** An order your balance could not carry: refused, and 50 taken. */
  | { kind: "forfeit"; side: Side; units: number };

export interface TakerRound<I, E> extends Hand<I, E> {
  n: number;
  /** Set once you act, or the clock acts for you. */
  move: Move | null;
  /** When the quote went up, and how long you took to act on it. */
  quotedAt: number | null;
  decidedMs: number | null;
  /** What you said you made, when you were asked and answered. */
  reported: number | null;
  /** The report clock ran out before you answered. */
  reportLate: boolean;
  /** Penalties this round: a forfeited order, a wrong or missing report. */
  adjust: number;
}

export interface TakerTable<I, E, S extends TakerSettings> {
  settings: S;
  balance: number;
  stage: Stage;
  round: TakerRound<I, E>;
  history: TakerRound<I, E>[];
  /** When the current stage's clock runs out, if it has one. */
  deadline: number | null;
  /** A bad order, said back to you. */
  problem: string | null;
  finalReported: number | null;
  finalAdjust: number;
  ending: "complete" | "bust" | "stopped" | null;
}

function newRound<I, E, S extends TakerSettings>(n: number, settings: S, deal: Dealer<I, E, S>, rand: Rand): TakerRound<I, E> {
  return { ...deal(n, settings, rand), n, move: null, quotedAt: null, decidedMs: null, reported: null, reportLate: false, adjust: 0 };
}

const clock = (seconds: number | null, now: number) => (seconds === null ? null : now + seconds * 1000);

export function startGame<I, E, S extends TakerSettings>(settings: S, deal: Dealer<I, E, S>, rand: Rand): TakerTable<I, E, S> {
  const round = newRound(1, settings, deal, rand);
  return {
    settings,
    balance: STARTING_BALANCE,
    stage: round.event ? "event" : "rolling",
    round,
    history: [],
    deadline: null,
    problem: null,
    finalReported: null,
    finalAdjust: 0,
    ending: null,
  };
}

type Table = TakerTable<unknown, unknown, TakerSettings>;

/** The event has been read: deal. */
export function ackEvent<T extends Table>(t: T): T {
  return t.stage === "event" ? { ...t, stage: "rolling" } : t;
}

/** The hand is on the table: the quote is up and your clock starts. */
export function landed<T extends Table>(t: T, now: number): T {
  if (t.stage !== "rolling") return t;
  return { ...t, stage: "trading", round: { ...t.round, quotedAt: now }, deadline: clock(t.settings.tradeSeconds, now) };
}

/** A taker's profit once the hand settles. */
export function tradePnl(side: Side, units: number, q: Quote, settled: number): number {
  return side === "buy" ? units * (settled - q.ask) : units * (q.bid - settled);
}

/** Your profit this round, as the rules count it — what a report is checked against. */
export function roundPnl(r: Pick<TakerRound<unknown, unknown>, "move" | "quote" | "settle">): number {
  return r.move?.kind === "trade" ? tradePnl(r.move.side, r.move.units, r.quote, r.settle) : 0;
}

/** Which side the fair value favoured: buy below it, sell above it, or neither. */
export function rightCall(q: Quote, fair: number): Side | "skip" {
  if (fair > q.ask) return "buy";
  if (fair < q.bid) return "sell";
  return "skip";
}

/** Whether an order fits: a buy must be affordable, a sale must survive the worst case. */
export function orderFits(side: Side, units: number, balance: number, q: Quote, worst: number): boolean {
  if (side === "buy") return units * q.ask <= balance;
  return units * (worst - q.bid) <= balance;
}

/** Turn the hand over and pay: the trade's profit and every penalty. */
function settle<T extends Table>(t: T, round: T["round"]): T {
  return { ...t, round, balance: t.balance + roundPnl(round) + round.adjust, stage: "settled", deadline: null, problem: null };
}

const decided = (r: Table["round"], now: number) => (r.quotedAt === null ? null : Math.max(0, now - r.quotedAt));

/** Buy, sell or skip. An order must be whole and at least one unit. */
export function yourOrder<T extends Table>(t: T, order: { side: Side; units: number } | "skip", now: number): T {
  if (t.stage !== "trading") return t;
  const r = { ...t.round, decidedMs: decided(t.round, now) };
  if (order === "skip") return settle(t, { ...r, move: { kind: "skip" } });
  const units = order.units;
  if (!Number.isInteger(units) || units < 1) return { ...t, problem: "Trade at least one whole unit." };
  if (!orderFits(order.side, units, t.balance, r.quote, r.worst)) {
    // The reference's rule: an order the balance cannot carry is forfeited,
    // and costs 50 — knowing your limits is part of the game.
    return settle(t, { ...r, move: { kind: "forfeit", side: order.side, units }, adjust: -PENALTY });
  }
  return {
    ...t,
    round: { ...r, move: { kind: "trade", side: order.side, units } },
    stage: "report",
    deadline: clock(t.settings.reportSeconds, now),
    problem: null,
  };
}

/** The trading clock ran out: no trade, and — as in the reference — no penalty. */
export function tradingTimedOut<T extends Table>(t: T): T {
  if (t.stage !== "trading") return t;
  return settle(t, { ...t.round, move: { kind: "timeout" }, decidedMs: null });
}

/** What you said you made. */
export function submitReport<T extends Table>(t: T, reported: number): T {
  if (t.stage !== "report") return t;
  return settle(t, { ...t.round, reported, adjust: t.round.adjust + reportAdjustment(roundPnl(t.round), reported) });
}

/** The report clock ran out: no answer is a wrong answer. */
export function reportTimedOut<T extends Table>(t: T): T {
  if (t.stage !== "report") return t;
  return settle(t, { ...t.round, reportLate: true, adjust: t.round.adjust + reportAdjustment(roundPnl(t.round), NaN) });
}

/** On from a settled round: the next one, the final question, or the end. */
export function nextRound<I, E, S extends TakerSettings>(t: TakerTable<I, E, S>, deal: Dealer<I, E, S>, rand: Rand): TakerTable<I, E, S> {
  if (t.stage !== "settled") return t;
  const history = [...t.history, t.round];
  const bust = t.balance < 0;
  if (bust || t.round.n >= t.settings.rounds) {
    const ending = bust ? "bust" : "complete";
    return { ...t, history, stage: t.settings.showBalance || bust ? "over" : "final", deadline: null, ending };
  }
  const round = newRound(t.round.n + 1, t.settings, deal, rand);
  return { ...t, history, round, stage: round.event ? "event" : "rolling", deadline: null, problem: null };
}

/** With the balance hidden, the game ends by asking for it; wrong costs 50. */
export function submitFinal<T extends Table>(t: T, reported: number): T {
  if (t.stage !== "final") return t;
  const finalAdjust = reported === t.balance ? 0 : -PENALTY;
  return { ...t, balance: t.balance + finalAdjust, finalReported: reported, finalAdjust, stage: "over" };
}

/** Stopped by hand. Rounds already settled stand; the one in progress does not. */
export function stopGame<T extends Table>(t: T): T {
  if (t.stage === "over") return t;
  const history = t.stage === "settled" ? [...t.history, t.round] : t.history;
  return { ...t, history, stage: "over", deadline: null, ending: "stopped" };
}

// --- the end ----------------------------------------------------------------------

export interface TakerOutcome {
  balance: number;
  pnl: number;
  /** Rounds you traded in, and how many of those made money. */
  trades: number;
  won: number;
  /** Rounds you reported, and how many of those were exact. */
  reports: number;
  exact: number;
  /** Rounds where the quote left an edge, and how many you took on the right side. */
  edges: number;
  rightSide: number;
  /** Your average time from quote to order, over the rounds you acted in. */
  avgDecisionMs: number | null;
}

export function outcome(t: Table): TakerOutcome {
  const h = t.history;
  // Every trade is followed by a report, so the two counts share a list.
  const traded = h.filter((r) => r.move?.kind === "trade");
  const withEdge = h.filter((r) => rightCall(r.quote, r.fair) !== "skip");
  const times = h.map((r) => r.decidedMs).filter((ms): ms is number => ms !== null);
  return {
    balance: t.balance,
    pnl: t.balance - STARTING_BALANCE,
    trades: traded.length,
    won: traded.filter((r) => roundPnl(r) > 0).length,
    reports: traded.length,
    exact: traded.filter((r) => r.reported === roundPnl(r)).length,
    edges: withEdge.length,
    rightSide: withEdge.filter((r) => r.move?.kind === "trade" && r.move.side === rightCall(r.quote, r.fair)).length,
    avgDecisionMs: times.length ? Math.round(times.reduce((a, b) => a + b, 0) / times.length) : null,
  };
}

/** A round, in a few words, for the round-by-round table. */
export function moveLine(r: Pick<TakerRound<unknown, unknown>, "move" | "quote">): string {
  const m = r.move;
  switch (m?.kind) {
    case "trade":
      return `${m.side === "buy" ? "bought" : "sold"} ${m.units} at ${m.side === "buy" ? r.quote.ask : r.quote.bid}`;
    case "forfeit":
      return `${m.side} ${m.units} — over budget`;
    case "skip":
      return "skipped";
    case "timeout":
      return "time ran out";
    default:
      return "—";
  }
}

export { PENALTY, STARTING_BALANCE };
