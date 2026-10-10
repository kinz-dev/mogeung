/**
 * The team mode's arithmetic. `R-P7`.
 *
 * The lanes are the thing to pin: two idle seats rank `WAITING` alike on the
 * queue, and only the send log says which holds the baton and which waits on
 * a teammate. A test that got them backwards would still render five tidy
 * columns.
 */

import { describe, expect, it } from "vitest";
import type { AttentionItem, SeatLaunch, SendEntry, Session, Team } from "@/wire/types";
import { newSeat, newTeam } from "./teams";
import { edges, laneFor, parseSeatKey, ring, seatKey, seatViews, sessionOf, sourceOf, teamTab } from "./teamMode";

const team = (): Team => ({
  ...newTeam(),
  id: "t1",
  name: "port",
  repo: "/r",
  seats: [newSeat("planner"), newSeat("builder", "codex"), newSeat("tester", "opencode")],
});

const launch = (seat: string, over: Partial<SeatLaunch> = {}): SeatLaunch => ({
  team_id: "t1",
  team: "port",
  seat,
  cli: "claude_code",
  pane_id: `%${seat}`,
  tmux_target: `geung-port-${seat}:0.0`,
  tmux_session: `geung-port-${seat}`,
  dir: `/w/${seat}`,
  worktree: true,
  files_dir: "/f",
  started: 1_000,
  signals: { kind: "hooked", events: 2, unread: 0 },
  ...over,
});

const session = (id: string, seat: string, over: Partial<Session> = {}): Session =>
  ({
    id,
    alive: true,
    live_status: "busy",
    last_event_at: new Date(2_000 * 1000).toISOString(),
    last_activity: "Edit src/seat.rs",
    seat: { team_id: "t1", team: "port", seat, signals: "hooked" },
    ...over,
  }) as unknown as Session;

const item = (id: string, reason: AttentionItem["reason"], detail = ""): AttentionItem => ({ session_id: id, reason, score: 0, detail });

const delivered = (id: string, from: string | undefined, to: string, at = 10): SendEntry => ({
  id,
  team_id: "t1",
  at,
  kind: from ? "agent" : "watchdog",
  from,
  to,
  text: "x",
  why: "test",
  status: "delivered",
  hop: from ? 1 : 0,
});

describe("the lanes", () => {
  it("puts a seat with an approval, an error or a held send under needs you", () => {
    const s = session("a", "planner", { live_status: "idle" });
    expect(laneFor("planner", s, item("a", "awaiting_permission", "needs approval for Bash"), []).lane).toBe("needs_you");
    expect(laneFor("planner", s, item("a", "awaiting_permission", "needs approval for Bash"), []).line).toBe("needs approval for Bash");
    expect(laneFor("planner", session("a", "planner", { error: "boom" } as never), item("a", "failed", "boom"), []).lane).toBe("needs_you");
    const held = session("a", "planner", { held_send: { count: 2, why: "the hop budget of 3 is spent", since: "2026-10-10T00:00:00Z" } });
    const r = laneFor("planner", held, item("a", "awaiting_input"), []);
    expect(r.lane).toBe("needs_you");
    expect(r.line).toBe("2 sends held — the hop budget of 3 is spent");
  });

  it("tells parked from blocked by who last told whom", () => {
    const idle = session("a", "planner", { live_status: "idle" });
    const waiting = item("a", "awaiting_input", "waiting for you");
    // Nothing in hand: idle is waiting for you.
    expect(laneFor("planner", idle, waiting, []).lane).toBe("needs_you");
    // A message arrived and it went idle without passing it on: parked with the baton.
    const got = laneFor("planner", idle, waiting, [delivered("s1", "builder", "planner")]);
    expect(got.lane).toBe("parked");
    expect(got.other).toBe("builder");
    expect(got.line).toContain("holds the baton from builder");
    // It told builder and went idle: blocked on builder.
    const told = laneFor("planner", idle, waiting, [delivered("s1", "builder", "planner", 5), delivered("s2", "planner", "builder", 9)]);
    expect(told.lane).toBe("blocked");
    expect(told.other).toBe("builder");
    expect(told.line).toBe("waiting on builder");
    // A watchdog's nudge is not a baton.
    expect(laneFor("planner", idle, waiting, [delivered("s3", undefined, "planner")]).lane).toBe("needs_you");
    // A held entry is not a delivery: the last *delivered* one decides.
    const heldLater: SendEntry = { ...delivered("s4", "planner", "builder", 20), status: "held" };
    expect(laneFor("planner", idle, waiting, [delivered("s1", "builder", "planner", 5), heldLater]).lane).toBe("parked");
  });

  it("puts a busy seat under now, and a dead or unstarted one under just finished", () => {
    expect(laneFor("planner", session("a", "planner"), item("a", "running", "working"), []).lane).toBe("now");
    expect(laneFor("planner", session("a", "planner"), item("a", "running"), []).line).toBe("Edit src/seat.rs");
    expect(laneFor("planner", session("a", "planner", { alive: false }), undefined, []).lane).toBe("finished");
    expect(laneFor("planner", undefined, undefined, []).lane).toBe("finished");
    expect(laneFor("planner", undefined, undefined, []).line).toBe("not running");
  });
});

describe("the views", () => {
  it("finds a seat's live row by its tag before its chosen id, and counts its sends", () => {
    const t = team();
    const launches = [launch("planner", { session_id: "old" }), launch("builder", { cli: "codex", signals: { kind: "plain", events: 0, unread: 0 } })];
    const sessions = {
      old: session("old", "planner", { alive: false }),
      fresh: session("fresh", "planner"),
      b: session("b", "builder", { live_status: "idle" }),
    };
    expect(sessionOf(launches[0], sessions)?.id).toBe("fresh");
    expect(sessionOf(launch("tester"), sessions)).toBeUndefined();
    const queue = [item("fresh", "running", "working"), item("b", "awaiting_input", "waiting for you")];
    const sends = {
      team_id: "t1", team: "port", chain: 1, hop_budget: 3, this_hour: 1, hourly_cap: 60, paused: false, held: 0,
      log: [delivered("s1", "planner", "builder", 1_500), { ...delivered("s2", "builder", "planner", 1_600), status: "refused" as const }],
    };
    const views = seatViews(t, launches, sessions, queue, sends);
    expect(views.map((v) => v.seat.name)).toEqual(["planner", "builder", "tester"]);
    expect(views[0]).toMatchObject({ lane: "now", badge: "running", source: "hook", sent: 1, received: 0, lastAt: 2_000 });
    expect(views[1]).toMatchObject({ lane: "parked", badge: "WAITING", source: "transcript", sent: 0, received: 1, other: "planner" });
    expect(views[2]).toMatchObject({ lane: "finished", badge: null, source: "none", launch: undefined, lastAt: undefined });
    expect(teamTab(t, views, sends)).toBe("port · 2/3");
    // The planner goes idle after telling the builder: blocked on it; the builder still holds the baton.
    const idle = seatViews(t, launches, { ...sessions, fresh: session("fresh", "planner", { live_status: "idle" }) }, [item("fresh", "awaiting_input", "waiting for you")], sends);
    expect(idle.map((v) => v.lane)).toEqual(["blocked", "parked", "finished"]);
    expect(teamTab({ ...t, paused: true }, idle, undefined)).toBe("port · 2/3 · paused");
    // With no log at all, both idle seats need you.
    const bare = seatViews(t, launches, { ...sessions, fresh: session("fresh", "planner", { live_status: "idle" }) }, [item("fresh", "awaiting_input", "waiting for you")], undefined);
    expect(teamTab(t, bare, undefined)).toBe("port · 2/3 · 2 need you");
  });

  it("names a state's source: a hook that has spoken, a plugin, else the transcript", () => {
    const s = session("a", "planner");
    expect(sourceOf(launch("planner"), s)).toBe("hook");
    expect(sourceOf(launch("planner", { signals: { kind: "hooked", events: 0, unread: 0 } }), s)).toBe("transcript");
    expect(sourceOf(launch("tester", { cli: "opencode" }), s)).toBe("plugin");
    expect(sourceOf(launch("builder", { cli: "codex", signals: { kind: "plain", events: 0, unread: 0 } }), s)).toBe("transcript");
    expect(sourceOf(undefined, s)).toBe("none");
  });
});

describe("the graph", () => {
  it("keeps the last send per direction, held ones included, and draws nothing for a nudge", () => {
    const log: SendEntry[] = [
      delivered("s1", "planner", "builder", 1),
      delivered("s2", "builder", "planner", 2),
      { ...delivered("s3", "planner", "builder", 3), status: "held" },
      delivered("s4", undefined, "builder", 4),
      { ...delivered("s5", "builder", "tester", 5), status: "refused" },
    ];
    const e = edges(log);
    expect(e.map((x) => `${x.from}→${x.to}:${x.entry.id}`)).toEqual(["planner→builder:s3", "builder→planner:s2"]);
    expect(e[0].entry.status).toBe("held");
    expect(ring(1, 100, 100)).toEqual([{ x: 50, y: 50 }]);
    const four = ring(4, 200, 200);
    expect(four).toHaveLength(4);
    expect(four[0].y).toBeLessThan(four[2].y);
    expect(seatKey("t1", "planner")).toBe("t1/planner");
    expect(parseSeatKey("t1/planner")).toEqual({ teamId: "t1", seat: "planner" });
    expect(parseSeatKey("nope")).toBeNull();
  });
});
