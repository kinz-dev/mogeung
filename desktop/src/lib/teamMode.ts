/**
 * The team mode's arithmetic. `R-P7`, feature 0051.
 *
 * Pure functions over what the store already holds — teams, launch records,
 * sessions, the queue, the send logs — so the mode fetches nothing and the
 * tests need no daemon. The decisions of 2026-10-09 are the shape: OpenRig's
 * five lanes for seats in a team, each card carrying mogeung's badge inside;
 * topology as a graph and a table of the same seats; a seat's state naming
 * its source.
 *
 * **The lanes are read off the send log, not guessed.** OpenRig's *parked
 * with baton* and *blocked on agents* are facts about who last told whom
 * what: a seat idle after a message arrived holds the baton and has not
 * passed it on; a seat idle after sending one is waiting on the seat it sent
 * to. Both are idle, both would rank `WAITING` on the queue, and the lane is
 * what tells them apart — which is the thing the mode shows that the queue
 * cannot.
 */

import type { AttentionItem, Seat, SeatLaunch, SendEntry, Session, Team, TeamSends } from "@/wire/types";
import { isScreenGate, needsHuman, reasonLabel } from "@/wire/types";

export type Lane = "needs_you" | "parked" | "blocked" | "now" | "finished";

/** OpenRig's five, in the order the pulse draws them. */
export const LANES: readonly { id: Lane; label: string; title: string }[] = [
  { id: "needs_you", label: "Needs you", title: "an approval, an error, a limit, a held message, or a seat idle with nothing in hand" },
  { id: "parked", label: "Parked with baton", title: "idle after a message arrived — holds the baton and has not passed it on" },
  { id: "blocked", label: "Blocked on agents", title: "idle after sending — waiting on the seat it told" },
  { id: "now", label: "Now", title: "working" },
  { id: "finished", label: "Just finished", title: "exited, or stopped; the worktree stays" },
];

/** Where a seat's state was read from — every state names its source. */
export type StateSource = "hook" | "plugin" | "transcript" | "screen" | "none";

export interface SeatView {
  team: Team;
  seat: Seat;
  launch: SeatLaunch | undefined;
  session: Session | undefined;
  item: AttentionItem | undefined;
  lane: Lane;
  /** One line under the name: the reason, the baton, the activity. */
  line: string;
  /** mogeung's own badge — `APPROVE`, `WAITING`, `running` — or none for a seat not running. */
  badge: string | null;
  source: StateSource;
  /** Whole percent of the context window, where a status line gave one. */
  context: number | undefined;
  /** Deliveries from and to this seat, on the team's log. */
  sent: number;
  received: number;
  /** When anything last happened, unix seconds, or `undefined` for a seat never started. */
  lastAt: number | undefined;
  /** The seat this one waits on, in the blocked and parked lanes. */
  other: string | undefined;
}

/** `team_id/seat`, the key the mode selects by. */
export function seatKey(teamId: string, seat: string): string {
  return `${teamId}/${seat}`;
}

export function parseSeatKey(key: string): { teamId: string; seat: string } | null {
  const i = key.indexOf("/");
  if (i <= 0) return null;
  return { teamId: key.slice(0, i), seat: key.slice(i + 1) };
}

// -- The mode's own tile tree: pane ids. --------------------------------------

export const BOARD_PANE = "board";
const SEAT_PREFIX = "seat:";
const TERM_PREFIX = "term:";

export function seatPaneId(key: string): string {
  return `${SEAT_PREFIX}${key}`;
}
export function termPaneId(key: string): string {
  return `${TERM_PREFIX}${key}`;
}
export function seatOfPane(id: string): string | null {
  return id.startsWith(SEAT_PREFIX) ? id.slice(SEAT_PREFIX.length) : null;
}
export function termOfPane(id: string): string | null {
  return id.startsWith(TERM_PREFIX) ? id.slice(TERM_PREFIX.length) : null;
}
/** Where a machine's arrangement of the mode is kept, beside the shells'. */
export function teamLayoutKey(machineId: string | null | undefined): string {
  return `mogeung.teamLayout:${machineId ?? "unknown"}`;
}

/**
 * The session row a launch has now: the chosen id's where the CLI took one,
 * else the row tagged with this seat — a live one first, so a seat restarted
 * into a fresh session is read from the fresh one.
 */
export function sessionOf(launch: SeatLaunch, sessions: Record<string, Session>): Session | undefined {
  const tagged = Object.values(sessions).filter((s) => s.seat?.team_id === launch.team_id && s.seat?.seat === launch.seat);
  const live = tagged.find((s) => s.alive);
  if (live) return live;
  if (launch.session_id && sessions[launch.session_id]) return sessions[launch.session_id];
  return tagged[0];
}

/** The entries that concern one seat, oldest first. */
function involving(log: readonly SendEntry[], seat: string): SendEntry[] {
  return log.filter((e) => e.to === seat || e.from === seat);
}

function lastDelivered(mine: readonly SendEntry[]): SendEntry | undefined {
  for (let i = mine.length - 1; i >= 0; i--) {
    const e = mine[i];
    if (e.status === "delivered" || e.status === "released") return e;
  }
  return undefined;
}

/** Which lane, and the line that says why. */
export function laneFor(
  seat: string,
  session: Session | undefined,
  item: AttentionItem | undefined,
  mine: readonly SendEntry[],
): { lane: Lane; line: string; other?: string } {
  if (!session) return { lane: "finished", line: "not running" };
  if (!session.alive) return { lane: "finished", line: "exited — the worktree stays" };
  const held = session.held_send;
  if (held) {
    return {
      lane: "needs_you",
      line: held.count > 1 ? `${held.count} sends held — ${held.why}` : `a send held — ${held.why}`,
    };
  }
  const reason = item?.reason;
  if (reason && needsHuman(reason) && reason !== "awaiting_input") {
    return { lane: "needs_you", line: item?.detail || reasonLabel(reason) };
  }
  const idle = session.live_status === "idle" || reason === "awaiting_input";
  if (idle) {
    const last = lastDelivered(mine);
    if (last && last.to === seat && last.from) {
      return { lane: "parked", line: `holds the baton from ${last.from} — idle, not passed on`, other: last.from };
    }
    if (last && last.from === seat) {
      return { lane: "blocked", line: `waiting on ${last.to}`, other: last.to };
    }
    return { lane: "needs_you", line: item?.detail || "waiting for you" };
  }
  return { lane: "now", line: session.last_activity || item?.detail || "working" };
}

/** Where the state came from: a gate read off the pane (`R-P11`) when the
 *  queue's reason is that reading, else a hook that has spoken, a plugin,
 *  else the transcript. A hook's fact outranks a screen's reading, so the
 *  screen is the source only when the detail says *(screen)*. */
export function sourceOf(launch: SeatLaunch | undefined, session: Session | undefined, item?: AttentionItem): StateSource {
  if (!launch) return "none";
  if (session?.screen && isScreenGate(session.screen.shape) && item?.detail.endsWith("(screen)")) return "screen";
  if (launch.signals.kind === "hooked") {
    if (launch.cli === "opencode") return "plugin";
    return launch.signals.events > 0 ? "hook" : "transcript";
  }
  return session ? "transcript" : "none";
}

const toUnix = (iso: string | null | undefined): number | undefined => {
  if (!iso) return undefined;
  const t = Date.parse(iso);
  return Number.isFinite(t) ? Math.floor(t / 1000) : undefined;
};

/** One view per seat of the team, in the definition's order. */
export function seatViews(
  team: Team,
  launches: readonly SeatLaunch[],
  sessions: Record<string, Session>,
  queue: readonly AttentionItem[],
  sends: TeamSends | undefined,
): SeatView[] {
  const log = sends?.log ?? [];
  return team.seats.map((seat) => {
    const launch = launches.find((l) => l.team_id === team.id && l.seat === seat.name);
    const session = launch ? sessionOf(launch, sessions) : undefined;
    const item = session ? queue.find((q) => q.session_id === session.id) : undefined;
    const mine = involving(log, seat.name);
    const { lane, line, other } = laneFor(seat.name, session, item, mine);
    const delivered = mine.filter((e) => e.status === "delivered" || e.status === "released");
    const times = [toUnix(session?.last_event_at), launch?.signals.last_event_at, launch?.started].filter(
      (t): t is number => typeof t === "number",
    );
    return {
      team,
      seat,
      launch,
      session,
      item,
      lane,
      line,
      badge: session && item ? reasonLabel(item.reason) : null,
      source: sourceOf(launch, session, item),
      context: launch?.signals.context_used,
      sent: delivered.filter((e) => e.from === seat.name).length,
      received: delivered.filter((e) => e.to === seat.name).length,
      lastAt: times.length ? Math.max(...times) : undefined,
      other,
    };
  });
}

/** An edge of the graph: the last send between two seats, in that direction. */
export interface Edge {
  from: string;
  to: string;
  entry: SendEntry;
}

/**
 * The last delivery, or hold, between each ordered pair — a held edge is a
 * message waiting for you on the edge. A watchdog's nudge and your own send
 * have no seat at the tail and draw nothing.
 */
export function edges(log: readonly SendEntry[]): Edge[] {
  const last = new Map<string, SendEntry>();
  for (const e of log) {
    if (!e.from) continue;
    if (e.status !== "delivered" && e.status !== "released" && e.status !== "held") continue;
    last.set(`${e.from}→${e.to}`, e);
  }
  return [...last.values()].map((entry) => ({ from: entry.from!, to: entry.to, entry }));
}

/** Where to draw N nodes: on a circle, in the order given, so a seat keeps its place. */
export function ring(n: number, w: number, h: number, pad = 44): { x: number; y: number }[] {
  const cx = w / 2;
  const cy = h / 2;
  const r = Math.max(10, Math.min(w, h) / 2 - pad);
  if (n === 1) return [{ x: cx, y: cy }];
  return Array.from({ length: n }, (_, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / n;
    return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) };
  });
}

/** The chip on a team's tab: its name, how many seats run, how many want you, paused. */
export function teamTab(team: Team, views: readonly SeatView[], sends: TeamSends | undefined): string {
  const running = views.filter((v) => v.session?.alive).length;
  const needs = views.filter((v) => v.lane === "needs_you").length;
  const parts = [`${team.name}`, `${running}/${team.seats.length}`];
  if (needs > 0) parts.push(`${needs} need${needs === 1 ? "s" : ""} you`);
  if (sends?.paused ?? team.paused) parts.push("paused");
  return parts.join(" · ");
}

/** Unix seconds to a coarse *ago*. */
export function agoShort(at: number | undefined, now: number): string {
  if (at === undefined) return "—";
  const s = Math.max(0, Math.floor(now - at));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}
