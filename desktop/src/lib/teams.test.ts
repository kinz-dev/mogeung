/**
 * The helpers behind the Teams window say what the daemon will say. `R-P3`.
 *
 * `firstProblem` is a mirror of `team::validate` in the daemon, and the point
 * of pinning it is the mirror staying true: a form that greys Save for a
 * reason the daemon would accept, or accepts one the daemon refuses, is a
 * form you stop trusting.
 */

import { describe, expect, it } from "vitest";
import { firstProblem, freeSeatName, newSeat, newTeam, permissionFor, seatSignals, signalsLine, teamSummary, validSeatName } from "./teams";

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
});
