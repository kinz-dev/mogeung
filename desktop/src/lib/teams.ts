/**
 * Pure helpers for team definitions. `R-P3`, feature 0051.
 *
 * The window edits a `Team` as a draft and sends it whole; the daemon
 * validates, stamps and mints, and answers with the whole set. Nothing here
 * holds authority — these build the shapes the wire wants and say what the
 * daemon will say about them, so a form can show it before a click fails.
 */

import type { PermissionMode, Policy, Seat, SessionSource, Team } from "@/wire/types";
import { PERMISSION_LABEL } from "@/wire/types";

/** The policy a new team starts with: the owner's numbers (three hops,
 *  sixty an hour), a handover you read first, no press, nothing declared. */
export function defaultPolicy(): Policy {
  return { hop_budget: 3, hourly_cap: 60, handover: "after_read", readiness_press: false, wakes: [], watchdogs: [] };
}

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
      return "hooks via a plugin in a directory mogeung owns, the screen";
    case "codex":
    case "qwen_code":
      return seat.own_home
        ? "hooks in the seat's own home, the screen"
        : "the screen only — hooks need a home of this seat's own (ADR-0043 clause 5)";
  }
}

/** What the daemon would refuse, in its words, or `null`. The daemon's
 *  `validate` is the authority; this is the subset a form can answer while
 *  you type, so Save is grey with a reason rather than red after a click. */
export function firstProblem(team: Team): string | null {
  if (!team.name.trim()) return "a team needs a name";
  if (team.repo.trim() && !team.repo.startsWith("/")) return "the repository must be an absolute path";
  if (team.policy.hourly_cap < 1) return "the hourly cap must be at least one — pause the team to stop every send";
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
