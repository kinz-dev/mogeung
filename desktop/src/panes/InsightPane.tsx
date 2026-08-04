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

import { useEffect, useState } from "react";
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

type View = "search" | "digest" | "analytics" | "prompts" | "failures" | "decisions" | "file" | "docs";

const VIEWS: { id: View; label: string; blurb: string }[] = [
  { id: "search", label: "Search", blurb: "every transcript and all prompt history" },
  { id: "analytics", label: "Analytics", blurb: "sessions, prompts, and when you work" },
  { id: "digest", label: "Digest", blurb: "one day, from evidence" },
  { id: "prompts", label: "Prompts", blurb: "what you keep re-asking" },
  { id: "failures", label: "Failures", blurb: "the same error, across sessions" },
  { id: "decisions", label: "Decisions", blurb: "decision-shaped sentences — candidates only" },
  { id: "file", label: "Blame", blurb: "which session produced a file" },
  { id: "docs", label: "Docs", blurb: "markdown inventory and staleness" },
];

const AXIS = { stroke: "var(--dim)", fontSize: 10 };

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

      {burn.length > 0 && (
        <ChartFrame title="Token burn per day" hint="tokens, never dollars — ADR-0005">
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
          <Listing
            empty="nothing loaded"
            onLoad={() => send({ cmd: "fetch_prompt_library" })}
            items={insight.prompts}
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
        )}

        {view === "failures" && (
          <Listing
            empty="nothing loaded"
            onLoad={() => send({ cmd: "fetch_recurring" })}
            items={insight.failures}
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
        )}

        {view === "decisions" && (
          <Listing
            empty={s ? "nothing loaded" : "select a session"}
            onLoad={() => s && send({ cmd: "fetch_decisions", session_id: s.id })}
            items={s ? (insight.decisions[s.id] ?? null) : null}
            note="candidates for a human to skim — never authority"
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

function Listing<T>({
  items,
  render,
  onLoad,
  empty,
  note,
}: {
  items: T[] | null;
  render: (item: T, i: number) => React.ReactNode;
  onLoad: () => void;
  empty: string;
  note?: string;
}) {
  useEffect(() => {
    if (items === null) onLoad();
    // Once per mount: this is a "fetch what is missing" door, not a subscription.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (items === null) return <Empty>{empty}</Empty>;
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      {note && <Dim className="block px-2 py-1 text-2xs">{note}</Dim>}
      {items.length === 0 ? <Empty>nothing found</Empty> : items.map(render)}
    </div>
  );
}
