/**
 * An Event Contracts game in progress, as pure functions over an `EventRun`:
 * the screen draws one and says what time it is, a test plays one from a seed.
 *
 * Each round goes **quoting** (the brief, your bid and ask, a clock if you set
 * one) → **review** (the flow against your quote, the truth, both scores and a
 * verdict). Then the next, until the last. Stopped early, the rounds already
 * reviewed stand.
 */

import {
  LEVELS,
  calibrationPoints,
  flow,
  inventory,
  makeContracts,
  mid,
  modelQuote,
  pnlPoints,
  quoteProblem,
  verdict,
  type Contract,
  type EventSettings,
  type Fill,
  type Quote,
  type Tone,
} from "@/lib/eventContracts";
import type { Rand } from "@/lib/tradingGame";

export interface EventRound {
  contract: Contract;
  outcome: "quoted" | "skipped" | "timeout";
  quote: Quote | null;
  fills: Fill[];
  /** Contracts held at the close; marked at the truth in the P&L. */
  held: number;
  pnl: number;
  calibration: number;
  score: number;
  /** From the brief showing to your quote; `null` when the clock closed the round. */
  ms: number | null;
  /** The balanced quote, for the review to compare yours with. */
  model: Quote;
  verdict: { tone: Tone; text: string } | null;
}

export interface EventRun {
  settings: EventSettings;
  contracts: Contract[];
  /** The round on screen, from 0. */
  i: number;
  stage: "quoting" | "review" | "over";
  rounds: EventRound[];
  askedAt: number;
  deadline: number | null;
  problem: string | null;
  ending: "complete" | "stopped" | null;
}

const clock = (seconds: number | null, now: number) => (seconds === null ? null : now + seconds * 1000);
const r1 = (x: number) => Math.round(x * 10) / 10;

export function startRun(settings: EventSettings, rand: Rand, now: number): EventRun {
  return {
    settings,
    contracts: makeContracts(settings, rand),
    i: 0,
    stage: "quoting",
    rounds: [],
    askedAt: now,
    deadline: clock(settings.seconds, now),
    problem: null,
    ending: null,
  };
}

export const contract = (r: EventRun) => r.contracts[r.i];

function review(r: EventRun, round: Omit<EventRound, "contract" | "model">): EventRun {
  const c = contract(r);
  const full: EventRound = { ...round, contract: c, model: modelQuote(c, LEVELS[r.settings.level]) };
  return { ...r, stage: "review", deadline: null, problem: null, rounds: [...r.rounds, full] };
}

/** Your market. A bad one is refused in words and the clock keeps running; a good one meets the flow. */
export function quote(r: EventRun, q: Quote, rand: Rand, now: number): EventRun {
  if (r.stage !== "quoting") return r;
  const c = contract(r);
  const rules = LEVELS[r.settings.level];
  const problem = quoteProblem(q, c.brief.maxWidth);
  if (problem) return { ...r, problem };
  const fills = flow(q, c, rules, rand);
  const pnl = pnlPoints(fills, c.truth);
  const calibration = calibrationPoints(mid(q), c.truth);
  return review(r, {
    outcome: "quoted",
    quote: q,
    fills,
    held: inventory(fills),
    pnl,
    calibration,
    score: r1(pnl + calibration),
    ms: Math.max(0, now - r.askedAt),
    verdict: verdict(q, c, rules, fills),
  });
}

const standAside = (outcome: "skipped" | "timeout", ms: number | null): Omit<EventRound, "contract" | "model"> => ({
  outcome,
  quote: null,
  fills: [],
  held: 0,
  pnl: 0,
  calibration: 0,
  score: 0,
  ms,
  verdict: null,
});

/** Standing aside: no score, and none lost. */
export function skip(r: EventRun, now: number): EventRun {
  return r.stage === "quoting" ? review(r, standAside("skipped", Math.max(0, now - r.askedAt))) : r;
}

export function timedOut(r: EventRun): EventRun {
  return r.stage === "quoting" ? review(r, standAside("timeout", null)) : r;
}

/** On from a review: the next round, or the end. */
export function nextRound(r: EventRun, now: number): EventRun {
  if (r.stage !== "review") return r;
  if (r.i + 1 >= r.contracts.length) return { ...r, stage: "over", ending: "complete" };
  return { ...r, i: r.i + 1, stage: "quoting", askedAt: now, deadline: clock(r.settings.seconds, now), problem: null };
}

export function stopRun(r: EventRun): EventRun {
  return r.stage === "over" ? r : { ...r, stage: "over", deadline: null, ending: "stopped" };
}

export interface EventOutcome {
  score: number;
  pnl: number;
  calibration: number;
  quoted: number;
  /** Rounds whose mid sat within two points of the update. */
  centred: number;
  /** Average distance of your mids from the truth, in points. */
  avgMiss: number | null;
}

export function eventOutcome(r: EventRun): EventOutcome {
  const quoted = r.rounds.filter((x) => x.quote);
  const miss = quoted.map((x) => Math.abs(mid(x.quote!) - x.contract.truth));
  return {
    score: r1(r.rounds.reduce((t, x) => t + x.score, 0)),
    pnl: r1(r.rounds.reduce((t, x) => t + x.pnl, 0)),
    calibration: r1(r.rounds.reduce((t, x) => t + x.calibration, 0)),
    quoted: quoted.length,
    centred: quoted.filter((x) => Math.abs(mid(x.quote!) - x.contract.implied) <= 2).length,
    avgMiss: miss.length ? r1(miss.reduce((a, b) => a + b, 0) / miss.length) : null,
  };
}
