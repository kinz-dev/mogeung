/**
 * An ETF Arbitrage Game in progress, as pure functions over an `EtfTable` —
 * the screen draws one and says what time it is, and a test plays a whole
 * game from a seed without a clock or a DOM.
 *
 * A round goes: **intro** (the basket fills in and the headline shows; the
 * screen says when it is on the table) → **active** (the clock runs, if there
 * is one, and the four of you race: each bot trades when its own delay is up,
 * sooner once you have traded) → **summary** (the NAV revealed, everyone's
 * trade and P&L). It ends when all four have acted or the clock runs out; a
 * player who never acted passed.
 *
 * Time is passed in, never read: `now` is milliseconds on any clock the caller
 * likes, and the bots' turns are stored as the moment each is due.
 */

import {
  BOT_NAMES,
  LEVELS,
  MAX_UNITS,
  botDecision,
  botDelay,
  hurried,
  makeMarkets,
  score,
  type BotStyle,
  type EtfAction,
  type EtfSettings,
  type Market,
  type Rand,
  type Score,
} from "@/lib/etfGame";

export type Stage = "intro" | "active" | "summary" | "over";

export interface EtfPlayer {
  id: number;
  name: string;
  human: boolean;
  style: BotStyle | null;
  /** In cents. */
  pnl: number;
}

export interface Act {
  action: EtfAction;
  units: number;
  /** Seconds from the round opening; `null` when the clock acted for you. */
  secs: number | null;
  /** The first trade of the round — the one a bonus is paid on, if it made money. */
  first: boolean;
  timedOut: boolean;
  score: Score;
}

export interface EtfRound {
  n: number;
  market: Market;
  acts: Record<number, Act>;
}

export interface EtfTable {
  settings: EtfSettings;
  markets: Market[];
  players: EtfPlayer[];
  /** The current round, from 1. */
  n: number;
  stage: Stage;
  acts: Record<number, Act>;
  openedAt: number | null;
  deadline: number | null;
  /** When each bot still to act is due, and whether it is only passing because time is up. */
  due: Record<number, { at: number; pass: boolean }>;
  history: EtfRound[];
  problem: string | null;
  ending: "complete" | "stopped" | null;
}

export const YOU = 0;

export const market = (t: EtfTable) => t.markets[t.n - 1];

export function startGame(settings: EtfSettings, rand: Rand): EtfTable {
  const markets = makeMarkets(settings, rand);
  const names = [...BOT_NAMES];
  for (let i = names.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [names[i], names[j]] = [names[j], names[i]];
  }
  const players: EtfPlayer[] = [
    { id: YOU, name: "You", human: true, style: null, pnl: 0 },
    ...LEVELS[settings.level].bots.map((style, i) => ({ id: i + 1, name: names[i], human: false, style, pnl: 0 })),
  ];
  return {
    settings,
    markets,
    players,
    n: 1,
    stage: "intro",
    acts: {},
    openedAt: null,
    deadline: null,
    due: {},
    history: [],
    problem: null,
    ending: null,
  };
}

/**
 * The basket is on the table: the clock starts and every bot is given its
 * moment — its own delay, or, if that would outlast the clock, a pass half a
 * second before the end.
 */
export function openRound(t: EtfTable, now: number, rand: Rand): EtfTable {
  if (t.stage !== "intro") return t;
  const s = t.settings.seconds;
  const due: EtfTable["due"] = {};
  for (const p of t.players) {
    if (p.human) continue;
    const delay = botDelay(p.style!, rand);
    due[p.id] = s !== null && delay >= s ? { at: now + (s - 0.5) * 1000, pass: true } : { at: now + delay * 1000, pass: false };
  }
  return { ...t, stage: "active", openedAt: now, deadline: s === null ? null : now + s * 1000, due, problem: null };
}

/** One player's trade: first if no one has traded before it, and paid at once. */
function act(t: EtfTable, id: number, action: EtfAction, units: number, secs: number | null, timedOut = false): EtfTable {
  const first = action !== "pass" && !Object.values(t.acts).some((a) => a.action !== "pass");
  const s = score(action, units, market(t), first);
  const due = { ...t.due };
  delete due[id];
  return {
    ...t,
    acts: { ...t.acts, [id]: { action, units: action === "pass" ? 0 : units, secs, first, timedOut, score: s } },
    players: t.players.map((p) => (p.id === id ? { ...p, pnl: p.pnl + s.net } : p)),
    due,
  };
}

const allActed = (t: EtfTable) => t.players.every((p) => t.acts[p.id]);

/** The round is over: whoever has not acted passed, and it goes into the history. */
function endRound(t: EtfTable): EtfTable {
  let next = t;
  for (const p of t.players) if (!next.acts[p.id]) next = act(next, p.id, "pass", 0, null, true);
  return {
    ...next,
    stage: "summary",
    deadline: null,
    due: {},
    history: [...next.history, { n: next.n, market: market(next), acts: next.acts }],
  };
}

const secsSince = (t: EtfTable, now: number) => (t.openedAt === null ? 0 : Math.max(0, (now - t.openedAt) / 1000));

/**
 * Time has passed: every bot now due trades, in the order they were due — so
 * the first of them may take the bonus — and the round ends if everyone has
 * acted or the clock has run out.
 */
export function tick(t: EtfTable, now: number, rand: Rand): EtfTable {
  if (t.stage !== "active") return t;
  let next = t;
  const ready = Object.entries(t.due)
    .map(([id, d]) => ({ id: Number(id), ...d }))
    .filter((d) => d.at <= now)
    .sort((a, b) => a.at - b.at);
  for (const d of ready) {
    const bot = next.players.find((p) => p.id === d.id)!;
    const secs = secsSince(next, d.at);
    if (d.pass) next = act(next, d.id, "pass", 0, secs);
    else {
      const choice = botDecision(bot.style!, market(next), rand);
      next = act(next, d.id, choice.action, choice.units, secs);
    }
  }
  if (allActed(next) || (next.deadline !== null && now >= next.deadline)) return endRound(next);
  return next;
}

/**
 * Your trade: 1–20 whole units, or a pass. The bots still thinking hurry up —
 * each acts within half a second to two — as the reference's do.
 */
export function yourTrade(t: EtfTable, action: EtfAction, units: number, now: number, rand: Rand): EtfTable {
  if (t.stage !== "active" || t.acts[YOU]) return t;
  if (action !== "pass" && (!Number.isInteger(units) || units < 1 || units > MAX_UNITS)) {
    return { ...t, problem: `Trade 1 to ${MAX_UNITS} whole units.` };
  }
  let next = act(t, YOU, action, units, secsSince(t, now));
  const due: EtfTable["due"] = {};
  for (const id of Object.keys(next.due).map(Number)) due[id] = { at: now + hurried(rand) * 1000, pass: false };
  next = { ...next, due, problem: null };
  return allActed(next) ? endRound(next) : next;
}

/** On from a summary: the next round's basket, or the end. */
export function nextRound(t: EtfTable): EtfTable {
  if (t.stage !== "summary") return t;
  if (t.n >= t.markets.length) return { ...t, stage: "over", ending: "complete" };
  return { ...t, n: t.n + 1, stage: "intro", acts: {}, openedAt: null, deadline: null, due: {}, problem: null };
}

/** Stopped by hand. Rounds already summed up stand; the one in play does not. */
export function stopGame(t: EtfTable): EtfTable {
  if (t.stage === "over") return t;
  // A round in play was never scored in the history, but its trades are in
  // the balances — take them back out.
  const players =
    t.stage === "summary" ? t.players : t.players.map((p) => ({ ...p, pnl: p.pnl - (t.acts[p.id]?.score.net ?? 0) }));
  return { ...t, players, stage: "over", deadline: null, due: {}, ending: "stopped" };
}

// --- the end ----------------------------------------------------------------------------

/** The round's winner: the best net profit, if anyone made one. */
export function roundWinner(r: Pick<EtfRound, "acts">): number | null {
  let best: number | null = null;
  for (const [id, a] of Object.entries(r.acts)) {
    if (a.score.net > 0 && (best === null || a.score.net > r.acts[best].score.net)) best = Number(id);
  }
  return best;
}

export interface EtfOutcome {
  /** Your P&L in cents. */
  pnl: number;
  place: number;
  standings: EtfPlayer[];
  roundsWon: number;
  trades: number;
  /** Trades on the side the NAV favoured. */
  right: number;
  /** Rounds you were first in, and took the bonus. */
  firsts: number;
  /** Your average seconds to act, over the rounds you acted in. */
  avgSecs: number | null;
}

export function outcome(t: EtfTable): EtfOutcome {
  const standings = [...t.players].sort((a, b) => b.pnl - a.pnl || a.id - b.id);
  const mine = t.history.map((r) => r.acts[YOU]).filter((a): a is Act => !!a);
  const traded = mine.filter((a) => a.action !== "pass");
  const timed = mine.filter((a) => !a.timedOut && a.secs !== null).map((a) => a.secs!);
  return {
    pnl: t.players[YOU].pnl,
    place: standings.findIndex((p) => p.human) + 1,
    standings,
    roundsWon: t.history.filter((r) => roundWinner(r) === YOU).length,
    trades: traded.length,
    right: traded.filter((a) => a.score.gross > 0).length,
    firsts: traded.filter((a) => a.score.bonus > 0).length,
    avgSecs: timed.length ? timed.reduce((a, b) => a + b, 0) / timed.length : null,
  };
}
