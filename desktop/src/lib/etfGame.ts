/**
 * The ETF Arbitrage Game — the easter egg's fourth game. Asked for 2026-09-24
 * with a reference: tradermath.org's *Market Games → ETF Arbitrage Game*.
 *
 * **Its rules, kept.** A game trades one ETF on a basket of four stocks, the
 * basket and its weights fixed for the game. Each round the stock prices move
 * and the ETF is quoted *bid / ask* at a premium or a discount to its **NAV** —
 * the weighted sum of the stock prices. Work the NAV out; buy when it is above
 * the ask, sell when it is below the bid, or skip. You choose 1–20 units. A
 * trade costs a flat $10, and the first player to trade in a round earns 20% on
 * top of a profit. You race three bots with personalities of their own — fast
 * and careless, slow and careful — and the table ranks all four by P&L.
 *
 * The baskets, the price moves, the premium ranges, the scoring and the bots'
 * personalities were read from the reference's own game script and ported;
 * the headlines and every other word are ours.
 *
 * **Loosened,** as the dice game's clock was: the time per round is any second
 * from 1 to 600 or none, not 8–30; rounds and clock are yours at any
 * difficulty; the W × P column can be hidden, so the multiplication is yours
 * too; and "market events" does what it says — off, prices drift without
 * news; on, headlines move them. (In the reference the switch has no effect.)
 *
 * Money is kept in **cents**, so a session's P&L adds up exactly.
 *
 * Pure throughout, randomness passed in, so a test can play a game from a seed.
 */

import { seeded, type Rand } from "@/lib/tradingGame";

export { seeded, type Rand };

// --- the baskets --------------------------------------------------------------------

export type Theme = "tech" | "banks" | "energy" | "health" | "retail" | "industrial" | "chips" | "telecom" | "aerospace" | "auto" | "media";

export interface Basket {
  name: string;
  theme: Theme;
  stocks: { ticker: string; base: number }[];
}

const basket = (name: string, theme: Theme, stocks: [string, number][]): Basket => ({
  name,
  theme,
  stocks: stocks.map(([ticker, base]) => ({ ticker, base })),
});

/** The reference's nineteen, with its starting prices. */
export const BASKETS: Basket[] = [
  basket("Tech", "tech", [["AAPL", 185], ["MSFT", 375], ["GOOGL", 140], ["NVDA", 280]]),
  basket("Finance", "banks", [["JPM", 195], ["BAC", 35], ["GS", 380], ["MS", 88]]),
  basket("Energy", "energy", [["XOM", 105], ["CVX", 155], ["COP", 115], ["SLB", 52]]),
  basket("Healthcare", "health", [["JNJ", 160], ["UNH", 520], ["PFE", 28], ["ABT", 110]]),
  basket("Consumer", "retail", [["AMZN", 180], ["WMT", 165], ["PG", 155], ["KO", 60]]),
  basket("Industrial", "industrial", [["CAT", 290], ["HON", 200], ["UPS", 145], ["BA", 215]]),
  basket("Semiconductor", "chips", [["NVDA", 280], ["AMD", 155], ["INTC", 32], ["AVGO", 170]]),
  basket("Telecom", "telecom", [["T", 22], ["VZ", 42], ["TMUS", 165], ["CMCSA", 44]]),
  basket("Retail", "retail", [["WMT", 165], ["COST", 740], ["HD", 350], ["TGT", 145]]),
  basket("Aerospace", "aerospace", [["BA", 215], ["LMT", 450], ["RTX", 105], ["NOC", 470]]),
  basket("Auto", "auto", [["TSLA", 250], ["GM", 38], ["F", 12], ["TM", 190]]),
  basket("Pharma", "health", [["LLY", 780], ["MRK", 125], ["ABBV", 175], ["BMY", 52]]),
  basket("Media", "media", [["DIS", 95], ["NFLX", 620], ["WBD", 10], ["PARA", 14]]),
  basket("Big Tech", "tech", [["AAPL", 200], ["MSFT", 400], ["GOOGL", 150], ["AMZN", 200]]),
  basket("Banks", "banks", [["JPM", 200], ["BAC", 40], ["GS", 400], ["WFC", 60]]),
  basket("Oil & Gas", "energy", [["XOM", 100], ["CVX", 150], ["COP", 100], ["SLB", 50]]),
  basket("Health", "health", [["JNJ", 160], ["UNH", 500], ["PFE", 30], ["ABT", 110]]),
  basket("Big Box", "retail", [["WMT", 160], ["COST", 700], ["HD", 350], ["TGT", 150]]),
  basket("Transport", "industrial", [["UPS", 140], ["FDX", 260], ["DAL", 50], ["UNP", 250]]),
];

const COMPANY: Record<string, string> = {
  AAPL: "Apple", MSFT: "Microsoft", GOOGL: "Alphabet", NVDA: "Nvidia", JPM: "JPMorgan", BAC: "Bank of America",
  GS: "Goldman Sachs", MS: "Morgan Stanley", XOM: "Exxon", CVX: "Chevron", COP: "ConocoPhillips", SLB: "SLB",
  JNJ: "Johnson & Johnson", UNH: "UnitedHealth", PFE: "Pfizer", ABT: "Abbott", AMZN: "Amazon", WMT: "Walmart",
  PG: "Procter & Gamble", KO: "Coca-Cola", CAT: "Caterpillar", HON: "Honeywell", UPS: "UPS", BA: "Boeing",
  AMD: "AMD", INTC: "Intel", AVGO: "Broadcom", T: "AT&T", VZ: "Verizon", TMUS: "T-Mobile", CMCSA: "Comcast",
  COST: "Costco", HD: "Home Depot", TGT: "Target", LMT: "Lockheed Martin", RTX: "RTX", NOC: "Northrop Grumman",
  TSLA: "Tesla", GM: "General Motors", F: "Ford", TM: "Toyota", LLY: "Eli Lilly", MRK: "Merck", ABBV: "AbbVie",
  BMY: "Bristol Myers", DIS: "Disney", NFLX: "Netflix", WBD: "Warner Bros.", PARA: "Paramount", WFC: "Wells Fargo",
  FDX: "FedEx", DAL: "Delta", UNP: "Union Pacific",
};

// --- news ---------------------------------------------------------------------------

/** How strongly a headline moves prices: `++` a rally, `--` a rout. */
export type Sentiment = "++" | "+" | "-" | "--";

/** The range a headline moves each stock by, as a fraction — the reference's. */
export const MOVE: Record<Sentiment, [number, number]> = {
  "++": [0.02, 0.05],
  "+": [0.005, 0.03],
  "-": [-0.03, -0.005],
  "--": [-0.05, -0.02],
};

type Headline = { text: string; sentiment: Sentiment };
const h = (sentiment: Sentiment, text: string): Headline => ({ text, sentiment });

const MARKET_NEWS: Headline[] = [
  h("++", "Central bank cuts rates by half a point"),
  h("++", "Inflation cools far faster than forecast"),
  h("++", "Broad tariff truce announced"),
  h("+", "Jobs report beats expectations"),
  h("+", "Consumer sentiment climbs to a one-year high"),
  h("+", "Factory activity expands for a third month"),
  h("-", "Bond yields jump on a hot inflation print"),
  h("-", "Jobless claims rise for a third week"),
  h("-", "Growth abroad slows more than expected"),
  h("--", "Surprise rate hike rattles markets"),
  h("--", "Geopolitical scare sends volatility soaring"),
];

const THEME_NEWS: Record<Theme, Headline[]> = {
  tech: [
    h("++", "Tech earnings beat across the board"),
    h("+", "AI spending forecasts raised again"),
    h("-", "Regulators open a probe into the big platforms"),
    h("--", "Growth scare triggers a tech selloff"),
  ],
  banks: [
    h("++", "Banks post record trading revenue"),
    h("+", "Loan demand picks up as credit eases"),
    h("-", "Tougher capital rules proposed for big banks"),
    h("--", "Loan defaults jump to a multi-year high"),
  ],
  energy: [
    h("++", "Producers agree deeper output cuts"),
    h("+", "Cold snap lifts gas prices"),
    h("-", "Record shale output weighs on crude"),
    h("--", "Oil demand outlook slashed"),
  ],
  health: [
    h("++", "Landmark drug approval lifts the sector"),
    h("+", "Coverage expansion bill gains support"),
    h("-", "Drug-pricing reform moves forward"),
    h("--", "Regulator rejects a closely watched drug"),
  ],
  retail: [
    h("++", "Holiday sales smash records"),
    h("+", "Consumer spending rises for a fifth month"),
    h("-", "Inventories pile up as shoppers pull back"),
    h("--", "Card delinquencies hit a decade high"),
  ],
  industrial: [
    h("++", "Infrastructure package unlocks new contracts"),
    h("+", "Factory orders beat forecasts"),
    h("-", "Supply snags delay shipments"),
    h("--", "Freight volumes fall sharply"),
  ],
  chips: [
    h("++", "AI build-out sends chip orders soaring"),
    h("+", "Chip subsidies start to flow"),
    h("-", "Consumer chip inventories swell"),
    h("--", "Export curbs tighten on advanced chips"),
  ],
  telecom: [
    h("++", "Spectrum auction ends cheaper than feared"),
    h("+", "Merger approval brightens the outlook"),
    h("-", "Carriers start a price war"),
    h("--", "Stricter licensing rules proposed"),
  ],
  aerospace: [
    h("++", "Record defence budget passes"),
    h("+", "Airline places a huge jet order"),
    h("-", "Safety review grounds a popular aircraft"),
    h("--", "Major defence programmes cancelled"),
  ],
  auto: [
    h("++", "Electric vehicle sales jump 40%"),
    h("+", "Car sales beat estimates"),
    h("-", "New tariffs proposed on imported cars"),
    h("--", "Mass recall halts production lines"),
  ],
  media: [
    h("++", "Streaming growth reaccelerates"),
    h("+", "Ad spending rebounds"),
    h("-", "Cord-cutting speeds up"),
    h("--", "Box office slumps to a decade low"),
  ],
};

/** One company's bad day: it falls, and its neighbours in the basket pick up a little. */
const SHOCKS: { text: string; drop: [number, number]; lift: [number, number] }[] = [
  { text: "{company} cuts its full-year outlook", drop: [-0.04, -0.07], lift: [0.005, 0.02] },
  { text: "{company} misses earnings by a wide margin", drop: [-0.05, -0.09], lift: [0.01, 0.03] },
  { text: "{company} recalls its flagship product", drop: [-0.05, -0.08], lift: [0.01, 0.03] },
  { text: "{company} loses its biggest customer", drop: [-0.06, -0.1], lift: [0.01, 0.03] },
  { text: "{company} faces a regulator's investigation", drop: [-0.03, -0.06], lift: [0.01, 0.02] },
  { text: "{company} chief executive steps down without notice", drop: [-0.04, -0.08], lift: [0.01, 0.02] },
];

// --- settings -------------------------------------------------------------------------

export type Level = "easy" | "medium" | "hard";

export type BotStyle = "rookie" | "cautious" | "slowpoke" | "balanced" | "gambler" | "patient" | "quant" | "sniper" | "whale";

export interface LevelRules {
  rounds: number;
  seconds: number;
  /** The step weights come in: tenths, or hundredths. */
  weightStep: number;
  /** How far, in percent, the ETF may sit from its NAV. */
  premium: [number, number];
  /** Easy prices are whole dollars. */
  wholePrices: boolean;
  bots: [BotStyle, BotStyle, BotStyle];
}

/** The reference's levels, to the number. */
export const LEVELS: Record<Level, LevelRules> = {
  easy: { rounds: 6, seconds: 20, weightStep: 0.1, premium: [1.5, 4], wholePrices: true, bots: ["rookie", "cautious", "slowpoke"] },
  medium: { rounds: 8, seconds: 15, weightStep: 0.01, premium: [0.8, 3], wholePrices: false, bots: ["balanced", "gambler", "patient"] },
  hard: { rounds: 10, seconds: 12, weightStep: 0.01, premium: [0.5, 2], wholePrices: false, bots: ["quant", "sniper", "whale"] },
};

export interface EtfSettings {
  level: Level;
  /** 4–15. */
  rounds: number;
  /** Seconds per round; `null` for no clock. */
  seconds: number | null;
  /** Headlines move prices. Off, they drift without news. */
  events: boolean;
  /** The W × P column — off, the multiplication is yours too. */
  showWxP: boolean;
}

export const DEFAULT_ETF: EtfSettings = { level: "easy", rounds: 6, seconds: 20, events: false, showWxP: true };

export const ROUNDS = [4, 15] as const;
export const MAX_UNITS = 20;
/** A trade's flat cost, in cents. */
export const COST = 1000;
/** The first trader's bonus on a profit. */
export const FIRST_BONUS = 0.2;

// --- the market -------------------------------------------------------------------------

export interface Stock {
  ticker: string;
  weight: number;
  price: number;
  /** Last round's price; `null` in the first. */
  prev: number | null;
}

export interface Market {
  basket: string;
  stocks: Stock[];
  /** The fair value: Σ weight × price, to the cent. */
  nav: number;
  bid: number;
  ask: number;
  /** Where the quote sits from the NAV, in percent: positive a premium, negative a discount. */
  premiumPct: number;
  headline: string | null;
  sentiment: Sentiment | null;
}

const between = (lo: number, hi: number, rand: Rand) => lo + rand() * (hi - lo);
const cents = (x: number) => Math.round(x * 100) / 100;
const pick = <T>(xs: readonly T[], rand: Rand): T => xs[Math.floor(rand() * xs.length)];

/**
 * Weights for `n` stocks in steps of `step`, each at least one step, summing to
 * one — the reference's way: one step each, the rest dealt at random, shuffled.
 */
export function makeWeights(n: number, step: number, rand: Rand): number[] {
  const units = Math.round(1 / step);
  const w = new Array<number>(n).fill(1);
  for (let i = 0; i < units - n; i++) w[Math.floor(rand() * n)]++;
  for (let i = n - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [w[i], w[j]] = [w[j], w[i]];
  }
  return w.map((u) => cents(u * step));
}

export const navOf = (stocks: Pick<Stock, "weight" | "price">[]) => cents(stocks.reduce((t, s) => t + s.weight * s.price, 0));

/** A price moved by a fraction: whole dollars on easy (and always by at least one), cents otherwise. */
function move(price: number, by: number, whole: boolean): number {
  if (whole) {
    const next = Math.round(price * (1 + by));
    return next === price ? Math.max(1, price + (by >= 0 ? 1 : -1)) : Math.max(1, next);
  }
  return cents(Math.max(0.01, price * (1 + by)));
}

/**
 * A whole game's markets, drawn up front. The first round is the basket as it
 * opens; each later one moves it — on news when events are on (a company's bad
 * day one time in five, otherwise a headline for the sector or the market; a
 * mild one moves a stock the other way one time in five), by a drift of up to
 * 3% either way when they are off.
 */
export function makeMarkets(settings: EtfSettings, rand: Rand): Market[] {
  const rules = LEVELS[settings.level];
  const b = pick(BASKETS, rand);
  const weights = makeWeights(b.stocks.length, rules.weightStep, rand);
  let prices = b.stocks.map((s) =>
    rules.wholePrices ? s.base + Math.round(between(-5, 5, rand)) : cents(s.base * (1 + between(-0.03, 0.03, rand))),
  );
  let prev: number[] | null = null;
  const recent: string[] = [];
  const out: Market[] = [];
  for (let n = 0; n < settings.rounds; n++) {
    let headline: string | null = null;
    let sentiment: Sentiment | null = null;
    if (n > 0) {
      prev = prices;
      if (!settings.events) {
        prices = prices.map((p) => move(p, between(-0.03, 0.03, rand), rules.wholePrices));
      } else {
        const roll = rand();
        if (roll < 0.2) {
          const target = Math.floor(rand() * b.stocks.length);
          const shock = pick(SHOCKS, rand);
          headline = shock.text.replace("{company}", COMPANY[b.stocks[target].ticker] ?? b.stocks[target].ticker);
          sentiment = "-";
          prices = prices.map((p, i) =>
            move(p, i === target ? between(shock.drop[0], shock.drop[1], rand) : between(shock.lift[0], shock.lift[1], rand), rules.wholePrices),
          );
        } else {
          const pool = roll < 0.68 ? THEME_NEWS[b.theme] : MARKET_NEWS;
          const fresh = pool.filter((x) => !recent.includes(x.text));
          const news = pick(fresh.length ? fresh : pool, rand);
          recent.push(news.text);
          if (recent.length > 5) recent.shift();
          headline = news.text;
          sentiment = news.sentiment;
          const [lo, hi] = MOVE[news.sentiment];
          const mild = news.sentiment === "+" || news.sentiment === "-";
          prices = prices.map((p) => move(p, mild && rand() >= 0.8 ? between(-hi, -lo, rand) : between(lo, hi, rand), rules.wholePrices));
        }
      }
    }
    const stocks = b.stocks.map((s, i) => ({ ticker: s.ticker, weight: weights[i], price: prices[i], prev: prev ? prev[i] : null }));
    const nav = navOf(stocks);
    const off = between(rules.premium[0], rules.premium[1], rand) / 100;
    const sign = rand() < 0.5 ? 1 : -1;
    const mid = nav * (1 + sign * off);
    const half = cents(0.0005 * mid);
    out.push({
      basket: b.name,
      stocks,
      nav,
      bid: cents(mid - half),
      ask: cents(mid + half),
      premiumPct: cents(sign * off * 100),
      headline,
      sentiment,
    });
  }
  return out;
}

// --- trading --------------------------------------------------------------------------

export type EtfAction = "buy" | "sell" | "pass";

/** A round's result for one trade, in cents. */
export interface Score {
  gross: number;
  cost: number;
  bonus: number;
  net: number;
}

/** The side the NAV favours: buy under it, sell over it. */
export function rightSide(m: Pick<Market, "nav" | "bid" | "ask">): "buy" | "sell" | null {
  if (m.nav > m.ask) return "buy";
  if (m.nav < m.bid) return "sell";
  return null;
}

/**
 * What a trade makes: the edge on each unit, less the $10, plus — for the
 * first to trade, on a profit — a fifth more. A pass makes nothing.
 */
export function score(action: EtfAction, units: number, m: Pick<Market, "nav" | "bid" | "ask">, first: boolean): Score {
  if (action === "pass") return { gross: 0, cost: 0, bonus: 0, net: 0 };
  const edge = action === "buy" ? m.nav - m.ask : m.bid - m.nav;
  const gross = Math.round(edge * units * 100);
  const afterCost = gross - COST;
  const bonus = first && afterCost > 0 ? Math.round(afterCost * FIRST_BONUS) : 0;
  return { gross, cost: COST, bonus, net: afterCost + bonus };
}

// --- the bots ---------------------------------------------------------------------------

export interface Personality {
  /** Seconds before it trades. */
  delay: [number, number];
  /** How often it picks the right side. */
  correct: number;
  units: [number, number];
  /** How often it piles on 3–8 more units than it meant to. */
  oversize: number;
  /** It passes when a middling trade on the right side would net less than this many dollars. */
  skipBelow: number;
  /** In a word, for the table. */
  blurb: string;
}

/** The reference's nine, to the number. */
export const PERSONALITIES: Record<BotStyle, Personality> = {
  rookie: { delay: [5, 9], correct: 0.6, units: [2, 5], oversize: 0, skipBelow: 0, blurb: "new to this" },
  cautious: { delay: [6, 10], correct: 0.8, units: [1, 4], oversize: 0, skipBelow: 20, blurb: "skips thin edges" },
  slowpoke: { delay: [7, 12], correct: 0.75, units: [3, 7], oversize: 0, skipBelow: 0, blurb: "takes its time" },
  balanced: { delay: [3, 5], correct: 0.88, units: [4, 10], oversize: 0, skipBelow: 0, blurb: "steady" },
  gambler: { delay: [2, 4], correct: 0.65, units: [10, 18], oversize: 0.25, skipBelow: 0, blurb: "fast, big, careless" },
  patient: { delay: [4, 7], correct: 0.95, units: [2, 6], oversize: 0, skipBelow: 12, blurb: "picky and accurate" },
  quant: { delay: [1.5, 3], correct: 0.95, units: [6, 14], oversize: 0, skipBelow: 0, blurb: "fast and accurate" },
  sniper: { delay: [2, 4], correct: 1, units: [5, 10], oversize: 0, skipBelow: 8, blurb: "never wrong, sometimes idle" },
  whale: { delay: [1, 2.5], correct: 0.8, units: [14, 20], oversize: 0.35, skipBelow: 0, blurb: "fastest, biggest" },
};

export const BOT_NAMES = ["Ada", "Blaise", "Carl", "Emmy", "Kurt", "Sofia", "Alan", "Grace", "Srini", "Hypatia", "Pierre", "Maryam", "Leon", "Ramsey"];

/** A bot's trade, as the reference's bots decide one. */
export function botDecision(style: BotStyle, m: Pick<Market, "nav" | "bid" | "ask">, rand: Rand): { action: EtfAction; units: number } {
  const p = PERSONALITIES[style];
  const right = rightSide(m) ?? (rand() < 0.5 ? "buy" : "sell");
  if (p.skipBelow > 0) {
    const middling = (p.units[0] + p.units[1]) / 2;
    if (score(right, middling, m, false).net < p.skipBelow * 100) return { action: "pass", units: 0 };
  }
  const action = rand() < p.correct ? right : right === "buy" ? "sell" : "buy";
  let units = Math.round(between(p.units[0], p.units[1], rand));
  if (p.oversize && rand() < p.oversize) units = Math.min(MAX_UNITS, units + Math.round(between(3, 8, rand)));
  return { action, units: Math.max(1, Math.min(MAX_UNITS, units)) };
}

/** Seconds before a bot acts: its own delay — hurried along once you have traded. */
export const botDelay = (style: BotStyle, rand: Rand) => between(PERSONALITIES[style].delay[0], PERSONALITIES[style].delay[1], rand);
export const HURRY: [number, number] = [0.5, 2];
export const hurried = (rand: Rand) => between(HURRY[0], HURRY[1], rand);

// --- words --------------------------------------------------------------------------------

/** `$1,234.50`, from cents. */
export function usd(c: number): string {
  const abs = Math.abs(c) / 100;
  return `${c < 0 ? "−" : ""}$${abs.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

/** `+$37.20`, `−$10.00`, from cents. */
export const signedUsd = (c: number) => `${c >= 0 ? "+" : ""}${usd(c)}`;
