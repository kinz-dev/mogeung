/**
 * Mental math — the one thing in this window nobody is told about.
 *
 * Asked for on 2026-09-23 as an easter egg, "just for fun": a mental
 * arithmetic drill that opens from a key sequence documented nowhere. So it is
 * deliberately **not** an `Action` — an action is listed in the shortcuts
 * window, offered by the palette and rebindable, and each of those is a
 * place it would be found. It holds no store field and no prefs key either;
 * what it keeps, it keeps under its own `localStorage` key.
 *
 * The rules are rankyourbrain.net's mental-math drill, kept to the number:
 * four difficulties with its ranges, an answer that goes in the moment it is
 * right, a per-question clock that skips without penalty, and a two-minute
 * standard run. Added here: an **endless** mode with no clock at all, for
 * training rather than scoring, and a breakdown by operation, because *which
 * one am I slow at* is the question training asks.
 *
 * Everything below is pure and takes `now` and `rand`, so the tests can drive
 * a whole run without a timer.
 */

export type Difficulty = "easy" | "normal" | "advanced" | "expert";
export type Op = "add" | "sub" | "mul" | "div";
export type Kind = Op | "mixed";
export type Duration = 60 | 120 | 180 | 300;

export const DIFFICULTIES: Difficulty[] = ["easy", "normal", "advanced", "expert"];
export const OPS: Op[] = ["add", "sub", "mul", "div"];
export const DURATIONS: Duration[] = [60, 120, 180, 300];
/** The only length a run is a *score* at, as on the original. */
export const STANDARD: Duration = 120;

export const SYMBOL: Record<Kind, string> = { add: "+", sub: "−", mul: "×", div: "÷", mixed: "( )" };

/**
 * How long a question stands before it skips itself. `"auto"` is the
 * original's rule and the default: the difficulty's seconds on a timed run,
 * and no clock at all when endless. A number is that many seconds on either;
 * `"off"` lets a question wait as long as you do.
 */
export type QuestionClock = "auto" | "off" | number;
export const QUESTION_CLOCKS: QuestionClock[] = ["auto", 3, 5, 10, 15, 20, 30, "off"];

export interface Settings {
  difficulty: Difficulty;
  ops: Record<Op, boolean>;
  /** `null` is endless: no run clock. */
  duration: Duration | null;
  question: QuestionClock;
}

export const DEFAULT_SETTINGS: Settings = {
  difficulty: "easy",
  ops: { add: true, sub: true, mul: true, div: true },
  duration: STANDARD,
  question: "auto",
};

/** The seconds a question stands under these settings, or `null` for none. */
export function questionSeconds(s: Settings): number | null {
  if (s.question === "off") return null;
  if (s.question === "auto") return s.duration === null ? null : LEVELS[s.difficulty].seconds;
  return s.question;
}

type Span = readonly [number, number];

interface Level {
  add: readonly [Span, Span];
  sub: readonly [Span, Span];
  /** Without negatives, subtraction is addition read backwards. */
  negatives: boolean;
  mul: readonly [Span, Span];
  maxDivisor: number;
  /** The quotient's range — the original reuses the multiplier's. */
  quotient: Span;
  /** Share of questions that are two-step, when two or more ops are on. */
  mixed: number;
  /** How long a question stands before it skips itself. */
  seconds: number;
  label: string;
  tagline: string;
}

export const LEVELS: Record<Difficulty, Level> = {
  easy: {
    add: [[2, 100], [2, 100]],
    sub: [[2, 100], [2, 100]],
    negatives: false,
    mul: [[2, 12], [2, 100]],
    maxDivisor: 12,
    quotient: [2, 100],
    mixed: 0,
    seconds: 8,
    label: "Easy",
    tagline: "two-digit sums and times tables — a warm-up",
  },
  normal: {
    add: [[10, 500], [10, 500]],
    sub: [[10, 500], [10, 500]],
    negatives: true,
    mul: [[2, 19], [2, 50]],
    maxDivisor: 19,
    quotient: [2, 50],
    mixed: 0,
    seconds: 12,
    label: "Normal",
    tagline: "bigger numbers, and answers below zero",
  },
  advanced: {
    add: [[100, 999], [100, 999]],
    sub: [[100, 999], [100, 999]],
    negatives: true,
    mul: [[12, 99], [2, 20]],
    maxDivisor: 25,
    quotient: [2, 20],
    mixed: 0.15,
    seconds: 16,
    label: "Advanced",
    tagline: "three digits, and now and then two steps",
  },
  expert: {
    add: [[500, 9999], [500, 9999]],
    sub: [[500, 9999], [500, 9999]],
    negatives: true,
    mul: [[15, 99], [15, 99]],
    maxDivisor: 99,
    quotient: [15, 99],
    mixed: 0.4,
    seconds: 20,
    label: "Expert",
    tagline: "four digits, two-digit squares, multi-step",
  },
};

const n = (x: number) => x.toLocaleString("en-US");
const span = (s: Span) => `${n(s[0])}–${n(s[1])}`;

/** What each operation means at a difficulty, in words, for the setup screen. */
export function opHint(d: Difficulty, op: Op): string {
  const l = LEVELS[d];
  switch (op) {
    case "add":
      return `${span(l.add[0])} + ${span(l.add[1])}`;
    case "sub":
      return l.negatives ? `${span(l.sub[0])} − ${span(l.sub[1])}, below zero too` : "addition backwards, never below zero";
    case "mul":
      return d === "advanced"
        ? `${span(l.mul[0])} × ${span(l.mul[1])}, sometimes 12–49 × 12–49`
        : `${span(l.mul[0])} × ${span(l.mul[1])}`;
    case "div":
      return `whole answers, divisors 2–${l.maxDivisor}`;
  }
}

export function mixedHint(d: Difficulty): string | null {
  const share = Math.round(LEVELS[d].mixed * 100);
  if (share === 0) return null;
  return d === "expert"
    ? `${share}% are multi-step, like (a × b) + (c ÷ d)`
    : `${share}% are two-step, like (a × b) + c`;
}

export interface Problem {
  text: string;
  answer: number;
  kind: Kind;
}

type Rand = () => number;

const pick = (rand: Rand, [lo, hi]: Span) => Math.floor(rand() * (hi - lo + 1)) + lo;

function single(d: Difficulty, op: Op, rand: Rand): Problem {
  const l = LEVELS[d];
  switch (op) {
    case "add": {
      const a = pick(rand, l.add[0]);
      const b = pick(rand, l.add[1]);
      return { text: `${n(a)} + ${n(b)}`, answer: a + b, kind: op };
    }
    case "sub": {
      const a = pick(rand, l.sub[0]);
      const b = pick(rand, l.sub[1]);
      return l.negatives
        ? { text: `${n(a)} − ${n(b)}`, answer: a - b, kind: op }
        : { text: `${n(a + b)} − ${n(a)}`, answer: b, kind: op };
    }
    case "mul": {
      const [x, y] = d === "advanced" && rand() < 0.2 ? ([[12, 49], [12, 49]] as const) : l.mul;
      const a = pick(rand, x);
      const b = pick(rand, y);
      return { text: `${a} × ${b}`, answer: a * b, kind: op };
    }
    case "div": {
      const divisor = pick(rand, [2, l.maxDivisor]);
      const quotient = pick(rand, l.quotient);
      return { text: `${n(divisor * quotient)} ÷ ${divisor}`, answer: quotient, kind: op };
    }
  }
}

function mixed(d: Difficulty, rand: Rand): Problem {
  const forms = d === "expert" ? 5 : 3;
  const at = (s: Span) => pick(rand, s);
  switch (Math.floor(rand() * forms)) {
    case 0: {
      const [a, b, c] = [at([2, 20]), at([2, 20]), at([10, 100])];
      return { text: `(${a} × ${b}) + ${c}`, answer: a * b + c, kind: "mixed" };
    }
    case 1: {
      const [a, b, c] = [at([5, 30]), at([5, 30]), at([2, 10])];
      return { text: `(${a} + ${b}) × ${c}`, answer: (a + b) * c, kind: "mixed" };
    }
    case 2: {
      const [a, b, c] = [at([5, 25]), at([5, 25]), at([10, 100])];
      return { text: `(${a} × ${b}) − ${c}`, answer: a * b - c, kind: "mixed" };
    }
    case 3: {
      const [a, b, d2, k] = [at([5, 20]), at([5, 20]), at([2, 10]), at([2, 20])];
      return { text: `(${a} × ${b}) + (${d2 * k} ÷ ${d2})`, answer: a * b + k, kind: "mixed" };
    }
    default: {
      const [a, b, c] = [at([5, 20]), at([5, 20]), at([15, 50])];
      const e = at([5, c - 1]);
      return { text: `(${a} + ${b}) × (${c} − ${e})`, answer: (a + b) * (c - e), kind: "mixed" };
    }
  }
}

/**
 * The next question. Never the one just asked — the same sum twice in a row
 * reads as the game having stuck.
 */
export function generate(s: Pick<Settings, "difficulty" | "ops">, rand: Rand = Math.random, previous?: string): Problem {
  const on = OPS.filter((op) => s.ops[op]);
  const ops = on.length > 0 ? on : OPS;
  for (let tries = 0; ; tries++) {
    const p =
      ops.length >= 2 && rand() < LEVELS[s.difficulty].mixed
        ? mixed(s.difficulty, rand)
        : single(s.difficulty, ops[Math.floor(rand() * ops.length)], rand);
    if (p.text !== previous || tries >= 8) return p;
  }
}

/** What is left of a typed answer once everything that is not one is gone. */
export function cleanAnswer(typed: string): string {
  const t = typed.replace(/[−–]/g, "-").replace(/[^\d-]/g, "");
  return t.startsWith("-") ? `-${t.slice(1).replace(/-/g, "")}` : t.replace(/-/g, "");
}

/** Is this what was asked? Separators and a typographic minus are forgiven. */
export function isRight(typed: string, answer: number): boolean {
  const t = typed.trim().replace(/[\s,_]/g, "").replace(/[−–]/g, "-");
  return /^-?\d+$/.test(t) && Number(t) === answer;
}

// --- a run ------------------------------------------------------------------

export interface KindTally {
  solved: number;
  missed: number;
  /** Time spent on the ones solved, for an average that means *speed*. */
  ms: number;
}

/** The last question you did not get, shown under the next one. */
export interface Missed {
  text: string;
  answer: number;
  /** What you typed, when it was a wrong answer rather than a skip. */
  typed: string | null;
  why: "skipped" | "wrong" | "timeout";
}

export interface Run {
  settings: Settings;
  startedAt: number;
  /** `null` when endless. */
  endsAt: number | null;
  problem: Problem;
  shownAt: number;
  /** Question number, from 1 — a new one means the answer box empties. */
  seq: number;
  correct: number;
  wrong: number;
  skipped: number;
  streak: number;
  bestStreak: number;
  byKind: Partial<Record<Kind, KindTally>>;
  missed: Missed | null;
  endedAt: number | null;
  /** Stopped by hand before its clock ran out: played, not a score. */
  early: boolean;
}

export function startRun(settings: Settings, now: number, rand: Rand = Math.random): Run {
  return {
    settings,
    startedAt: now,
    endsAt: settings.duration === null ? null : now + settings.duration * 1000,
    problem: generate(settings, rand),
    shownAt: now,
    seq: 1,
    correct: 0,
    wrong: 0,
    skipped: 0,
    streak: 0,
    bestStreak: 0,
    byKind: {},
    missed: null,
    endedAt: null,
    early: false,
  };
}

function tally(run: Run, solved: boolean, now: number): Run["byKind"] {
  const t = run.byKind[run.problem.kind] ?? { solved: 0, missed: 0, ms: 0 };
  return {
    ...run.byKind,
    [run.problem.kind]: solved
      ? { ...t, solved: t.solved + 1, ms: t.ms + (now - run.shownAt) }
      : { ...t, missed: t.missed + 1 },
  };
}

function next(run: Run, now: number, rand: Rand): Pick<Run, "problem" | "shownAt" | "seq"> {
  return { problem: generate(run.settings, rand, run.problem.text), shownAt: now, seq: run.seq + 1 };
}

/** The answer box changed: the next question if it is now right, else `null`. */
export function answer(run: Run, typed: string, now: number, rand: Rand = Math.random): Run | null {
  if (run.endedAt !== null || !isRight(typed, run.problem.answer)) return null;
  const streak = run.streak + 1;
  return {
    ...run,
    ...next(run, now, rand),
    correct: run.correct + 1,
    streak,
    bestStreak: Math.max(run.bestStreak, streak),
    byKind: tally(run, true, now),
    missed: null,
  };
}

/**
 * Enter: move on. With nothing typed it is a skip; with a wrong answer typed
 * it counts as wrong — the original keeps a *Wrong* column it never fills,
 * and this is what fills it.
 */
export function giveUp(run: Run, typed: string, now: number, rand: Rand = Math.random): Run {
  if (run.endedAt !== null) return run;
  const t = typed.trim();
  return miss(run, t === "" ? "skipped" : "wrong", t === "" ? null : t, now, rand);
}

function miss(run: Run, why: Missed["why"], typed: string | null, now: number, rand: Rand): Run {
  return {
    ...run,
    ...next(run, now, rand),
    wrong: run.wrong + (why === "wrong" ? 1 : 0),
    skipped: run.skipped + (why === "wrong" ? 0 : 1),
    streak: 0,
    byKind: tally(run, false, now),
    missed: { text: run.problem.text, answer: run.problem.answer, typed, why },
  };
}

/** Milliseconds the question has left, or `null` when it has no clock. */
export function questionLeft(run: Run, now: number): number | null {
  const seconds = questionSeconds(run.settings);
  if (seconds === null) return null;
  return Math.max(0, run.shownAt + seconds * 1000 - now);
}

/** Time passing: the run ends on its clock, a question skips on its own. */
export function tick(run: Run, now: number, rand: Rand = Math.random): Run {
  if (run.endedAt !== null) return run;
  if (run.endsAt !== null && now >= run.endsAt) return { ...run, endedAt: run.endsAt };
  if (questionLeft(run, now) === 0) return miss(run, "timeout", null, now, rand);
  return run;
}

/** Stopped by hand. A timed run stopped before its clock is not a score. */
export function endRun(run: Run, now: number): Run {
  if (run.endedAt !== null) return run;
  return { ...run, endedAt: now, early: run.endsAt !== null && now < run.endsAt };
}

export interface Summary {
  score: number;
  total: number;
  /** Whole percent of questions answered right, skips included. */
  accuracy: number;
  ms: number;
  perMinute: number;
  avgMs: number | null;
  kinds: { kind: Kind; solved: number; missed: number; avgMs: number | null }[];
}

export function summarize(run: Run, now = run.endedAt ?? Date.now()): Summary {
  const ms = Math.max(0, (run.endedAt ?? now) - run.startedAt);
  const total = run.correct + run.wrong + run.skipped;
  const kinds = ([...OPS, "mixed"] as Kind[]).flatMap((kind) => {
    const t = run.byKind[kind];
    return t ? [{ kind, solved: t.solved, missed: t.missed, avgMs: t.solved ? t.ms / t.solved : null }] : [];
  });
  const solvedMs = kinds.reduce((a, k) => a + (run.byKind[k.kind]?.ms ?? 0), 0);
  return {
    score: run.correct,
    total,
    accuracy: total ? Math.round((run.correct / total) * 100) : 0,
    ms,
    perMinute: ms ? Math.round((run.correct / (ms / 60000)) * 10) / 10 : 0,
    avgMs: run.correct ? solvedMs / run.correct : null,
    kinds,
  };
}

/** The original's verdicts, read per two minutes so every length is judged alike. */
export function verdict(score: number, duration: Duration): { text: string; tier: 0 | 1 | 2 | 3 | 4 } {
  const per = (score * STANDARD) / duration;
  if (per >= 50) return { text: "Outstanding", tier: 4 };
  if (per >= 40) return { text: "Excellent", tier: 3 };
  if (per >= 30) return { text: "Great job", tier: 2 };
  if (per >= 20) return { text: "Good effort", tier: 1 };
  return { text: "Keep practising", tier: 0 };
}

// --- bests ------------------------------------------------------------------

export const opsKey = (ops: Record<Op, boolean>) =>
  OPS.filter((o) => ops[o])
    .map((o) => SYMBOL[o])
    .join("");

/**
 * Where a best lives. A timed best is a score at one length; an endless best
 * is the longest streak, since an endless run's count only says how long you
 * stayed. A question clock other than `"auto"` is part of the setting — a
 * streak under three seconds a question is not one under thirty — and
 * `"auto"` adds nothing, so a best from before the clock could be set is
 * still found.
 */
export function bestKey(s: Settings): string {
  const base =
    s.duration === null ? `endless:${s.difficulty}:${opsKey(s.ops)}` : `${s.duration}:${s.difficulty}:${opsKey(s.ops)}`;
  return s.question === "auto" ? base : `${base}:q${s.question}`;
}

/** The run's claim on a best, or `null` when it does not have one. */
export function bestOf(run: Run): number | null {
  if (run.endedAt === null || run.early) return null;
  return run.settings.duration === null ? run.bestStreak : run.correct;
}

export function recordBest(
  bests: Record<string, number>,
  run: Run,
): { bests: Record<string, number>; previous: number | null; isNew: boolean } {
  const key = bestKey(run.settings);
  const previous = bests[key] ?? null;
  const mine = bestOf(run);
  if (mine === null || mine === 0 || (previous !== null && mine <= previous)) return { bests, previous, isNew: false };
  return { bests: { ...bests, [key]: mine }, previous, isNew: true };
}

// --- what is kept -------------------------------------------------------------

const KEY = "mogeung.mentalMath";

export interface Saved {
  settings: Settings;
  bests: Record<string, number>;
  /** Opened at least once — the first time says so. */
  found: boolean;
}

export function loadSaved(): Saved {
  const fresh: Saved = { settings: DEFAULT_SETTINGS, bests: {}, found: false };
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "null") as Partial<Saved> | null;
    if (!raw || typeof raw !== "object") return fresh;
    const s = (raw.settings ?? {}) as Partial<Settings>;
    const ops = { ...DEFAULT_SETTINGS.ops };
    for (const op of OPS) if (typeof s.ops?.[op] === "boolean") ops[op] = s.ops[op];
    const bests: Record<string, number> = {};
    for (const [k, v] of Object.entries(raw.bests ?? {})) if (typeof v === "number" && v > 0) bests[k] = v;
    return {
      settings: {
        difficulty: DIFFICULTIES.includes(s.difficulty as Difficulty) ? (s.difficulty as Difficulty) : "easy",
        ops: OPS.some((o) => ops[o]) ? ops : DEFAULT_SETTINGS.ops,
        duration: s.duration === null ? null : DURATIONS.includes(s.duration as Duration) ? (s.duration as Duration) : STANDARD,
        question: QUESTION_CLOCKS.includes(s.question as QuestionClock) ? (s.question as QuestionClock) : "auto",
      },
      bests,
      found: raw.found === true,
    };
  } catch {
    return fresh;
  }
}

export function save(s: Saved): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // A private window or a full disk: the game still plays, it just forgets.
  }
}

// --- the way in ---------------------------------------------------------------

/** The first six digits of π, typed with nothing that takes text focused. */
export const SECRET = "314159";
/** A pause longer than this between digits starts the sequence over. */
export const SECRET_GAP_MS = 1500;

/**
 * One key further into the sequence. On a wrong key the longest tail of what
 * was typed that still starts it is kept, so `3 1 3 1 4 1 5 9` opens the game
 * rather than needing a clean start.
 */
export function secretStep(progress: string, key: string): string {
  const typed = progress + key;
  for (let i = 0; i < typed.length; i++) {
    const tail = typed.slice(i);
    if (SECRET.startsWith(tail)) return tail;
  }
  return "";
}
