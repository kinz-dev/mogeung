/**
 * The Market Making Dice Game — the easter egg's third game. Asked for
 * 2026-09-24 with a reference, tradermath.org's *Market Games → Market Making
 * Dice Game*, and one change: *"I want a more flexible setting on the time
 * limit."*
 *
 * **Its rules, kept.** Each round rolls a set of six-sided dice, one or more
 * of them face down. A market maker quotes on their total — *bid at ask*, one
 * either side of its guess — and you buy at the ask, sell at the bid, or skip,
 * against a clock. The dice then settle at their sum. You start with 500; a
 * purchase must be affordable and a sale must survive the highest the hidden
 * dice could roll, or the order is forfeited and costs 50. After a trade you
 * report your profit or loss — exactly, or the round's profit is forfeited and
 * 50 taken besides. Market events change what a die can show. With the balance
 * hidden, the game ends by asking for it.
 *
 * The presets, the faces each event leaves, the maker's quote (the face-up
 * dice, plus a uniformly random guess for each hidden one — so it is often
 * wrong, and knowing it is wrong is the game) and the penalties were read from
 * the reference's own game script and ported, not reinvented; the wording is
 * ours.
 *
 * **The time limit, loosened.** The reference ties it to the difficulty and a
 * custom game to 2–60 seconds in steps of 2. Here any whole number of seconds
 * from 1 to 600, or none at all, at any difficulty — Hard at your own pace is a
 * game worth having. And a second clock the reference does not have, off by
 * default: one on the report, for practising the arithmetic under time too.
 *
 * Pure throughout, randomness passed in, so a test can play a game from a seed.
 */

import { SECONDS } from "@/lib/arcadeClock";
import type { Dealer, TakerSettings } from "@/lib/takerTable";
import type { Describe } from "@/lib/takerStats";
import { PENALTY, STARTING_BALANCE, reportAdjustment, seeded, type Quote, type Rand } from "@/lib/tradingGame";

export { PENALTY, STARTING_BALANCE, reportAdjustment, seeded, type Quote, type Rand };

// --- dice ------------------------------------------------------------------------

export interface Die {
  /** The face it shows, 1–6. */
  show: number;
  /** What it is worth this round: its face, unless an event says otherwise. */
  worth: number;
  open: boolean;
}

export type DiceEvent = "high" | "low" | "ends" | "even" | "odd" | "oneIsSeven";

export const EVENTS: DiceEvent[] = ["high", "low", "ends", "even", "odd", "oneIsSeven"];

export function eventText(e: DiceEvent): string {
  switch (e) {
    case "high":
      return "Only dice showing 5 or 6 this round";
    case "low":
      return "Only dice showing 1 or 2 this round";
    case "ends":
      return "Only dice showing 1 or 6 this round";
    case "even":
      return "Only dice showing even numbers (2, 4, 6) this round";
    case "odd":
      return "Only dice showing odd numbers (1, 3, 5) this round";
    case "oneIsSeven":
      return "A die showing 1 is worth 7 this round";
  }
}

/**
 * A die's six sides this round, as the reference maps them: an event that
 * keeps two faces puts each on three sides, one that keeps three puts each on
 * two — so every face left is equally likely.
 */
export function faces(e: DiceEvent | null): { show: number; worth: number }[] {
  const plain = (shows: number[]) => shows.map((show) => ({ show, worth: show }));
  switch (e) {
    case "high":
      return plain([5, 5, 5, 6, 6, 6]);
    case "low":
      return plain([1, 1, 1, 2, 2, 2]);
    case "ends":
      return plain([1, 1, 1, 6, 6, 6]);
    case "even":
      return plain([2, 2, 4, 4, 6, 6]);
    case "odd":
      return plain([1, 1, 3, 3, 5, 5]);
    case "oneIsSeven":
      return [{ show: 1, worth: 7 }, ...plain([2, 3, 4, 5, 6])];
    default:
      return plain([1, 2, 3, 4, 5, 6]);
  }
}

const worths = (e: DiceEvent | null) => faces(e).map((f) => f.worth);
export const minWorth = (e: DiceEvent | null) => Math.min(...worths(e));
export const maxWorth = (e: DiceEvent | null) => Math.max(...worths(e));
/** What a hidden die is worth on average. */
export const meanWorth = (e: DiceEvent | null) => worths(e).reduce((a, b) => a + b, 0) / 6;

/** A whole number from `lo` to `hi`, both included. */
const between = (lo: number, hi: number, rand: Rand) => lo + Math.floor(rand() * (hi - lo + 1));

/**
 * A round's event, or none: a chance of two in the number of rounds — so a
 * game expects about two — then one of the six evenly.
 */
export function pickEvent(rounds: number, rand: Rand): DiceEvent | null {
  if (rand() >= 2 / rounds) return null;
  return EVENTS[Math.floor(rand() * EVENTS.length)];
}

/**
 * A roll: `n` dice, of which one to all are face down — the first ones, as the
 * reference lays them out.
 */
export function roll(n: number, e: DiceEvent | null, rand: Rand): Die[] {
  const hidden = between(1, n, rand);
  const sides = faces(e);
  return Array.from({ length: n }, (_, i) => ({ ...sides[between(0, 5, rand)], open: i >= hidden }));
}

export const total = (dice: Die[]) => dice.reduce((t, d) => t + d.worth, 0);
const openSum = (dice: Die[]) => dice.filter((d) => d.open).reduce((t, d) => t + d.worth, 0);
const hiddenCount = (dice: Die[]) => dice.filter((d) => !d.open).length;

/** The fair value: what you can see, and the average of what you cannot. */
export const fairValue = (dice: Die[], e: DiceEvent | null) => openSum(dice) + hiddenCount(dice) * meanWorth(e);

/** The most the dice could settle at, given what is face up. */
export const worstCase = (dice: Die[], e: DiceEvent | null) => openSum(dice) + hiddenCount(dice) * maxWorth(e);

/**
 * The market maker's quote, as the reference's makes it: the face-up dice as
 * they are, and for each hidden one a guess drawn evenly from what it could
 * show — then one either side. It sees what you see and guesses the rest
 * badly; the fair value does not.
 */
export function makerQuote(dice: Die[], e: DiceEvent | null, rand: Rand): Quote {
  const lo = minWorth(e);
  const hi = maxWorth(e);
  const guess = dice.reduce((t, d) => t + (d.open ? d.worth : between(lo, hi, rand)), 0);
  return { bid: guess - 1, ask: guess + 1 };
}

// --- settings --------------------------------------------------------------------

export type Difficulty = "easy" | "medium" | "hard" | "custom";

/** `count` is how many dice: 3–10. Rounds: 3–5. */
export interface DiceSettings extends TakerSettings {
  difficulty: Difficulty;
}

/** What a difficulty decides. The clocks are yours at any difficulty. */
type Preset = Pick<DiceSettings, "count" | "rounds" | "tradeSeconds" | "showBalance" | "events">;

/** The reference's own presets, to the number. */
export const PRESETS: Record<Exclude<Difficulty, "custom">, Preset> = {
  easy: { count: 3, rounds: 3, tradeSeconds: 60, showBalance: true, events: false },
  medium: { count: 5, rounds: 4, tradeSeconds: 20, showBalance: true, events: true },
  hard: { count: 8, rounds: 5, tradeSeconds: 5, showBalance: false, events: true },
};

export const DEFAULT_DICE: DiceSettings = { difficulty: "easy", ...PRESETS.easy, reportSeconds: null };

export const LIMITS = {
  count: [3, 10],
  rounds: [3, 5],
  /** Any whole second in here — the reference allows 2–60 in steps of 2. */
  seconds: SECONDS,
} as const;

/**
 * The dice game's dealer for the shared market-taking rules
 * (`lib/takerTable.ts`): an event from the second round on, when events are
 * on; a roll; and the maker's quote on it.
 */
export const dealDice: Dealer<Die, DiceEvent, DiceSettings> = (n, settings, rand) => {
  const event = n > 1 && settings.events ? pickEvent(settings.rounds, rand) : null;
  const items = roll(settings.count, event, rand);
  return {
    event,
    items,
    quote: makerQuote(items, event, rand),
    settle: total(items),
    fair: fairValue(items, event),
    worst: worstCase(items, event),
  };
};

/** How a die is written in the history: its worth. */
export const describeDice: Describe<Die, DiceEvent> = { item: (d) => String(d.worth), open: (d) => d.open, event: eventText };
