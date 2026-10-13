/**
 * The helpers behind the Teams window say what the daemon will say. `R-P3`.
 *
 * `firstProblem` is a mirror of `team::validate` in the daemon, and the point
 * of pinning it is the mirror staying true: a form that greys Save for a
 * reason the daemon would accept, or accepts one the daemon refuses, is a
 * form you stop trusting.
 */

import { describe, expect, it } from "vitest";
import { DEFAULT_WIND_DOWN, countersLine, effortProblem, effortsFor, firstProblem, freeSeatName, newSeat, newTeam, permissionFor, seatSignals, sendBy, sendLine, sendsFor, signalsLine, teamSummary, validSeatName, windDownLine, withModel } from "./teams";

describe("team helpers", () => {
  it("starts a team with the owner's numbers", () => {
    const t = newTeam();
    expect(t.policy.hop_budget).toBe(3);
    expect(t.policy.hourly_cap).toBe(60);
    expect(t.permission).toBe("default");
    expect(t.id).toBe("");
    // `R-P15`: three minutes to answer a wind-down, and a message that names the command.
    expect(t.policy.wind_down_minutes).toBe(3);
    expect(t.policy.wind_down).toBe(DEFAULT_WIND_DOWN);
    expect(DEFAULT_WIND_DOWN).toContain("{note}");
  });

  /** `R-P15`. The strip's one line while a wind-down runs, and after. */
  it("says how a wind-down is going, and how it went", () => {
    const now = 1_000_000;
    expect(
      windDownLine({ started: now - 30, deadline: now + 125, seats: [{ seat: "a", status: "asked", at: now - 30 }, { seat: "b", status: "noted", at: now - 2, note: "x" }, { seat: "c", status: "gone", at: now - 30 }] }, now),
    ).toBe("winding down · 2 of 3 answered, 1 with a note · 2:05 left");
    expect(windDownLine({ started: now - 300, deadline: now - 120, seats: [{ seat: "a", status: "timed_out", at: now - 120 }, { seat: "b", status: "stopped", at: now - 200 }] }, now)).toBe(
      "wound down — 0 of 2 left a note · a timed out, b stopped without a note",
    );
    // Past the deadline but a seat still asked: no negative clock.
    expect(windDownLine({ started: 0, deadline: now - 5, seats: [{ seat: "a", status: "asked", at: 0 }] }, now)).toContain("0:00 left");
    expect(sendBy({ id: "1", team_id: "t", at: 0, kind: "wind_down", to: "a", text: "", why: "", status: "delivered", hop: 0 })).toBe("you (wind down)");
    // The mirror of `team::validate`'s two new refusals.
    const t = newTeam();
    t.name = "x";
    expect(firstProblem({ ...t, policy: { ...t.policy, wind_down: "  " } })).toContain("wind-down message is empty");
    expect(firstProblem({ ...t, policy: { ...t.policy, wind_down_minutes: 0 } })).toContain("between one and sixty minutes");
    expect(firstProblem({ ...t, policy: { ...t.policy, wind_down_minutes: 61 } })).toContain("between one and sixty minutes");
  });

  it("names a seat the way the daemon addresses one", () => {
    expect(validSeatName("reviewer")).toBe(true);
    expect(validSeatName("builder-2")).toBe(true);
    expect(validSeatName("Reviewer")).toBe(false);
    expect(validSeatName("2nd")).toBe(false);
    expect(validSeatName("")).toBe(false);
    const t = { ...newTeam(), seats: [newSeat("seat"), newSeat("seat-2")] };
    expect(freeSeatName(t)).toBe("seat-3");
    expect(freeSeatName(newTeam())).toBe("seat");
  });

  it("refuses what the daemon refuses, in its words", () => {
    expect(firstProblem(newTeam())).toBe("a team needs a name");
    const t = { ...newTeam(), name: "t", seats: [newSeat("a"), newSeat("b")] };
    expect(firstProblem(t)).toBeNull();
    expect(firstProblem({ ...t, repo: "relative" })).toContain("absolute");
    expect(firstProblem({ ...t, seats: [newSeat("a"), newSeat("a")] })).toContain("two seats are named `a`");
    expect(firstProblem({ ...t, policy: { ...t.policy, hourly_cap: 0 } })).toContain("hourly cap");
    expect(firstProblem({ ...t, policy: { ...t.policy, wakes: [{ on_seat: "a", on: "stop", to: "zz", text: "go" }] } })).toContain("`zz`");
    expect(firstProblem({ ...t, policy: { ...t.policy, wakes: [{ on_seat: "a", on: "stop", to: "a", text: "go" }] } })).toContain("itself");
    expect(firstProblem({ ...t, policy: { ...t.policy, watchdogs: [{ silent_minutes: 0, while_busy: true, text: "?" }] } })).toContain("silent minute");
  });

  it("reads the inherited mode as the team's and the signals from the cli and the home", () => {
    const t = { ...newTeam(), name: "t", permission: "accept_edits" as const };
    const plain = newSeat("a", "codex");
    expect(permissionFor(t, plain)).toBe("accept_edits");
    expect(permissionFor(t, { ...plain, permission: "plan" })).toBe("plan");
    expect(seatSignals(plain)).toContain("screen only");
    // `R-P5`: Codex stays plain even in a home of its own, and says why;
    // Qwen's hooks need the home and get them there.
    expect(seatSignals({ ...plain, own_home: true })).toContain("screen only");
    expect(seatSignals({ ...newSeat("q", "qwen_code"), own_home: true })).toContain("own home");
    expect(seatSignals(newSeat("q", "qwen_code"))).toContain("screen only");
    expect(seatSignals(newSeat("c"))).toContain("--settings");
    expect(seatSignals(newSeat("o", "opencode"))).toContain("plugin");
    expect(teamSummary({ ...t, seats: [plain], paused: true })).toBe("1 seat · paused");
  });

  /** `R-P5`. The line under a running seat: a plain seat says so, so its
   *  silence is never read as a hooked seat's; a hooked seat counts. */
  it("says what a running seat's hooks have said, or that it is plain", () => {
    const now = 1_000_000;
    expect(signalsLine({ kind: "plain", events: 0, unread: 0 }, now)).toMatch(/^plain — /);
    expect(signalsLine({ kind: "hooked", events: 0, unread: 0 }, now)).toBe("hooked — no event yet");
    expect(
      signalsLine({ kind: "hooked", events: 1, last_event: "SessionStart", last_event_at: now - 5, unread: 0 }, now),
    ).toBe("hooked — 1 event, last SessionStart just now");
    expect(
      signalsLine(
        { kind: "hooked", events: 12, last_event: "Stop", last_event_at: now - 125, context_used: 43, model: "Haiku 4.5", unread: 2 },
        now,
      ),
    ).toBe("hooked — 12 events, last Stop 2m ago · context 43% · Haiku 4.5 · 2 unread");
    expect(signalsLine({ kind: "hooked", events: 3, last_event: "Stop", last_event_at: now - 7200, unread: 0 }, now)).toContain("2h ago");
  });

  /** `R-P6`. Fence 4 on screen: the counters against their budgets, and a
   *  log line that says who, whom, what, what became of it and when. */
  it("renders a team's counters and its send log lines", () => {
    const now = 2_000_000;
    const sends = { team_id: "t1", team: "port", chain: 2, hop_budget: 3, this_hour: 5, hourly_cap: 60, paused: false, held: 0, log: [] };
    expect(countersLine(sends)).toBe("hops 2/3 · 5/60 this hour");
    expect(countersLine({ ...sends, held: 1, paused: true })).toBe("hops 2/3 · 5/60 this hour · 1 held · paused");
    const agent = { id: "s1", team_id: "t1", at: now - 90, kind: "agent" as const, from: "builder", to: "planner", text: "tests pass", why: "`builder` said so", status: "delivered" as const, hop: 2 };
    expect(sendLine(agent, now)).toBe("builder → planner “tests pass” · delivered · hop 2 · 1m ago");
    const held = { ...agent, id: "s2", status: "held" as const, hop: 0, note: "the hop budget of 3 is spent" };
    expect(sendLine(held, now)).toBe("builder → planner “tests pass” · held — the hop budget of 3 is spent · 1m ago");
    const wake = { ...agent, id: "s3", kind: "wake" as const, from: "planner", to: "builder", text: "x".repeat(100), hop: 1 };
    expect(sendLine(wake, now)).toMatch(/^wake \(planner\) → builder “x{72}…” · delivered · hop 1/);
    const you = { ...agent, id: "s4", kind: "human" as const, from: undefined, hop: 0, why: "you pressed send" };
    expect(sendLine(you, now)).toMatch(/^you → planner/);
    const dog = { ...agent, id: "s5", kind: "watchdog" as const, from: undefined, to: "builder", hop: 0 };
    expect(sendLine(dog, now)).toMatch(/^watchdog → builder/);
    const hand = { ...agent, id: "s6", kind: "handover" as const, from: undefined, to: "builder", hop: 1, composed_by: "qwen3 @ localhost" };
    expect(sendLine(hand, now)).toMatch(/^handover \(qwen3 @ localhost\) → builder/);
    expect(sendsFor([agent, held, wake, dog], "builder").map((e) => e.id)).toEqual(["s1", "s2", "s3", "s5"]);
    expect(sendsFor([agent, held, wake, dog], "planner").map((e) => e.id)).toEqual(["s1", "s2", "s3"]);
  });
});

// `R-P16`: the same refusals the daemon makes, before a click fails.
describe("a seat's thinking level", () => {
  it("is refused where its CLI cannot take it, in the daemon's words", () => {
    expect(effortProblem({ ...newSeat("editor"), effort: "high" })).toBeNull();
    expect(effortProblem({ ...newSeat("editor"), effort: "ultra" })).toContain("has no `ultra` thinking level");
    expect(effortProblem({ ...newSeat("tester", "qwen_code"), effort: "low" })).toContain("takes no thinking level at launch");
    expect(effortProblem({ ...newSeat("report", "opencode"), effort: "high" })).toContain("name the model too");
    expect(effortProblem({ ...newSeat("report", "opencode"), effort: "high", model: "anthropic/claude-opus-5-5" })).toBeNull();
    const t = { ...newTeam(), name: "newsroom", seats: [{ ...newSeat("tester", "qwen_code"), effort: "low" as const }] };
    expect(firstProblem(t)).toContain("qwen takes no thinking level");
  });

  it("falls back to the CLI's levels when the catalogue has not come", () => {
    expect(effortsFor([], newSeat("editor"))).toEqual(["low", "medium", "high", "xhigh", "max"]);
    expect(effortsFor([], newSeat("tester", "qwen_code"))).toEqual([]);
  });

  it("is dropped by a switch to a CLI that cannot take it, and kept by one that can", () => {
    const s = { ...newSeat("editor"), effort: "max" as const };
    expect(withModel([], s, { cli: "qwen_code" }).effort).toBeUndefined();
    expect(withModel([], s, { cli: "codex" }).effort).toBe("max");
  });
});
