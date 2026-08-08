/**
 * The cost chart's data, as a pure function over the report. `R-J21`.
 *
 * In `lib/` rather than inside the pane for the reason `visibleQueue` is: a
 * chart series is arithmetic, and arithmetic that lives in a component can
 * only be tested by mounting one. The shaping here is not trivial — a stacked
 * chart needs one key per model on **every** row, including the days that
 * model did not run, or recharts draws a gap where a zero belongs.
 */

import type { ModelBurn, UsageReport } from "@/wire/types";

/** Colours run out before legibility does. See `GRAPH_COLORS`. */
const MAX_BANDS = 6;
export const OTHER = "other";

export interface CostSeries {
  /** One row per day, `{ day, [model]: dollars }`, oldest first. */
  rows: Record<string, string | number>[];
  /** The stack's keys, dearest first — the order the bands are drawn in. */
  models: string[];
}

/** The label a model is charted under. Fast mode is a different price, so a
 * different band. */
export function bandOf(m: Pick<ModelBurn, "model" | "fast">): string {
  return `${m.model.replace(/^claude-/, "")}${m.fast ? " (fast)" : ""}`;
}

/**
 * Days → stacked-bar rows.
 *
 * **Ranked over the whole period, not per day**, so a band keeps its colour
 * as you read across the chart. Ranking per day would repaint the stack every
 * column, which reads as noise rather than as a series.
 *
 * Models past `MAX_BANDS` fold into one `other` band rather than cycling the
 * palette: two bands the same colour is a chart that lies about which is which.
 * An **unpriced** model contributes no dollars at all — it is absent here and
 * named in `unpriced_models`, because drawing it at zero would say it was free.
 */
export function costSeries(usage: UsageReport | null | undefined): CostSeries {
  if (!usage || usage.days.length === 0) return { rows: [], models: [] };

  const total = new Map<string, number>();
  for (const day of usage.days) {
    for (const m of day.by_model) {
      if (m.cost_usd === null) continue;
      total.set(bandOf(m), (total.get(bandOf(m)) ?? 0) + m.cost_usd);
    }
  }
  if (total.size === 0) return { rows: [], models: [] };

  const ranked = [...total.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  const named = ranked.slice(0, MAX_BANDS).map(([name]) => name);
  const folded = ranked.length > MAX_BANDS;
  const models = folded ? [...named, OTHER] : named;

  const rows = usage.days.map((day) => {
    // Every key on every row, zeroes included — a missing key is a hole in
    // the stack rather than a day that cost nothing.
    const row: Record<string, string | number> = { day: day.day.slice(5) };
    for (const m of models) row[m] = 0;
    for (const m of day.by_model) {
      if (m.cost_usd === null) continue;
      const key = named.includes(bandOf(m)) ? bandOf(m) : OTHER;
      if (key === OTHER && !folded) continue;
      row[key] = (row[key] as number) + m.cost_usd;
    }
    return row;
  });

  return { rows, models };
}
