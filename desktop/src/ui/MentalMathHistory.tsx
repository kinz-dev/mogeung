/**
 * The mental-math game's history: how you are doing over many games.
 * [`lib/mentalMathStats.ts`](../lib/mentalMathStats.ts) says what is kept and
 * how it is judged; this draws it.
 *
 * **Trends are read per setting, or not at all.** Answers per minute on Easy
 * and on Expert are different quantities, and a single line through both
 * would mostly measure which one you happened to play. So the screen opens
 * on the difficulty of your latest game, and *all* is there but labelled for
 * what it is.
 *
 * The charts are hand-drawn SVG. The window carries no charting library, and
 * two line charts with a fitted line are not a reason to add one.
 */

import { useMemo, useState } from "react";
import { cn } from "@/lib/cn";
import { DIFFICULTIES, LEVELS, type Difficulty } from "@/lib/mentalMath";
import {
  byOperation,
  filterRecords,
  fitLine,
  hardest,
  metrics,
  overview,
  settingLabel,
  type Filter,
  type GameRecord,
  type Mode,
} from "@/lib/mentalMathStats";
import { BigButton, COLOR, Choice, Key, Label, OP_NAME, clock, fs, secs } from "@/ui/arcadeKit";
import { historyPlace } from "@/lib/arcade";

const W = 1000;
const H = 220;
const PAD = { left: 56, right: 16, top: 14, bottom: 26 };

/**
 * One value per game, in order, with its least-squares line dashed through it.
 * Each point carries a `<title>`, so hovering says which game it was.
 */
export function TrendChart({
  values,
  labels,
  color,
  format,
  label,
}: {
  values: number[];
  labels: string[];
  color: string;
  format: (n: number) => string;
  label: string;
}) {
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  const span = hi - lo || Math.max(1, Math.abs(hi) * 0.2);
  const min = lo - span * 0.12;
  const max = hi + span * 0.12;
  const n = values.length;
  const x = (i: number) => PAD.left + (n === 1 ? (W - PAD.left - PAD.right) / 2 : (i / (n - 1)) * (W - PAD.left - PAD.right));
  const y = (v: number) => PAD.top + (1 - (v - min) / (max - min)) * (H - PAD.top - PAD.bottom);
  const fit = fitLine(values);
  const grid = [min + (max - min) * 0.15, (min + max) / 2, max - (max - min) * 0.15];
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full" role="img" aria-label={label}>
      {grid.map((g) => (
        <g key={g}>
          <line x1={PAD.left} x2={W - PAD.right} y1={y(g)} y2={y(g)} stroke="var(--border)" strokeWidth={1} />
          <text x={PAD.left - 8} y={y(g) + 5} textAnchor="end" fontSize={15} fill="var(--dim)">
            {format(g)}
          </text>
        </g>
      ))}
      {n > 1 && (
        <polyline
          points={values.map((v, i) => `${x(i)},${y(v)}`).join(" ")}
          fill="none"
          stroke={color}
          strokeOpacity={0.45}
          strokeWidth={2}
        />
      )}
      {fit && (
        <line
          data-trend
          x1={x(0)}
          y1={y(fit.intercept)}
          x2={x(n - 1)}
          y2={y(fit.intercept + fit.slope * (n - 1))}
          stroke={color}
          strokeWidth={3}
          strokeDasharray="10 8"
        />
      )}
      {values.map((v, i) => (
        <circle key={i} cx={x(i)} cy={y(v)} r={n > 60 ? 3 : 5} fill={color}>
          <title>{`${labels[i]} — ${format(v)}`}</title>
        </circle>
      ))}
      <text x={PAD.left} y={H - 6} fontSize={14} fill="var(--dim)">
        first
      </text>
      <text x={W - PAD.right} y={H - 6} fontSize={14} fill="var(--dim)" textAnchor="end">
        latest
      </text>
    </svg>
  );
}

/** A line of small multiples: one operation's seconds per answer, game by game. */
function Spark({ series, color }: { series: (number | null)[]; color: string }) {
  const pts = series.map((v, i) => [i, v] as const).filter((p): p is readonly [number, number] => p[1] !== null);
  if (pts.length < 2) return <span className="text-[var(--dim)]">—</span>;
  const vs = pts.map((p) => p[1]);
  const lo = Math.min(...vs);
  const hi = Math.max(...vs);
  const w = 160;
  const h = 34;
  const x = (i: number) => 2 + (i / Math.max(1, series.length - 1)) * (w - 4);
  const y = (v: number) => 2 + (1 - (v - lo) / (hi - lo || 1)) * (h - 4);
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="inline-block h-[1.6em] w-[7em] align-middle" aria-hidden>
      <polyline points={pts.map(([i, v]) => `${x(i)},${y(v)}`).join(" ")} fill="none" stroke={color} strokeWidth={2} />
    </svg>
  );
}

/** The trend in words, per ten games, because a slope of 0.03 per game means nothing to anyone. */
function trendWords(values: number[], unit: string, higherIsBetter = true): string {
  if (values.length < 3) return `${3 - values.length} more game${values.length === 2 ? "" : "s"} for a trend`;
  const fit = fitLine(values)!;
  const per10 = fit.slope * 10;
  const mean = values.reduce((a, b) => a + b, 0) / values.length;
  if (Math.abs(per10) < Math.max(0.05, Math.abs(mean) * 0.02)) return "holding steady";
  const better = per10 > 0 === higherIsBetter;
  return `${better ? "improving" : "slipping"}: ${per10 > 0 ? "+" : ""}${per10.toFixed(1)}${unit} every 10 games`;
}

function Tile({ n, label }: { n: string | number; label: string }) {
  return (
    <div className="rounded-lg border border-[var(--border)] bg-[var(--bg-panel)] px-[1em] py-[0.6em]">
      <div className="font-mono font-semibold text-[var(--text-strong)] tabular-nums" style={fs(18, 3.6, 34)}>
        {n}
      </div>
      <div className="text-[var(--dim)]" style={fs(12, 1.7, 15)}>
        {label}
      </div>
    </div>
  );
}

const when = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

export function History({
  records,
  onBack,
  initial,
  failed,
}: {
  records: GameRecord[];
  onBack: () => void;
  /** The difficulty to open on — usually your latest game's. */
  initial: Difficulty | "all";
  /** The last game could not be saved. Said, not hidden. */
  failed: boolean;
}) {
  const [filter, setFilter] = useState<Filter>({ difficulty: initial, mode: "all" });
  const shown = useMemo(() => filterRecords(records, filter), [records, filter]);
  const scored = useMemo(() => shown.filter((r) => !r.early), [shown]);
  const ov = useMemo(() => overview(shown), [shown]);
  const ops = useMemo(() => byOperation(shown), [shown]);
  const hard = useMemo(() => hardest(shown), [shown]);
  const rates = scored.map((r) => metrics(r).perMinute);
  const accs = scored.map((r) => metrics(r).accuracy);
  const labels = scored.map((r) => `${when(r.at)} · ${settingLabel(r)}`);
  const color = filter.difficulty === "all" ? "var(--blue)" : COLOR[filter.difficulty];
  const small = fs(12, 1.7, 15);

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
        <div>
          <Label>difficulty</Label>
          <div className="flex flex-wrap gap-2">
            {(["all", ...DIFFICULTIES] as const).map((d) => (
              <Choice key={d} on={filter.difficulty === d} onClick={() => setFilter({ ...filter, difficulty: d })}>
                {d === "all" ? "all" : LEVELS[d].label}
              </Choice>
            ))}
          </div>
        </div>
        <div>
          <Label>mode</Label>
          <div className="flex flex-wrap gap-2">
            {(["all", "timed", "endless"] as const).map((m) => (
              <Choice key={m} on={filter.mode === m} onClick={() => setFilter({ ...filter, mode: m as Mode | "all" })}>
                {m}
              </Choice>
            ))}
          </div>
        </div>
        <BigButton className="ml-auto" onClick={onBack}>
          back <Key>Esc</Key>
        </BigButton>
      </div>

      {failed && (
        <div className="text-[var(--amber)]" style={small}>
          The last game could not be written to {historyPlace("mental-math")} — it is shown here, and will be gone when the window
          closes.
        </div>
      )}

      {shown.length === 0 ? (
        <div className="py-[8vh] text-center text-[var(--dim)]">
          {records.length === 0
            ? "No games recorded yet — finish one and it lands here."
            : "No games at this setting yet."}
        </div>
      ) : (
        <>
          <div className="grid grid-cols-3 gap-2 lg:grid-cols-6">
            <Tile n={ov.games} label={ov.games === 1 ? "game" : "games"} />
            <Tile n={clock(ov.ms)} label="time trained" />
            <Tile n={ov.correct} label="answered right" />
            <Tile n={`${ov.accuracy}%`} label="accuracy" />
            <Tile n={ov.bestPerMinute ?? "—"} label="best per minute" />
            <Tile n={ov.dayStreak} label={ov.dayStreak === 1 ? "day in a row" : "days in a row"} />
          </div>
          {filter.difficulty === "all" && (
            <div className="text-[var(--dim)]" style={small}>
              All difficulties together: the line below measures which level you played as much as how you played it.
            </div>
          )}

          {scored.length > 0 && (
            <div className="grid gap-[3vh] lg:grid-cols-2">
              <div>
                <Label>answers per minute</Label>
                <TrendChart values={rates} labels={labels} color={color} format={(n) => n.toFixed(1)} label="answers per minute, game by game" />
                <div className="text-[var(--dim)]" style={small}>
                  {trendWords(rates, "/min")}
                </div>
              </div>
              <div>
                <Label>accuracy</Label>
                <TrendChart values={accs} labels={labels} color="var(--green)" format={(n) => `${Math.round(n)}%`} label="accuracy, game by game" />
                <div className="text-[var(--dim)]" style={small}>
                  {trendWords(accs, "%")}
                </div>
              </div>
            </div>
          )}

          {ops.length > 0 && (
            <div>
              <Label>speed by operation</Label>
              <table className="w-full" aria-label="speed by operation">
                <thead>
                  <tr className="text-[var(--dim)]" style={small}>
                    <th className="pb-1 text-left font-normal">operation</th>
                    <th className="pb-1 text-right font-normal">right</th>
                    <th className="pb-1 text-right font-normal">missed</th>
                    <th className="pb-1 text-right font-normal">average</th>
                    <th className="pb-1 text-right font-normal">earlier → recent</th>
                    <th className="pb-1 text-right font-normal">game by game</th>
                  </tr>
                </thead>
                <tbody>
                  {ops.map((o) => {
                    const faster = o.earlier !== null && o.recent !== null ? o.recent < o.earlier : null;
                    return (
                      <tr key={o.kind} className="border-t border-[var(--border)]">
                        <td className="py-[0.4em]">
                          <span className="inline-block w-[2em] font-mono text-[var(--text-strong)]">{o.symbol}</span>
                          {OP_NAME[o.kind]}
                        </td>
                        <td className="text-right font-mono tabular-nums">{o.solved}</td>
                        <td className="text-right font-mono tabular-nums">{o.missed}</td>
                        <td className="text-right font-mono tabular-nums">{secs(o.avgMs)}</td>
                        <td
                          className={cn(
                            "text-right font-mono tabular-nums",
                            faster === true && "text-[var(--green)]",
                            faster === false && "text-[var(--amber)]",
                          )}
                        >
                          {o.earlier === null || o.recent === null
                            ? "—"
                            : `${o.earlier.toFixed(1)}s → ${o.recent.toFixed(1)}s ${faster ? "▼" : "▲"}`}
                        </td>
                        <td className="text-right">
                          <Spark series={o.series} color={color} />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}

          <div className="grid gap-[3vh] lg:grid-cols-2">
            {hard.length > 0 && (
              <div>
                <Label>hardest so far</Label>
                <ul aria-label="hardest questions">
                  {hard.map((q) => (
                    <li key={q.text} className="flex items-baseline gap-3 border-t border-[var(--border)] py-[0.35em]">
                      <span className="font-mono text-[var(--text-strong)]">{q.text}</span>
                      <span className="ml-auto text-[var(--dim)]" style={small}>
                        {q.missed > 0 ? `missed ${q.missed} of ${q.seen}` : `seen ${q.seen}`}
                        {q.avgMs !== null && ` · ${secs(q.avgMs)}`}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <div>
              <Label>recent games</Label>
              <table className="w-full" aria-label="recent games">
                <tbody>
                  {[...shown].reverse().slice(0, 8).map((r) => {
                    const m = metrics(r);
                    return (
                      <tr key={r.at} className="border-t border-[var(--border)]">
                        <td className="py-[0.35em] text-[var(--dim)]" style={small}>
                          {when(r.at)}
                        </td>
                        <td style={small}>{settingLabel(r)}</td>
                        <td className="text-right font-mono tabular-nums">{r.correct}</td>
                        <td className="text-right font-mono tabular-nums">{m.perMinute}/min</td>
                        <td className="text-right font-mono tabular-nums">{m.accuracy}%</td>
                        <td className="text-right text-[var(--dim)]" style={small}>
                          {r.early ? "stopped" : ""}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}

      <div className="text-[var(--dim)]" style={small}>
        Every game is kept in <span className="font-mono">{historyPlace("mental-math")}</span>, one line each — yours to copy, read or
        delete.
      </div>
    </div>
  );
}
