/**
 * The Card Game — tradermath.org's *Market Games → Card Game* (market
 * taking), the easter egg's fifth game. Asked for 2026-09-24, with the two
 * games after it: *"please port all of the 3 remaining game over"*.
 *
 * **Its rules, kept.** Each round deals a hand of playing cards worth 2–14
 * (the ace 14) — at most one of them face up. A market maker quotes *bid at
 * ask* on the hand's total, and you buy, sell or skip against a clock; the
 * cards then settle at their sum. The rest is the dice game's, because it is
 * the same game with a different hand, and lives once in
 * [`lib/takerTable.ts`](takerTable.ts): start with 500, an order over budget
 * is forfeited at 50, report your P&L exactly or forfeit the round's profit
 * and 50, and with the balance hidden, say it at the end.
 *
 * **The maker quotes blind.** It deals itself a *different* hand from the
 * same deck and quotes that one's sum, one either side — "the market maker
 * provides random quotes", as the reference puts it. The card you can see,
 * and the deck's average for the rest, is your edge.
 *
 * The deck and its market events are the Trading Game's (`lib/tradingGame.ts`)
 * — the reference shares them between the two games too. One place this port
 * is more exact than the reference: the worst a hand could come to is worked
 * out from the cards still in the deck (four aces at most), where the
 * reference counts every hidden card as an ace.
 *
 * The clocks are the arcade's (`lib/arcadeClock.ts`): any second or none.
 */

import { SECONDS } from "@/lib/arcadeClock";
import type { Dealer, TakerSettings } from "@/lib/takerTable";
import type { Describe } from "@/lib/takerStats";
import { deckFor, eventText, maxPrice, pickEvent, settlePrice, theoreticalValue, type Card, type MarketEvent, type Rand } from "@/lib/tradingGame";

export { eventText, type Card, type MarketEvent };

export type Difficulty = "easy" | "medium" | "hard" | "custom";

/** `count` is how many cards: 3–5. Rounds: 3–8. */
export interface CardSettings extends TakerSettings {
  difficulty: Difficulty;
}

type Preset = Pick<CardSettings, "count" | "rounds" | "tradeSeconds" | "showBalance" | "events">;

/** The reference's own presets, to the number. */
export const PRESETS: Record<Exclude<Difficulty, "custom">, Preset> = {
  easy: { count: 3, rounds: 4, tradeSeconds: 60, showBalance: true, events: false },
  medium: { count: 4, rounds: 6, tradeSeconds: 20, showBalance: true, events: true },
  hard: { count: 5, rounds: 8, tradeSeconds: 5, showBalance: false, events: true },
};

export const DEFAULT_CARDS: CardSettings = { difficulty: "easy", ...PRESETS.easy, reportSeconds: null };

export const LIMITS = {
  count: [3, 5],
  rounds: [3, 8],
  seconds: SECONDS,
} as const;

function shuffled(cards: Card[], rand: Rand): Card[] {
  const deck = [...cards];
  for (let i = deck.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [deck[i], deck[j]] = [deck[j], deck[i]];
  }
  return deck;
}

/** A hand: `n` cards off this round's deck, the first face up half the time, the rest face down. */
export function dealHand(n: number, e: MarketEvent | null, rand: Rand): Card[] {
  const openCount = Math.floor(2 * rand());
  return shuffled(deckFor(e), rand)
    .slice(0, n)
    .map((c, i) => ({ ...c, open: i < openCount }));
}

/** The maker's quote: the sum of a hand it dealt itself, one either side. */
export function blindQuote(n: number, e: MarketEvent | null, rand: Rand): { bid: number; ask: number } {
  const guess = settlePrice(shuffled(deckFor(e), rand).slice(0, n));
  return { bid: guess - 1, ask: guess + 1 };
}

/** The card game's dealer for the shared market-taking rules. */
export const dealCards: Dealer<Card, MarketEvent, CardSettings> = (n, settings, rand) => {
  const event = n > 1 && settings.events ? pickEvent(settings.rounds, rand) : null;
  const items = dealHand(settings.count, event, rand);
  return {
    event,
    items,
    quote: blindQuote(settings.count, event, rand),
    settle: settlePrice(items),
    fair: theoreticalValue(items, event),
    worst: maxPrice(items, event),
  };
};

/** How a card is written in the history: `Q♥`. */
export const describeCards: Describe<Card, MarketEvent> = { item: (c) => `${c.rank}${c.suit}`, open: (c) => c.open, event: eventText };
