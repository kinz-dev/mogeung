/**
 * The Teams window: what it asks, what it sends, and what it refuses to send.
 * `R-P3`, feature 0051.
 *
 * Pinned here is the shape on the wire — a new team goes up with no id and
 * the owner's defaults, a seat goes up with the team's mode as an absence —
 * and the two reasons Save is grey, because a settings window that silently
 * does nothing is the failure this family of windows is worst at.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import type { ClientMsg, Team } from "@/wire/types";
import { newSeat, newTeam } from "@/lib/teams";
import { TeamsWindow } from "./TeamsWindow";

const sent: ClientMsg[] = [];

const team = (over: Partial<Team> = {}): Team => ({
  ...newTeam(),
  id: "t1",
  name: "port-openrig",
  repo: "/home/you/mogeung",
  seats: [newSeat("planner"), { ...newSeat("builder", "codex"), permission: "plan" }],
  created: 1,
  updated: 2,
  ...over,
});

const arrives = (teams: Team[], allowed?: boolean) =>
  act(() => useStore.setState(allowed === undefined ? { teams } : { teams, teamsAllowed: allowed }));

const lastSave = (): Team => {
  const m = [...sent].reverse().find((x) => x.cmd === "team_save");
  if (!m || m.cmd !== "team_save") throw new Error("no team_save was sent");
  return m.team;
};

describe("the Teams window", () => {
  beforeEach(() => {
    sent.length = 0;
    useStore.setState({ showTeams: true, teams: [], teamsAllowed: true, send: (m) => void sent.push(m) });
  });

  it("asks the daemon for the teams every time it opens", () => {
    render(<TeamsWindow />);
    expect(sent).toEqual([{ cmd: "team_list" }]);
  });

  it("lists the teams and shows the picked one's definition", () => {
    render(<TeamsWindow />);
    arrives([team()]);
    expect(screen.getByText("port-openrig")).toBeInTheDocument();
    expect(screen.getByLabelText("team name")).toHaveValue("port-openrig");
    expect(screen.getByLabelText("repository")).toHaveValue("/home/you/mogeung");
    expect(screen.getByLabelText("seat 1 name")).toHaveValue("planner");
    expect(screen.getByLabelText("seat 2 name")).toHaveValue("builder");
    // The inherited mode reads as the team's; the override as its own.
    const starts = screen.getAllByText(/starts as/);
    expect(starts[0].textContent).toContain("default");
    expect(starts[1].textContent).toContain("plan");
  });

  it("saves a new team with no id and the owner's defaults", () => {
    render(<TeamsWindow />);
    fireEvent.click(screen.getByText("New team"));
    fireEvent.change(screen.getByLabelText("team name"), { target: { value: "docs sweep" } });
    fireEvent.change(screen.getByLabelText("repository"), { target: { value: "/home/you/docs" } });
    fireEvent.click(screen.getByText("Save"));
    const t = lastSave();
    expect(t.id).toBe("");
    expect(t.name).toBe("docs sweep");
    expect(t.policy).toMatchObject({ hop_budget: 3, hourly_cap: 60, handover: "after_read", readiness_press: false });
    expect(t.permission).toBe("default");
    expect(t.repo).toBe("/home/you/docs");
  });

  it("adds seats with free names and the team's mode as an absence", () => {
    render(<TeamsWindow />);
    fireEvent.click(screen.getByText("New team"));
    fireEvent.change(screen.getByLabelText("team name"), { target: { value: "t" } });
    fireEvent.click(screen.getByText("seat"));
    fireEvent.click(screen.getByText("seat"));
    expect(screen.getByLabelText("seat 1 name")).toHaveValue("seat");
    expect(screen.getByLabelText("seat 2 name")).toHaveValue("seat-2");
    fireEvent.change(screen.getByLabelText("seat 2 name"), { target: { value: "builder" } });
    fireEvent.click(screen.getByText("Save"));
    const t = lastSave();
    expect(t.seats.map((s) => s.name)).toEqual(["seat", "builder"]);
    expect(t.seats[0].permission).toBeUndefined();
    expect(t.seats[0].cli).toBe("claude_code");
    expect(t.seats[0].works_in).toBe("worktree");
  });

  it("keeps Save grey with the daemon's own reason while the definition cannot be addressed", () => {
    render(<TeamsWindow />);
    fireEvent.click(screen.getByText("New team"));
    const save = screen.getByText("Save").closest("button")!;
    expect(save).toBeDisabled();
    expect(screen.getByText("a team needs a name")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("team name"), { target: { value: "t" } });
    fireEvent.click(screen.getByText("seat"));
    fireEvent.change(screen.getByLabelText("seat 1 name"), { target: { value: "Bad Name" } });
    expect(save).toBeDisabled();
    expect(screen.getByText(/must be lower-case letters/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("seat 1 name"), { target: { value: "builder" } });
    expect(save).not.toBeDisabled();
    expect(sent.filter((m) => m.cmd === "team_save")).toHaveLength(0);
  });

  it("keeps Save grey when the daemon said this connection may not write", () => {
    render(<TeamsWindow />);
    arrives([team()], false);
    fireEvent.change(screen.getByLabelText("team name"), { target: { value: "renamed" } });
    expect(screen.getByText("Save").closest("button")).toBeDisabled();
    expect(screen.getByText(/--allow-run/)).toBeInTheDocument();
    expect(screen.getByText("Delete").closest("button")).toBeDisabled();
  });

  it("deletes only on the second press, and says so", () => {
    render(<TeamsWindow />);
    arrives([team()]);
    fireEvent.click(screen.getByText("Delete"));
    expect(sent.filter((m) => m.cmd === "team_delete")).toHaveLength(0);
    fireEvent.click(screen.getByText("Really delete"));
    expect(sent).toContainEqual({ cmd: "team_delete", id: "t1" });
  });

  /** `R-P4`. Boot starts what is saved; Stop is grey until something runs. */
  it("boots a saved team and stops a running one", () => {
    render(<TeamsWindow />);
    arrives([team()]);
    const boot = screen.getByText("Boot team").closest("button")!;
    const stop = screen.getByText("Stop team").closest("button")!;
    expect(boot).not.toBeDisabled();
    expect(stop).toBeDisabled();
    expect(stop.title).toContain("no seat of this team is running");
    fireEvent.click(boot);
    expect(sent).toContainEqual({ cmd: "team_boot", id: "t1" });

    // A draft you are still typing is not what Boot starts.
    fireEvent.change(screen.getByLabelText("team name"), { target: { value: "renamed" } });
    expect(boot).toBeDisabled();
    expect(boot.title).toContain("save your changes first");
  });

  it("shows where a seat runs, and offers to remove a stopped seat's worktree", () => {
    render(<TeamsWindow />);
    arrives([team()]);
    act(() =>
      useStore.setState({
        seatLaunches: [
          {
            team_id: "t1",
            team: "port-openrig",
            seat: "planner",
            cli: "claude_code",
            session_id: "abc",
            pane_id: "%3",
            tmux_target: "geung-port-openrig-planner:0.0",
            tmux_session: "geung-port-openrig-planner",
            team_session: "geung-port-openrig",
            dir: "/home/you/.mogeung/worktrees/mogeung/port-openrig-planner",
            worktree: true,
            files_dir: "/f",
            started: 1,
            signals: { kind: "hooked", events: 3, last_event: "Stop", last_event_at: Date.now() / 1000 - 90, context_used: 12, unread: 0 },
          },
        ],
      }),
    );
    expect(screen.getByTestId("seat-0-status").textContent).toContain("running in geung-port-openrig-planner");
    // `R-P5`: what its hooks have said, under where it runs.
    expect(screen.getByTestId("seat-0-status").textContent).toContain("hooked — 3 events, last Stop 1m ago · context 12%");
    expect(screen.getByText("Stop team").closest("button")).not.toBeDisabled();
    fireEvent.click(screen.getByText("Stop team"));
    expect(sent).toContainEqual({ cmd: "team_stop", id: "t1" });
    // The builder is not running: its worktree can be asked away.
    expect(screen.getByTestId("seat-1-status").textContent).toContain("stopped");
    fireEvent.click(screen.getByText("remove worktree"));
    expect(sent).toContainEqual({ cmd: "seat_worktree_remove", team_id: "t1", seat: "builder" });
  });

  /** `R-P6`. Fence 4 on screen: the counters, the switch that acts at once,
   *  a held send with release and drop, and the log. */
  it("shows the policy's counters, pauses at once, and releases or drops a held send", () => {
    render(<TeamsWindow />);
    arrives([team()]);
    const now = Date.now() / 1000;
    act(() =>
      useStore.setState({
        teamSends: [
          {
            team_id: "t1",
            team: "port-openrig",
            chain: 3,
            hop_budget: 3,
            this_hour: 4,
            hourly_cap: 60,
            paused: false,
            held: 1,
            log: [
              { id: "s1", team_id: "t1", at: now - 300, kind: "wake", from: "planner", to: "builder", text: "your turn", why: "wake on `planner` Stop", status: "delivered", hop: 1 },
              { id: "s2", team_id: "t1", at: now - 60, kind: "agent", from: "builder", to: "planner", text: "tests pass", why: "`builder` said so", status: "held", hop: 0, note: "the hop budget of 3 is spent" },
            ],
          },
        ],
      }),
    );
    expect(screen.getByTestId("sends").textContent).toContain("hops 3/3 · 4/60 this hour · 1 held");
    fireEvent.click(screen.getByText("Pause sends"));
    expect(sent).toContainEqual({ cmd: "team_pause", id: "t1", paused: true });
    expect(screen.getByTestId("held-sends").textContent).toContain("builder → planner “tests pass” · held — the hop budget of 3 is spent");
    fireEvent.click(screen.getByText("release"));
    expect(sent).toContainEqual({ cmd: "send_release", team_id: "t1", id: "s2" });
    fireEvent.click(screen.getByText("drop"));
    expect(sent).toContainEqual({ cmd: "send_drop", team_id: "t1", id: "s2" });
    expect(screen.getByTestId("send-log").textContent).toContain("wake (planner) → builder “your turn” · delivered · hop 1");
    // Nothing of this is in the definition: pausing did not dirty the draft.
    expect(screen.getByText("Save").closest("button")).toBeDisabled();
    // Paused from the daemon reads as paused, and the button offers to resume.
    act(() => useStore.setState({ teamSends: [{ ...useStore.getState().teamSends[0], paused: true }] }));
    expect(screen.getByText("Resume sends")).toBeInTheDocument();
  });
});
