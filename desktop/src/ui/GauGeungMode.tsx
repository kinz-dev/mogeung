/**
 * Gau Geung: the team board, the seat pane and a seat's terminal, as panes
 * of the main tile tree. `R-P7`, feature 0051; panes since `R-J132`.
 *
 * Every design question was decided on 2026-10-09 from the wireframes, and
 * one of them was reversed by the first use on 2026-10-11:
 *
 * - **Panes, not a mode.** `R-P7` opened a mode over the whole window on
 *   `Alt+g`, with a tile tree of its own and `Esc` to leave — the wall's
 *   shape, for the wall's reason. The first real team showed what that costs:
 *   the Attention list, the rail and `Alt+a`'s chat were all underneath it,
 *   and the mode's panels could be arranged among themselves but never with
 *   anything else. *"The whole UI should be free like a normal UI."* So the
 *   board and the seat pane are panes in the main tile tree now, a seat's
 *   terminal is a pane beside them, and `Alt+g` raises the board or closes
 *   it when it is in front. They dock, split, tab and pop out as every other
 *   pane does, the layout is the one layout, and nothing is underneath.
 * - **Five lanes for seats in a team.** OpenRig's pulse — needs you, parked
 *   with baton, blocked on agents, now, just finished — for seats only, each
 *   card carrying mogeung's badge inside. A session outside a team keeps its
 *   badge on the queue and never enters a lane. `lib/teamMode.ts` reads the
 *   lanes off the send log.
 * - **Graph and table together.** Topology shows who waits on whom as edges
 *   — the last send between two seats, a held edge a message waiting for you
 *   — beside a table of the same seats with context, tokens and a state that
 *   names its source. No toggle.
 * - **The seat pane follows the pick.** A pick on the board opens the seat
 *   pane beside it when it is not there and raises it when it is a background
 *   tab; a terminal pane per seat arrives on *Attach terminal*, keyed by the
 *   seat so a second attach raises the first (`lib/panes.ts`).
 * - **Two teams at once.** Tabs on the board pick one team or all of them;
 *   each team keeps its own counters and its own pause switch, and a seat
 *   addresses only its own team.
 *
 * Nothing here is fetched. Teams, launches, sessions, the queue and the send
 * logs are what the store already holds; the board is a different reading of
 * them. The only verbs it sends are the ones the Teams window already does —
 * pause, release, drop, restart — and `team_list` when the board mounts.
 */

import { useEffect, useMemo } from "react";
import { Anchor, Ban, Check, Copy, Moon, Pause, Play, RotateCcw, Square, SquareTerminal, Users } from "lucide-react";
import { useStore } from "@/store";
import { Badge, Button, Chip, Dim, Empty, IconButton, Mono } from "@/ui/primitives";
import { TerminalView } from "@/ui/Terminal";
import { usePaneId } from "@/lib/paneScope";
import { attachSeatTerminal, showSeatPane } from "@/lib/panes";
import { cn } from "@/lib/cn";
import { interactive } from "@/ui/styles";
import { compact, shortDir } from "@/lib/format";
import { copyPath } from "@/lib/clipboard";
import { showDialog } from "@/lib/dialogWindow";
import { attachArgs, reachFor, spawnAs } from "@/lib/tmux";
import { WIND_DOWN_LABEL, countersLine, sendLine, sendsFor, signalsLine, windDownLine } from "@/lib/teams";
import { LANES, agoShort, edges, parseSeatKey, ring, seatKey, seatViews, teamTab, termOfPane, type Lane, type SeatView } from "@/lib/teamMode";
import { SCREEN_LABEL, isScreenGate, needsHuman, sourceLabel, type AttentionReason, type Team, type TeamSends } from "@/wire/types";

const now = () => Date.now() / 1000;

function badgeColor(reason: AttentionReason | undefined): { color: string; dark?: boolean } {
  if (reason === "failed" || reason === "rate_limited") return { color: "var(--red)" };
  if (reason && needsHuman(reason)) return { color: "var(--amber)", dark: true };
  return { color: "var(--dim)" };
}

function laneTone(lane: Lane): string {
  switch (lane) {
    case "needs_you":
      return "border-[var(--amber)]";
    case "parked":
      return "border-[var(--border-hover)]";
    case "blocked":
      return "border-[var(--border)] opacity-90";
    case "now":
      return "border-[var(--border)]";
    case "finished":
      return "border-[var(--border)] opacity-70";
  }
}

/** The teams a window can show in the mode: every one it holds, newest first as the daemon sends them. */
function useTeams(): Team[] {
  return useStore((s) => s.teams);
}

function useViews(team: Team): SeatView[] {
  const launches = useStore((s) => s.seatLaunches);
  const sessions = useStore((s) => s.sessions);
  const queue = useStore((s) => s.queue);
  const sends = useStore((s) => s.teamSends.find((t) => t.team_id === team.id));
  return useMemo(() => seatViews(team, launches, sessions, queue, sends), [team, launches, sessions, queue, sends]);
}

// -- The pulse. ---------------------------------------------------------------

function SeatCard({ v, selected, onPick }: { v: SeatView; selected: boolean; onPick: () => void }) {
  const tone = badgeColor(v.item?.reason);
  return (
    <button
      type="button"
      onClick={onPick}
      data-testid={`card-${v.seat.name}`}
      className={cn(
        interactive,
        "flex w-full flex-col items-start gap-0.5 rounded border px-1.5 py-1 text-left text-xs",
        "hover:bg-[var(--bg-hover)]",
        selected ? "bg-[var(--selection-bg)]" : "bg-[var(--bg-panel)]",
      )}
      title={v.line}
    >
      <span className="flex w-full items-center gap-1">
        <span className="font-semibold text-[var(--text-strong)]">{v.seat.label || v.seat.name}</span>
        {v.badge && (
          <Badge color={tone.color} dark={tone.dark}>
            {v.badge}
          </Badge>
        )}
        <Dim className="ml-auto text-2xs">{agoShort(v.lastAt, now())}</Dim>
      </span>
      <Dim className="line-clamp-2 text-2xs">{v.line}</Dim>
    </button>
  );
}

function Pulse({ views, sends, selected, pick }: { views: SeatView[]; sends: TeamSends | undefined; selected: string | null; pick: (v: SeatView) => void }) {
  const lastDelivered = [...(sends?.log ?? [])].reverse().find((e) => e.status === "delivered" || e.status === "released");
  return (
    <div className="grid grid-cols-5 gap-1.5" data-testid="pulse">
      {LANES.map((lane) => {
        const mine = views.filter((v) => v.lane === lane.id);
        return (
          <div key={lane.id} className={cn("flex min-h-[7rem] flex-col gap-1 rounded border p-1.5", laneTone(lane.id))} title={lane.title} data-testid={`lane-${lane.id}`}>
            <Dim className="text-2xs uppercase tracking-wide">
              {lane.label} · {mine.length}
            </Dim>
            {mine.map((v) => (
              <SeatCard key={v.seat.name} v={v} selected={selected === seatKey(v.team.id, v.seat.name)} onPick={() => pick(v)} />
            ))}
            {lane.id === "finished" && lastDelivered && (
              <Dim className="mt-auto truncate text-2xs" title={lastDelivered.text}>
                last: {lastDelivered.from ?? lastDelivered.kind} → {lastDelivered.to} · {agoShort(lastDelivered.at, now())} ago
              </Dim>
            )}
            {mine.length === 0 && lane.id !== "finished" && <Dim className="text-2xs opacity-60">—</Dim>}
          </div>
        );
      })}
    </div>
  );
}

// -- Topology: the graph and the table. ---------------------------------------

function Graph({ views, sends, selected, pick }: { views: SeatView[]; sends: TeamSends | undefined; selected: string | null; pick: (v: SeatView) => void }) {
  const w = 360;
  const h = 220;
  const pos = ring(views.length, w, h);
  const at = new Map(views.map((v, i) => [v.seat.name, pos[i]]));
  const es = edges(sends?.log ?? []).filter((e) => at.has(e.from) && at.has(e.to));
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="h-[220px] w-full max-w-[360px]" role="img" aria-label="who waits on whom" data-testid="graph">
      <defs>
        <marker id="gg-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--dim)" />
        </marker>
      </defs>
      {es.map((e) => {
        const a = at.get(e.from)!;
        const b = at.get(e.to)!;
        // Shorten both ends so the arrow lands on the node's edge, not its centre.
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const len = Math.hypot(dx, dy) || 1;
        const r = 26;
        const x1 = a.x + (dx / len) * r;
        const y1 = a.y + (dy / len) * r;
        const x2 = b.x - (dx / len) * r;
        const y2 = b.y - (dy / len) * r;
        const held = e.entry.status === "held";
        const label = held ? "held" : e.entry.hop > 0 ? `hop ${e.entry.hop}` : "";
        return (
          <g key={`${e.from}→${e.to}`}>
            <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={held ? "var(--amber)" : "var(--dim)"} strokeWidth={held ? 2 : 1} strokeDasharray={held ? "4 3" : undefined} markerEnd="url(#gg-arrow)" />
            {label && (
              <text x={(x1 + x2) / 2} y={(y1 + y2) / 2 - 4} fontSize="9" fill={held ? "var(--amber)" : "var(--dim)"} textAnchor="middle">
                {label}
              </text>
            )}
          </g>
        );
      })}
      {views.map((v, i) => {
        const p = pos[i];
        const key = seatKey(v.team.id, v.seat.name);
        const wants = v.lane === "needs_you";
        return (
          <g key={v.seat.name} onClick={() => pick(v)} className="cursor-pointer" data-testid={`node-${v.seat.name}`}>
            <circle cx={p.x} cy={p.y} r={24} fill={selected === key ? "var(--selection-bg)" : "var(--bg-panel)"} stroke={wants ? "var(--amber)" : "var(--border)"} strokeWidth={wants ? 2 : 1} />
            <text x={p.x} y={p.y - 2} fontSize="10" textAnchor="middle" fill="var(--text-strong)" fontWeight={600}>
              {v.seat.name}
            </text>
            <text x={p.x} y={p.y + 10} fontSize="8" textAnchor="middle" fill="var(--dim)">
              {v.badge ?? "—"}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

function sourceWord(v: SeatView): string {
  switch (v.source) {
    case "hook":
      return "(hook)";
    case "plugin":
      return "(plugin)";
    case "screen":
      return "(screen)";
    case "transcript":
      return "(transcript)";
    case "none":
      return "";
  }
}

function Table({ views, selected, pick }: { views: SeatView[]; selected: string | null; pick: (v: SeatView) => void }) {
  return (
    <div className="min-w-0 flex-1 overflow-x-auto">
      <table className="w-full text-2xs" data-testid="topology">
        <thead>
          <tr className="text-left text-[var(--dim)]">
            <th className="px-1.5 py-1 font-normal">seat</th>
            <th className="px-1.5 py-1 font-normal">cli · model</th>
            <th className="px-1.5 py-1 font-normal">worktree · branch</th>
            <th className="px-1.5 py-1 font-normal">state · source</th>
            <th className="px-1.5 py-1 font-normal">context</th>
            <th className="px-1.5 py-1 font-normal">tokens</th>
            <th className="px-1.5 py-1 font-normal">sent · recv</th>
            <th className="px-1.5 py-1 font-normal">last</th>
          </tr>
        </thead>
        <tbody>
          {views.map((v) => {
            const key = seatKey(v.team.id, v.seat.name);
            const tone = badgeColor(v.item?.reason);
            const model = v.launch?.signals.model ?? v.seat.model ?? "default";
            const plain = v.launch?.signals.kind === "plain";
            return (
              <tr
                key={v.seat.name}
                data-testid={`row-${v.seat.name}`}
                onClick={() => pick(v)}
                className={cn("cursor-pointer border-t border-[var(--line)] align-top hover:bg-[var(--bg-hover)]", selected === key && "bg-[var(--selection-bg)]")}
              >
                <td className="px-1.5 py-1 font-semibold text-[var(--text-strong)]">{v.seat.name}</td>
                <td className="px-1.5 py-1">
                  <Mono>
                    {sourceLabel(v.seat.cli)} · {model}
                  </Mono>
                </td>
                <td className="px-1.5 py-1">
                  <Mono>{v.launch ? `${shortDir(v.launch.dir)}${v.launch.dir.split("/").pop()}` : v.seat.works_in === "worktree" ? "worktree" : "checkout"}</Mono>
                  {v.session?.git_branch && <Dim className="block">{v.session.git_branch}</Dim>}
                </td>
                <td className="px-1.5 py-1">
                  {v.badge ? (
                    <Badge color={tone.color} dark={tone.dark}>
                      {v.badge}
                    </Badge>
                  ) : (
                    <Dim>not running</Dim>
                  )}
                  <Dim className="block">
                    {v.line} {sourceWord(v)}
                  </Dim>
                </td>
                <td className="px-1.5 py-1">
                  {v.context !== undefined ? (
                    <span className="flex items-center gap-1">
                      <span className="inline-block h-1.5 w-12 overflow-hidden rounded-sm border border-[var(--border)]">
                        <span className="block h-full bg-[var(--dim)]" style={{ width: `${v.context}%` }} />
                      </span>
                      <Mono>{v.context}%</Mono>
                    </span>
                  ) : (
                    <Dim>{plain ? "unknown · no hooks" : v.launch ? "no reading yet" : "—"}</Dim>
                  )}
                </td>
                <td className="px-1.5 py-1">
                  <Mono>{v.session ? `${compact(v.session.tokens_in)} · ${compact(v.session.tokens_out)}` : "—"}</Mono>
                </td>
                <td className="px-1.5 py-1">
                  <Mono>
                    {v.sent} · {v.received}
                  </Mono>
                </td>
                <td className="px-1.5 py-1">
                  <Mono>{agoShort(v.lastAt, now())}</Mono>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

// -- The policy strip and the held sends. -------------------------------------

function PolicyStrip({ team, sends, allowed }: { team: Team; sends: TeamSends | undefined; allowed: boolean }) {
  const send = useStore((s) => s.send);
  const paused = sends?.paused ?? team.paused;
  const held = sends?.log.filter((e) => e.status === "held") ?? [];
  const p = team.policy;
  return (
    <div className="flex flex-col gap-1 border-t border-[var(--line)] pt-1.5" data-testid="policy">
      <div className="flex flex-wrap items-center gap-2 text-2xs">
        <Dim className="uppercase tracking-wide">policy</Dim>
        <Mono>{sends ? countersLine(sends) : `hops 0/${p.hop_budget} · 0/${p.hourly_cap} this hour`}</Mono>
        <Dim>
          wakes {p.wakes.length} · watchdogs {p.watchdogs.length} · handover: {p.handover.replace("_", " ")}
        </Dim>
        <span className="ml-auto" />
        <Button
          size="sm"
          variant={paused ? "solid" : "outline"}
          disabled={!allowed}
          title={paused ? "resume: automatic sends are judged and delivered again" : "pause every automatic send for this team — held, not lost; typing in a pane always works"}
          onClick={() => send({ cmd: "team_pause", id: team.id, paused: !paused })}
        >
          {paused ? <Play size={12} /> : <Pause size={12} />} {paused ? "Resume sends" : "Pause all automatic sends"}
        </Button>
      </div>
      {held.map((e) => (
        <div key={e.id} className="flex flex-wrap items-center gap-1 text-2xs" data-testid="held">
          <Chip color="var(--amber)">held</Chip>
          <span className="min-w-0 flex-1 truncate" title={`${e.text}\n\n${e.why}${e.note ? ` — ${e.note}` : ""}`}>
            {sendLine(e, now())}
          </span>
          <Button size="sm" variant="outline" disabled={!allowed} title="type it into the seat now — your act, so the chain starts again" onClick={() => send({ cmd: "send_release", team_id: team.id, id: e.id })}>
            <Check size={12} /> release
          </Button>
          <Button size="sm" disabled={!allowed} title="never deliver it" onClick={() => send({ cmd: "send_drop", team_id: team.id, id: e.id })}>
            <Ban size={12} /> drop
          </Button>
        </div>
      ))}
    </div>
  );
}

// -- One team's board. --------------------------------------------------------

function TeamSection({ team }: { team: Team }) {
  const views = useViews(team);
  const sends = useStore((s) => s.teamSends.find((t) => t.team_id === team.id));
  const allowed = useStore((s) => s.teamsAllowed);
  const selected = useStore((s) => s.gauSeat);
  const send = useStore((s) => s.send);
  // `alive` is the daemon's word (`R-J134`): a record of a seat that exited
  // enables Stop now, which forgets it, and not Wind down, which would ask
  // nobody.
  const running = useStore((s) => s.seatLaunches.some((l) => l.team_id === team.id && l.alive));
  const onRecord = useStore((s) => s.seatLaunches.some((l) => l.team_id === team.id));
  const winding = sends?.wind_down?.seats.some((x) => x.status === "asked") ?? false;
  const pick = (v: SeatView) => {
    useStore.setState({ gauSeat: seatKey(v.team.id, v.seat.name) });
    showSeatPane();
  };
  return (
    <div className="flex flex-col gap-2 rounded border border-[var(--line)] p-2" data-testid={`team-${team.id}`}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-semibold text-[var(--text-strong)]">{team.name}</span>
        <Dim className="text-2xs">{teamTab(team, views, sends)}</Dim>
        <span className="ml-auto flex items-center gap-1">
          {/* The quick way down (`R-P15`): ask every seat for its note and
              stop each as it answers; Stop now ends them at once. */}
          <Button
            size="sm"
            variant="outline"
            disabled={!allowed || !running || winding}
            title={
              winding
                ? "winding down — every seat has been asked for its note"
                : running
                  ? `ask every seat for a note of where it got to, then stop each as it answers — ${team.policy.wind_down_minutes} min at most; the next handover reads the notes`
                  : "no seat is running"
            }
            onClick={() => send({ cmd: "team_wind_down", id: team.id })}
          >
            <Moon size={12} /> Wind down
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={!allowed || !onRecord}
            title={
              winding
                ? "cut the wind-down short — the seats still asked are stopped without a note"
                : running
                  ? "end every seat at once, no note asked — the worktrees stay"
                  : onRecord
                    ? "nothing is running — forget the exited seats' records"
                    : "no seat is running"
            }
            onClick={() => send({ cmd: "team_stop", id: team.id })}
          >
            <Square size={12} /> Stop now
          </Button>
        </span>
      </div>
      {sends?.wind_down && (
        <div className="flex flex-col gap-0.5 rounded border border-[var(--amber)] px-2 py-1 text-2xs" data-testid="wind-down">
          <span className="font-semibold text-[var(--text-strong)]">{windDownLine(sends.wind_down, now())}</span>
          {sends.wind_down.seats.map((x) => (
            <Dim key={x.seat} className="truncate" title={x.note ?? undefined}>
              {x.seat} · {WIND_DOWN_LABEL[x.status]}
              {x.note ? ` — “${x.note.length > 90 ? `${x.note.slice(0, 90)}…` : x.note}”` : ""}
            </Dim>
          ))}
        </div>
      )}
      <Pulse views={views} sends={sends} selected={selected} pick={pick} />
      <div className="flex flex-wrap gap-2">
        <Graph views={views} sends={sends} selected={selected} pick={pick} />
        <Table views={views} selected={selected} pick={pick} />
      </div>
      <PolicyStrip team={team} sends={sends} allowed={allowed} />
    </div>
  );
}

export function TeamBoard() {
  const teams = useTeams();
  const which = useStore((s) => s.gauTeam);
  const shown = which ? teams.filter((t) => t.id === which) : teams;
  // Ask for the teams, the seats and the sends when the board comes up: a
  // reconnect is a new daemon's worth of them, and the board must not open
  // on a stale set.
  const send = useStore((s) => s.send);
  useEffect(() => {
    send({ cmd: "team_list" });
  }, [send]);
  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="team-board">
      <BoardHeader />
      {teams.length === 0 ? (
        <Empty hint="define one in the Teams window (Alt+Shift+g), then boot it — its seats appear here, in the five lanes">
          no teams yet
        </Empty>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-2">
          {shown.map((t) => (
            <TeamSection key={t.id} team={t} />
          ))}
        </div>
      )}
    </div>
  );
}

// -- The seat pane. -----------------------------------------------------------

export function SeatDetail() {
  const key = useStore((s) => s.gauSeat);
  const teams = useTeams();
  const launches = useStore((s) => s.seatLaunches);
  const sessions = useStore((s) => s.sessions);
  const queue = useStore((s) => s.queue);
  const sends = useStore((s) => s.teamSends);
  const select = useStore((s) => s.select);
  const allowed = useStore((s) => s.teamsAllowed);
  const send = useStore((s) => s.send);
  const parsed = key ? parseSeatKey(key) : null;
  const team = parsed ? teams.find((t) => t.id === parsed.teamId) : undefined;
  const view = useMemo(() => {
    if (!team || !parsed) return undefined;
    return seatViews(team, launches, sessions, queue, sends.find((t) => t.team_id === team.id)).find((v) => v.seat.name === parsed.seat);
  }, [team, parsed, launches, sessions, queue, sends]);
  if (!team || !parsed || !view) {
    return <Empty hint="pick a seat in the pulse, the graph or the table — this pane follows the pick">no seat picked</Empty>;
  }
  const teamLog = sends.find((t) => t.team_id === team.id)?.log ?? [];
  const log = sendsFor(teamLog, view.seat.name).slice(-20).reverse();
  // The last handover for this seat, whole. `R-P10`: shown in full, never a
  // summary of itself, with who composed it; held ones wait for you here.
  const handover = [...teamLog].reverse().find((e) => e.kind === "handover" && e.to === view.seat.name);
  const tone = badgeColor(view.item?.reason);
  const l = view.launch;
  return (
    <div className="flex h-full flex-col gap-2 overflow-y-auto p-2 text-xs" data-testid="seat-detail">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="text-sm font-semibold text-[var(--text-strong)]">{view.seat.name}</span>
        <Chip>seat</Chip>
        <Chip>
          {sourceLabel(view.seat.cli)} · {l?.signals.model ?? view.seat.model ?? "default"}
        </Chip>
        <Chip>{(view.seat.permission ?? team.permission).replace("_", " ")}</Chip>
        {view.badge && (
          <Badge color={tone.color} dark={tone.dark}>
            {view.badge}
          </Badge>
        )}
      </div>
      <Dim className="text-2xs">
        {l ? `${l.dir} · started ${agoShort(l.started, now())} ago by mogeung · ${l.tmux_session}` : "not running — boot the team in the Teams window"}
      </Dim>
      <Dim className="text-2xs">{view.line}</Dim>
      {l && <Dim className="text-2xs">{signalsLine(l.signals, now())}</Dim>}
      {/* What its pane showed (`R-P11`, ADR-0044): the shape, when it was
          read, and the lines it was read from — so a misread is visible
          rather than silent. A quiet shape is a word; a gate shows its
          excerpt; mogeung never answers a gate from here. */}
      {view.session?.screen && (
        <div data-testid="screen">
          <Dim className="block text-2xs uppercase tracking-wide">
            screen · read {agoShort(view.session.screen.read_at, now())} ago · shape: {SCREEN_LABEL[view.session.screen.shape]}
          </Dim>
          {view.session.screen.excerpt.length > 0 && (
            <pre className="mt-0.5 max-h-40 overflow-auto whitespace-pre-wrap rounded bg-[var(--bg)] p-1.5 font-mono text-2xs text-[var(--text)]">
              {view.session.screen.excerpt.join("\n")}
            </pre>
          )}
          {isScreenGate(view.session.screen.shape) && (
            <Dim className="block text-2xs">
              {view.session.screen.shape === "press_enter"
                ? "the one shape the policy may press through — readiness press, off unless the team says so"
                : "a reading of the pane, never answered by mogeung: attach the terminal and answer it yourself"}
            </Dim>
          )}
        </div>
      )}
      <div className="flex flex-wrap gap-1">
        <Button
          size="sm"
          variant="outline"
          disabled={!view.session?.tmux_target && !l}
          title="a terminal pane beside this one, attached to the seat's own tmux pane — typing goes to the seat"
          onClick={() => attachSeatTerminal(key!, `${team.name}/${view.seat.name}`)}
        >
          <SquareTerminal size={12} /> Attach terminal
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!view.session}
          title="select this seat's session — the transcript, the diff and the Agent pane follow it"
          onClick={() => view.session && select(view.session.id)}
        >
          <Anchor size={12} /> Select session
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!allowed || !l}
          title={
            l
              ? `stop this seat and start it again; a handover is composed from its earlier session and its teammates as the team's policy says (${team.policy.handover.replace("_", " ")})`
              : "not running"
          }
          onClick={() => send({ cmd: "seat_restart", team_id: team.id, seat: view.seat.name })}
        >
          <RotateCcw size={12} /> Restart with handover
        </Button>
      </div>
      {handover && (
        <div className="rounded border border-[var(--line)] p-1.5" data-testid="handover">
          <div className="flex flex-wrap items-center gap-1">
            <Dim className="text-2xs uppercase tracking-wide">handover · {handover.why}</Dim>
            <Chip color={handover.status === "held" ? "var(--amber)" : "var(--dim)"}>{handover.status}</Chip>
            {handover.composed_by && <Dim className="text-2xs">composed by {handover.composed_by}</Dim>}
            <Dim className="ml-auto text-2xs">{agoShort(handover.at, now())} ago</Dim>
          </div>
          {handover.status === "refused" ? (
            <Dim className="mt-1 block text-2xs">{handover.note}</Dim>
          ) : (
            <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded bg-[var(--bg)] p-1.5 font-mono text-2xs text-[var(--text)]">
              {handover.text}
            </pre>
          )}
          {handover.status === "held" && (
            <div className="mt-1 flex items-center gap-1">
              <Button size="sm" variant="solid" disabled={!allowed} title="type it into the seat now, whole" onClick={() => send({ cmd: "send_release", team_id: team.id, id: handover.id })}>
                <Check size={12} /> Deliver
              </Button>
              <Button size="sm" disabled={!allowed} title="never deliver it — the seat starts without one" onClick={() => send({ cmd: "send_drop", team_id: team.id, id: handover.id })}>
                <Ban size={12} /> Discard
              </Button>
            </div>
          )}
        </div>
      )}
      <div>
        <Dim className="block text-2xs uppercase tracking-wide">send log · this seat</Dim>
        {log.length === 0 && <Dim className="text-2xs">nothing sent to or from it yet</Dim>}
        {log.map((e) => (
          <Dim key={e.id} className="block truncate text-2xs" title={`${e.text}\n\n${e.why}${e.note ? ` — ${e.note}` : ""}`}>
            {sendLine(e, now())}
          </Dim>
        ))}
      </div>
      {view.session && (
        <Dim className="text-2xs">
          tokens in {compact(view.session.tokens_in)} · out {compact(view.session.tokens_out)}
          {view.session.files_changed > 0 ? ` · diff ${view.session.files_changed} file(s) +${view.session.insertions} −${view.session.deletions}` : ""}
        </Dim>
      )}
      {l && (
        <div>
          <Dim className="block text-2xs uppercase tracking-wide">launch · what this seat was given</Dim>
          <Dim className="block text-2xs">files under {l.files_dir}</Dim>
          {l.session_id && <Dim className="block text-2xs">session id {l.session_id}</Dim>}
          <Dim className="block text-2xs">{view.seat.own_home ? "a home of its own under ~/.mogeung" : "your home, read only — nothing of mogeung's written there"}</Dim>
        </div>
      )}
    </div>
  );
}

// -- A seat's terminal pane. --------------------------------------------------

/**
 * One seat's terminal: a `tmux attach` of its own, like an Agent pane. The
 * seat is in the pane's id (`seat-term:<team>/<seat>`), so a layout restored
 * next week names it still; a seat that is gone says so in the pane rather
 * than vanishing, because a tab that vanishes is one you have to recall.
 */
export function SeatTerminal() {
  const id = usePaneId() ?? "";
  const key = termOfPane(id);
  const parsed = key ? parseSeatKey(key) : null;
  const launch = useStore((s) => (parsed ? s.seatLaunches.find((l) => l.team_id === parsed.teamId && l.seat === parsed.seat) : undefined));
  const daemon = useStore((s) => s.daemon);
  const machineId = useStore((s) => s.machineId);
  const reach = useMemo(() => reachFor(daemon, machineId), [daemon, machineId]);
  const command = useMemo(() => (launch && reach ? spawnAs(reach, attachArgs(launch.tmux_target)) : null), [launch, reach]);
  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <div className="min-h-0 flex-1">
        <TerminalView
          id={id}
          command={command}
          refusal={
            !launch ? (
              <div className="max-w-md space-y-2">
                <div className="text-sm font-semibold text-[var(--text-strong)]">This seat is not running.</div>
                <p className="text-xs text-[var(--dim)]">Its team was stopped, or the daemon forgot it. Boot the team in the Teams window and attach again.</p>
              </div>
            ) : !reach ? (
              <div className="max-w-md space-y-2">
                <div className="text-sm font-semibold text-[var(--text-strong)]">That daemon is on another machine, and has not been told how to reach it.</div>
              </div>
            ) : null
          }
        />
      </div>
    </div>
  );
}

// -- The board's header: the teams, their counters, the ways out. -------------

function BoardHeader() {
  const teams = useTeams();
  const which = useStore((s) => s.gauTeam);
  const launches = useStore((s) => s.seatLaunches);
  const sessions = useStore((s) => s.sessions);
  const queue = useStore((s) => s.queue);
  const sends = useStore((s) => s.teamSends);
  const tabs = useMemo(
    () =>
      teams.map((t) => ({
        id: t.id,
        label: teamTab(t, seatViews(t, launches, sessions, queue, sends.find((x) => x.team_id === t.id)), sends.find((x) => x.team_id === t.id)),
      })),
    [teams, launches, sessions, queue, sends],
  );
  const current = which ? sends.find((t) => t.team_id === which) : undefined;
  // The team's tmux session, for a terminal (`R-P8`): the daemon's grouped
  // layout, which this window does not mirror. Named on any of its seats.
  const teamSession = which ? launches.find((l) => l.team_id === which && l.team_session)?.team_session : undefined;
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-[var(--border)] px-3 py-1.5">
      <span className="text-sm font-semibold tracking-wider text-[var(--text-strong)] uppercase">Gau Geung</span>
      <button
        type="button"
        onClick={() => useStore.setState({ gauTeam: null })}
        className={cn(interactive, "rounded border px-1.5 py-px text-2xs", which === null ? "border-[var(--text-strong)] text-[var(--text-strong)]" : "border-[var(--border)] text-[var(--dim)]")}
      >
        all teams · {teams.length}
      </button>
      {tabs.map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => useStore.setState({ gauTeam: t.id })}
          className={cn(interactive, "rounded border px-1.5 py-px text-2xs", which === t.id ? "border-[var(--text-strong)] text-[var(--text-strong)]" : "border-[var(--border)] text-[var(--dim)]")}
          data-testid={`tab-${t.id}`}
        >
          {t.label}
        </button>
      ))}
      {current && <Mono className="text-2xs text-[var(--dim)]">{countersLine(current)}</Mono>}
      <span className="ml-auto flex items-center gap-1">
        {teamSession && (
          <Button
            size="sm"
            variant="outline"
            title="copy the command that attaches a terminal to the team's own tmux session — every seat tiled in one window, and a window per seat (R-P8)"
            onClick={() => void copyPath(`tmux attach -t ${teamSession}`, "the attach command")}
          >
            <Copy size={12} /> tmux attach -t {teamSession}
          </Button>
        )}
        <IconButton title="the Teams window — define, boot and stop a team (Alt+Shift+g)" onClick={() => showDialog("teams")}>
          <Users size={13} />
        </IconButton>
      </span>
    </div>
  );
}
