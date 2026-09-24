/**
 * What the ETF game remembers — one line per session in
 * `~/.mogeung/etf-arbitrage.jsonl` (see [`lib/arcade.ts`](arcade.ts)) — and
 * the lobby's statistics made from it.
 *
 * Like the other games, a record keeps what happened — each round's NAV and
 * quote, what you did and how fast — rather than only what it came to, so a
 * better statistic later is computed over every session already played.
 */

import { appendLine, loadLines } from "@/lib/arcade";
import { rightSide, type EtfSettings } from "@/lib/etfGame";
import { YOU, outcome, roundWinner, type EtfTable } from "@/lib/etfTable";

export interface EtfRoundRecord {
  n: number;
  headline: string | null;
  nav: number;
  bid: number;
  ask: number;
  premiumPct: number;
  /** `buy`, `sell` or `pass`, and whether the clock passed for you. */
  you: string;
  units: number;
  timedOut: boolean;
  secs: number | null;
  /** In cents. */
  net: number;
  first: boolean;
  /** The side the NAV favoured. */
  right: "buy" | "sell" | null;
  /** Who made the round's best profit: `You`, a bot's name, or nobody. */
  winner: string | null;
}

export interface EtfSessionRecord {
  v: 1;
  at: string;
  settings: EtfSettings;
  basket: string;
  ending: "complete" | "stopped";
  /** In cents. */
  pnl: number;
  place: number;
  roundsWon: number;
  trades: number;
  right: number;
  firsts: number;
  avgSecs: number | null;
  /** Every player's P&L in cents, with the bots' personalities. */
  table: { name: string; style: string | null; pnl: number }[];
  rounds: EtfRoundRecord[];
}

/** The record of a finished session, or `null` when no round was played to its end. */
export function toEtfSession(t: EtfTable, at: Date = new Date()): EtfSessionRecord | null {
  if (t.history.length === 0 || t.ending === null) return null;
  const o = outcome(t);
  const name = (id: number | null) => (id === null ? null : (t.players.find((p) => p.id === id)?.name ?? null));
  return {
    v: 1,
    at: at.toISOString(),
    settings: t.settings,
    basket: t.markets[0].basket,
    ending: t.ending,
    pnl: o.pnl,
    place: o.place,
    roundsWon: o.roundsWon,
    trades: o.trades,
    right: o.right,
    firsts: o.firsts,
    avgSecs: o.avgSecs === null ? null : Math.round(o.avgSecs * 100) / 100,
    table: t.players.map((p) => ({ name: p.name, style: p.style, pnl: p.pnl })),
    rounds: t.history.map((r) => {
      const a = r.acts[YOU];
      return {
        n: r.n,
        headline: r.market.headline,
        nav: r.market.nav,
        bid: r.market.bid,
        ask: r.market.ask,
        premiumPct: r.market.premiumPct,
        you: a.action,
        units: a.units,
        timedOut: a.timedOut,
        secs: a.secs === null ? null : Math.round(a.secs * 100) / 100,
        net: a.score.net,
        first: a.first,
        right: rightSide(r.market),
        winner: name(roundWinner(r)),
      };
    }),
  };
}

const num = (v: unknown, d: number) => (typeof v === "number" && Number.isFinite(v) ? v : d);

export function parseEtfSession(line: string): EtfSessionRecord | null {
  try {
    const v = JSON.parse(line) as Partial<EtfSessionRecord>;
    if (!v || typeof v !== "object" || typeof v.at !== "string" || Number.isNaN(Date.parse(v.at))) return null;
    if (typeof v.pnl !== "number" || typeof v.place !== "number") return null;
    if (!v.settings || typeof v.settings !== "object" || !Array.isArray(v.rounds)) return null;
    return {
      v: 1,
      at: v.at,
      settings: v.settings as EtfSettings,
      basket: typeof v.basket === "string" ? v.basket : "",
      ending: v.ending === "stopped" ? "stopped" : "complete",
      pnl: v.pnl,
      place: v.place,
      roundsWon: num(v.roundsWon, 0),
      trades: num(v.trades, 0),
      right: num(v.right, 0),
      firsts: num(v.firsts, 0),
      avgSecs: typeof v.avgSecs === "number" ? v.avgSecs : null,
      table: Array.isArray(v.table) ? v.table : [],
      rounds: v.rounds as EtfRoundRecord[],
    };
  } catch {
    return null;
  }
}

export async function loadEtfSessions(): Promise<EtfSessionRecord[]> {
  return (await loadLines("etf-arbitrage"))
    .map(parseEtfSession)
    .filter((s): s is EtfSessionRecord => s !== null)
    .sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

export function appendEtfSession(s: EtfSessionRecord): Promise<boolean> {
  return appendLine("etf-arbitrage", s);
}

export interface EtfStanding {
  sessions: number;
  /** Sessions played to the end — a stopped one is not a result. */
  scored: EtfSessionRecord[];
  /** In cents. */
  best: number | null;
  latest: number | null;
  average: number | null;
  wins: number;
  /** Share of your trades on the right side, whole percent. */
  accuracy: number | null;
  /** Your average seconds to act, over every session. */
  speed: number | null;
}

export function etfStanding(sessions: EtfSessionRecord[]): EtfStanding {
  const scored = sessions.filter((s) => s.ending !== "stopped");
  const pnls = scored.map((s) => s.pnl);
  const trades = scored.reduce((t, s) => t + s.trades, 0);
  const right = scored.reduce((t, s) => t + s.right, 0);
  const secs = scored.flatMap((s) => s.rounds.filter((r) => !r.timedOut && typeof r.secs === "number").map((r) => r.secs!));
  return {
    sessions: sessions.length,
    scored,
    best: pnls.length ? Math.max(...pnls) : null,
    latest: pnls.length ? pnls[pnls.length - 1] : null,
    average: pnls.length ? Math.round(pnls.reduce((a, b) => a + b, 0) / pnls.length) : null,
    wins: scored.filter((s) => s.place === 1).length,
    accuracy: trades ? Math.round((right / trades) * 100) : null,
    speed: secs.length ? secs.reduce((a, b) => a + b, 0) / secs.length : null,
  };
}
