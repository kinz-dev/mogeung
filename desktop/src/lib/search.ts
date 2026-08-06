/**
 * Matching, for the searches this client runs itself.
 *
 * A port of `crates/mogeung-ui/src/search.rs`. Several engines run together and
 * the best score wins — and the winner is **named** in the result, because a
 * search that silently prefers one strategy is worse than one that says why it
 * matched.
 */

export type Engine = "exact" | "words" | "fuzzy";

export interface Hit {
  score: number;
  engine: Engine;
}

/** Case-sensitive only when the query has an uppercase letter — ripgrep's
 * smart case, because that is what the hands expect. */
function smartCase(query: string, hay: string): [string, string] {
  return /[A-Z]/.test(query) ? [query, hay] : [query.toLowerCase(), hay.toLowerCase()];
}

function exactScore(q: string, h: string): number | null {
  const at = h.indexOf(q);
  if (at < 0) return null;
  // Earlier is better, and a hit at a word boundary better still.
  const boundary = at === 0 || /[\s/_.\-([{]/.test(h[at - 1]) ? 30 : 0;
  return 200 + boundary - Math.min(at, 100) / 2;
}

function wordsScore(q: string, h: string): number | null {
  const terms = q.split(/\s+/).filter(Boolean);
  if (terms.length < 2) return null;
  let total = 0;
  for (const t of terms) {
    const at = h.indexOf(t);
    if (at < 0) return null;
    total += 40 - Math.min(at, 60) / 4;
  }
  return total;
}

/** Subsequence, the palette's own scorer: every character in order. */
export function fuzzyScore(q: string, h: string): number | null {
  if (!q) return 0;
  let hi = 0;
  let score = 0;
  let streak = 0;
  for (const ch of q) {
    const at = h.indexOf(ch, hi);
    if (at < 0) return null;
    streak = at === hi ? streak + 1 : 0;
    score += 10 + streak * 4 - Math.min(at - hi, 20);
    hi = at + 1;
  }
  return score;
}

/** The best of the engines, or `null` when nothing matched. */
export function best(query: string, hay: string): Hit | null {
  const trimmed = query.trim();
  if (!trimmed) return null;
  const [q, h] = smartCase(trimmed, hay);
  const candidates: Hit[] = [];
  const e = exactScore(q, h);
  if (e !== null) candidates.push({ score: e, engine: "exact" });
  const w = wordsScore(q, h);
  if (w !== null) candidates.push({ score: w, engine: "words" });
  const f = fuzzyScore(q.replace(/\s+/g, ""), h);
  if (f !== null) candidates.push({ score: f, engine: "fuzzy" });
  if (candidates.length === 0) return null;
  return candidates.reduce((a, b) => (b.score > a.score ? b : a));
}

/** An item and why it matched, or `null` when nothing was being searched. */
export interface Ranked<T> {
  item: T;
  hit: Hit | null;
}

/**
 * Filter and order a list by one query, several engines at once. `R-F10`.
 *
 * A blank query returns **everything, in its original order** rather than
 * nothing — these lists are worth reading unsearched, and the daemon has
 * already ordered them by something meaningful (a count, a recency). Ranking an
 * unqueried list by a score of zero would silently throw that order away.
 *
 * `text` rather than a field name so one function serves lists of different
 * shapes: a prompt cluster's representative, a failure's example line, a doc's
 * path. Concatenating a couple of fields in the accessor is the intended way to
 * search more than one of them.
 */
export function rank<T>(items: readonly T[], query: string, text: (t: T) => string): Ranked<T>[] {
  if (!query.trim()) return items.map((item) => ({ item, hit: null }));
  const out: Ranked<T>[] = [];
  for (const item of items) {
    const hit = best(query, text(item));
    if (hit) out.push({ item, hit });
  }
  // Stable within a score: `sort` has been stable since ES2019, so items the
  // engines cannot separate keep the order the daemon gave them.
  return out.sort((a, b) => (b.hit?.score ?? 0) - (a.hit?.score ?? 0));
}

/**
 * Which engine actually decided the order, or `null` when nothing was searched.
 *
 * The whole rule `search.ts` was built on: a box that silently prefers one
 * strategy is worse than one that says which it used.
 */
export function winner<T>(ranked: readonly Ranked<T>[]): Engine | null {
  return ranked[0]?.hit?.engine ?? null;
}

/** Path-aware ranking for go-to-file. The basename counts for more. */
export function scorePath(query: string, path: string): number | null {
  const q = query.trim().toLowerCase();
  if (!q) return 0;
  const p = path.toLowerCase();
  const baseAt = p.lastIndexOf("/") + 1;
  const inBase = fuzzyScore(q, p.slice(baseAt));
  const inWhole = fuzzyScore(q, p);
  if (inBase === null && inWhole === null) return null;
  return Math.max((inBase ?? 0) + 60, inWhole ?? 0);
}

/**
 * Where an arrow lands in the search results. `R-F13`.
 *
 * Returns the new `[inList, index]`. `inList === false` means the arrows belong
 * to the query box again, which is the state that lets Enter go back to meaning
 * "run this search".
 *
 * Deliberately **not** a wrapping cursor. Wrapping is right for a modal
 * palette, where the list is the only thing there; here the list sits under a
 * box you are still editing, and a cursor that jumped from the last hit to the
 * first would leave no keyboard way back to the query.
 */
export function searchMove(
  inList: boolean,
  at: number | null,
  len: number,
  down: boolean,
): [boolean, number | null] {
  if (len === 0) return [false, null];
  if (down) {
    if (inList && at !== null) return [true, Math.min(at + 1, len - 1)];
    // Down from the box enters at the top, wherever the mouse last left the
    // selection — arriving mid-list because of an earlier click would read as
    // the cursor jumping.
    return [true, 0];
  }
  if (!inList) return [false, at];
  if (at === null || at === 0) return [false, at];
  return [true, at - 1];
}
