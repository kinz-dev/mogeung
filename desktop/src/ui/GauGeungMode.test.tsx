/**
 * Gau Geung, the team mode. `R-P7`.
 *
 * Pinned: it is a mode — nothing until opened, `Esc` leaves, and the window
 * underneath is not touched; the five lanes hold the team's seats with
 * mogeung's badge inside each card; the topology table names every seat's
 * state and its source; the pause switch and a held send's release and drop
 * go to the daemon as the Teams window's do; picking a seat fills the seat
 * pane; and the tabs show one team or all of them.
 *
 * Rendered through the real dockview, as the terminal panel's tests are,
 * because the tile tree is the design decision (a pane, not a drawer).
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import type { AttentionItem, ClientMsg, SeatLaunch, Session, Team, TeamSends } from "@/wire/types";
import { newSeat, newTeam } from "@/lib/teams";
import { GauGeungMode } from "./GauGeungMode";

vi.mock("@/ui/Terminal", () => ({
  TerminalView: ({ id }: { id: string }) => <div data-testid={`terminal-${id}`} />,
}));

const sent: ClientMsg[] = [];

const team = (): Team => ({
  ...newTeam(),
  id: "t1",
  name: "port",
  repo: "/r",
  seats: [newSeat("planner"), newSeat("builder", "codex")],
  created: 1,
  updated: 2,
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
  started: Date.now() / 1000 - 600,
  signals: { kind: "hooked", events: 3, last_event: "Stop", last_event_at: Date.now() / 1000 - 60, context_used: 41, model: "Opus 5.5", unread: 0 },
  ...over,
});

const session = (id: string, seat: string, over: Partial<Session> = {}): Session =>
  ({
    id,
    alive: true,
    live_status: "busy",
    last_event_at: new Date().toISOString(),
    last_activity: "Edit src/seat.rs",
    tokens_in: 212_000,
    tokens_out: 18_000,
    files_changed: 0,
    insertions: 0,
    deletions: 0,
    tmux_target: `geung-port-${seat}:0.0`,
    seat: { team_id: "t1", team: "port", seat, signals: "hooked" },
    ...over,
  }) as unknown as Session;

const item = (id: string, reason: AttentionItem["reason"], detail: string): AttentionItem => ({ session_id: id, reason, score: 0, detail });

const sends = (over: Partial<TeamSends> = {}): TeamSends => ({
  team_id: "t1",
  team: "port",
  chain: 3,
  hop_budget: 3,
  this_hour: 4,
  hourly_cap: 60,
  paused: false,
  held: 1,
  log: [
    { id: "s1", team_id: "t1", at: Date.now() / 1000 - 300, kind: "agent", from: "planner", to: "builder", text: "start on task 1", why: "`planner` said so", status: "delivered", hop: 1 },
    { id: "s2", team_id: "t1", at: Date.now() / 1000 - 60, kind: "agent", from: "builder", to: "planner", text: "done", why: "`builder` said so", status: "held", hop: 0, note: "the hop budget of 3 is spent" },
  ],
  ...over,
});

function board() {
  useStore.setState({
    showGauGeung: true,
    gauTeam: null,
    gauSeat: null,
    teams: [team()],
    teamsAllowed: true,
    seatLaunches: [launch("planner"), launch("builder", { cli: "codex", signals: { kind: "plain", events: 0, unread: 0 } })],
    sessions: {
      p: session("p", "planner", { live_status: "idle", held_send: { count: 1, why: "the hop budget of 3 is spent", since: new Date().toISOString() } }),
      b: session("b", "builder"),
    },
    queue: [item("p", "awaiting_input", "a send held for it — the hop budget of 3 is spent"), item("b", "running", "Edit src/seat.rs")],
    teamSends: [sends()],
    selected: null,
    send: (m) => void sent.push(m),
  });
  render(<GauGeungMode />);
}

beforeEach(() => {
  cleanup();
  sent.length = 0;
});

describe("the Gau Geung mode", () => {
  it("shows nothing until opened, and asks for the teams when it opens", () => {
    useStore.setState({ showGauGeung: false, send: (m) => void sent.push(m) });
    render(<GauGeungMode />);
    expect(screen.queryByRole("dialog", { name: "Gau Geung" })).toBeNull();
    expect(sent).toEqual([]);
    cleanup();
    board();
    expect(screen.getByRole("dialog", { name: "Gau Geung" })).toBeInTheDocument();
    expect(sent).toContainEqual({ cmd: "team_list" });
  });

  it("puts the seats in the five lanes with mogeung's badge inside each card", async () => {
    board();
    const pulse = await screen.findByTestId("pulse");
    expect(pulse).toBeInTheDocument();
    const needs = screen.getByTestId("lane-needs_you");
    expect(needs.textContent).toContain("Needs you · 1");
    expect(needs.textContent).toContain("planner");
    expect(needs.textContent).toContain("WAITING");
    expect(needs.textContent).toContain("a send held — the hop budget of 3 is spent");
    const nowLane = screen.getByTestId("lane-now");
    expect(nowLane.textContent).toContain("Now · 1");
    expect(nowLane.textContent).toContain("builder");
    expect(nowLane.textContent).toContain("running");
    expect(screen.getByTestId("lane-parked").textContent).toContain("Parked with baton · 0");
    expect(screen.getByTestId("lane-finished").textContent).toContain("last: planner → builder");
  });

  it("names every seat's state and its source in the table, and draws the graph", async () => {
    board();
    const table = await screen.findByTestId("topology");
    const planner = screen.getByTestId("row-planner");
    expect(planner.textContent).toContain("claude · Opus 5.5");
    expect(planner.textContent).toContain("(hook)");
    expect(planner.textContent).toContain("41%");
    expect(planner.textContent).toContain("212.0k · 18.0k");
    expect(planner.textContent).toContain("1 · 0");
    const builder = screen.getByTestId("row-builder");
    expect(builder.textContent).toContain("codex · default");
    expect(builder.textContent).toContain("(transcript)");
    expect(builder.textContent).toContain("unknown · no hooks");
    expect(table.querySelectorAll("tbody tr")).toHaveLength(2);
    const graph = screen.getByTestId("graph");
    expect(graph.querySelectorAll("line")).toHaveLength(2);
    expect(graph.textContent).toContain("held");
    expect(graph.textContent).toContain("hop 1");
  });

  it("shows the counters, pauses at once, and releases or drops a held send", async () => {
    board();
    const policy = await screen.findByTestId("policy");
    expect(policy.textContent).toContain("hops 3/3 · 4/60 this hour · 1 held");
    fireEvent.click(screen.getByText("Pause all automatic sends"));
    expect(sent).toContainEqual({ cmd: "team_pause", id: "t1", paused: true });
    expect(screen.getByTestId("held").textContent).toContain("builder → planner “done” · held — the hop budget of 3 is spent");
    fireEvent.click(screen.getByText("release"));
    expect(sent).toContainEqual({ cmd: "send_release", team_id: "t1", id: "s2" });
    fireEvent.click(screen.getByText("drop"));
    expect(sent).toContainEqual({ cmd: "send_drop", team_id: "t1", id: "s2" });
    // The tab carries the team's state.
    expect(screen.getByTestId("tab-t1").textContent).toBe("port · 2/2 · 1 needs you");
    act(() => useStore.setState({ teamSends: [sends({ paused: true })] }));
    expect(screen.getByText("Resume sends")).toBeInTheDocument();
    expect(screen.getByTestId("tab-t1").textContent).toContain("paused");
  });

  it("fills the seat pane with the picked seat, and attaches its terminal as a pane", async () => {
    board();
    await screen.findByTestId("pulse");
    expect(screen.getByText("no seat picked")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("card-planner"));
    expect(useStore.getState().gauSeat).toBe("t1/planner");
    const detail = await screen.findByTestId("seat-detail");
    expect(detail.textContent).toContain("planner");
    expect(detail.textContent).toContain("claude · Opus 5.5");
    expect(detail.textContent).toContain("hooked — 3 events, last Stop 1m ago · context 41% · Opus 5.5");
    expect(detail.textContent).toContain("planner → builder “start on task 1” · delivered · hop 1");
    expect(detail.textContent).toContain("your home, read only");
    fireEvent.click(screen.getByText("Attach terminal"));
    expect(await screen.findByTestId("terminal-term:t1/planner")).toBeInTheDocument();
    // The row picks too, and the pane follows.
    fireEvent.click(screen.getByTestId("row-builder"));
    expect(useStore.getState().gauSeat).toBe("t1/builder");
    expect((await screen.findByTestId("seat-detail")).textContent).toContain("builder");
  });

  it("leaves on Escape without touching the selection underneath", async () => {
    board();
    useStore.setState({ selected: "elsewhere" });
    await screen.findByTestId("pulse");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(useStore.getState().showGauGeung).toBe(false);
    expect(useStore.getState().selected).toBe("elsewhere");
  });

  it("shows one team or all of them from the tabs", async () => {
    board();
    act(() =>
      useStore.setState({
        teams: [team(), { ...team(), id: "t2", name: "docs", seats: [newSeat("writer")] }],
      }),
    );
    await screen.findByTestId("team-t1");
    expect(screen.getByTestId("team-t2")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("tab-t2"));
    expect(useStore.getState().gauTeam).toBe("t2");
    expect(screen.queryByTestId("team-t1")).toBeNull();
    expect(screen.getByTestId("team-t2").textContent).toContain("docs · 0/1");
    fireEvent.click(screen.getByText("all teams · 2"));
    expect(screen.getByTestId("team-t1")).toBeInTheDocument();
  });
});
