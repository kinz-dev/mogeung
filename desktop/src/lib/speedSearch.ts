/**
 * Type-to-search in the Files tree. `R-J122`.
 *
 * Asked 2026-10-08: *"when the files is in focus, I can just type a file name
 * and it can fuzzy search and show the matching files in the tree. That is the
 * most useful feature in files panel."* IntelliJ's speed search, and the
 * reason it is in the **tree** rather than the filter box above it: the box
 * answers with a flat list and wants a click first, where this is a key away
 * from wherever you were looking and keeps the shape you navigate by.
 *
 * **Over the whole worktree, like the filter.** Matching only the rows that
 * happen to be expanded is the failure the filter was already rebuilt to avoid
 * — a file two collapsed folders down could not be found however exactly you
 * typed it. So the walk (`list_tree`) is the haystack, and the tree drawn is
 * the matching files with every folder above them open.
 *
 * **The name, unless you type a slash.** A subsequence over the whole path
 * matches nearly everything in a monorepo by its third letter — `fil` is in
 * `src/ui/tools/` before it is in any name. A `/` in the query is how you say
 * the directory is part of what you mean, and then the whole path is matched.
 *
 * Scored by `best` from `search.ts` — the palette's engines — so this and
 * go-to-file agree about what a good match is.
 */

import { best } from "@/lib/search";
import { basename } from "@/lib/explorer";

export interface SpeedRow {
  /** The real path: what `openFile` and `reveal` take. */
  path: string;
  /** What the row shows — a segment, or an added folder's own name. */
  name: string;
  isDir: boolean;
  depth: number;
  /** Offsets into `name` of the characters the query matched, ascending. */
  marks: number[];
}

export interface SpeedResult {
  rows: SpeedRow[];
  /** The matching files, in the order the rows draw them — what the arrows walk. */
  files: string[];
  /** The best-scoring file, which is where the cursor lands as you type. */
  best: string | null;
  /** How many files matched before `cap` cut the list. */
  total: number;
}

/** Case-sensitive only when the query has an uppercase letter — the rule every box here uses. */
function fold(query: string, hay: string): [string, string] {
  return /[A-Z]/.test(query) ? [query, hay] : [query.toLowerCase(), hay.toLowerCase()];
}

/**
 * Which characters of `hay` the query matched, or `null` when it did not.
 *
 * A contiguous run first, because that is what you typed when you typed a
 * name, and the greedy subsequence would light up its letters scattered across
 * the front of the word instead. The subsequence is `fuzzyScore`'s walk, so a
 * file that scored is a file that can be marked. Spaces in the query are not
 * characters to find.
 */
export function fuzzyMarks(query: string, hay: string): number[] | null {
  const [q, h] = fold(query.trim(), hay);
  if (!q) return [];
  const at = h.indexOf(q);
  if (at >= 0) return Array.from({ length: q.length }, (_, i) => at + i);
  const out: number[] = [];
  let hi = 0;
  for (const ch of q.replace(/\s+/g, "")) {
    const found = h.indexOf(ch, hi);
    if (found < 0) return null;
    out.push(found);
    hi = found + 1;
  }
  return out;
}

/**
 * A path as the tree draws it: one `(path, name)` per row from the top.
 *
 * An absolute path is a file in a folder added to the workspace (`R-J40`), and
 * the tree draws that folder as **one** top-level row named for itself — so
 * its segments start at the folder, not at `/`. A path under no known root
 * keeps its directory as one row rather than inventing a row per component of
 * somebody's home directory.
 */
function segments(path: string, roots: readonly string[]): { paths: string[]; names: string[] } {
  if (path.startsWith("/")) {
    const root = roots
      .filter((r) => path.startsWith(`${r.replace(/\/+$/, "")}/`))
      .sort((a, b) => b.length - a.length)[0];
    if (root) {
      const base = root.replace(/\/+$/, "");
      const rest = path.slice(base.length + 1).split("/");
      const paths = [base];
      for (const part of rest) paths.push(`${paths[paths.length - 1]}/${part}`);
      return { paths, names: [basename(base), ...rest] };
    }
    const dir = path.slice(0, path.lastIndexOf("/")) || "/";
    return { paths: [dir, path], names: [dir, basename(path)] };
  }
  const names = path.split("/");
  const paths: string[] = [];
  for (const part of names) paths.push(paths.length ? `${paths[paths.length - 1]}/${part}` : part);
  return { paths, names };
}

interface Node {
  path: string;
  name: string;
  isDir: boolean;
  marks: Set<number>;
  children: Map<string, Node>;
}

/** The order `list_dir` answers in: folders first, then by name. */
function byListing(a: Node, b: Node): number {
  if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
  return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}

/**
 * The tree of files matching `query`, with every folder above them open.
 *
 * `roots` are the workspace's added folders, in the order the tree draws them.
 * They come **after** the session's own files, which is where the unfiltered
 * tree puts them, so a hit does not jump about as you add a letter.
 *
 * Past `cap` hits the **best-scoring** are kept, not the first: the first 300
 * of a monorepo in name order are all under `a`.
 */
export function speedSearch(
  paths: readonly string[],
  query: string,
  roots: readonly string[] = [],
  cap = 300,
): SpeedResult {
  const q = query.trim();
  if (!q) return { rows: [], files: [], best: null, total: 0 };
  const whole = q.includes("/");

  type Hit = { path: string; segs: ReturnType<typeof segments>; marks: number[]; score: number };
  const hits: Hit[] = [];
  for (const path of paths) {
    const segs = segments(path, roots);
    const shown = segs.names.join("/");
    const name = segs.names[segs.names.length - 1];
    const hay = whole ? shown : name;
    const hit = best(q, hay);
    if (!hit) continue;
    const marks = fuzzyMarks(q, hay);
    if (!marks) continue;
    const offset = whole ? 0 : shown.length - name.length;
    hits.push({ path, segs, marks: marks.map((m) => m + offset), score: hit.score });
  }

  const total = hits.length;
  const kept = total > cap ? [...hits].sort((a, b) => b.score - a.score).slice(0, cap) : hits;

  const top = new Map<string, Node>();
  const added = new Map<string, Node>();
  let bestHit: Hit | null = null;
  for (const hit of kept) {
    if (!bestHit || hit.score > bestHit.score) bestHit = hit;
    const { paths: at, names } = hit.segs;
    let level = hit.path.startsWith("/") ? added : top;
    let start = 0;
    names.forEach((name, i) => {
      const leaf = i === names.length - 1;
      let node = level.get(at[i]);
      if (!node) {
        node = { path: at[i], name, isDir: !leaf, marks: new Set(), children: new Map() };
        level.set(at[i], node);
      }
      for (const m of hit.marks) if (m >= start && m < start + name.length) node.marks.add(m - start);
      start += name.length + 1;
      level = node.children;
    });
  }

  const rows: SpeedRow[] = [];
  const files: string[] = [];
  const walk = (nodes: Iterable<Node>, depth: number, sort: boolean) => {
    const list = [...nodes];
    if (sort) list.sort(byListing);
    for (const n of list) {
      rows.push({ path: n.path, name: n.name, isDir: n.isDir, depth, marks: [...n.marks].sort((a, b) => a - b) });
      if (!n.isDir) files.push(n.path);
      walk(n.children.values(), depth + 1, true);
    }
  };
  walk(top.values(), 0, true);
  // Added folders in the workspace's own order, as the unfiltered tree has them.
  const rank = (p: string) => {
    const i = roots.findIndex((r) => r.replace(/\/+$/, "") === p);
    return i < 0 ? roots.length : i;
  };
  walk([...added.values()].sort((a, b) => rank(a.path) - rank(b.path)), 0, false);

  return { rows, files, best: bestHit?.path ?? null, total };
}
