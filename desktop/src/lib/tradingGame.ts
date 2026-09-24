/**
 * The Trading Game — the easter egg's second game. Asked for 2026-09-24 with a
 * reference: tradermath.org's *Market Games → Trading Game*.
 *
 * **Its rules, kept.** Each round deals a hand of cards worth 2–14 (the ace is
 * 14), some face up. One player quotes a market — *bid at ask* — and the others
 * may buy at the ask, sell at the bid, or skip. The cards are then turned over
 * and the market settles at their sum. Four players: you and three bots,
 * each starting with 500. You take turns making the market (rotational), or
 * race to quote first (open outcry). After each round you trade in, you report
 * your profit or loss — exactly, or the round's profit is forfeited and 50 is
 * taken besides. Market events may filter or re-value the deck. An order must
 * fit your balance: a purchase must be affordable, and a sale must survive the
 * worst the hidden cards could be.
 *
 * The presets, the bots' quoting (about 8 a card, blind to the face-up cards —
 * that is the edge you are meant to find) and the bots' trading were read from
 * the reference's own game script and ported, not reinvented; the wording is
 * ours.
 *
 * Everything here is pure and takes its randomness as an argument, so a test
 * can play a whole game from a seed.
 */

export type Rank = "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "10" | "J" | "Q" | "K" | "A";
export type Suit = "♠" | "♥" | "♦" | "♣";

export const RANKS: Rank[] = ["2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A"];
export const SUITS: Suit[] = ["♠", "♥", "♦", "♣"];
export const BASE_WORTH: Record<Rank, number> = {
  "2": 2, "3": 3, "4": 4, "5": 5, "6": 6, "7": 7, "8": 8, "9": 9, "10": 10, J: 11, Q: 12, K: 13, A: 14,
};
export const isRed = (s: Suit) => s === "♥" || s === "♦";

export interface Card {
  rank: Rank;
  suit: Suit;
  /** This round's worth: the rank's, unless an event re-valued it. */
  worth: number;
  open: boolean;
}

export type MarketEvent =
  | { kind: "worth"; rank: Rank; worth: number }
  | { kind: "high" }
  | { kind: "low" }
  | { kind: "even" }
  | { kind: "odd" }
  | { kind: "red" }
  | { kind: "black" };

export function eventText(e: MarketEvent): string {
  switch (e.kind) {
    case "worth":
      return `Card ${e.rank} is worth ${e.worth} this round`;
    case "high":
      return "Only cards worth 10 or more this round";
    case "low":
      return "Only cards worth 7 or less this round";
    case "even":
      return "Only even cards this round";
    case "odd":
      return "Only uneven cards this round";
    case "red":
      return "Only red cards this round";
    case "black":
      return "Only black cards this round";
  }
}

export type Rand = () => number;

/** mulberry32: a small seeded stream, so a game can be replayed from its seed. */
export function seeded(seed: number): Rand {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const pickOf = <T>(xs: readonly T[], rand: Rand): T => xs[Math.floor(rand() * xs.length)];

// --- settings -------------------------------------------------------------------

export type Difficulty = "easy" | "medium" | "hard" | "custom";
export type MakingMode = "rotational" | "openOutcry";

export interface TgSettings {
  difficulty: Difficulty;
  mode: MakingMode;
  /** 3–5. */
  cards: number;
  /** 4–12. */
  rounds: number;
  /** Seconds to quote or to trade, 2–60. */
  timeToAnswer: number;
  /** The spread your own quote must keep: `minSpread ≤ ask − bid ≤ maxSpread`. */
  minSpread: number;
  maxSpread: number;
  showBalance: boolean;
  events: boolean;
}

type Preset = Omit<TgSettings, "difficulty" | "mode">;

/** The reference's own presets, to the number. */
export const PRESETS: Record<Exclude<Difficulty, "custom">, Preset> = {
  easy: { cards: 3, rounds: 4, timeToAnswer: 60, minSpread: 1, maxSpread: 6, showBalance: true, events: false },
  medium: { cards: 4, rounds: 6, timeToAnswer: 20, minSpread: 2, maxSpread: 5, showBalance: true, events: true },
  hard: { cards: 5, rounds: 8, timeToAnswer: 5, minSpread: 2, maxSpread: 4, showBalance: false, events: true },
};

export const DEFAULT_TG: TgSettings = { difficulty: "easy", mode: "rotational", ...PRESETS.easy };

/** The ranges a custom game may take, as the reference allows them. */
export const LIMITS = {
  cards: [3, 5],
  rounds: [4, 12],
  timeToAnswer: [2, 60],
  minSpread: [1, 3],
  maxSpread: [2, 8],
} as const;

export const STARTING_BALANCE = 500;
export const PENALTY = 50;
/** An open-outcry round with no valid quote in this long is skipped. */
export const OUTCRY_SECONDS = 15;

// --- the deck -------------------------------------------------------------------

function passes(e: MarketEvent | null, rank: Rank, suit: Suit): boolean {
  const w = BASE_WORTH[rank];
  switch (e?.kind) {
    case "high":
      return w >= 10;
    case "low":
      return w <= 7;
    case "even":
      return w % 2 === 0;
    case "odd":
      return w % 2 === 1;
    case "red":
      return isRed(suit);
    case "black":
      return !isRed(suit);
    default:
      return true;
  }
}

/** The 52 cards, or the ones an event leaves, at this round's worth. */
export function deckFor(e: MarketEvent | null): Card[] {
  const out: Card[] = [];
  for (const suit of SUITS) {
    for (const rank of RANKS) {
      if (!passes(e, rank, suit)) continue;
      const worth = e?.kind === "worth" && e.rank === rank ? e.worth : BASE_WORTH[rank];
      out.push({ rank, suit, worth, open: false });
    }
  }
  return out;
}

/**
 * A round's event, or none. From the second round on, with a chance of two in
 * the number of rounds — so a game expects about two — and then one of the
 * seven kinds evenly, a re-valued card taking a worth of 1–19 other than its own.
 */
export function pickEvent(rounds: number, rand: Rand): MarketEvent | null {
  const rank = pickOf(RANKS, rand);
  let worth: number;
  do {
    worth = Math.floor(rand() * 19) + 1;
  } while (worth === BASE_WORTH[rank]);
  if (rand() > 2 / rounds) return null;
  const kinds: MarketEvent[] = [
    { kind: "worth", rank, worth },
    { kind: "high" },
    { kind: "low" },
    { kind: "even" },
    { kind: "odd" },
    { kind: "red" },
    { kind: "black" },
  ];
  return pickOf(kinds, rand);
}

/** A hand: shuffled from this round's deck, with 0 to n−1 cards face up. */
export function deal(n: number, e: MarketEvent | null, rand: Rand): Card[] {
  const deck = deckFor(e);
  for (let i = deck.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [deck[i], deck[j]] = [deck[j], deck[i]];
  }
  const hand = deck.slice(0, n);
  const openCount = Math.floor(rand() * n);
  const order = hand.map((_, i) => i);
  for (let i = order.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [order[i], order[j]] = [order[j], order[i]];
  }
  for (const i of order.slice(0, openCount)) hand[i] = { ...hand[i], open: true };
  return hand;
}

/** Where the market settles: the sum of the hand. */
export const settlePrice = (cards: Card[]) => cards.reduce((t, c) => t + c.worth, 0);

/**
 * What one hidden card is worth on average — the mean over this round's deck,
 * less the cards you can see.
 */
export function hiddenCardValue(cards: Card[], e: MarketEvent | null): number {
  const seen = cards.filter((c) => c.open);
  const rest = deckFor(e).filter((d) => !seen.some((s) => s.rank === d.rank && s.suit === d.suit));
  return rest.reduce((t, c) => t + c.worth, 0) / Math.max(1, rest.length);
}

/** The fair value of the hand: what you can see, and the average of what you cannot. */
export function theoreticalValue(cards: Card[], e: MarketEvent | null): number {
  const open = cards.filter((c) => c.open).reduce((t, c) => t + c.worth, 0);
  const hidden = cards.filter((c) => !c.open).length;
  return open + hidden * hiddenCardValue(cards, e);
}

/** The most the hand could settle at, given what is face up. */
export function maxPrice(cards: Card[], e: MarketEvent | null): number {
  const seen = cards.filter((c) => c.open);
  const hidden = cards.length - seen.length;
  const rest = deckFor(e)
    .filter((d) => !seen.some((s) => s.rank === d.rank && s.suit === d.suit))
    .map((c) => c.worth)
    .sort((a, b) => b - a);
  return seen.reduce((t, c) => t + c.worth, 0) + rest.slice(0, hidden).reduce((t, w) => t + w, 0);
}

// --- quotes and orders -----------------------------------------------------------

export interface Quote {
  bid: number;
  ask: number;
}

/**
 * A bot's quote, as the reference's bots make one: about 8 a card (the event
 * moving that), give or take 5, one either side. **Blind to the face-up cards**
 * — "the market maker provides random quotes", and reading the hand better
 * than the quote does is the whole game.
 */
export function botQuote(s: Pick<TgSettings, "cards">, e: MarketEvent | null, rand: Rand): Quote {
  let perCard = 8;
  if (e?.kind === "low") perCard = 4.5;
  else if (e?.kind === "high") perCard = 12;
  else if (e?.kind === "worth") perCard = (104 - BASE_WORTH[e.rank] + e.worth) / 13;
  const mid = Math.round(perCard * s.cards) + (Math.floor(10 * rand()) - 5);
  return { bid: mid - 1, ask: mid + 1 };
}

/** An open-outcry bot shouts the same, shifted by up to one. */
export function outcryQuote(s: Pick<TgSettings, "cards">, e: MarketEvent | null, rand: Rand): Quote {
  const q = botQuote(s, e, rand);
  const shift = Math.floor(3 * rand()) - 1;
  const ask = q.ask + shift;
  const bid = Math.max(0, Math.min(q.bid + shift, ask - 1));
  return { bid, ask };
}

/** Why a quote of yours is refused, or `null`. */
export function quoteProblem(q: Quote, s: Pick<TgSettings, "minSpread" | "maxSpread">): string | null {
  if (!Number.isInteger(q.bid) || !Number.isInteger(q.ask)) return "Quote in whole points.";
  if (q.bid < 0) return "The bid cannot be below zero.";
  if (q.bid >= q.ask) return "The bid must be less than the ask.";
  const spread = q.ask - q.bid;
  if (spread < s.minSpread || spread > s.maxSpread) {
    return `The spread must be ${s.minSpread}–${s.maxSpread} points; this one is ${spread}.`;
  }
  return null;
}

export type Side = "buy" | "sell";
export type Action = { action: Side; volume: number } | { action: "skip" };

/** Whether an order fits a balance: a buy must be affordable, a sale must survive the worst case. */
export function orderFits(side: Side, volume: number, balance: number, q: Quote, cards: Card[], e: MarketEvent | null): boolean {
  if (side === "buy") return volume * q.ask <= balance;
  return volume * Math.max(maxPrice(cards, e) - q.bid, 1) <= balance;
}

/** The most units an order of this side can be. */
export function maxVolume(side: Side, balance: number, q: Quote, cards: Card[], e: MarketEvent | null): number {
  const per = side === "buy" ? q.ask : Math.max(maxPrice(cards, e) - q.bid, 1);
  return per > 0 ? Math.max(0, Math.floor(balance / per)) : 0;
}

/** A taker's profit on a trade once the market settles. */
export function tradePnl(side: Side, volume: number, q: Quote, price: number): number {
  return side === "buy" ? volume * (price - q.ask) : volume * (q.bid - price);
}

/**
 * A bot's trade against a quote — the reference's bot, ported. It estimates the
 * hand as the face-up cards plus about 8 a hidden card, buys a quote well below
 * that and sells one well above, trades small inside it, and now and then does
 * something careless on purpose.
 */
export function botTrade(q: Quote, cards: Card[], rand: Rand): Action {
  const r = rand();
  const lean = rand();
  const reckless = rand() > 0.8;
  const wild = rand() > 0.85;
  const { bid, ask } = q;
  const open = cards.filter((c) => c.open);
  const hidden = cards.length - open.length;
  const perHidden = 8 + 2 * (lean - 0.5);
  const est = open.reduce((t, c) => t + c.worth, 0) + hidden * perHidden;
  const spread = ask - bid;
  if (wild && rand() < 0.1) {
    const pick = rand() < 0.33 ? "skip" : rand() < 0.5 ? "buy" : "sell";
    const v = Math.floor(10 * rand()) + 1;
    return pick === "skip" ? { action: "skip" } : { action: pick, volume: v };
  }
  const clamp = (v: number) => Math.max(1, Math.min(10, v));
  if (ask < 0.5 * est) return { action: "buy", volume: clamp(Math.floor(10 * r) + 5) };
  if (bid > 1.5 * est) return { action: "sell", volume: clamp(Math.floor(10 * r) + 5) };
  const outside = est > ask || est < bid;
  const inside = est >= bid && est <= ask;
  let skip = Math.min(0.4, 1 / (1 + spread / 5)) * (1 - 0.5 * r);
  if (inside && spread < 5) skip = Math.min(0.8, 0.4 + (5 - spread) / 5);
  if (outside) skip = reckless ? 0.15 : 0;
  if (rand() < skip) return { action: "skip" };
  const width = Math.min(1, spread / 20);
  const size = 0.5 + r;
  const volumeFor = (distance: number) => {
    const base = outside ? Math.floor(7 + 3 * width) : Math.floor(1 + 9 * width);
    const v = outside
      ? Math.max(3, Math.min(10, Math.floor(base * size)))
      : clamp(Math.floor(base * (1 + Math.min(1, distance / 15)) * size));
    return clamp(v + (Math.floor(3 * rand()) - 1));
  };
  if (est > ask + 1 || (reckless && bid > est)) return { action: "buy", volume: volumeFor(Math.abs(ask - est)) };
  if (est < bid - 1 || (reckless && ask < est)) return { action: "sell", volume: volumeFor(Math.abs(bid - est)) };
  const v = clamp(Math.floor(Math.floor(1 + 4 * width) * size) + (Math.floor(3 * rand()) - 1));
  return { action: rand() < 0.5 + 0.3 * (lean - 0.5) ? "buy" : "sell", volume: v };
}

/**
 * Every player's profit for a round: each taker's own, and the market maker the
 * other side of all of them.
 */
export function settle(maker: number, q: Quote, trades: Record<number, Action>, price: number): Record<number, number> {
  const out: Record<number, number> = { [maker]: 0 };
  for (const [id, a] of Object.entries(trades)) {
    const player = Number(id);
    if (player === maker || a.action === "skip") continue;
    const p = tradePnl(a.action, a.volume, q, price);
    out[player] = (out[player] ?? 0) + p;
    out[maker] -= p;
  }
  return out;
}

/**
 * What a reported profit costs when it is wrong: this round's profit is
 * forfeited and 50 taken besides; a loss stays a loss, and the 50 is added.
 */
export function reportAdjustment(actual: number, reported: number): number {
  if (reported === actual) return 0;
  return actual >= 0 ? -actual - PENALTY : -PENALTY;
}

export type QuoteScenario = "good_good" | "good_bad" | "bad_good" | "bad_bad";

/**
 * How your market went, as the reference judges a maker: was the quote centred
 * on what the cards said (within half the spread of the fair value), and did
 * the settlement land inside it?
 */
export function makerFeedback(q: Quote, cards: Card[], e: MarketEvent | null, price: number): {
  scenario: QuoteScenario;
  title: string;
  body: string;
  fair: number;
} {
  const fair = theoreticalValue(cards, e);
  const mid = (q.bid + q.ask) / 2;
  const centred = Math.abs(fair - mid) <= 0.5 * (q.ask - q.bid);
  const inside = price >= q.bid && price <= q.ask;
  const f = fair.toFixed(1);
  if (centred && inside)
    return { scenario: "good_good", fair, title: "Excellent", body: `Your quote sat on the fair value (${f}) and the market settled inside it.` };
  if (centred)
    return { scenario: "good_bad", fair, title: "Unlucky", body: `Your quote sat on the fair value (${f}), and the cards went outside it anyway.` };
  if (inside)
    return { scenario: "bad_good", fair, title: "Got lucky", body: `The settlement fell inside your quote, but it was not centred on the fair value (${f}).` };
  return { scenario: "bad_bad", fair, title: "Room to improve", body: `Your quote was off the fair value (${f}) and the market settled outside it.` };
}

/** 1st, 2nd, 3rd, 4th. */
export const ordinal = (n: number) => ["1st", "2nd", "3rd", "4th"][n - 1] ?? `${n}th`;
