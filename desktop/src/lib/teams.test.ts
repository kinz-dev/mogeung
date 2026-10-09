/**
 * The helpers behind the Teams window say what the daemon will say. `R-P3`.
 *
 * `firstProblem` is a mirror of `team::validate` in the daemon, and the point
 * of pinning it is the mirror staying true: a form that greys Save for a
 * reason the daemon would accept, or accepts one the daemon refuses, is a
 * form you stop trusting.
 */

import { describe, expect, it } from "vitest";
import { firstProblem, freeSeatName, newSeat, newTeam, permissionFor, seatSignals, teamSummary, validSeatName } from "./teams";

describe("team helpers", () => {
  it("starts a team with the owner's numbers", () => {
    const t = newTeam();
    expect(t.policy.hop_budget).toBe(3);
    expect(t.policy.hourly_cap).toBe(60);
    expect(t.permission).toBe("default");
    expect(t.id).toBe("");
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
    expect(seatSignals({ ...plain, own_home: true })).toContain("own home");
    expect(seatSignals(newSeat("c"))).toContain("--settings");
    expect(teamSummary({ ...t, seats: [plain], paused: true })).toBe("1 seat · paused");
  });
});
