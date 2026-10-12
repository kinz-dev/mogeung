/**
 * Pure helpers for team definitions. `R-P3`, feature 0051.
 *
 * The window edits a `Team` as a draft and sends it whole; the daemon
 * validates, stamps and mints, and answers with the whole set. Nothing here
 * holds authority — these build the shapes the wire wants and say what the
 * daemon will say about them, so a form can show it before a click fails.
 */

import type { PermissionMode, Policy, Seat, SeatSignals, SendEntry, SessionSource, Team, TeamSends, WindDown, WindDownStatus } from "@/wire/types";
import { PERMISSION_LABEL } from "@/wire/types";

/**
 * The wind-down message a new team starts with (`R-P15`) — the daemon's
 * default, repeated here so a new team's form shows it before the first
 * save. `{note}` is replaced by the note command's name when typed.
 */
export const DEFAULT_WIND_DOWN =
  "mogeung is stopping this team. Finish nothing new. In one message, write a note for whoever picks this seat up next: " +
  "what you were doing, what is done, what is unfinished and where it stands, the decisions you made, and anything the " +
  "next session must not redo. Send it with the `{note}` command on your PATH — `{note} \"<text>\"` or on stdin — then stop. " +
  "You will be stopped once the note is in, or after a few minutes.";

/** The policy a new team starts with: the owner's numbers (three hops,
 *  sixty an hour), a handover you read first, no press, nothing declared,
 *  and three minutes to answer a wind-down. */
export function defaultPolicy(): Policy {
  return {
    hop_budget: 3,
    hourly_cap: 60,
    handover: "after_read",
    readiness_press: false,
    wakes: [],
    watchdogs: [],
    wind_down: DEFAULT_WIND_DOWN,
    wind_down_minutes: 3,
  };
}

/**
 * One line for a wind-down (`R-P15`): how far along, and how long is left —
 * or how it went, once every seat is finished.
 */
export function windDownLine(w: WindDown, now: number): string {
  const done = w.seats.filter((s) => s.status !== "asked").length;
  const noted = w.seats.filter((s) => s.status === "noted").length;
  if (done === w.seats.length) {
    const how = w.seats.map((s) => `${s.seat} ${WIND_DOWN_LABEL[s.status]}`).join(", ");
    return `wound down — ${noted} of ${w.seats.length} left a note · ${how}`;
  }
  const left = Math.max(0, w.deadline - now);
  const m = Math.floor(left / 60);
  const sec = Math.floor(left % 60);
  return `winding down · ${done} of ${w.seats.length} answered, ${noted} with a note · ${m}:${String(sec).padStart(2, "0")} left`;
}

export const WIND_DOWN_LABEL: Record<WindDownStatus, string> = {
  asked: "asked",
  noted: "left a note",
  stopped: "stopped without a note",
  timed_out: "timed out",
  gone: "already gone",
};

export function newTeam(): Team {
  return {
    id: "",
    name: "",
    repo: "",
    guidance: "",
    permission: "default",
    policy: defaultPolicy(),
    seats: [],
    paused: false,
    created: 0,
    updated: 0,
  };
}

export function newSeat(name: string, cli: SessionSource = "claude_code"): Seat {
  return { name, label: "", cli, works_in: "worktree", guidance: "", own_home: false };
}

/** A seat name that is free in this team: `seat`, `seat-2`, `seat-3`… */
export function freeSeatName(team: Team, base = "seat"): string {
  const taken = new Set(team.seats.map((s) => s.name));
  if (!taken.has(base)) return base;
  for (let n = 2; ; n++) {
    const candidate = `${base}-${n}`;
    if (!taken.has(candidate)) return candidate;
  }
}

/** The daemon's rule for a seat name, mirrored so the form can say so first. */
export function validSeatName(name: string): boolean {
  return /^[a-z][a-z0-9-]{0,31}$/.test(name);
}

/** The mode a seat is started in: its own, else the team's. */
export function permissionFor(team: Team, seat: Seat): PermissionMode {
  return seat.permission ?? team.permission;
}

/** What the daemon's `team::signals_for` says — the same sentence, so the
 *  form and the mirror agree about what a seat will have before it is booted. */
export function seatSignals(seat: Seat): string {
  switch (seat.cli) {
    case "claude_code":
      return "hooks via --settings, a status line, the screen — none touch your home";
    case "opencode":
      return "hooks via a plugin file mogeung owns, named in its inline config; the screen";
    case "qwen_code":
      return seat.own_home
        ? "hooks in the seat's own home, the screen"
        : "the screen only — hooks need a home of this seat's own (ADR-0043 clause 5)";
    case "codex":
      return "the screen only — Codex's hooks live in its config with trust hashes, not yet written even in a home of its own";
  }
}

/** `now` and `at` in unix seconds. Coarse on purpose: a seat's last word is
 *  read at a glance, not timed. */
function ago(at: number, now: number): string {
  const s = Math.max(0, Math.floor(now - at));
  if (s < 10) return "just now";
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

/**
 * One line for what a running seat's hooks have said — `R-P5`. A plain seat
 * says it is plain, so a silence on its line is not read as a hooked seat's;
 * a hooked seat says how many events, the last one and when, the context
 * fill and the model where a status line gave them, and how many calls
 * were not read.
 */
export function signalsLine(s: SeatSignals, now: number): string {
  if (s.kind === "plain") return "plain — known from its transcript and its screen only";
  const parts: string[] = [];
  if (s.events === 0) {
    parts.push("hooked — no event yet");
  } else {
    const last = s.last_event
      ? `, last ${s.last_event}${s.last_event_at !== undefined ? ` ${ago(s.last_event_at, now)}` : ""}`
      : "";
    parts.push(`hooked — ${s.events} event${s.events === 1 ? "" : "s"}${last}`);
  }
  if (s.context_used !== undefined) parts.push(`context ${s.context_used}%`);
  if (s.model) parts.push(s.model);
  if (s.unread > 0) parts.push(`${s.unread} unread`);
  return parts.join(" · ");
}

/** One line for a team's counters — `R-P6`, fence 4's *budgeted*: the chain
 *  against the hop budget, the hour against the cap, held and paused. The
 *  same words the seat's own status line shows. */
export function countersLine(s: TeamSends): string {
  const parts = [`hops ${s.chain}/${s.hop_budget}`, `${s.this_hour}/${s.hourly_cap} this hour`];
  if (s.held > 0) parts.push(`${s.held} held`);
  if (s.paused) parts.push("paused");
  return parts.join(" · ");
}

/** Who did it, for a log line: *you*, *wake*, *watchdog*, or the seat. */
export function sendBy(e: SendEntry): string {
  switch (e.kind) {
    case "human":
      return "you";
    case "wake":
      return `wake (${e.from ?? "?"})`;
    case "watchdog":
      return "watchdog";
    case "agent":
      return e.from ?? "a seat";
    case "handover":
      return e.composed_by ? `handover (${e.composed_by})` : "handover";
    case "press":
      return "press";
    case "wind_down":
      return "you (wind down)";
  }
}

/** One line of the send log — `R-P6`, fence 4's *visible*: who → whom,
 *  the text, what became of it, how long ago. The text is cut for the
 *  line; the whole rides on the entry for the title. */
export function sendLine(e: SendEntry, now: number): string {
  const text = e.text.length > 72 ? `${e.text.slice(0, 72)}…` : e.text;
  const hop = e.hop > 0 ? ` · hop ${e.hop}` : "";
  const note = e.status !== "delivered" && e.status !== "released" && e.note ? ` — ${e.note}` : "";
  return `${sendBy(e)} → ${e.to} “${text}” · ${e.status}${hop}${note} · ${ago(e.at, now)}`;
}

/** The entries that concern one seat: sent to it, or by it. Newest last. */
export function sendsFor(log: SendEntry[], seat: string): SendEntry[] {
  return log.filter((e) => e.to === seat || e.from === seat);
}

/** What the daemon would refuse, in its words, or `null`. The daemon's
 *  `validate` is the authority; this is the subset a form can answer while
 *  you type, so Save is grey with a reason rather than red after a click. */
export function firstProblem(team: Team): string | null {
  if (!team.name.trim()) return "a team needs a name";
  if (team.repo.trim() && !team.repo.startsWith("/")) return "the repository must be an absolute path";
  if (team.policy.hourly_cap < 1) return "the hourly cap must be at least one — pause the team to stop every send";
  if (!team.policy.wind_down.trim()) return "the wind-down message is empty — a seat asked for nothing writes nothing";
  if (team.policy.wind_down_minutes < 1 || team.policy.wind_down_minutes > 60) return "a wind-down waits between one and sixty minutes";
  const seen = new Set<string>();
  for (const s of team.seats) {
    if (!validSeatName(s.name)) return `seat name \`${s.name}\` must be lower-case letters, digits and dashes, starting with a letter`;
    if (seen.has(s.name)) return `two seats are named \`${s.name}\``;
    seen.add(s.name);
  }
  for (const w of team.policy.wakes) {
    if (!seen.has(w.on_seat)) return `a wake watches \`${w.on_seat}\`, which is not a seat of this team`;
    if (!seen.has(w.to)) return `a wake addresses \`${w.to}\`, which is not a seat of this team`;
    if (w.on_seat === w.to) return `a wake on \`${w.to}\` would send to itself`;
    if (!w.text.trim()) return `a wake from \`${w.on_seat}\` has no text to send`;
  }
  for (const d of team.policy.watchdogs) {
    if (d.silent_minutes < 1) return "a watchdog needs at least one silent minute";
    if (d.seat && !seen.has(d.seat)) return `a watchdog names \`${d.seat}\`, which is not a seat of this team`;
    if (!d.text.trim()) return "a watchdog has no text to send";
  }
  return null;
}

/** One line for a team's row: seats and their CLIs, and whether it is paused. */
export function teamSummary(team: Team): string {
  const n = team.seats.length;
  const seats = n === 1 ? "1 seat" : `${n} seats`;
  return team.paused ? `${seats} · paused` : seats;
}

/** The label a permission picker shows, with the team's own marked. */
export function permissionLabel(mode: PermissionMode): string {
  return PERMISSION_LABEL[mode];
}
