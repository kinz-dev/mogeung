/**
 * A Fermi game in progress, as pure functions over a `FermiRun`: the screen
 * draws one and says what time it is, a test plays one from a seed.
 *
 * Each question goes **asking** (your range, a clock if you set one, hints if
 * they are on) → **reveal** (the answer, where your range sat, the points).
 * Then the next, until the last. Stopped early, the questions answered stand.
 */

import { drawQuestions, parseAmount, rangeProblem, scoreRange, type FermiSettings, type Question, type Scored } from "@/lib/fermi";
import type { Rand } from "@/lib/tradingGame";

export interface FermiAnswer {
  question: Question;
  outcome: "answered" | "skipped" | "timeout";
  low: number | null;
  high: number | null;
  /** How many hints you looked at. */
  hints: number;
  /** From the question showing to your answer; `null` when the clock answered. */
  ms: number | null;
  scored: Scored | null;
  points: number;
}

export interface FermiRun {
  settings: FermiSettings;
  questions: Question[];
  /** The question on screen, from 0. */
  i: number;
  stage: "asking" | "reveal" | "over";
  answers: FermiAnswer[];
  askedAt: number;
  deadline: number | null;
  hintsShown: number;
  problem: string | null;
  ending: "complete" | "stopped" | null;
}

const clock = (seconds: number | null, now: number) => (seconds === null ? null : now + seconds * 1000);

export function startRun(settings: FermiSettings, rand: Rand, now: number): FermiRun {
  return {
    settings,
    questions: drawQuestions(settings, rand),
    i: 0,
    stage: "asking",
    answers: [],
    askedAt: now,
    deadline: clock(settings.seconds, now),
    hintsShown: 0,
    problem: null,
    ending: null,
  };
}

export const current = (r: FermiRun) => r.questions[r.i];

function close(r: FermiRun, a: Omit<FermiAnswer, "question" | "hints">): FermiRun {
  return { ...r, stage: "reveal", deadline: null, problem: null, answers: [...r.answers, { ...a, question: current(r), hints: r.hintsShown }] };
}

/** Your range, as typed. A range that does not read is refused in words, and the clock keeps running. */
export function submit(r: FermiRun, lowRaw: string, highRaw: string, now: number): FermiRun {
  if (r.stage !== "asking") return r;
  const low = parseAmount(lowRaw);
  const high = parseAmount(highRaw);
  const problem =
    (lowRaw.trim() && low === null) || (highRaw.trim() && high === null)
      ? "That does not read as a number — try 8.1e9, 8.1bn or 8,100,000,000."
      : rangeProblem(low, high);
  if (problem) return { ...r, problem };
  const scored = scoreRange(low!, high!, current(r).answer);
  return close(r, { outcome: "answered", low, high, ms: Math.max(0, now - r.askedAt), scored, points: scored.points });
}

/** Passed on: no points, and none lost. */
export function skip(r: FermiRun, now: number): FermiRun {
  if (r.stage !== "asking") return r;
  return close(r, { outcome: "skipped", low: null, high: null, ms: Math.max(0, now - r.askedAt), scored: null, points: 0 });
}

export function timedOut(r: FermiRun): FermiRun {
  if (r.stage !== "asking") return r;
  return close(r, { outcome: "timeout", low: null, high: null, ms: null, scored: null, points: 0 });
}

/** One more hint, if the question has one and hints are on. */
export function showHint(r: FermiRun): FermiRun {
  if (r.stage !== "asking" || !r.settings.hints || r.hintsShown >= current(r).hints.length) return r;
  return { ...r, hintsShown: r.hintsShown + 1 };
}

/** On from a reveal: the next question, or the end. */
export function nextQuestion(r: FermiRun, now: number): FermiRun {
  if (r.stage !== "reveal") return r;
  if (r.i + 1 >= r.questions.length) return { ...r, stage: "over", ending: "complete" };
  return { ...r, i: r.i + 1, stage: "asking", askedAt: now, deadline: clock(r.settings.seconds, now), hintsShown: 0, problem: null };
}

/** Stopped by hand: the questions already answered stand. */
export function stopRun(r: FermiRun): FermiRun {
  return r.stage === "over" ? r : { ...r, stage: "over", deadline: null, ending: "stopped" };
}

export interface FermiOutcome {
  points: number;
  /** What a perfect game of this length would score. */
  max: number;
  answered: number;
  hits: number;
  /** The typical miss of your midpoints, as log₁₀ — the median over answered questions. */
  medianDistance: number | null;
}

export function fermiOutcome(r: FermiRun): FermiOutcome {
  const answered = r.answers.filter((a) => a.scored);
  const d = answered.map((a) => a.scored!.distance).sort((a, b) => a - b);
  return {
    points: r.answers.reduce((t, a) => t + a.points, 0),
    max: 200 * r.answers.length,
    answered: answered.length,
    hits: answered.filter((a) => a.scored!.hit).length,
    medianDistance: d.length ? (d.length % 2 ? d[(d.length - 1) / 2] : (d[d.length / 2 - 1] + d[d.length / 2]) / 2) : null,
  };
}
