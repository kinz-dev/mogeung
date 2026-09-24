/**
 * What the Event Contracts game remembers — one line per session in
 * `~/.mogeung/event-contracts.jsonl` (see [`lib/arcade.ts`](arcade.ts)) — and
 * the lobby's statistics made from it.
 *
 * Every round keeps its brief, the truth, your quote and the flow, so how you
 * price — centre, width, which way you lean — can be looked at again later.
 */

import { appendLine, loadLines } from "@/lib/arcade";
import { mid, type EventSettings, type Fill } from "@/lib/eventContracts";
import { eventOutcome, type EventRun } from "@/lib/eventRun";

export interface EventRoundRecord {
  title: string;
  prior: number;
  direction: string;
  ratio: number | null;
  implied: number;
  truth: number;
  happened: boolean;
  outcome: "quoted" | "skipped" | "timeout";
  bid: number | null;
  ask: number | null;
  modelBid: number;
  modelAsk: number;
  fills: Fill[];
  pnl: number;
  calibration: number;
  score: number;
  ms: number | null;
  verdict: string | null;
}

export interface EventSessionRecord {
  v: 1;
  at: string;
  settings: EventSettings;
  ending: "complete" | "stopped";
  score: number;
  pnl: number;
  calibration: number;
  quoted: number;
  centred: number;
  rounds: EventRoundRecord[];
}

const r2 = (x: number) => Math.round(x * 100) / 100;

export function toEventSession(run: EventRun, at: Date = new Date()): EventSessionRecord | null {
  if (run.rounds.length === 0 || run.ending === null) return null;
  const o = eventOutcome(run);
  return {
    v: 1,
    at: at.toISOString(),
    settings: run.settings,
    ending: run.ending,
    score: o.score,
    pnl: o.pnl,
    calibration: o.calibration,
    quoted: o.quoted,
    centred: o.centred,
    rounds: run.rounds.map((x) => ({
      title: x.contract.brief.title,
      prior: x.contract.brief.prior,
      direction: x.contract.brief.direction,
      ratio: x.contract.brief.ratio,
      implied: r2(x.contract.implied),
      truth: r2(x.contract.truth),
      happened: x.contract.happened,
      outcome: x.outcome,
      bid: x.quote?.bid ?? null,
      ask: x.quote?.ask ?? null,
      modelBid: x.model.bid,
      modelAsk: x.model.ask,
      fills: x.fills,
      pnl: x.pnl,
      calibration: x.calibration,
      score: x.score,
      ms: x.ms,
      verdict: x.verdict?.tone ?? null,
    })),
  };
}

export function parseEventSession(line: string): EventSessionRecord | null {
  try {
    const v = JSON.parse(line) as Partial<EventSessionRecord>;
    if (!v || typeof v !== "object" || typeof v.at !== "string" || Number.isNaN(Date.parse(v.at))) return null;
    if (typeof v.score !== "number" || !Array.isArray(v.rounds) || !v.settings || typeof v.settings !== "object") return null;
    return {
      v: 1,
      at: v.at,
      settings: v.settings as EventSettings,
      ending: v.ending === "stopped" ? "stopped" : "complete",
      score: v.score,
      pnl: typeof v.pnl === "number" ? v.pnl : 0,
      calibration: typeof v.calibration === "number" ? v.calibration : 0,
      quoted: typeof v.quoted === "number" ? v.quoted : 0,
      centred: typeof v.centred === "number" ? v.centred : 0,
      rounds: v.rounds as EventRoundRecord[],
    };
  } catch {
    return null;
  }
}

export async function loadEventSessions(): Promise<EventSessionRecord[]> {
  return (await loadLines("event-contracts"))
    .map(parseEventSession)
    .filter((s): s is EventSessionRecord => s !== null)
    .sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

export function appendEventSession(s: EventSessionRecord): Promise<boolean> {
  return appendLine("event-contracts", s);
}

export interface EventStanding {
  sessions: number;
  scored: EventSessionRecord[];
  /** Per round, so sessions of different lengths compare. */
  best: number | null;
  latest: number | null;
  average: number | null;
  /** Share of quoted rounds whose mid sat within two points of the update. */
  centred: number | null;
  /** Your quotes' average width against the balanced one's, in cents: positive wider. */
  widthLean: number | null;
}

export const perRound = (s: EventSessionRecord) => (s.rounds.length ? Math.round((s.score / s.rounds.length) * 10) / 10 : 0);

export function eventStanding(sessions: EventSessionRecord[]): EventStanding {
  const scored = sessions.filter((s) => s.ending !== "stopped");
  const per = scored.map(perRound);
  const quoted = scored.flatMap((s) => s.rounds).filter((x) => x.bid !== null && x.ask !== null);
  const lean = quoted.map((x) => x.ask! - x.bid! - (x.modelAsk - x.modelBid));
  return {
    sessions: sessions.length,
    scored,
    best: per.length ? Math.max(...per) : null,
    latest: per.length ? per[per.length - 1] : null,
    average: per.length ? Math.round((per.reduce((a, b) => a + b, 0) / per.length) * 10) / 10 : null,
    centred: quoted.length
      ? Math.round((quoted.filter((x) => Math.abs(mid({ bid: x.bid!, ask: x.ask! }) - x.implied) <= 2).length / quoted.length) * 100)
      : null,
    widthLean: lean.length ? Math.round((lean.reduce((a, b) => a + b, 0) / lean.length) * 10) / 10 : null,
  };
}
