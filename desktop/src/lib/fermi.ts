/**
 * The Fermi Estimation Game — tradermath.org's *Market Games → Fermi
 * Estimation Game*, the easter egg's sixth. Asked for 2026-09-24: *"please
 * port all of the 3 remaining game over"*.
 *
 * **Its rules, kept.** Each question asks for a quantity nobody knows offhand,
 * in a stated unit. You answer with a **range** — a low and a high — and are
 * scored on a log scale, so being out by ×2 costs the same whichever way:
 *
 * - Every answer starts with a **50-point credit**.
 * - A **hit** (the answer inside your range) adds *base points* for how close
 *   the range's geometric midpoint came, times a multiplier for how narrow the
 *   range was — up to 200 in all.
 * - A **miss** keeps some of the credit: nothing for a tight miss, more the
 *   wider the range was.
 * - A skip or a timeout scores 0.
 *
 * The scoring table is the reference's, published on its own settings page,
 * to the number. Its questions are not: the reference draws them from its
 * server, so this bank is ours — sixty-one questions with answers that are
 * well established and stable, the time-sensitive ones dated.
 *
 * **Loosened,** as the other games' clocks were: any time per question from 1
 * to 600 seconds, or none; how many questions and whether hints show are yours
 * at any difficulty; and a "mixed" pool draws from all three.
 */

import type { Rand } from "@/lib/tradingGame";

export type Level = "easy" | "medium" | "hard";
export type Pool = Level | "mixed";

export interface Question {
  id: string;
  level: Level;
  text: string;
  unit: string;
  answer: number;
  hints: string[];
  /** Where the answer comes from, or when it was true, when that matters. */
  note?: string;
}

const q = (id: string, level: Level, text: string, unit: string, answer: number, hints: string[], note?: string): Question => ({
  id,
  level,
  text,
  unit,
  answer,
  hints,
  note,
});

export const BANK: Question[] = [
  // --- easy: everyday things, one or two steps -------------------------------------
  q("e-day-seconds", "easy", "How many seconds are there in a day?", "seconds", 86_400, ["There are 3,600 seconds in an hour."]),
  q("e-year-hours", "easy", "How many hours are there in a 365-day year?", "hours", 8_760, ["24 hours a day."]),
  q("e-week-minutes", "easy", "How many minutes are there in a week?", "minutes", 10_080, ["1,440 minutes in a day."]),
  q("e-century-days", "easy", "How many days are there in a century?", "days", 36_525, ["A leap year every four years."]),
  q("e-everest", "easy", "How tall is Mount Everest?", "metres", 8_849, ["Airliners cruise at about 10,000 m."]),
  q("e-moon", "easy", "How far away is the Moon, on average?", "km", 384_400, ["Earth's circumference is about 40,000 km."]),
  q("e-bones", "easy", "How many bones are there in an adult human body?", "bones", 206, ["Each hand has 27."]),
  q("e-world", "easy", "How many people live in the world?", "people", 8.1e9, ["India and China have about 1.4 billion each."], "UN estimate, 2024"),
  q("e-usa", "easy", "How many people live in the United States?", "people", 3.4e8, ["About a quarter of India's population."], "Census estimate, 2024"),
  q("e-piano", "easy", "How many keys does a standard piano have?", "keys", 88, ["52 of them are white."]),
  q("e-un", "easy", "How many member states does the United Nations have?", "countries", 193, ["Africa alone has 54."], "2024"),
  q("e-marathon", "easy", "How long is a marathon?", "metres", 42_195, ["About 26 miles, and a mile is 1.6 km."]),
  q("e-eiffel", "easy", "How tall is the Eiffel Tower, to its tip?", "metres", 330, ["About as tall as an 80-storey building."]),
  q("e-sound", "easy", "How fast does sound travel through air at sea level?", "metres per second", 343, ["Thunder a kilometre away arrives about three seconds after the flash."]),
  q("e-nile", "easy", "How long is the Nile?", "km", 6_650, ["About one and a half times the width of the United States."]),
  q("e-heart", "easy", "How many times does a resting heart beat in a day?", "beats", 100_000, ["About 70 beats a minute at rest."]),
  q("e-earth-diameter", "easy", "How wide is the Earth, pole to pole through the middle?", "km", 12_714, ["Its circumference is about 40,000 km."]),
  q("e-london", "easy", "How many people live in London?", "people", 8.9e6, ["A little over a tenth of the UK's 68 million."], "ONS estimate, 2023"),
  q("e-chess-squares", "easy", "How many squares are on a chessboard, counting every size from 1 × 1 to 8 × 8?", "squares", 204, [
    "64 of them are 1 × 1, and just one is 8 × 8.",
  ]),
  q("e-pool", "easy", "How many litres of water does an Olympic swimming pool hold?", "litres", 2.5e6, [
    "50 m long, 25 m wide, at least 2 m deep; a cubic metre is 1,000 litres.",
  ]),

  // --- medium: several steps, or a fact worth reasoning towards -------------------
  q("m-sun-distance", "medium", "How far is the Earth from the Sun, on average?", "km", 1.496e8, ["Sunlight takes about eight minutes to get here."]),
  q("m-neurons", "medium", "How many neurons are there in the human brain?", "neurons", 8.6e10, ["About ten for every person alive."]),
  q("m-bible", "medium", "How many words are there in the King James Bible?", "words", 783_000, ["About 1,200 pages in a typical printing."]),
  q("m-trench", "medium", "How deep is the deepest point in the ocean, the Challenger Deep?", "metres", 10_935, ["Deeper than Everest is tall."]),
  q("m-cells", "medium", "How many human cells make up a human body?", "cells", 3.0e13, ["Red blood cells are most of them by count."], "Sender, Fuchs & Milo, 2016"),
  q("m-languages", "medium", "How many languages are spoken in the world today?", "languages", 7_000, ["Papua New Guinea alone has over 800."], "Ethnologue, 2024"),
  q("m-burj", "medium", "How tall is the Burj Khalifa?", "metres", 828, ["About two and a half Eiffel Towers."]),
  q("m-australia", "medium", "How many people live in Australia?", "people", 2.7e7, ["About a third of Germany's population."], "ABS estimate, 2024"),
  q("m-us-gdp", "medium", "What was the United States' GDP in 2023?", "US dollars", 2.74e13, ["About $80,000 for each of 340 million people."], "BEA, 2023"),
  q("m-hairs", "medium", "How many hairs are on a typical human head?", "hairs", 100_000, ["About 200 per square centimetre of scalp."]),
  q("m-equator", "medium", "How long is the equator?", "km", 40_075, ["An airliner at 900 km/h would take about two days to fly it."]),
  q("m-iss-height", "medium", "How high above the ground does the International Space Station orbit?", "km", 420, ["A little further than London is from Paris."]),
  q("m-iss-speed", "medium", "How fast does the International Space Station travel?", "km/h", 27_600, ["It circles the Earth about every 90 minutes."]),
  q("m-year-seconds", "medium", "How many seconds are there in a year?", "seconds", 3.156e7, ["86,400 in a day."]),
  q("m-blood", "medium", "How much blood is in an adult's body?", "litres", 5, ["About 7% of body weight."]),
  q("m-mcdonalds", "medium", "How many McDonald's restaurants are there worldwide?", "restaurants", 42_000, ["About a third are in the United States."], "2023"),
  q("m-oil", "medium", "How many barrels of oil does the world use each day?", "barrels a day", 1.0e8, ["A barrel is 159 litres."], "IEA, 2023"),
  q("m-wikipedia", "medium", "How many articles does the English Wikipedia have?", "articles", 6.9e6, ["It passed six million in 2020."], "2024"),
  q("m-nigeria", "medium", "How many people live in Nigeria?", "people", 2.25e8, ["About two and a half times Germany's population."], "UN estimate, 2024"),
  q("m-pacific", "medium", "How large is the Pacific Ocean?", "square km", 1.65e8, ["About a third of the Earth's surface, which is 510 million km²."]),

  // --- hard: far from everyday, or many steps ---------------------------------------
  q("h-earth-mass", "hard", "What is the mass of the Earth?", "kg", 5.97e24, ["Radius 6,371 km; on average rock and metal are about 5.5 times as dense as water."]),
  q("h-sun-mass", "hard", "What is the mass of the Sun?", "kg", 1.99e30, ["About 333,000 Earths."]),
  q("h-universe-age", "hard", "How old is the universe?", "years", 1.38e10, ["About three times the age of the Earth, which is 4.5 billion years."]),
  q("h-proxima", "hard", "How far away is Proxima Centauri, the nearest star after the Sun?", "km", 4.0e13, [
    "4.2 light-years, and light covers about 9.5 trillion km a year.",
  ]),
  q("h-trees", "hard", "How many trees are there on Earth?", "trees", 3.0e12, ["About 400 for every person."], "Crowther et al., 2015"),
  q("h-ants", "hard", "How many ants are alive on Earth?", "ants", 2.0e16, ["About 2.5 million for every person."], "Schultheiss et al., 2022"),
  q("h-sun-power", "hard", "How much power does the Sun give off?", "watts", 3.83e26, [
    "150 million km away, the Earth receives about 1,360 watts on every square metre facing it.",
  ]),
  q("h-atmosphere", "hard", "What is the mass of the Earth's atmosphere?", "kg", 5.15e18, ["Air presses down with about 10 tonnes on every square metre."]),
  q("h-rbc", "hard", "How many red blood cells are in an adult?", "cells", 2.5e13, ["About 5 million in every microlitre of blood."]),
  q("h-emails", "hard", "How many emails are sent worldwide each day?", "emails", 3.5e11, ["About 4.3 billion people use email."], "Radicati, 2023"),
  q("h-searches", "hard", "How many Google searches are made each day?", "searches", 8.5e9, ["About one per person on Earth."], "2024"),
  q("h-youtube", "hard", "How many hours of video are uploaded to YouTube every minute?", "hours", 500, [
    "A day's uploads would take about 80 years to watch.",
  ], "YouTube, 2022"),
  q("h-electricity", "hard", "How much electricity does the world generate in a year?", "terawatt-hours", 29_900, ["About 3.7 MWh for each person."], "Ember, 2023"),
  q("h-cars", "hard", "How many cars are there in the world?", "cars", 1.5e9, ["About one for every five people."], "2023"),
  q("h-olympus", "hard", "How high does Olympus Mons, Mars's great volcano, rise above the planet's mean surface?", "metres", 21_900, [
    "About two and a half Everests.",
  ]),
  q("h-orbit-speed", "hard", "How fast does the Earth travel around the Sun?", "km/h", 107_000, ["One lap of about 940 million km a year."]),
  q("h-books", "hard", "How many different books have ever been published?", "books", 1.3e8, ["About one for every sixty people alive."], "Google Books, 2010"),
  q("h-moon-mass", "hard", "What is the mass of the Moon?", "kg", 7.35e22, ["About an eighty-first of the Earth's."]),
  q("h-hair", "hard", "How wide is a human hair?", "micrometres", 70, ["A millimetre is 1,000 micrometres."]),
  q("h-earths-in-sun", "hard", "How many Earths would fit inside the Sun, by volume?", "Earths", 1.3e6, ["The Sun is about 109 times as wide as the Earth."]),
  q("h-green", "hard", "What is the wavelength of green light?", "nanometres", 530, ["Visible light runs from about 400 to 700 nm."]),
];

// --- scoring --------------------------------------------------------------------------

/** Every submitted answer starts with this much. */
export const CREDIT = 50;
export const MAX_POINTS = 200;

/** How close the midpoint came, as log₁₀ of the ratio — the reference's bands. */
export const BASE_BANDS: { upTo: number; points: number; label: string }[] = [
  { upTo: 0.05, points: 100, label: "near perfect" },
  { upTo: 0.1, points: 80, label: "excellent" },
  { upTo: 0.2, points: 60, label: "very close" },
  { upTo: 0.3, points: 40, label: "good" },
  { upTo: 0.5, points: 20, label: "in the ballpark" },
  { upTo: 1, points: 10, label: "right order of magnitude" },
  { upTo: Infinity, points: 0, label: "too far off" },
];

/** How wide the range was, as log₁₀(high ÷ low): a hit's multiplier and what a miss keeps. */
export const SPREAD_BANDS: { upTo: number; mult: number; missKeeps: number }[] = [
  { upTo: 0.3, mult: 1.5, missKeeps: 0 },
  { upTo: 0.5, mult: 1.2, missKeeps: 20 },
  { upTo: 1, mult: 1.0, missKeeps: 35 },
  { upTo: Infinity, mult: 0.8, missKeeps: 40 },
];

export interface Scored {
  hit: boolean;
  /** The range's geometric midpoint. */
  mid: number;
  /** |log₁₀(mid ÷ answer)|. */
  distance: number;
  /** log₁₀(high ÷ low). */
  spread: number;
  base: (typeof BASE_BANDS)[number];
  band: (typeof SPREAD_BANDS)[number];
  points: number;
}

/** A range scored against the answer, by the reference's table. */
export function scoreRange(low: number, high: number, answer: number): Scored {
  const mid = Math.sqrt(low * high);
  const distance = Math.abs(Math.log10(mid / answer));
  const spread = Math.log10(high / low);
  const base = BASE_BANDS.find((b) => distance <= b.upTo)!;
  const band = SPREAD_BANDS.find((b) => spread <= b.upTo)!;
  const hit = low <= answer && answer <= high;
  const points = hit ? Math.min(MAX_POINTS, Math.round(CREDIT + base.points * band.mult)) : band.missKeeps;
  return { hit, mid, distance, spread, base, band, points };
}

/** Why a range is refused, or `null`. */
export function rangeProblem(low: number | null, high: number | null): string | null {
  if (low === null || high === null) return "Give both a low and a high.";
  if (!(low > 0) || !(high > 0)) return "Both ends must be above zero — the scale is logarithmic.";
  if (low > high) return "The low end is above the high end.";
  return null;
}

// --- reading and writing numbers ------------------------------------------------------

const WORDS: Record<string, number> = {
  k: 1e3,
  thousand: 1e3,
  m: 1e6,
  mn: 1e6,
  mm: 1e6,
  million: 1e6,
  b: 1e9,
  bn: 1e9,
  billion: 1e9,
  t: 1e12,
  tn: 1e12,
  trillion: 1e12,
  quadrillion: 1e15,
};

/**
 * A number as a person might type an estimate: `86400`, `86,400`, `8.1e9`,
 * `8.1 billion`, `8.1bn`, `3.5k`, `2x10^6`, `2×10^6`, `10^24`. `null` when it
 * is none of those.
 */
export function parseAmount(raw: string): number | null {
  const s = raw.trim().toLowerCase().replace(/[,_\s]/g, "").replace(/×/g, "x");
  if (s === "") return null;
  const pow = /^(?:(\d*\.?\d+)(?:x|\*))?10\^(-?\d+)$/.exec(s);
  if (pow) return (pow[1] ? Number(pow[1]) : 1) * 10 ** Number(pow[2]);
  const m = /^(\d*\.?\d+(?:e[+-]?\d+)?)([a-z]*)$/.exec(s);
  if (!m) return null;
  const n = Number(m[1]);
  if (!Number.isFinite(n)) return null;
  if (m[2] === "") return n;
  const scale = WORDS[m[2]];
  return scale ? n * scale : null;
}

const SUPER: Record<string, string> = { "-": "⁻", 0: "⁰", 1: "¹", 2: "²", 3: "³", 4: "⁴", 5: "⁵", 6: "⁶", 7: "⁷", 8: "⁸", 9: "⁹" };

/** Three significant figures, trailing zeros dropped. */
const sig = (n: number) => String(Number(n.toPrecision(3)));

/** An amount as a person would say it: `86,400`, `8.1 billion`, `5.97 × 10²⁴`. */
export function formatAmount(n: number): string {
  if (!Number.isFinite(n)) return "—";
  const a = Math.abs(n);
  if (a >= 1e15 || (a > 0 && a < 0.01)) {
    const exp = Math.floor(Math.log10(a));
    const mant = n / 10 ** exp;
    return `${sig(mant)} × 10${String(exp)
      .split("")
      .map((c) => SUPER[c])
      .join("")}`;
  }
  if (a >= 1e12) return `${sig(n / 1e12)} trillion`;
  if (a >= 1e9) return `${sig(n / 1e9)} billion`;
  if (a >= 1e6) return `${sig(n / 1e6)} million`;
  return Number(n.toPrecision(Math.max(3, Math.floor(Math.log10(Math.max(a, 1))) + 1))).toLocaleString("en-US");
}

/** How far off, as a factor: `×1.3`. */
export const factorText = (distance: number) => `×${(10 ** distance).toFixed(distance < 1 ? 1 : 0)}`;

// --- settings and a game's questions ----------------------------------------------------

export interface FermiSettings {
  pool: Pool;
  /** 5–15. */
  questions: number;
  /** Seconds per question; `null` for no clock. */
  seconds: number | null;
  hints: boolean;
}

/** The reference's presets, to the number. */
export const PRESETS: Record<Level, Omit<FermiSettings, "pool">> = {
  easy: { questions: 8, seconds: 60, hints: true },
  medium: { questions: 10, seconds: 30, hints: false },
  hard: { questions: 12, seconds: 20, hints: false },
};

export const DEFAULT_FERMI: FermiSettings = { pool: "easy", ...PRESETS.easy };
export const QUESTIONS = [5, 15] as const;

/** A game's questions: drawn from the pool without repeats, as many as it has. */
export function drawQuestions(settings: Pick<FermiSettings, "pool" | "questions">, rand: Rand): Question[] {
  const pool = BANK.filter((x) => settings.pool === "mixed" || x.level === settings.pool);
  const deck = [...pool];
  for (let i = deck.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [deck[i], deck[j]] = [deck[j], deck[i]];
  }
  return deck.slice(0, Math.min(settings.questions, deck.length));
}
