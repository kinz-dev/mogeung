/**
 * The Event Contracts Game — tradermath.org's *Market Games → Event Contracts
 * Game*, the easter egg's seventh. Asked for 2026-09-24: *"please port all of
 * the 3 remaining game over"*.
 *
 * **Its rules, kept.** Each round is one contract that pays 100¢ if an event
 * happens and 0¢ if not. A brief gives a **base rate** and, on most rounds,
 * one piece of evidence with a **likelihood ratio**; you update in odds —
 * base 25% is 1:3, evidence 3× for makes it 1:1, so 50¢ — and **quote a
 * market**, a bid and an ask in whole cents no wider than the round allows.
 * Then the flow: **noise traders** pay you the spread but walk away from a
 * wide one; **informed traders** know the true probability, which sits within
 * a stated band of your update, and deal only when your quote leaves them an
 * edge. You score **P&L points** — your fills, with what you are left holding
 * marked at the true probability — and **calibration points**, a Brier-style
 * bonus for a midpoint closer to the truth than a coin flip's 50.
 *
 * What the reference publishes — the brief's shape, the odds-form update, the
 * two kinds of trader, what each score measures and the review's verdicts —
 * is kept. What it keeps on its server — the events, the flow, the exact
 * scales — is ours: the events are written here, the flow is the simplest one
 * that behaves as described (so that on every level the best width sits
 * strictly inside the widest allowed), and the scales are chosen so both
 * scores matter.
 *
 * **Loosened,** as the other games' clocks were: any time per round from 1 to
 * 600 seconds, or none; rounds and clock yours at any difficulty.
 *
 * Pure throughout, randomness passed in, so a test can play a game from a seed.
 */

import type { Rand } from "@/lib/tradingGame";

export type Level = "easy" | "medium" | "hard";
export type Direction = "for" | "against" | "none";

// --- the events ---------------------------------------------------------------------

interface Template {
  title: string;
  /** Evidence that makes it more likely, and less. */
  up: string;
  down: string;
  /** Where its base rate may sit, in percent. */
  prior: [number, number];
}

const t = (title: string, prior: [number, number], up: string, down: string): Template => ({ title, prior, up, down });

export const EVENTS: Template[] = [
  t("Will it rain in the city centre tomorrow afternoon?", [15, 45], "A cold front is now forecast to arrive by noon", "Overnight model runs push the front further north"),
  t("Will the home side win Saturday's match?", [35, 55], "Their top scorer is fit again", "Their first-choice goalkeeper was ruled out this morning"),
  t("Will the central bank cut rates at this week's meeting?", [20, 50], "Inflation came in below every forecast", "Wage growth picked up again last month"),
  t("Will the product ship on its announced date?", [45, 70], "The final build passed certification early", "A key supplier flagged a two-week delay"),
  t("Will the evening flight land on time?", [65, 85], "Tailwinds on the route are unusually strong today", "Fog is forecast at the destination"),
  t("Will regulators approve the merger?", [45, 75], "The regulator accepted the proposed remedies", "A second competition authority opened its own review"),
  t("Will the stock close higher today?", [45, 55], "Futures are up sharply overnight", "A large holder filed to sell shares"),
  t("Will the challenger win the election?", [25, 45], "The final poll shows the gap closing", "Early turnout favours the incumbent's strongholds"),
  t("Will the film top the weekend box office?", [35, 60], "Thursday previews beat the tracking", "Critics' reviews came in poor"),
  t("Will the marathon course record fall on Sunday?", [5, 25], "The forecast is cool and still", "A warm headwind is expected on the second half"),
  t("Will the bill pass the upper house?", [45, 70], "Two undecided members announced support", "The vote has already been postponed once"),
  t("Will the company beat its earnings estimate?", [55, 75], "Card-spending data point to strong sales", "Two analysts cut their numbers this week"),
  t("Will the rocket launch go ahead on schedule?", [50, 70], "The weather team rates conditions 90% favourable", "Engineers are reviewing an unexpected sensor reading"),
  t("Will the strike be called off before Monday?", [25, 50], "Both sides returned to talks overnight", "Members rejected the latest offer"),
  t("Will snow close the schools on Friday?", [10, 30], "Forecasters upgraded the snow warning", "Road crews say they have grit to spare"),
  t("Will the start-up close its funding round this quarter?", [40, 65], "A lead investor has signed a term sheet", "Its largest customer paused their contract"),
  t("Will the trial drug meet its main goal?", [25, 45], "An independent board let the trial run to full size", "A similar drug failed last year"),
  t("Will the favourite win the tennis final?", [60, 80], "The opponent needed treatment in the semi-final", "The favourite has lost their last two meetings"),
  t("Will the concert sell out within a day?", [35, 60], "Presale demand crashed the ticket site", "Prices are well above the last tour's"),
  t("Will the house-price index rise this month?", [45, 65], "Mortgage approvals jumped last month", "Unsold listings hit a two-year high"),
  t("Will the bridge reopen by the end of the month?", [30, 55], "The contractor says repairs are ahead of schedule", "Inspectors found more corrosion than expected"),
  t("Will the team reach the playoffs?", [30, 55], "They have won their last five games", "Their captain is out for the season"),
  t("Will the new phone outsell last year's model at launch?", [45, 65], "Pre-orders are 30% ahead of last year's", "The price has gone up by a fifth"),
  t("Will the city hit its recycling target this year?", [20, 40], "A new kerbside scheme started in spring", "Collection crews were cut by a tenth"),
];

// --- the levels -----------------------------------------------------------------------

export interface LevelRules {
  rounds: number;
  seconds: number;
  /** How far the truth may sit from the update, in points either way. */
  band: number;
  /** The widest quote allowed, in cents. */
  maxWidth: number;
  /** Traders in a round, about. */
  traders: number;
  /** The share of them who know the truth. */
  informed: number;
  /** Likelihood ratios the evidence may carry. */
  ratios: number[];
  /** The step base rates come in. */
  priorStep: number;
  /** How often a round has evidence at all. */
  evidence: number;
}

/** Rounds and seconds are the reference's presets; the rest is this port's. */
export const LEVELS: Record<Level, LevelRules> = {
  easy: { rounds: 6, seconds: 60, band: 2, maxWidth: 14, traders: 10, informed: 0.2, ratios: [2, 3, 4], priorStep: 5, evidence: 0.8 },
  medium: { rounds: 8, seconds: 45, band: 4, maxWidth: 12, traders: 12, informed: 0.3, ratios: [1.5, 2, 2.5, 3, 4, 5], priorStep: 5, evidence: 0.85 },
  hard: { rounds: 10, seconds: 30, band: 6, maxWidth: 10, traders: 14, informed: 0.4, ratios: [1.5, 2.5, 3.5, 6, 8], priorStep: 1, evidence: 0.9 },
};

export interface EventSettings {
  level: Level;
  /** 4–12. */
  rounds: number;
  /** Seconds per round; `null` for no clock. */
  seconds: number | null;
}

export const DEFAULT_EVENTS: EventSettings = { level: "easy", rounds: 6, seconds: 60 };
export const ROUNDS = [4, 12] as const;

// --- a round ---------------------------------------------------------------------------

export interface Brief {
  title: string;
  /** The base rate, in percent. */
  prior: number;
  direction: Direction;
  ratio: number | null;
  evidence: string | null;
  band: number;
  maxWidth: number;
  traders: number;
}

export interface Contract {
  brief: Brief;
  /** What the brief implies — the update done right — in percent. */
  implied: number;
  /** The true probability, within the band of the update. */
  truth: number;
  happened: boolean;
}

const between = (lo: number, hi: number, rand: Rand) => lo + rand() * (hi - lo);
const pick = <T>(xs: readonly T[], rand: Rand): T => xs[Math.floor(rand() * xs.length)];
const clampP = (p: number) => Math.max(1, Math.min(99, p));

/** The odds-form update: odds times the ratio for, divided against. */
export function posterior(prior: number, ratio: number | null, direction: Direction): number {
  if (!ratio || direction === "none") return prior;
  const odds = (prior / (100 - prior)) * (direction === "for" ? ratio : 1 / ratio);
  return (100 * odds) / (1 + odds);
}

/** A game's contracts, drawn up front: different events, each with its brief and its truth. */
export function makeContracts(settings: EventSettings, rand: Rand): Contract[] {
  const rules = LEVELS[settings.level];
  const deck = [...EVENTS];
  for (let i = deck.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [deck[i], deck[j]] = [deck[j], deck[i]];
  }
  return Array.from({ length: settings.rounds }, (_, n) => {
    const e = deck[n % deck.length];
    const step = rules.priorStep;
    const prior = Math.round(between(e.prior[0], e.prior[1], rand) / step) * step;
    const hasEvidence = rand() < rules.evidence;
    const direction: Direction = hasEvidence ? (rand() < 0.5 ? "for" : "against") : "none";
    const ratio = hasEvidence ? pick(rules.ratios, rand) : null;
    const implied = posterior(prior, ratio, direction);
    const truth = clampP(implied + between(-rules.band, rules.band, rand));
    const traders = rules.traders + Math.floor(between(-2, 3, rand));
    return {
      brief: {
        title: e.title,
        prior,
        direction,
        ratio,
        evidence: direction === "for" ? e.up : direction === "against" ? e.down : null,
        band: rules.band,
        maxWidth: rules.maxWidth,
        traders,
      },
      implied,
      truth,
      happened: rand() * 100 < truth,
    };
  });
}

// --- quoting and the flow -------------------------------------------------------------

export interface Quote {
  bid: number;
  ask: number;
}

/** Why a quote is refused, or `null`. */
export function quoteProblem(q: Quote, maxWidth: number): string | null {
  if (!Number.isInteger(q.bid) || !Number.isInteger(q.ask)) return "Quote in whole cents.";
  if (q.bid < 0 || q.ask > 100) return "A quote runs from 0¢ to 100¢.";
  if (q.bid >= q.ask) return "The bid must be below the ask.";
  if (q.ask - q.bid > maxWidth) return `This round's widest quote is ${maxWidth}¢; that one is ${q.ask - q.bid}¢.`;
  return null;
}

export const mid = (q: Quote) => (q.bid + q.ask) / 2;

/** A trade against your quote: you `sold` to a buyer at your ask, or `bought` from a seller at your bid. */
export interface Fill {
  side: "sold" | "bought";
  size: number;
  price: number;
  informed: boolean;
}

/** Noise traders will pay up to this much half-spread, and no more — drawn evenly between. */
export const PATIENCE: [number, number] = [1, 7];
export const NOISE_SIZE: [number, number] = [1, 4];
export const INFORMED_SIZE: [number, number] = [3, 6];

const size = ([lo, hi]: [number, number], rand: Rand) => lo + Math.floor(rand() * (hi - lo + 1));

/**
 * The flow against a quote: each trader in turn is informed or noise. The
 * informed buy when the truth is above your ask and sell when it is below
 * your bid; noise flips a coin for its side and trades only if your spread is
 * within its patience.
 */
export function flow(q: Quote, c: Contract, rules: Pick<LevelRules, "informed">, rand: Rand): Fill[] {
  const fills: Fill[] = [];
  const half = (q.ask - q.bid) / 2;
  for (let i = 0; i < c.brief.traders; i++) {
    if (rand() < rules.informed) {
      const s = size(INFORMED_SIZE, rand);
      if (c.truth > q.ask) fills.push({ side: "sold", size: s, price: q.ask, informed: true });
      else if (c.truth < q.bid) fills.push({ side: "bought", size: s, price: q.bid, informed: true });
      continue;
    }
    const buys = rand() < 0.5;
    const patience = between(PATIENCE[0], PATIENCE[1], rand);
    const s = size(NOISE_SIZE, rand);
    if (half <= patience) fills.push(buys ? { side: "sold", size: s, price: q.ask, informed: false } : { side: "bought", size: s, price: q.bid, informed: false });
  }
  return fills;
}

/** What a fill was worth to you, in cents, marked at the truth. */
export const fillValue = (f: Fill, truth: number) => (f.side === "sold" ? f.price - truth : truth - f.price) * f.size;

/** Contracts held at the close: bought less sold. */
export const inventory = (fills: Fill[]) => fills.reduce((t, f) => t + (f.side === "bought" ? f.size : -f.size), 0);

const r1 = (x: number) => Math.round(x * 10) / 10;

/** P&L points: the fills' worth, marked at the truth, at a point for every two cents. */
export const pnlPoints = (fills: Fill[], truth: number) => r1(fills.reduce((t, f) => t + fillValue(f, truth), 0) / 2);

/**
 * Calibration points: a Brier-style bonus on your midpoint — the coin flip's
 * squared error less yours, scaled. A mid on the truth earns the most there is
 * for that truth; 50 earns nothing; a mid further off than 50 loses.
 */
export const CALIBRATION_SCALE = 40;
export const calibrationPoints = (m: number, truth: number) => r1((CALIBRATION_SCALE * ((50 - truth) ** 2 - (m - truth) ** 2)) / 2500);

// --- the balance, and a verdict -------------------------------------------------------

/**
 * What a quote of width `w` centred on the update is worth on average — over
 * where the truth may sit in the band and the traders who may turn up. Noise
 * pays the half-spread if its patience allows; the informed take whatever the
 * truth leaves outside your quote.
 */
export function expectedPnl(w: number, rules: LevelRules, traders = rules.traders): number {
  const half = w / 2;
  const accept = Math.max(0, Math.min(1, (PATIENCE[1] - half) / (PATIENCE[1] - PATIENCE[0])));
  const noiseSize = (NOISE_SIZE[0] + NOISE_SIZE[1]) / 2;
  const informedSize = (INFORMED_SIZE[0] + INFORMED_SIZE[1]) / 2;
  const b = rules.band;
  // E[max(0, |U| − half)] for U uniform on [−b, b].
  const leak = half >= b ? 0 : (b - half) ** 2 / (2 * b);
  return traders * ((1 - rules.informed) * accept * half * noiseSize - rules.informed * leak * informedSize);
}

/** The width that balances the two: the best on average, centred on the update. */
export function balancedWidth(rules: LevelRules): number {
  let best = 1;
  for (let w = 1; w <= rules.maxWidth; w++) if (expectedPnl(w, rules) > expectedPnl(best, rules)) best = w;
  return best;
}

/** The quote a careful player would make: the balanced width, centred on the update. */
export function modelQuote(c: Contract, rules: LevelRules): Quote {
  const w = balancedWidth(rules);
  const bid = Math.max(0, Math.min(100 - w, Math.round(c.implied - w / 2)));
  return { bid, ask: bid + w };
}

export type Tone = "good" | "warn" | "bad" | "neutral";

/** What the review says about a quote — the reference's four verdicts, in this port's words. */
export function verdict(q: Quote, c: Contract, rules: LevelRules, fills: Fill[]): { tone: Tone; text: string } {
  const off = r1(Math.abs(mid(q) - c.implied));
  const width = q.ask - q.bid;
  const model = balancedWidth(rules);
  const informed = fills.filter((f) => f.informed).length;
  if (off >= 5)
    return { tone: "bad", text: `Mis-centred: your mid sat ${off} points from what the brief implied. Fix the update before the width — no spread rescues a centre that far off.` };
  if (width >= model + 3)
    return { tone: "warn", text: `Too wide at ${width}¢: most of the room would not pay that spread, so you earned margin on very little volume. ${model}¢ was the balance here.` };
  if (width <= model - 3 && informed > 0)
    return {
      tone: "warn",
      text: `Too tight at ${width}¢: ${informed} informed trader${informed === 1 ? "" : "s"} took the other side of a spread that did not cover the noise band.`,
    };
  if (off <= 2) return { tone: "good", text: "Well centred and sensibly wide. What is left is which traders turned up, which is not yours to control." };
  return { tone: "neutral", text: `Close to fair value. Tightening the update by the last ${off} points is the edge left.` };
}

/** `1:3`, `3:2` — odds as a person writes them, reduced where they reduce cleanly. */
export function oddsText(p: number): string {
  const a = Math.round(p);
  const b = 100 - a;
  const g = (x: number, y: number): number => (y === 0 ? x : g(y, x % y));
  const d = g(a, b) || 1;
  return `${a / d}:${b / d}`;
}
