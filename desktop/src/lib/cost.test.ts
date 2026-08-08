/**
 * The cost series. `R-J21`.
 *
 * The arithmetic is small and the ways it goes quietly wrong are not: a
 * missing key draws a hole in a stacked bar, a per-day ranking repaints the
 * colours as you read across, and an unpriced model drawn at zero says a model
 * was free when we simply do not know what it cost.
 */

import { describe, expect, it } from "vitest";
import { OTHER, bandOf, costSeries } from "./cost";
import type { DayBurn, ModelBurn, UsageReport } from "@/wire/types";

const model = (name: string, cost: number | null, fast = false): ModelBurn => ({
  model: name,
  fast,
  tokens: { fresh_in: 0, cache_read: 0, cache_write_5m: 0, cache_write_1h: 0, out: 1000 },
  cost_usd: cost,
  messages: 1,
});

const day = (d: string, models: ModelBurn[]): DayBurn => ({
  day: d,
  tokens_in: 0,
  tokens_out: 0,
  sessions: 1,
  by_model: models,
  cost_usd: models.reduce((n, m) => n + (m.cost_usd ?? 0), 0),
});

const report = (days: DayBurn[]): UsageReport =>
  ({
    days,
    repos: [],
    sessions: [],
    window_tokens_in: 0,
    window_tokens_out: 0,
    limit_hits: [],
    est_window_limit_out: null,
    models: [],
    cost_usd_total: 0,
    cost_usd_today: 0,
    unpriced_models: [],
    rates_as_of: "2026-08-08",
    generated_at: null,
    files_scanned: 0,
    files_skipped: 0,
  }) as UsageReport;

describe("costSeries", () => {
  it("gives every row every key, so a quiet day is a zero and not a hole", () => {
    const s = costSeries(
      report([
        day("2026-08-01", [model("claude-opus-5", 3)]),
        day("2026-08-02", [model("claude-haiku-4-5", 1)]),
      ]),
    );
    expect(s.models).toEqual(["opus-5", "haiku-4-5"]);
    for (const row of s.rows) {
      for (const m of s.models) expect(row[m]).toBeTypeOf("number");
    }
    expect(s.rows[0]["haiku-4-5"]).toBe(0);
    expect(s.rows[1]["opus-5"]).toBe(0);
  });

  /**
   * Ranked over the whole period, not per day — otherwise a band changes
   * colour column to column and the chart reads as noise.
   */
  it("ranks bands once, over the whole period", () => {
    const s = costSeries(
      report([
        // Haiku wins day one…
        day("2026-08-01", [model("claude-haiku-4-5", 5), model("claude-opus-5", 1)]),
        // …and Opus wins the period.
        day("2026-08-02", [model("claude-opus-5", 90)]),
      ]),
    );
    expect(s.models[0]).toBe("opus-5");
  });

  /** Two bands the same colour is a chart that lies, so the tail folds. */
  it("folds past six models into one band", () => {
    const many = Array.from({ length: 9 }, (_, i) => model(`claude-m${i}`, 9 - i));
    const s = costSeries(report([day("2026-08-01", many)]));
    expect(s.models).toHaveLength(7);
    expect(s.models.at(-1)).toBe(OTHER);
    // The tail's dollars survive the fold rather than being dropped.
    expect(s.rows[0][OTHER]).toBe(3 + 2 + 1);
  });

  /**
   * **Unpriced is not free.** A model with no rate contributes nothing to the
   * chart and is named elsewhere; charting it at zero would claim we know it
   * cost nothing.
   */
  it("leaves an unpriced model out rather than drawing it at zero", () => {
    const s = costSeries(
      report([day("2026-08-01", [model("claude-opus-5", 2), model("claude-nonesuch-9", null)])]),
    );
    expect(s.models).toEqual(["opus-5"]);
    expect(s.rows[0]).not.toHaveProperty("nonesuch-9");
  });

  /** Fast mode is the same model at another price, so it is its own band. */
  it("charts fast mode apart from standard", () => {
    const s = costSeries(
      report([day("2026-08-01", [model("claude-opus-5", 2), model("claude-opus-5", 4, true)])]),
    );
    expect(s.models).toEqual(["opus-5 (fast)", "opus-5"]);
    expect(bandOf({ model: "claude-opus-5", fast: true })).toBe("opus-5 (fast)");
  });

  it("says nothing when there is nothing priced to say", () => {
    expect(costSeries(null)).toEqual({ rows: [], models: [] });
    expect(costSeries(report([]))).toEqual({ rows: [], models: [] });
    expect(costSeries(report([day("2026-08-01", [model("claude-x", null)])]))).toEqual({
      rows: [],
      models: [],
    });
  });
});
