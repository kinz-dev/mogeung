/**
 * What the Fermi game remembers — one line per session in
 * `~/.mogeung/fermi-estimation.jsonl` (see [`lib/arcade.ts`](arcade.ts)) — and
 * the lobby's statistics made from it.
 *
 * Every question keeps its answer and your range, so what you tend to get
 * wrong — too narrow, too low, one kind of quantity — can be worked out later
 * from sessions already played.
 */

import { appendLine, loadLines } from "@/lib/arcade";
import type { FermiSettings } from "@/lib/fermi";
import { fermiOutcome, type FermiRun } from "@/lib/fermiRun";

export interface FermiAnswerRecord {
  id: string;
  text: string;
  unit: string;
  answer: number;
  outcome: "answered" | "skipped" | "timeout";
  low: number | null;
  high: number | null;
  hints: number;
  ms: number | null;
  hit: boolean;
  /** |log₁₀(midpoint ÷ answer)|, and log₁₀(high ÷ low), when answered. */
  distance: number | null;
  spread: number | null;
  points: number;
}

export interface FermiSessionRecord {
  v: 1;
  at: string;
  settings: FermiSettings;
  ending: "complete" | "stopped";
  points: number;
  max: number;
  hits: number;
  answered: number;
  answers: FermiAnswerRecord[];
}

const r4 = (x: number) => Math.round(x * 10_000) / 10_000;

export function toFermiSession(run: FermiRun, at: Date = new Date()): FermiSessionRecord | null {
  if (run.answers.length === 0 || run.ending === null) return null;
  const o = fermiOutcome(run);
  return {
    v: 1,
    at: at.toISOString(),
    settings: run.settings,
    ending: run.ending,
    points: o.points,
    max: o.max,
    hits: o.hits,
    answered: o.answered,
    answers: run.answers.map((a) => ({
      id: a.question.id,
      text: a.question.text,
      unit: a.question.unit,
      answer: a.question.answer,
      outcome: a.outcome,
      low: a.low,
      high: a.high,
      hints: a.hints,
      ms: a.ms,
      hit: a.scored?.hit ?? false,
      distance: a.scored ? r4(a.scored.distance) : null,
      spread: a.scored ? r4(a.scored.spread) : null,
      points: a.points,
    })),
  };
}

export function parseFermiSession(line: string): FermiSessionRecord | null {
  try {
    const v = JSON.parse(line) as Partial<FermiSessionRecord>;
    if (!v || typeof v !== "object" || typeof v.at !== "string" || Number.isNaN(Date.parse(v.at))) return null;
    if (typeof v.points !== "number" || typeof v.max !== "number" || !Array.isArray(v.answers)) return null;
    if (!v.settings || typeof v.settings !== "object") return null;
    return {
      v: 1,
      at: v.at,
      settings: v.settings as FermiSettings,
      ending: v.ending === "stopped" ? "stopped" : "complete",
      points: v.points,
      max: v.max,
      hits: typeof v.hits === "number" ? v.hits : 0,
      answered: typeof v.answered === "number" ? v.answered : 0,
      answers: v.answers as FermiAnswerRecord[],
    };
  } catch {
    return null;
  }
}

export async function loadFermiSessions(): Promise<FermiSessionRecord[]> {
  return (await loadLines("fermi-estimation"))
    .map(parseFermiSession)
    .filter((s): s is FermiSessionRecord => s !== null)
    .sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

export function appendFermiSession(s: FermiSessionRecord): Promise<boolean> {
  return appendLine("fermi-estimation", s);
}

export interface FermiStanding {
  sessions: number;
  scored: FermiSessionRecord[];
  /** Points as a share of the most a session could score — comparable across lengths. */
  best: number | null;
  latest: number | null;
  average: number | null;
  /** Share of answered questions whose range held the answer. */
  hitRate: number | null;
  /** The typical miss of a midpoint, as a factor: 10 to the median distance. */
  typicalFactor: number | null;
  /** Of the misses, the share where the answer was above your range. */
  tooLow: number | null;
}

const pct = (s: FermiSessionRecord) => (s.max ? Math.round((s.points / s.max) * 100) : 0);

export function fermiStanding(sessions: FermiSessionRecord[]): FermiStanding {
  const scored = sessions.filter((s) => s.ending !== "stopped");
  const shares = scored.map(pct);
  const answered = scored.flatMap((s) => s.answers).filter((a) => a.outcome === "answered" && a.distance !== null);
  const d = answered.map((a) => a.distance!).sort((a, b) => a - b);
  const misses = answered.filter((a) => !a.hit);
  return {
    sessions: sessions.length,
    scored,
    best: shares.length ? Math.max(...shares) : null,
    latest: shares.length ? shares[shares.length - 1] : null,
    average: shares.length ? Math.round(shares.reduce((a, b) => a + b, 0) / shares.length) : null,
    hitRate: answered.length ? Math.round((answered.filter((a) => a.hit).length / answered.length) * 100) : null,
    typicalFactor: d.length ? 10 ** d[Math.floor((d.length - 1) / 2)] : null,
    tooLow: misses.length ? Math.round((misses.filter((a) => a.high !== null && a.answer > a.high).length / misses.length) * 100) : null,
  };
}

export { pct as sessionShare };
