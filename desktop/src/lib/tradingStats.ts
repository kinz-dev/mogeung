/**
 * What the Trading Game remembers — one line per session in
 * `~/.mogeung/trading-game.jsonl` (see [`lib/arcade.ts`](arcade.ts)) — and the
 * lobby's statistics made from it.
 *
 * Like the mental-math history, a record keeps what happened — the settings,
 * each round's hand, quote, trades and reports — rather than only what it came
 * to, so a better statistic later is computed over every session already
 * played.
 */

import { appendLine, loadLines } from "@/lib/arcade";
import { eventText, settlePrice, STARTING_BALANCE, type TgSettings } from "@/lib/tradingGame";
import { outcome, yourPnl, type Table } from "@/lib/tradingTable";

export interface RoundRecord {
  n: number;
  /** The hand as dealt, e.g. `"7♠ Q♥ 3♦"`, and which cards were face up. */
  cards: string;
  open: boolean[];
  event: string | null;
  maker: string | null;
  bid: number | null;
  ask: number | null;
  price: number;
  /** What you did: `maker`, `buy`, `sell`, `skip`, `timeout`, `forfeit`, `none`. */
  you: string;
  volume: number;
  pnl: number;
  reported: number | null;
  adjust: number;
}

export interface SessionRecord {
  v: 1;
  at: string;
  settings: TgSettings;
  ending: "complete" | "bust" | "stopped";
  balance: number;
  pnl: number;
  place: number;
  reports: number;
  exact: number;
  finalReported: number | null;
  rounds: RoundRecord[];
}

/** The record of a finished session, or `null` when not one round was played. */
export function toSession(t: Table, at: Date = new Date()): SessionRecord | null {
  if (t.history.length === 0 || t.ending === null) return null;
  const o = outcome(t);
  const name = (id: number | null) => t.players.find((p) => p.id === id)?.name ?? null;
  return {
    v: 1,
    at: at.toISOString(),
    settings: t.settings,
    ending: t.ending,
    balance: o.balance,
    pnl: o.pnl,
    place: o.place,
    reports: o.reports,
    exact: o.exact,
    finalReported: t.finalReported,
    rounds: t.history.map((r) => {
      const move = r.you;
      return {
        n: r.n,
        cards: r.cards.map((c) => `${c.rank}${c.suit}`).join(" "),
        open: r.cards.map((c) => c.open),
        event: r.event ? eventText(r.event) : null,
        maker: name(r.maker),
        bid: r.quote?.bid ?? null,
        ask: r.quote?.ask ?? null,
        price: r.price ?? settlePrice(r.cards),
        you: move.kind === "trade" || move.kind === "forfeit" ? (move.kind === "trade" ? move.side : "forfeit") : move.kind,
        volume: move.kind === "trade" || move.kind === "forfeit" ? move.volume : 0,
        pnl: yourPnl(r, t.you),
        reported: r.reported,
        adjust: r.adjust,
      };
    }),
  };
}

export function parseSession(line: string): SessionRecord | null {
  try {
    const v = JSON.parse(line) as Partial<SessionRecord>;
    if (!v || typeof v !== "object" || typeof v.at !== "string" || Number.isNaN(Date.parse(v.at))) return null;
    if (typeof v.pnl !== "number" || typeof v.balance !== "number" || typeof v.place !== "number") return null;
    if (!v.settings || typeof v.settings !== "object" || !Array.isArray(v.rounds)) return null;
    return {
      v: 1,
      at: v.at,
      settings: v.settings as TgSettings,
      ending: v.ending === "bust" || v.ending === "stopped" ? v.ending : "complete",
      balance: v.balance,
      pnl: v.pnl,
      place: v.place,
      reports: typeof v.reports === "number" ? v.reports : 0,
      exact: typeof v.exact === "number" ? v.exact : 0,
      finalReported: typeof v.finalReported === "number" ? v.finalReported : null,
      rounds: v.rounds as RoundRecord[],
    };
  } catch {
    return null;
  }
}

export async function loadSessions(): Promise<SessionRecord[]> {
  return (await loadLines("trading-game"))
    .map(parseSession)
    .filter((s): s is SessionRecord => s !== null)
    .sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

export function appendSession(s: SessionRecord): Promise<boolean> {
  return appendLine("trading-game", s);
}

export interface Standing {
  sessions: number;
  /** Sessions played to the end — a stopped one is not a result. */
  scored: SessionRecord[];
  best: number | null;
  latest: number | null;
  average: number | null;
  wins: number;
  /** Share of reported rounds that were exact, whole percent. */
  exactness: number | null;
}

export function standing(sessions: SessionRecord[]): Standing {
  const scored = sessions.filter((s) => s.ending !== "stopped");
  const pnls = scored.map((s) => s.pnl);
  const reports = scored.reduce((t, s) => t + s.reports, 0);
  const exact = scored.reduce((t, s) => t + s.exact, 0);
  return {
    sessions: sessions.length,
    scored,
    best: pnls.length ? Math.max(...pnls) : null,
    latest: pnls.length ? pnls[pnls.length - 1] : null,
    average: pnls.length ? Math.round(pnls.reduce((a, b) => a + b, 0) / pnls.length) : null,
    wins: scored.filter((s) => s.place === 1).length,
    exactness: reports ? Math.round((exact / reports) * 100) : null,
  };
}

/** `+$142`, `−$60`. */
export const money = (n: number) => `${n >= 0 ? "+" : "−"}$${Math.abs(Math.round(n))}`;

export { STARTING_BALANCE };
