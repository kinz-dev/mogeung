/**
 * What a market-taking game remembers — the dice game in
 * `~/.mogeung/dice-game.jsonl`, the card game in `~/.mogeung/card-game.jsonl`
 * (see [`lib/arcade.ts`](arcade.ts)) — and the lobby's statistics made from it.
 *
 * Like the other games, a record keeps what happened — every hand, quote,
 * order, report and how long you took — rather than only what it came to, so
 * a better statistic later is computed over every session already played.
 */

import { appendLine, loadLines, type ArcadeGame } from "@/lib/arcade";
import { outcome, roundPnl, type TakerSettings, type TakerTable } from "@/lib/takerTable";

export interface TakerRoundRecord {
  n: number;
  event: string | null;
  /** The hand as it read, e.g. `3 5 2` or `7♠ Q♥ 3♦`, and which were face up. */
  hand: string;
  open: boolean[];
  bid: number;
  ask: number;
  total: number;
  fair: number;
  /** `buy`, `sell`, `skip`, `timeout`, `forfeit`. */
  you: string;
  units: number;
  /** From the quote going up to your order, when you placed one. */
  ms: number | null;
  pnl: number;
  reported: number | null;
  adjust: number;
}

export interface TakerSessionRecord<S extends TakerSettings = TakerSettings> {
  v: 1;
  at: string;
  settings: S;
  ending: "complete" | "bust" | "stopped";
  balance: number;
  pnl: number;
  trades: number;
  won: number;
  reports: number;
  exact: number;
  edges: number;
  rightSide: number;
  avgDecisionMs: number | null;
  finalReported: number | null;
  rounds: TakerRoundRecord[];
}

/** How a game writes one of its hand's items, and says whether it was face up. */
export interface Describe<I, E> {
  item: (i: I) => string;
  open: (i: I) => boolean;
  event: (e: E) => string;
}

/** The record of a finished session, or `null` when not one round was played. */
export function toTakerSession<I, E, S extends TakerSettings>(
  t: TakerTable<I, E, S>,
  describe: Describe<I, E>,
  at: Date = new Date(),
): TakerSessionRecord<S> | null {
  if (t.history.length === 0 || t.ending === null) return null;
  return {
    v: 1,
    at: at.toISOString(),
    settings: t.settings,
    ending: t.ending,
    ...outcome(t),
    finalReported: t.finalReported,
    rounds: t.history.map((r) => {
      const m = r.move;
      return {
        n: r.n,
        event: r.event ? describe.event(r.event) : null,
        hand: r.items.map(describe.item).join(" "),
        open: r.items.map(describe.open),
        bid: r.quote.bid,
        ask: r.quote.ask,
        total: r.settle,
        fair: Math.round(r.fair * 100) / 100,
        you: m === null ? "none" : m.kind === "trade" || m.kind === "forfeit" ? (m.kind === "trade" ? m.side : "forfeit") : m.kind,
        units: m?.kind === "trade" || m?.kind === "forfeit" ? m.units : 0,
        ms: r.decidedMs,
        pnl: roundPnl(r),
        reported: r.reported,
        adjust: r.adjust,
      };
    }),
  };
}

const num = (v: unknown, d: number) => (typeof v === "number" && Number.isFinite(v) ? v : d);

export function parseTakerSession<S extends TakerSettings>(line: string): TakerSessionRecord<S> | null {
  try {
    const v = JSON.parse(line) as Partial<TakerSessionRecord<S>>;
    if (!v || typeof v !== "object" || typeof v.at !== "string" || Number.isNaN(Date.parse(v.at))) return null;
    if (typeof v.pnl !== "number" || typeof v.balance !== "number") return null;
    if (!v.settings || typeof v.settings !== "object" || !Array.isArray(v.rounds)) return null;
    return {
      v: 1,
      at: v.at,
      settings: v.settings as S,
      ending: v.ending === "bust" || v.ending === "stopped" ? v.ending : "complete",
      balance: v.balance,
      pnl: v.pnl,
      trades: num(v.trades, 0),
      won: num(v.won, 0),
      reports: num(v.reports, 0),
      exact: num(v.exact, 0),
      edges: num(v.edges, 0),
      rightSide: num(v.rightSide, 0),
      avgDecisionMs: typeof v.avgDecisionMs === "number" ? v.avgDecisionMs : null,
      finalReported: typeof v.finalReported === "number" ? v.finalReported : null,
      rounds: v.rounds as TakerRoundRecord[],
    };
  } catch {
    return null;
  }
}

export async function loadTakerSessions<S extends TakerSettings>(game: ArcadeGame): Promise<TakerSessionRecord<S>[]> {
  return (await loadLines(game))
    .map((l) => parseTakerSession<S>(l))
    .filter((s): s is TakerSessionRecord<S> => s !== null)
    .sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

export function appendTakerSession(game: ArcadeGame, s: TakerSessionRecord<TakerSettings>): Promise<boolean> {
  return appendLine(game, s);
}

export interface TakerStanding<S extends TakerSettings = TakerSettings> {
  sessions: number;
  /** Sessions played to the end — a stopped one is not a result. */
  scored: TakerSessionRecord<S>[];
  best: number | null;
  latest: number | null;
  average: number | null;
  /** Whole percents over every scored session, or `null` with nothing to count. */
  exactness: number | null;
  rightSide: number | null;
  /** Average time from quote to order, over every order placed. */
  decisionMs: number | null;
}

const pct = (part: number, whole: number) => (whole ? Math.round((part / whole) * 100) : null);

export function takerStanding<S extends TakerSettings>(sessions: TakerSessionRecord<S>[]): TakerStanding<S> {
  const scored = sessions.filter((s) => s.ending !== "stopped");
  const pnls = scored.map((s) => s.pnl);
  const sum = (f: (s: TakerSessionRecord<S>) => number) => scored.reduce((t, s) => t + f(s), 0);
  const times = scored.flatMap((s) => s.rounds.map((r) => r.ms)).filter((ms): ms is number => typeof ms === "number");
  return {
    sessions: sessions.length,
    scored,
    best: pnls.length ? Math.max(...pnls) : null,
    latest: pnls.length ? pnls[pnls.length - 1] : null,
    average: pnls.length ? Math.round(pnls.reduce((a, b) => a + b, 0) / pnls.length) : null,
    exactness: pct(sum((s) => s.exact), sum((s) => s.reports)),
    rightSide: pct(sum((s) => s.rightSide), sum((s) => s.edges)),
    decisionMs: times.length ? Math.round(times.reduce((a, b) => a + b, 0) / times.length) : null,
  };
}
