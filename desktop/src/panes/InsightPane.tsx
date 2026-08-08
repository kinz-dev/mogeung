/**
 * Cross-session intelligence. Pillar F, and pillar H's doc inventory.
 *
 * Redesigned rather than ported. The egui original was eight sub-views behind a
 * row of toggles, each a table — which is the right *data* and the wrong shape
 * for most of it: "sessions per day" and "when do I work" are shapes, and a
 * column of numbers makes you draw the chart in your head. `R-F11` asked for
 * exactly this and it is what the web ecosystem is unambiguously better at.
 *
 * Two things kept deliberately as text: the digest and the decision candidates.
 * Both are **evidence**, and evidence gets read, not glanced at. The digest in
 * particular is counts-and-files from transcripts, never the agents' own
 * summaries — turning it into a dashboard would invite exactly the "looks
 * authoritative" reading it was built to avoid.
 */

import { useEffect, useMemo, useState } from "react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip as RTooltip,
  XAxis,
  YAxis,
} from "recharts";
import { useStore, useSelectedSession } from "@/store";
import { Chip, Dim, Empty, Input, Mono, Row } from "@/ui/primitives";
import { compact, num, oneLine, stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import { rank, winner } from "@/lib/search";
import { costSeries } from "@/lib/cost";
import type { UsageReport } from "@/wire/types";
import { KitView } from "@/panes/KitView";

type View =
  | "search"
  | "digest"
  | "analytics"
  | "prompts"
  | "failures"
  | "decisions"
  | "file"
  | "docs"
  | "memory"
  | "skills";

const VIEWS: { id: View; label: string; blurb: string }[] = [
  { id: "search", label: "Search", blurb: "every transcript and all prompt history" },
  { id: "analytics", label: "Analytics", blurb: "sessions, prompts, and when you work" },
  { id: "digest", label: "Digest", blurb: "one day, from evidence" },
  { id: "prompts", label: "Prompts", blurb: "what you keep re-asking" },
  { id: "failures", label: "Failures", blurb: "the same error, across sessions" },
  { id: "decisions", label: "Decisions", blurb: "decision-shaped sentences — candidates only" },
  { id: "file", label: "Blame", blurb: "which session produced a file" },
  { id: "docs", label: "Docs", blurb: "markdown inventory and staleness" },
  // The two that look at what Claude Code was *told* rather than what it did.
  { id: "memory", label: "Memory", blurb: "what agents have saved about you and your projects" },
  { id: "skills", label: "Skills", blurb: "every skill on this machine, and what it actually says" },
];

const AXIS = { stroke: "var(--dim)", fontSize: 10 };

/**
 * Six, and the seventh model onwards is folded into `other`. `R-J21`.
 *
 * A stacked bar with a colour per model stops being readable long before a
 * palette runs out, and this machine's corpus has five. Folding rather than
 * cycling the palette: two bands the same colour is a chart that lies.
 */
const GRAPH_COLORS = ["var(--graph-0)", "var(--graph-1)", "var(--graph-2)", "var(--graph-3)", "var(--graph-4)", "var(--graph-5)"];

const usd = (n: number): string =>
  n >= 100 ? `$${n.toFixed(0)}` : n >= 1 ? `$${n.toFixed(2)}` : n > 0 ? `$${n.toFixed(3)}` : "$0";

/** A model row's display name. Fast mode is the same model at another price. */
const modelLabel = (m: { model: string; fast: boolean }): string =>
  `${m.model.replace(/^claude-/, "")}${m.fast ? " (fast)" : ""}`;

/**
 * What today and the whole history would have cost at API rates. `R-J21`.
 *
 * **The caveat is not decoration and does not get collapsed into a tooltip.**
 * [ADR-0024](../../../docs/decisions/0024-equivalent-cost-in-dollars.md)
 * permits a dollar figure on the condition that it is labelled equivalent API
 * cost rather than spend — these sessions run on a subscription, where no
 * money moves per token. The rates' date sits beside the number for the same
 * reason: ADR-0005 refused a price table because it goes stale, and a stale
 * table that *says when it was read* is a different object from one that does
 * not.
 */
function CostSummary({ usage }: { usage: UsageReport }) {
  const priced = usage.models.filter((m) => m.cost_usd !== null);
  const top = priced.slice(0, 6);
  const most = top[0]?.cost_usd ?? 0;

  return (
    <div className="rounded-sm border border-[var(--border)] bg-[var(--bg-raised)] p-2">
      <div className="mb-2 flex items-baseline gap-3">
        <div>
          <div className="text-lg leading-tight font-semibold text-[var(--text-strong)]">
            {usd(usage.cost_usd_today)}
          </div>
          <Dim className="text-2xs">today</Dim>
        </div>
        <div>
          <div className="text-lg leading-tight font-semibold text-[var(--text-strong)]">
            {usd(usage.cost_usd_total)}
          </div>
          <Dim className="text-2xs">all {usage.days.length} days on record</Dim>
        </div>
        <Dim className="ml-auto text-right text-2xs">
          equivalent API cost, not charged
          <br />
          rates as of {usage.rates_as_of}
        </Dim>
      </div>

      {top.map((m) => (
        <div key={`${m.model}-${m.fast}`} className="flex items-center gap-2 py-0.5">
          <Mono className="w-40 shrink-0 truncate text-2xs">{modelLabel(m)}</Mono>
          <div className="h-2 flex-1 rounded-sm bg-[var(--state-hover)]">
            <div
              className="h-full rounded-sm"
              style={{
                width: `${most > 0 ? Math.max(1, ((m.cost_usd ?? 0) / most) * 100) : 0}%`,
                background: GRAPH_COLORS[top.indexOf(m) % GRAPH_COLORS.length],
              }}
            />
          </div>
          <span className="w-16 shrink-0 text-right text-2xs">{usd(m.cost_usd ?? 0)}</span>
          <Dim className="w-24 shrink-0 text-right text-2xs">{compact(m.tokens.out)} out</Dim>
        </div>
      ))}

      {usage.unpriced_models.length > 0 && (
        <div className="mt-2 text-2xs text-[var(--amber)]">
          {/* Named, because a total that silently omits a model is worse than
              no total. Their tokens are in every token figure on this page. */}
          no published rate for {usage.unpriced_models.join(", ")} — their tokens are counted, their
          cost is not
        </div>
      )}
    </div>
  );
}

function ChartFrame({ title, children, hint }: { title: string; children: React.ReactNode; hint?: string }) {
  return (
    <div className="rounded-sm border border-[var(--border)] bg-[var(--bg-raised)] p-2">
      <div className="mb-1 flex items-baseline gap-2">
        <span className="text-xs font-semibold text-[var(--text-strong)]">{title}</span>
        {hint && <Dim className="text-2xs">{hint}</Dim>}
      </div>
      <div className="h-40">
        <ResponsiveContainer width="100%" height="100%">
          {children as React.ReactElement}
        </ResponsiveContainer>
      </div>
    </div>
  );
}

/**
 * The shape of a counted list. `R-F11`.
 *
 * The row's complaint was that "the prompt and analytics tables want shape, not
 * rows". Analytics got its charts when this pane was redesigned; Prompts and
 * Failures did not, and they are the two where the shape carries the actual
 * finding — one prompt asked forty times and the rest asked twice is a fact
 * about your week that a column of `×40` makes you reconstruct by reading.
 *
 * **Counts, never money.** [ADR-0024](../../../docs/decisions/0024-equivalent-cost-in-dollars.md)
 * puts a dollar figure on Analytics and keeps it off every other surface,
 * and it is worth restating at the one place an axis label would be tempting:
 * a bar chart of *cost per prompt* is three lines of code away and would make
 * this product something it has decided not to be.
 *
 * Horizontal, because the labels are sentences. A vertical bar chart of
 * prompts would either clip every label to four words or turn them on their
 * side, and the label *is* the data here — a bar you cannot read the name of
 * ranks nothing.
 */
function CountBars({
  title,
  hint,
  data,
  color,
}: {
  title: string;
  hint: string;
  data: { label: string; count: number }[];
  color: string;
}) {
  if (data.length === 0) return null;
  return (
    <div className="shrink-0 border-b border-[var(--border)] p-2">
      <div className="mb-1 flex items-baseline gap-2">
        <span className="text-xs font-semibold text-[var(--text-strong)]">{title}</span>
        <Dim className="text-2xs">{hint}</Dim>
      </div>
      {/* Height rides on the row count: a fixed frame either wastes half its
          space on three bars or crushes twelve into illegibility. */}
      <div style={{ height: Math.max(80, data.length * 18 + 24) }}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data} layout="vertical" margin={{ left: 4, right: 8, top: 4, bottom: 4 }}>
            <CartesianGrid stroke="var(--border)" horizontal={false} />
            <XAxis type="number" {...AXIS} allowDecimals={false} />
            <YAxis
              type="category"
              dataKey="label"
              {...AXIS}
              width={190}
              // The tick is the prompt itself, so it is the one thing here that
              // must not be shortened further than it already has been.
              tickFormatter={(l: string) => (l.length > 34 ? `${l.slice(0, 33)}…` : l)}
            />
            <RTooltip
              formatter={(v: number) => [num(v), "times"]}
              contentStyle={{ background: "var(--bg-raised)", border: "1px solid var(--window-stroke)", fontSize: 11 }}
            />
            <Bar dataKey="count" fill={color} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

function Analytics() {
  const a = useStore((s) => s.insight.analytics);
  const usage = useStore((s) => s.usage);
  const send = useStore((s) => s.send);

  useEffect(() => {
    if (!a) send({ cmd: "fetch_analytics" });
    if (!usage) send({ cmd: "fetch_usage" });
  }, [a, usage, send]);

  if (!a) return <Empty>reading the history…</Empty>;

  const hours = a.hour_histogram.map((count, hour) => ({ hour: `${hour}`, count }));
  const burn = (usage?.days ?? []).map((d) => ({ day: d.day.slice(5), out: d.tokens_out, in: d.tokens_in }));
  const cost = costSeries(usage);

  return (
    <div className="grid gap-2 p-2 md:grid-cols-2">
      <ChartFrame title="Sessions per day">
        <AreaChart data={a.sessions_per_day}>
          <CartesianGrid stroke="var(--border)" vertical={false} />
          <XAxis dataKey="day" {...AXIS} tickFormatter={(d: string) => d.slice(5)} />
          <YAxis {...AXIS} width={28} />
          <RTooltip contentStyle={{ background: "var(--bg-raised)", border: "1px solid var(--window-stroke)", fontSize: 11 }} />
          <Area dataKey="count" stroke="var(--graph-0)" fill="var(--graph-0)" fillOpacity={0.2} />
        </AreaChart>
      </ChartFrame>

      <ChartFrame title="Prompts per day">
        <AreaChart data={a.prompts_per_day}>
          <CartesianGrid stroke="var(--border)" vertical={false} />
          <XAxis dataKey="day" {...AXIS} tickFormatter={(d: string) => d.slice(5)} />
          <YAxis {...AXIS} width={28} />
          <RTooltip contentStyle={{ background: "var(--bg-raised)", border: "1px solid var(--window-stroke)", fontSize: 11 }} />
          <Area dataKey="count" stroke="var(--graph-1)" fill="var(--graph-1)" fillOpacity={0.2} />
        </AreaChart>
      </ChartFrame>

      <ChartFrame title="When you work" hint="local hour of day">
        <BarChart data={hours}>
          <CartesianGrid stroke="var(--border)" vertical={false} />
          <XAxis dataKey="hour" {...AXIS} interval={2} />
          <YAxis {...AXIS} width={28} />
          <RTooltip contentStyle={{ background: "var(--bg-raised)", border: "1px solid var(--window-stroke)", fontSize: 11 }} />
          <Bar dataKey="count" fill="var(--graph-3)" />
        </BarChart>
      </ChartFrame>

      {usage && usage.models.length > 0 && (
        <div className="md:col-span-2">
          <CostSummary usage={usage} />
        </div>
      )}

      {cost.rows.length > 0 && (
        <ChartFrame title="Cost per day, by model" hint={`equivalent API cost · rates as of ${usage?.rates_as_of}`}>
          <BarChart data={cost.rows}>
            <CartesianGrid stroke="var(--border)" vertical={false} />
            <XAxis dataKey="day" {...AXIS} />
            <YAxis {...AXIS} width={40} tickFormatter={(v: number) => usd(v)} />
            <RTooltip
              formatter={(v: number, name: string) => [usd(v), name]}
              contentStyle={{ background: "var(--bg-raised)", border: "1px solid var(--window-stroke)", fontSize: 11 }}
            />
            {/* Stacked, so the height is the day's cost and each band is a
                model. One `stackId` — split stacks would draw two totals. */}
            {cost.models.map((m, i) => (
              <Bar key={m} dataKey={m} stackId="cost" fill={GRAPH_COLORS[i % GRAPH_COLORS.length]} />
            ))}
          </BarChart>
        </ChartFrame>
      )}

      {burn.length > 0 && (
        <ChartFrame title="Token burn per day" hint="what the cost above is computed from">
          <AreaChart data={burn}>
            <CartesianGrid stroke="var(--border)" vertical={false} />
            <XAxis dataKey="day" {...AXIS} />
            <YAxis {...AXIS} width={40} tickFormatter={compact} />
            <RTooltip
              formatter={(v: number) => num(v)}
              contentStyle={{ background: "var(--bg-raised)", border: "1px solid var(--window-stroke)", fontSize: 11 }}
            />
            <Area dataKey="out" stroke="var(--graph-2)" fill="var(--graph-2)" fillOpacity={0.2} />
          </AreaChart>
        </ChartFrame>
      )}

      <div className="md:col-span-2">
        <div className="mb-1 text-xs font-semibold text-[var(--text-strong)]">Projects</div>
        <div className="rounded-sm border border-[var(--border)]">
          {a.projects.slice(0, 40).map((p) => (
            <div key={p.project} className="flex items-center gap-2 border-b border-[var(--border)] px-2 py-0.5 last:border-0">
              <Mono className="flex-1 truncate text-xs">{p.project}</Mono>
              <Dim className="text-2xs">{p.prompts} prompts</Dim>
              <Dim className="text-2xs">{p.sessions} sessions</Dim>
              {!p.has_transcripts && <Chip color="var(--dim)">no transcripts</Chip>}
            </div>
          ))}
        </div>
      </div>

      {usage && usage.limit_hits.length > 0 && (
        <div className="md:col-span-2">
          <div className="mb-1 text-xs font-semibold text-[var(--text-strong)]">Limit hits</div>
          <Dim className="mb-1 block text-2xs">
            Any threshold here is an estimate from observed hits — the CLI publishes no quota.
          </Dim>
          {usage.limit_hits.slice(0, 10).map((h, i) => (
            <div key={i} className="px-2 py-0.5 text-xs">
              <Dim>{stamp(h.at)}</Dim> — {compact(h.window_tokens_out)} out in the window
              {h.resets && <Dim> · resets {h.resets}</Dim>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function InsightPane() {
  const [view, setView] = useState<View>("analytics");
  const insight = useStore((s) => s.insight);
  const patch = useStore((s) => s.patchInsight);
  const send = useStore((s) => s.send);
  const select = useStore((s) => s.select);
  const sessions = useStore((s) => s.sessions);
  const s = useSelectedSession();

  return (
    <div className="flex h-full min-h-0 bg-[var(--bg-panel)]">
      <div className="w-40 shrink-0 border-r border-[var(--border)] py-1">
        {VIEWS.map((v) => (
          <button
            key={v.id}
            type="button"
            onClick={() => setView(v.id)}
            title={v.blurb}
            className={cn(
                "outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
              "block w-full px-2 py-1 text-left text-sm",
              view === v.id
                ? "bg-[var(--selection-bg)] text-[var(--text-strong)]"
                : "text-[var(--dim)] hover:bg-[var(--bg-faint)]",
            )}
          >
            {v.label}
          </button>
        ))}
      </div>

      <div className="flex min-w-0 flex-1 flex-col">
        {view === "memory" && <KitView kind="memory" />}
        {view === "skills" && <KitView kind="skill" />}

        {view === "analytics" && (
          <div className="min-h-0 flex-1 overflow-y-auto">
            <Analytics />
          </div>
        )}

        {view === "search" && (
          <>
            <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-2 py-1">
              <div className="flex-1">
                <Input
                  value={insight.query}
                  onChange={(v) => patch({ query: v })}
                  placeholder="every transcript and all prompt history — Enter to run"
                  onKeyDown={(e) => {
                    if (e.key !== "Enter" || !insight.query.trim()) return;
                    patch({ searchPending: true });
                    send({ cmd: "insight_search", query: insight.query.trim() });
                  }}
                />
              </div>
              {insight.searchPending && <Dim className="text-2xs">searching…</Dim>}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {!insight.results ? (
                <Empty hint="the rail's Search does this and two other corpora at once (Alt+5)">
                  nothing searched yet
                </Empty>
              ) : (
                <>
                  <Dim className="block px-2 py-1 text-2xs">
                    {insight.results[1].hits.length} hit(s) across {insight.results[1].files_scanned} file(s)
                    {insight.results[1].truncated && " — capped, refine the query"}
                  </Dim>
                  {insight.results[1].hits.map((h, i) => (
                    <Row
                      key={i}
                      onClick={() => {
                        if (!sessions[h.session_id]) return;
                        select(h.session_id);
                        useStore.setState({ focusEventTs: h.timestamp });
                      }}
                      className="flex gap-2 border-b border-[var(--border)] py-0.5"
                    >
                      <Dim className="w-10 shrink-0 text-2xs">{h.source.slice(0, 4)}</Dim>
                      <Dim className="w-32 shrink-0 truncate text-2xs">
                        {h.session_id.slice(0, 8)} {h.timestamp ? stamp(h.timestamp) : ""}
                      </Dim>
                      <Mono className="min-w-0 flex-1 truncate text-xs">{h.preview}</Mono>
                    </Row>
                  ))}
                </>
              )}
            </div>
          </>
        )}

        {view === "digest" && (
          <>
            <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-2 py-1">
              <input
                type="date"
                value={insight.day}
                onChange={(e) => {
                  patch({ day: e.target.value, digestPending: true });
                  send({ cmd: "fetch_digest", day: e.target.value });
                }}
                className="rounded-sm border border-[var(--border)] bg-[var(--bg)] px-1 py-0.5 text-xs"
              />
              <button
                type="button"
                onClick={() => {
                  patch({ digestPending: true });
                  send({ cmd: "fetch_digest", day: insight.day });
                }}
                className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] rounded-sm border border-[var(--border)] px-1.5 py-0.5 text-2xs"
              >
                load
              </button>
              <Dim className="text-2xs">counts and files — never the agents' own summaries</Dim>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {!insight.digest ? (
                <Empty>pick a day</Empty>
              ) : insight.digest.sessions.length === 0 ? (
                <Empty>no session activity on {insight.digest.day}</Empty>
              ) : (
                insight.digest.sessions.map((d) => (
                  <div key={d.session_id} className="border-b border-[var(--border)] px-2 py-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm text-[var(--text-strong)]">
                        {d.title ?? d.session_id.slice(0, 8)}
                      </span>
                      <Dim className="text-2xs">{d.repo}</Dim>
                      <Dim className="ml-auto shrink-0 text-2xs">
                        {d.turns} turns · {d.tool_calls} tools · {compact(d.tokens_out)} out
                      </Dim>
                    </div>
                    {d.errors > 0 && <span className="text-2xs text-[var(--red)]">{d.errors} error(s)</span>}
                    {d.files_touched.length > 0 && (
                      <Dim className="mt-0.5 block truncate text-2xs">{d.files_touched.join(" · ")}</Dim>
                    )}
                  </div>
                ))
              )}
            </div>
          </>
        )}

        {view === "prompts" && (
          <>
            <CountBars
              title="What you keep re-asking"
              hint="the twelve most repeated, by count — never by cost (ADR-0024)"
              color="var(--graph-1)"
              data={(insight.prompts ?? [])
                .slice()
                .sort((a, b) => b.count - a.count)
                .slice(0, 12)
                .map((c) => ({ label: oneLine(c.representative, 60), count: c.count }))}
            />
          <Listing
            empty="nothing loaded"
            onLoad={() => send({ cmd: "fetch_prompt_library" })}
            items={insight.prompts}
            searchable={(c) => c.representative}
            placeholder="filter the prompts you keep re-asking"
            render={(c, i) => (
              <div key={i} className="border-b border-[var(--border)] px-2 py-1">
                <div className="flex items-center gap-2">
                  <Chip color="var(--blue)">×{c.count}</Chip>
                  <Dim className="ml-auto text-2xs">
                    {stamp(c.first_used)} → {stamp(c.last_used)}
                  </Dim>
                </div>
                <div className="mt-0.5 text-sm">{oneLine(c.representative, 300)}</div>
              </div>
            )}
          />
          </>
        )}

        {view === "failures" && (
          <>
            <CountBars
              title="The same error, again"
              hint="the twelve most recurrent, by how many times they appear"
              color="var(--red)"
              data={(insight.failures ?? [])
                .slice()
                .sort((a, b) => b.count - a.count)
                .slice(0, 12)
                .map((f) => ({ label: oneLine(f.example, 60), count: f.count }))}
            />
          <Listing
            empty="nothing loaded"
            onLoad={() => send({ cmd: "fetch_recurring" })}
            items={insight.failures}
            searchable={(f) => f.example}
            placeholder="filter by the error text"
            render={(f, i) => (
              <div key={i} className="border-b border-[var(--border)] px-2 py-1">
                <div className="flex items-center gap-2">
                  <Chip color="var(--red)">×{f.count}</Chip>
                  <Dim className="text-2xs">{f.sessions.length} session(s)</Dim>
                </div>
                <Mono className="mt-0.5 block text-xs text-[var(--del-fg)]">{oneLine(f.example, 300)}</Mono>
              </div>
            )}
          />
          </>
        )}

        {view === "decisions" && (
          <Listing
            empty={s ? "nothing loaded" : "select a session"}
            onLoad={() => s && send({ cmd: "fetch_decisions", session_id: s.id })}
            items={s ? (insight.decisions[s.id] ?? null) : null}
            note="candidates for a human to skim — never authority"
            searchable={(d) => `${d.text} ${d.pattern}`}
            placeholder="filter the candidates"
            render={(d, i) => (
              <div key={i} className="border-b border-[var(--border)] px-2 py-1">
                <Dim className="text-2xs">
                  L{d.line} {d.timestamp ? stamp(d.timestamp) : ""} · {d.pattern}
                </Dim>
                <div className="text-sm">{d.text}</div>
              </div>
            )}
          />
        )}

        {view === "file" && (
          <>
            <div className="shrink-0 border-b border-[var(--border)] px-2 py-1">
              <Input
                value={insight.fileQuery}
                onChange={(v) => patch({ fileQuery: v })}
                placeholder="a path — Enter to find the sessions that touched it"
                onKeyDown={(e) => {
                  if (e.key === "Enter" && insight.fileQuery.trim())
                    send({ cmd: "fetch_file_sessions", path: insight.fileQuery.trim() });
                }}
              />
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {!insight.fileSessions ? (
                <Empty>type a path</Empty>
              ) : (
                insight.fileSessions[1].map((e, i) => (
                  <Row
                    key={i}
                    onClick={() => sessions[e.session_id] && select(e.session_id)}
                    className="border-b border-[var(--border)] py-1"
                  >
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm">{e.label}</span>
                      <Dim className="ml-auto shrink-0 text-2xs">{e.at ? stamp(e.at) : ""}</Dim>
                    </div>
                    {e.prompt && <Dim className="block truncate text-2xs">{e.prompt}</Dim>}
                    <Dim className="text-2xs">matched by {e.matched}</Dim>
                  </Row>
                ))
              )}
            </div>
          </>
        )}

        {view === "docs" && (
          <>
            <div className="shrink-0 border-b border-[var(--border)] px-2 py-1">
              <Input
                value={insight.docRepo}
                onChange={(v) => patch({ docRepo: v })}
                placeholder="a repo root — Enter to inventory its markdown"
                onKeyDown={(e) => {
                  if (e.key === "Enter" && insight.docRepo.trim())
                    send({ cmd: "fetch_doc_scan", repo: insight.docRepo.trim() });
                }}
              />
              <Dim className="mt-0.5 block text-2xs">
                proposals only — the daemon never writes to a watched repo
              </Dim>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {!insight.docs ? (
                <Empty hint={s?.repo_root ? `try ${s.repo_root}` : undefined}>nothing scanned</Empty>
              ) : (
                <>
                  {insight.docs[1].stale.length > 0 && (
                    <div className="px-2 py-1 text-2xs font-semibold tracking-wider text-[var(--amber)] uppercase">
                      Stale ({insight.docs[1].stale.length})
                    </div>
                  )}
                  {insight.docs[1].stale.map((d, i) => (
                    <div key={i} className="border-b border-[var(--border)] px-2 py-0.5">
                      <Mono className="text-xs">{d.doc}</Mono>
                      <Dim className="block text-2xs">
                        {d.referenced_path} moved {d.commits_since} commit(s) since
                      </Dim>
                    </div>
                  ))}
                  <div className="px-2 py-1 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
                    Docs ({insight.docs[1].docs.length})
                  </div>
                  {insight.docs[1].docs.map((d) => (
                    <div key={d.path} className="flex items-center gap-2 border-b border-[var(--border)] px-2 py-0.5">
                      <Mono className="flex-1 truncate text-xs">{d.path}</Mono>
                      <Chip color="var(--dim)">{d.kind}</Chip>
                      {d.orphan && <Chip color="var(--amber)">orphan</Chip>}
                    </div>
                  ))}
                </>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/**
 * A loaded list, with a filter over it. `R-F10`.
 *
 * The row asked for substring, regex, `rg` and fuzzy running together with the
 * best answer winning. Two of those were settled while `R-B36` was built and
 * the answers hold here: **no external binaries** — `rg` and `fzf` may not be
 * installed, where every other outside dependency in this product is either
 * required up front (`git`) or degrades to a named fallback (`tmux`) — and one
 * scale across the engines that do run, with **the winner named** beside the
 * count. Regex stays deferred; it needs a crate-sized decision, not a corner of
 * a pane.
 *
 * The filter lives here rather than in each view because all seven of these are
 * the same shape: a list the daemon ordered by something, that you want to cut
 * down. `searchable` is what to match against — absent, the view simply has no
 * box, which is the right answer for a list of four things.
 */
function Listing<T>({
  items,
  render,
  onLoad,
  empty,
  note,
  searchable,
  placeholder,
}: {
  items: T[] | null;
  render: (item: T, i: number) => React.ReactNode;
  onLoad: () => void;
  empty: string;
  note?: string;
  /** What a query is matched against. Omit for a list with no filter. */
  searchable?: (item: T) => string;
  placeholder?: string;
}) {
  const [query, setQuery] = useState("");
  useEffect(() => {
    if (items === null) onLoad();
    // Once per mount: this is a "fetch what is missing" door, not a subscription.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const ranked = useMemo(
    () => (searchable ? rank(items ?? [], query, searchable) : (items ?? []).map((item) => ({ item, hit: null }))),
    [items, query, searchable],
  );
  const engine = winner(ranked);

  if (items === null) return <Empty>{empty}</Empty>;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {searchable && (
        <div className="flex shrink-0 items-center gap-1.5 border-b border-[var(--border)] px-2 py-1">
          <Input
            value={query}
            onChange={setQuery}
            placeholder={placeholder ?? "filter"}
            ariaLabel="filter this view"
            onKeyDown={(e) => {
              if (e.key === "Escape") setQuery("");
            }}
          />
          {query.trim() && (
            <>
              <Dim className="shrink-0 text-2xs whitespace-nowrap">
                {ranked.length}/{items.length}
              </Dim>
              {/* Named, not inferred. A search that will not say how it matched
                  is one you cannot argue with when it ranks something oddly. */}
              {engine && (
                <Dim
                  className="shrink-0 text-2xs"
                  title="which engine won — exact substring, all-words, or subsequence"
                >
                  {engine}
                </Dim>
              )}
            </>
          )}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {note && <Dim className="block px-2 py-1 text-2xs">{note}</Dim>}
        {ranked.length === 0 ? (
          <Empty>{query.trim() ? "nothing matches" : "nothing found"}</Empty>
        ) : (
          ranked.map((r, i) => render(r.item, i))
        )}
      </div>
    </div>
  );
}
