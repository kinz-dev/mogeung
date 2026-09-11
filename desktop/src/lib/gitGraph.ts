/**
 * Lanes for the log's graph column. `R-D26`, ported from the archived egui
 * pane (`git show a16699d:crates/mogeung-ui/src/gitview.rs`, `lanes`).
 *
 * Every log row already carries its parents (`%p`), so the topology is
 * computed here from what the daemon sent and costs no extra call. The
 * algorithm is the one the archive settled on after drawing lying graphs:
 * lanes are found by **expectation** — each lane waits for one sha, and a
 * commit takes the lane that was waiting for it — joins collapse duplicate
 * expectations, and a freed lane is reused so disjoint histories do not
 * leak columns rightward. Straight lines only; curves are polish that waits.
 *
 * Computed over every loaded row, never per page: a page boundary that falls
 * inside a merge is invisible here because the expectations carry across it.
 */

import type { CommitInfo } from "@/wire/types";

export interface GraphRow {
  /** The lane the dot sits in. */
  lane: number;
  /** Lanes that were waiting for this commit and fold into it from above. */
  joins: number[];
  /** The lane each parent continues in, first parent first. */
  parents: number[];
  /** Lanes carrying a line into the top of the row. */
  above: boolean[];
  /** Lanes carrying a line out of the bottom of the row. */
  below: boolean[];
  /** Two or more parents. */
  merge: boolean;
}

/** More lanes than this are drawn in the last one — a graph nobody can read
 *  is worse than a narrow one, and eight is past what the archive ever saw. */
export const MAX_LANES = 8;

/** Whether `expected` (an abbreviated parent sha) names `c`. `%p` and `%h`
 *  abbreviate to the same length, and a full sha starts with its own
 *  abbreviation, so both spellings are covered. */
function names(expected: string, c: CommitInfo): boolean {
  return expected === c.short || c.sha.startsWith(expected) || expected.startsWith(c.short);
}

export function lanes(commits: readonly CommitInfo[]): { rows: GraphRow[]; width: number } {
  const active: (string | null)[] = [];
  const rows: GraphRow[] = [];
  let widest = 1;
  for (const c of commits) {
    const above = active.map((s) => s !== null);
    let lane = active.findIndex((s) => s !== null && names(s, c));
    if (lane < 0) {
      lane = active.indexOf(null);
      if (lane < 0) {
        lane = active.length;
        active.push(null);
      }
    }
    const joins: number[] = [];
    for (let i = 0; i < active.length; i++) {
      const s = active[i];
      if (i !== lane && s !== null && names(s, c)) {
        joins.push(i);
        active[i] = null;
      }
    }
    const parents: number[] = [];
    if (c.parents.length === 0) active[lane] = null;
    c.parents.forEach((p, k) => {
      if (k === 0) {
        active[lane] = p;
        parents.push(lane);
        return;
      }
      let l = active.findIndex((s) => s !== null && (s === p || s.startsWith(p) || p.startsWith(s)));
      if (l < 0) {
        l = active.indexOf(null);
        if (l < 0) {
          l = active.length;
          active.push(null);
        }
        active[l] = p;
      }
      parents.push(l);
    });
    while (active.length > 0 && active[active.length - 1] === null) active.pop();
    const below = active.map((s) => s !== null);
    widest = Math.max(widest, above.length, below.length, lane + 1);
    rows.push({ lane, joins, parents, above, below, merge: c.parents.length > 1 });
  }
  return { rows, width: Math.min(widest, MAX_LANES) };
}

/** Geometry the column is drawn with — shared by the cell and the header. */
export const LANE_PX = 12;
export const ROW_PX = 22;

/** The SVG for one row, as markup. A string rather than elements because the
 *  log is virtualised and a row is cheapest as one node with one child. */
export function graphCell(r: GraphRow, width: number): string {
  const lw = LANE_PX;
  const rh = ROW_PX;
  const cx = (i: number) => Math.min(i, MAX_LANES - 1) * lw + lw / 2;
  const col = (i: number) => `var(--graph-${i % 6})`;
  const line = (x1: number, y1: number, x2: number, y2: number, c: string) =>
    `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${c}" stroke-width="1.5"/>`;
  let s = "";
  r.above.forEach((on, i) => {
    if (on && !r.joins.includes(i)) s += line(cx(i), 0, cx(i), rh / 2, col(i));
  });
  for (const j of r.joins) s += line(cx(j), 0, cx(r.lane), rh / 2, col(j));
  r.below.forEach((on, i) => {
    if (!on) return;
    if (r.parents.includes(i) && i !== r.lane) s += line(cx(r.lane), rh / 2, cx(i), rh, col(i));
    else s += line(cx(i), rh / 2, cx(i), rh, col(i));
  });
  const dot = r.merge
    ? `<circle cx="${cx(r.lane)}" cy="${rh / 2}" r="3.5" fill="var(--bg-panel)" stroke="${col(r.lane)}" stroke-width="1.6"/>`
    : `<circle cx="${cx(r.lane)}" cy="${rh / 2}" r="3.5" fill="${col(r.lane)}"/>`;
  const w = width * lw;
  return `<svg width="${w}" height="${rh}" viewBox="0 0 ${w} ${rh}" aria-hidden="true">${s}${dot}</svg>`;
}
