/**
 * What the mental-math game remembers, and what it makes of it.
 * Kept by [`lib/arcade.ts`](arcade.ts) in `~/.mogeung/mental-math.jsonl`.
 *
 * Asked for 2026-09-23, looking at a results screen: *"keep it, record it to
 * some local file, so that after a few games, we can build analytics of the
 * user mental math capability, like some trend line"*.
 *
 * **What is kept is what happened, not what it came to.** A record holds the
 * settings, the raw counts and every question with its time and outcome —
 * never *accuracy* or *per minute*, which are computed here on reading. So a
 * better way to judge a game later is a change to this file, and every game
 * already played is judged by it, rather than a history frozen in whatever
 * the first formula said.
 *
 * One JSON line per game. Read tolerantly: a line that does not parse is skipped, never fatal
 * — the file is yours to edit, and one bad line must not cost the rest.
 */

import {
  DIFFICULTIES,
  OPS,
  SYMBOL,
  opsKey,
  type Answered,
  type Difficulty,
  type Duration,
  type Kind,
  type QuestionClock,
  type Run,
} from "@/lib/mentalMath";
import { appendLine, loadLines } from "@/lib/arcade";

export const RECORD_VERSION = 1;

export interface GameRecord {
  v: number;
  /** When the game ended, ISO. */
  at: string;
  difficulty: Difficulty;
  /** The operations that were on, as symbols: `+−×÷`. */
  ops: string;
  /** `null` is endless. */
  duration: Duration | null;
  question: QuestionClock;
  /** A timed game stopped by hand: played, not a score. */
  early: boolean;
  ms: number;
  correct: number;
  wrong: number;
  skipped: number;
  bestStreak: number;
  /** Every question: text, kind, milliseconds, and how it went. */
  log: Answered[];
}

/** The record of a finished run, or `null` for one where nothing was asked. */
export function toRecord(run: Run): GameRecord | null {
  if (run.endedAt === null || run.log.length === 0) return null;
  return {
    v: RECORD_VERSION,
    at: new Date(run.endedAt).toISOString(),
    difficulty: run.settings.difficulty,
    ops: opsKey(run.settings.ops),
    duration: run.settings.duration,
    question: run.settings.question,
    early: run.early,
    ms: run.endedAt - run.startedAt,
    correct: run.correct,
    wrong: run.wrong,
    skipped: run.skipped,
    bestStreak: run.bestStreak,
    log: run.log,
  };
}

const HOWS = new Set(["right", "wrong", "skipped", "timeout"]);
const KINDS = new Set<string>([...OPS, "mixed"]);

function asAnswered(v: unknown): Answered | null {
  if (!v || typeof v !== "object") return null;
  const a = v as Record<string, unknown>;
  if (typeof a.text !== "string" || typeof a.ms !== "number" || !KINDS.has(a.kind as string) || !HOWS.has(a.how as string)) {
    return null;
  }
  return { text: a.text, kind: a.kind as Kind, ms: a.ms, how: a.how as Answered["how"] };
}

/** One line, if it is a game this build can read. */
export function parseRecord(line: string): GameRecord | null {
  let v: Record<string, unknown>;
  try {
    v = JSON.parse(line);
  } catch {
    return null;
  }
  if (!v || typeof v !== "object") return null;
  const n = (k: string) => (typeof v[k] === "number" && Number.isFinite(v[k]) ? (v[k] as number) : null);
  const at = typeof v.at === "string" && !Number.isNaN(Date.parse(v.at)) ? v.at : null;
  const difficulty = DIFFICULTIES.includes(v.difficulty as Difficulty) ? (v.difficulty as Difficulty) : null;
  const [ms, correct, wrong, skipped, bestStreak] = [n("ms"), n("correct"), n("wrong"), n("skipped"), n("bestStreak")];
  if (at === null || difficulty === null || ms === null || correct === null || wrong === null || skipped === null) return null;
  const q = v.question;
  return {
    v: n("v") ?? RECORD_VERSION,
    at,
    difficulty,
    ops: typeof v.ops === "string" ? v.ops : "",
    duration: typeof v.duration === "number" ? (v.duration as Duration) : null,
    question: q === "auto" || q === "off" || typeof q === "number" ? (q as QuestionClock) : "auto",
    early: v.early === true,
    ms,
    correct,
    wrong,
    skipped,
    bestStreak: bestStreak ?? 0,
    log: Array.isArray(v.log) ? v.log.map(asAnswered).filter((a): a is Answered => a !== null) : [],
  };
}

/** The whole file, oldest first. */
export function parseRecords(text: string): GameRecord[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .map(parseRecord)
    .filter((r): r is GameRecord => r !== null)
    .sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

// --- one game ---------------------------------------------------------------

export interface GameMetrics {
  asked: number;
  /** Whole percent. */
  accuracy: number;
  perMinute: number;
  /** Average time on the ones you got. */
  avgMs: number | null;
}

export function metrics(r: GameRecord): GameMetrics {
  const asked = r.correct + r.wrong + r.skipped;
  const solved = r.log.filter((a) => a.how === "right");
  return {
    asked,
    accuracy: asked ? Math.round((r.correct / asked) * 100) : 0,
    perMinute: r.ms > 0 ? Math.round((r.correct / (r.ms / 60_000)) * 10) / 10 : 0,
    avgMs: solved.length ? solved.reduce((t, a) => t + a.ms, 0) / solved.length : null,
  };
}

/** A game's setting, as its results and its history row say it. */
export function settingLabel(r: Pick<GameRecord, "difficulty" | "duration" | "question">): string {
  const level = r.difficulty[0].toUpperCase() + r.difficulty.slice(1);
  const length = r.duration === null ? "endless" : `${r.duration / 60} min`;
  const clock = r.question === "auto" ? "" : r.question === "off" ? " · no question timer" : ` · ${r.question}s a question`;
  return `${level} · ${length}${clock}`;
}

// --- many games -------------------------------------------------------------

export type Mode = "timed" | "endless";

export interface Filter {
  difficulty: Difficulty | "all";
  mode: Mode | "all";
}

export function filterRecords(records: GameRecord[], f: Filter): GameRecord[] {
  return records.filter(
    (r) =>
      (f.difficulty === "all" || r.difficulty === f.difficulty) &&
      (f.mode === "all" || (f.mode === "endless") === (r.duration === null)),
  );
}

/** The least-squares line through `ys` plotted at 0, 1, 2… — `null` under two points. */
export function fitLine(ys: number[]): { slope: number; intercept: number } | null {
  const n = ys.length;
  if (n < 2) return null;
  const mx = (n - 1) / 2;
  const my = ys.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let den = 0;
  ys.forEach((y, x) => {
    num += (x - mx) * (y - my);
    den += (x - mx) ** 2;
  });
  const slope = den ? num / den : 0;
  return { slope, intercept: my - slope * mx };
}

export interface Overview {
  games: number;
  ms: number;
  correct: number;
  asked: number;
  accuracy: number;
  bestPerMinute: number | null;
  /** Days with at least one game, counted back from the latest one. */
  dayStreak: number;
}

function localDay(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

export function overview(records: GameRecord[]): Overview {
  const correct = records.reduce((t, r) => t + r.correct, 0);
  const asked = records.reduce((t, r) => t + r.correct + r.wrong + r.skipped, 0);
  const rates = records.filter((r) => !r.early).map((r) => metrics(r).perMinute);
  const days = new Set(records.map((r) => localDay(r.at)));
  let dayStreak = 0;
  if (records.length) {
    const d = new Date(records[records.length - 1].at);
    while (days.has(`${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`)) {
      dayStreak++;
      d.setDate(d.getDate() - 1);
    }
  }
  return {
    games: records.length,
    ms: records.reduce((t, r) => t + r.ms, 0),
    correct,
    asked,
    accuracy: asked ? Math.round((correct / asked) * 100) : 0,
    bestPerMinute: rates.length ? Math.max(...rates) : null,
    dayStreak,
  };
}

export interface OpTrend {
  kind: Kind;
  symbol: string;
  solved: number;
  missed: number;
  avgMs: number | null;
  /** Average seconds per game, in game order; `null` where it was not asked. */
  series: (number | null)[];
  /** The first half of the games that asked it, against the second. */
  earlier: number | null;
  recent: number | null;
}

/** How fast each operation is answered, and whether that is moving. */
export function byOperation(records: GameRecord[]): OpTrend[] {
  const out: OpTrend[] = [];
  for (const kind of [...OPS, "mixed"] as Kind[]) {
    const per = records.map((r) => {
      const mine = r.log.filter((a) => a.kind === kind);
      const right = mine.filter((a) => a.how === "right");
      return { right, missed: mine.length - right.length };
    });
    const solved = per.reduce((t, p) => t + p.right.length, 0);
    const missed = per.reduce((t, p) => t + p.missed, 0);
    if (solved + missed === 0) continue;
    const series = per.map((p) => (p.right.length ? p.right.reduce((t, a) => t + a.ms, 0) / p.right.length / 1000 : null));
    const played = series.filter((x): x is number => x !== null);
    const half = Math.floor(played.length / 2);
    const avg = (xs: number[]) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);
    const all = per.flatMap((p) => p.right.map((a) => a.ms));
    out.push({
      kind,
      symbol: SYMBOL[kind],
      solved,
      missed,
      avgMs: avg(all),
      series,
      earlier: played.length >= 2 ? avg(played.slice(0, half)) : null,
      recent: played.length >= 2 ? avg(played.slice(half)) : null,
    });
  }
  return out;
}

export interface Hard {
  text: string;
  kind: Kind;
  seen: number;
  missed: number;
  avgMs: number | null;
}

/**
 * The questions that cost you most: missed ones first, then the slowest you
 * got. Grouped by the question itself, so one you meet again counts together.
 */
export function hardest(records: GameRecord[], n = 8): Hard[] {
  const map = new Map<string, { kind: Kind; seen: number; missed: number; ms: number[] }>();
  for (const r of records) {
    for (const a of r.log) {
      const e = map.get(a.text) ?? { kind: a.kind, seen: 0, missed: 0, ms: [] };
      e.seen++;
      if (a.how === "right") e.ms.push(a.ms);
      else e.missed++;
      map.set(a.text, e);
    }
  }
  return [...map.entries()]
    .map(([text, e]) => ({
      text,
      kind: e.kind,
      seen: e.seen,
      missed: e.missed,
      avgMs: e.ms.length ? e.ms.reduce((a, b) => a + b, 0) / e.ms.length : null,
    }))
    .sort((a, b) => b.missed - a.missed || (b.avgMs ?? 0) - (a.avgMs ?? 0))
    .slice(0, n);
}

/**
 * This game against the ones before it at the same setting — what a results
 * screen can say the moment a game ends. `null` for a first game there.
 */
export function againstRecent(
  records: GameRecord[],
  game: GameRecord,
  n = 10,
): { games: number; avgPerMinute: number; bestPerMinute: number } | null {
  const same = records.filter(
    (r) =>
      r !== game &&
      r.at < game.at &&
      !r.early &&
      r.difficulty === game.difficulty &&
      r.duration === game.duration &&
      r.question === game.question &&
      r.ops === game.ops,
  );
  if (same.length === 0) return null;
  const rates = same.map((r) => metrics(r).perMinute);
  const recent = rates.slice(-n);
  return {
    games: recent.length,
    avgPerMinute: Math.round((recent.reduce((a, b) => a + b, 0) / recent.length) * 10) / 10,
    bestPerMinute: Math.max(...rates),
  };
}

// --- where it lives -----------------------------------------------------------

/** Every game recorded, oldest first. */
export async function loadHistory(): Promise<GameRecord[]> {
  return parseRecords((await loadLines("mental-math")).join("\n"));
}

/** Add one game. Returns whether it was kept, so the screen can say if not. */
export function appendHistory(r: GameRecord): Promise<boolean> {
  return appendLine("mental-math", r);
}
