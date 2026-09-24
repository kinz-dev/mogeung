/**
 * A Trading Game in progress: who is at the table, where the round stands, and
 * every move from one stage to the next — as pure functions, so the screen only
 * draws a `Table` and says when time has passed, and a test can play a whole
 * game from a seed without a clock or a DOM.
 *
 * A round goes: **event** (if one fires) → **making** (someone quotes) →
 * **taking** (the others trade) → **report** (you say what you made, if you
 * traded) → **settled** (the cards and every player's profit). Then the next
 * round, until the last — or until your balance goes negative, which ends the
 * game there, as it does in the reference. With the balance hidden, the game
 * ends by asking for it.
 */

import {
  OUTCRY_SECONDS,
  PENALTY,
  STARTING_BALANCE,
  botQuote,
  botTrade,
  deal,
  eventText,
  makerFeedback,
  maxVolume,
  orderFits,
  outcryQuote,
  pickEvent,
  quoteProblem,
  reportAdjustment,
  settle,
  settlePrice,
  tradePnl,
  type Action,
  type Card,
  type MarketEvent,
  type Quote,
  type Rand,
  type Side,
  type TgSettings,
} from "@/lib/tradingGame";

export const BOT_NAMES = ["Gauss", "Euler", "Noether"];

export interface Player {
  id: number;
  name: string;
  human: boolean;
  balance: number;
}

export type Stage = "event" | "making" | "taking" | "report" | "settled" | "final" | "over";

/** What you did this round. */
export type YourMove =
  | { kind: "none" }
  | { kind: "maker" }
  | { kind: "trade"; side: Side; volume: number }
  | { kind: "skip" }
  | { kind: "timeout" }
  /** An order your balance could not carry: refused, and 50 taken. */
  | { kind: "forfeit"; side: Side; volume: number };

export interface Round {
  n: number;
  cards: Card[];
  event: MarketEvent | null;
  maker: number | null;
  quote: Quote | null;
  /** Every taker's action, bots' included, once taking has begun. */
  trades: Record<number, Action>;
  you: YourMove;
  /** Set once the round settles. */
  price: number | null;
  pnl: Record<number, number>;
  /** What you said you made, when you were asked. */
  reported: number | null;
  /** Penalties to you this round: a wrong report, a forfeited order. */
  adjust: number;
  feedback: ReturnType<typeof makerFeedback> | null;
  /** Why the round had no trading, when it had none. */
  skipped: string | null;
}

export interface Table {
  settings: TgSettings;
  players: Player[];
  you: number;
  stage: Stage;
  round: Round;
  history: Round[];
  /** When the current stage's clock runs out, if it has one. */
  deadline: number | null;
  /** A bad quote you entered, said back to you. */
  problem: string | null;
  finalReported: number | null;
  finalAdjust: number;
  /** How it ended: every round played, a balance gone negative, or stopped by hand. */
  ending: "complete" | "bust" | "stopped" | null;
}

export const makerFor = (n: number, t: Pick<Table, "settings">): number | null =>
  t.settings.mode === "rotational" ? ((n - 1) % 4) + 1 : null;

const you = (t: Table) => t.players.find((p) => p.id === t.you)!;

function newRound(n: number, t: Pick<Table, "settings">, rand: Rand): Round {
  const event = n > 1 && t.settings.events ? pickEvent(t.settings.rounds, rand) : null;
  return {
    n,
    cards: deal(t.settings.cards, event, rand),
    event,
    maker: makerFor(n, t),
    quote: null,
    trades: {},
    you: { kind: "none" },
    price: null,
    pnl: {},
    reported: null,
    adjust: 0,
    feedback: null,
    skipped: null,
  };
}

/** The clock a stage runs on, if any. */
function deadlineFor(stage: Stage, t: Pick<Table, "settings" | "round" | "you">, now: number): number | null {
  if (stage === "making") {
    if (t.settings.mode === "openOutcry") return now + OUTCRY_SECONDS * 1000;
    return t.round.maker === t.you ? now + t.settings.timeToAnswer * 1000 : null;
  }
  if (stage === "taking" && t.round.maker !== t.you) return now + t.settings.timeToAnswer * 1000;
  return null;
}

function enter(t: Table, stage: Stage, now: number): Table {
  return { ...t, stage, problem: null, deadline: deadlineFor(stage, t, now) };
}

export function startGame(settings: TgSettings, rand: Rand, now: number): Table {
  const seat = 1 + Math.floor(rand() * 4);
  let b = 0;
  const players: Player[] = [1, 2, 3, 4].map((id) => ({
    id,
    name: id === seat ? "You" : BOT_NAMES[b++],
    human: id === seat,
    balance: STARTING_BALANCE,
  }));
  const shell = { settings };
  const round = newRound(1, shell, rand);
  const t: Table = {
    settings,
    players,
    you: seat,
    stage: "making",
    round,
    history: [],
    deadline: null,
    problem: null,
    finalReported: null,
    finalAdjust: 0,
    ending: null,
  };
  return enter(t, round.event ? "event" : "making", now);
}

/** The event card has been read. */
export function ackEvent(t: Table, now: number): Table {
  return t.stage === "event" ? enter(t, "making", now) : t;
}

/** A round with no trading in it, straight to settled. */
function noTrading(t: Table, why: string, move: YourMove = { kind: "none" }): Table {
  const round = { ...t.round, skipped: why, you: move, price: settlePrice(t.round.cards), pnl: {} };
  return { ...t, round, stage: "settled", deadline: null, problem: null };
}

/** The bots' trades against the quote now standing. */
function botsTrade(t: Table, rand: Rand): Record<number, Action> {
  const q = t.round.quote!;
  const trades: Record<number, Action> = {};
  for (const p of t.players) {
    if (p.human || p.id === t.round.maker) continue;
    const a = botTrade(q, t.round.cards, rand);
    if (a.action === "skip") {
      trades[p.id] = a;
      continue;
    }
    // A bot keeps the same rule you do: an order its balance cannot carry is
    // cut to what it can, or not made.
    const most = maxVolume(a.action, p.balance, q, t.round.cards, t.round.event);
    trades[p.id] = most > 0 ? { action: a.action, volume: Math.min(a.volume, most) } : { action: "skip" };
  }
  return trades;
}

function beginTaking(t: Table, rand: Rand, now: number): Table {
  const next = { ...t, round: { ...t.round, trades: botsTrade(t, rand) } };
  return enter(next, "taking", now);
}

/** Your quote, as maker or as a shout in open outcry. */
export function yourQuote(t: Table, q: Quote, rand: Rand, now: number): Table {
  if (t.stage !== "making" || t.round.quote) return t;
  const rotationalTurn = t.settings.mode === "rotational" && t.round.maker === t.you;
  if (!rotationalTurn && t.settings.mode !== "openOutcry") return t;
  const problem = quoteProblem(q, t.settings);
  if (problem) return { ...t, problem };
  const round = { ...t.round, quote: q, maker: t.you, you: { kind: "maker" } as YourMove };
  return beginTaking({ ...t, round }, rand, now);
}

/** A bot's quote: the rotational maker's turn, or the first bot shout in open outcry. */
export function botQuotes(t: Table, rand: Rand, now: number): Table {
  if (t.stage !== "making" || t.round.quote) return t;
  if (t.settings.mode === "rotational") {
    if (t.round.maker === t.you) return t;
    return beginTaking({ ...t, round: { ...t.round, quote: botQuote(t.settings, t.round.event, rand) } }, rand, now);
  }
  const bots = t.players.filter((p) => !p.human);
  const maker = bots[Math.floor(rand() * bots.length)].id;
  const quote = outcryQuote(t.settings, t.round.event, rand);
  return beginTaking({ ...t, round: { ...t.round, quote, maker } }, rand, now);
}

/** The making clock ran out with no quote. */
export function makingTimedOut(t: Table): Table {
  if (t.stage !== "making" || t.round.quote) return t;
  return t.settings.mode === "openOutcry"
    ? noTrading(t, "No valid quote in time — no trading this round.")
    : noTrading(t, "Time ran out before you quoted — no trading this round.", { kind: "timeout" });
}

/** Your trade, as a taker. */
export function yourTrade(t: Table, a: Action): Table {
  if (t.stage !== "taking" || t.round.maker === t.you) return t;
  if (a.action === "skip") {
    return settleRound({ ...t, round: { ...t.round, you: { kind: "skip" } } });
  }
  const volume = Math.floor(a.volume);
  if (!(volume >= 1)) return { ...t, problem: "Trade at least one unit." };
  const fits = orderFits(a.action, volume, you(t).balance, t.round.quote!, t.round.cards, t.round.event);
  if (!fits) {
    // The reference's rule: an order your balance cannot carry is forfeited,
    // and costs 50 — knowing your limits is part of the game.
    const round = { ...t.round, you: { kind: "forfeit", side: a.action, volume } as YourMove, adjust: t.round.adjust - PENALTY };
    return settleRound({ ...t, round });
  }
  const round = {
    ...t.round,
    you: { kind: "trade", side: a.action, volume } as YourMove,
    trades: { ...t.round.trades, [t.you]: { action: a.action, volume } },
  };
  return { ...t, round, stage: "report", deadline: null, problem: null };
}

/** The trading clock ran out before you acted. */
export function takingTimedOut(t: Table): Table {
  if (t.stage !== "taking" || t.round.maker === t.you) return t;
  return settleRound({ ...t, round: { ...t.round, you: { kind: "timeout" } } });
}

/** You were the maker and the bots have all traded. */
export function makerDone(t: Table): Table {
  if (t.stage !== "taking" || t.round.maker !== t.you) return t;
  return settleRound(t);
}

/** Your profit this round, as the rules count it — what a report is checked against. */
export function yourPnl(r: Round, youId: number): number {
  const q = r.quote;
  if (!q) return 0;
  const price = settlePrice(r.cards);
  if (r.maker === youId) return settle(youId, q, r.trades, price)[youId] ?? 0;
  const mine = r.trades[youId];
  return mine && mine.action !== "skip" ? tradePnl(mine.action, mine.volume, q, price) : 0;
}

/** What you said you made. */
export function submitReport(t: Table, reported: number): Table {
  if (t.stage !== "report") return t;
  const actual = yourPnl(t.round, t.you);
  const round = { ...t.round, reported, adjust: t.round.adjust + reportAdjustment(actual, reported) };
  return settleRound({ ...t, round });
}

/** Turn the cards, pay everyone, and apply your penalties. */
function settleRound(t: Table): Table {
  const r = t.round;
  const price = settlePrice(r.cards);
  const pnl = r.quote && r.maker !== null ? settle(r.maker, r.quote, r.trades, price) : {};
  const feedback = r.maker === t.you && r.quote ? makerFeedback(r.quote, r.cards, r.event, price) : null;
  const players = t.players.map((p) => ({
    ...p,
    balance: p.balance + (pnl[p.id] ?? 0) + (p.human ? r.adjust : 0),
  }));
  return { ...t, players, round: { ...r, price, pnl, feedback }, stage: "settled", deadline: null, problem: null };
}

/** On from a settled round: the next one, the final question, or the end. */
export function nextRound(t: Table, rand: Rand, now: number): Table {
  if (t.stage !== "settled") return t;
  const history = [...t.history, t.round];
  const bust = you(t).balance < 0;
  if (bust || t.round.n >= t.settings.rounds) {
    const ending = bust ? "bust" : "complete";
    return t.settings.showBalance || bust
      ? { ...t, history, stage: "over", deadline: null, ending }
      : { ...t, history, stage: "final", deadline: null, ending };
  }
  const round = newRound(t.round.n + 1, t, rand);
  return enter({ ...t, history, round }, round.event ? "event" : "making", now);
}

/** With the balance hidden, the game ends by asking for it. */
export function submitFinal(t: Table, reported: number): Table {
  if (t.stage !== "final") return t;
  const actual = you(t).balance;
  const finalAdjust = reported === actual ? 0 : -PENALTY;
  const players = t.players.map((p) => (p.human ? { ...p, balance: p.balance + finalAdjust } : p));
  return { ...t, players, finalReported: reported, finalAdjust, stage: "over" };
}

/** Stopped by hand. Rounds already played stand; the one in progress does not. */
export function stopGame(t: Table): Table {
  if (t.stage === "over") return t;
  const history = t.stage === "settled" ? [...t.history, t.round] : t.history;
  return { ...t, history, stage: "over", deadline: null, ending: "stopped" };
}

// --- the end ----------------------------------------------------------------------

export interface Outcome {
  balance: number;
  pnl: number;
  place: number;
  standings: Player[];
  /** Rounds you reported a profit in, and how many of those were exact. */
  reports: number;
  exact: number;
}

export function outcome(t: Table): Outcome {
  const standings = [...t.players].sort((a, b) => b.balance - a.balance);
  const mine = you(t);
  const reported = t.history.filter((r) => r.reported !== null);
  return {
    balance: mine.balance,
    pnl: mine.balance - STARTING_BALANCE,
    place: standings.findIndex((p) => p.human) + 1,
    standings,
    reports: reported.length,
    exact: reported.filter((r) => r.reported === yourPnl(r, t.you)).length,
  };
}

/** A round, in a few words, for the round-by-round table and the history. */
export function roundLine(r: Round, t: Pick<Table, "players" | "you">): string {
  const maker = t.players.find((p) => p.id === r.maker)?.name ?? "nobody";
  switch (r.you.kind) {
    case "maker":
      return `you made the market`;
    case "trade":
      return `you ${r.you.side === "buy" ? "bought" : "sold"} ${r.you.volume} (${maker} made it)`;
    case "forfeit":
      return `order forfeited — over your balance`;
    case "skip":
      return "you skipped";
    case "timeout":
      return "time ran out";
    default:
      return r.skipped ?? "no trading";
  }
}

export { eventText };
