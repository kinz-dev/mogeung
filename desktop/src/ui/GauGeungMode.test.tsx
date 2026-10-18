/**
 * Gau Geung, the team board. `R-P7`; panes in the main tile tree since `R-J132`.
 *
 * Pinned: the board asks for the teams when it mounts; the five lanes hold
 * the team's seats with mogeung's badge inside each card; the topology table
 * names every seat's state and its source; the pause switch and a held send's
 * release and drop go to the daemon as the Teams window's do; picking a seat
 * fills the seat pane; the tabs show one team or all of them; and — the
 * `R-J132` half — the board, the seat pane and a seat's terminal are panes of
 * the **main** dock, so `Alt+g` raises or closes the board, a pick opens the
 * seat pane beside it, and an attach adds a terminal pane, with the Attention
 * list and the rail still on screen and the selection untouched.
 */

import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useStore } from "@/store";
import type { AttentionItem, ClientMsg, SeatLaunch, Session, Team, TeamSends } from "@/wire/types";
import { newSeat, newTeam } from "@/lib/teams";
import { getDock, toggleTeamBoard } from "@/lib/panes";
import { BOARD_PANE, SEAT_PANE } from "@/lib/teamMode";
import { TeamBoard } from "./GauGeungMode";

vi.mock("@/ui/Terminal", () => ({
  TerminalView: ({ id }: { id: string }) => <div data-testid={`terminal-${id}`} />,
}));

class FakeSocket {
  static OPEN = 1;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  constructor(public url: string) {}
  send() {}
  close() {}
}

beforeAll(() => {
  vi.stubGlobal("WebSocket", FakeSocket);
  // The same stubs the queue tests explain: dockview and the virtualised
  // list measure the DOM, and in jsdom nothing has a size.
  class SizedResizeObserver {
    constructor(private cb: ResizeObserverCallback) {}
    observe(el: Element) {
      this.cb([{ target: el, contentRect: { width: 1200, height: 800 } } as unknown as ResizeObserverEntry], this as unknown as ResizeObserver);
    }
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal("ResizeObserver", SizedResizeObserver);
  for (const prop of ["offsetHeight", "clientHeight"]) {
    Object.defineProperty(HTMLElement.prototype, prop, { configurable: true, get: () => 800 });
  }
  for (const prop of ["offsetWidth", "clientWidth"]) {
    Object.defineProperty(HTMLElement.prototype, prop, { configurable: true, get: () => 1200 });
  }
});

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
  alive: true,
  team_session: "geung-port",
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
    title: null,
    name: null,
    cwd: `/w/${seat}`,
    repo_root: `/w/${seat}`,
    open_tools: [],
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
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

function seed() {
  useStore.setState({
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
}

/** The board on its own, as the dock renders it in a pane. */
function board() {
  seed();
  render(<TeamBoard />);
}

beforeEach(() => {
  cleanup();
  sent.length = 0;
});

afterEach(() => {
  cleanup();
  useStore.setState({ sessions: {}, queue: [], teams: [], seatLaunches: [], teamSends: [], gauSeat: null, gauTeam: null, selected: null });
  localStorage.clear();
});

describe("the Gau Geung board", () => {
  it("asks for the teams when it mounts", () => {
    board();
    expect(sent).toContainEqual({ cmd: "team_list" });
    expect(screen.getByTestId("team-board")).toBeInTheDocument();
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
    // And, for one team, the attach command for a terminal (`R-P8`).
    fireEvent.click(screen.getByTestId("tab-t1"));
    expect(screen.getByText("tmux attach -t geung-port")).toBeInTheDocument();
    act(() => useStore.setState({ teamSends: [sends({ paused: true })] }));
    expect(screen.getByText("Resume sends")).toBeInTheDocument();
    expect(screen.getByTestId("tab-t1").textContent).toContain("paused");
  });

  /** `R-P19`. The picked seat is a card inside its team, under the header —
   *  not a pane of its own — and a second pick of it, or its close button,
   *  puts it away. */
  it("opens the picked seat as a card under its team, and closes it again", async () => {
    board();
    await screen.findByTestId("pulse");
    expect(screen.queryByTestId("seat-detail")).toBeNull();
    fireEvent.click(screen.getByTestId("card-planner"));
    expect(useStore.getState().gauSeat).toBe("t1/planner");
    const detail = await screen.findByTestId("seat-detail");
    const team = screen.getByTestId("team-t1");
    expect(team.contains(detail)).toBe(true);
    // Under the header, above the lanes.
    expect(detail.compareDocumentPosition(screen.getByTestId("pulse")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The same pick again closes it; so does its own button.
    fireEvent.click(screen.getByTestId("card-planner"));
    expect(useStore.getState().gauSeat).toBeNull();
    expect(screen.queryByTestId("seat-detail")).toBeNull();
    fireEvent.click(screen.getByTestId("card-planner"));
    fireEvent.click(await screen.findByTitle(/close the seat's card/));
    expect(useStore.getState().gauSeat).toBeNull();
  });

  /** `R-P19`: *"make it a bit more colorful"*. The seat and its model wear
   *  the CLI's colour, the permission mode one of its own. */
  it("colours the seat and its model by the CLI and the mode by the mode", async () => {
    board();
    await screen.findByTestId("pulse");
    fireEvent.click(screen.getByTestId("card-planner"));
    const detail = await screen.findByTestId("seat-detail");
    expect(within(detail).getByText("claude · Opus 5.5").style.color).toBe("var(--blue)");
    const mode = within(detail).getByText("auto");
    expect(mode.style.color).toBe("var(--green)");
    expect(mode.style.backgroundColor).toContain("var(--green)");
    // An override wears its own: the builder, a Codex seat, put in plan.
    act(() =>
      useStore.setState((st) => ({
        teams: st.teams.map((t) => ({ ...t, seats: t.seats.map((x) => (x.name === "builder" ? { ...x, permission: "plan" as const } : x)) })),
      })),
    );
    fireEvent.click(screen.getByTestId("card-builder"));
    const b = await screen.findByTestId("seat-detail");
    expect(within(b).getByText("plan").style.color).toBe("var(--graph-3)");
    expect(within(b).getByText(/^codex · /).style.color).toBe("var(--purple)");
  });

  it("fills the seat's card with what the board knows of it", async () => {
    board();
    await screen.findByTestId("pulse");
    fireEvent.click(screen.getByTestId("card-planner"));
    expect(useStore.getState().gauSeat).toBe("t1/planner");
    const detail = await screen.findByTestId("seat-detail");
    expect(detail.textContent).toContain("planner");
    expect(detail.textContent).toContain("claude · Opus 5.5");
    expect(detail.textContent).toContain("hooked — 3 events, last Stop 1m ago · context 41% · Opus 5.5");
    expect(detail.textContent).toContain("planner → builder “start on task 1” · delivered · hop 1");
    expect(detail.textContent).toContain("your home, read only");
    // `R-P11`: what its pane showed, with the lines it was read from.
    act(() =>
      useStore.setState((s) => ({
        sessions: {
          ...s.sessions,
          p: { ...s.sessions.p, screen: { shape: "permission_menu", excerpt: ["Do you want to proceed?", "❯ 1. Yes", "  2. No"], read_at: Date.now() / 1000 - 3 } },
        },
      })),
    );
    const sc = await screen.findByTestId("screen");
    expect(sc.textContent).toContain("shape: permission menu");
    expect(sc.textContent).toContain("Do you want to proceed?");
    expect(sc.textContent).toContain("never answered by mogeung");
    // The row picks too, and the card follows.
    fireEvent.click(screen.getByTestId("row-builder"));
    expect(useStore.getState().gauSeat).toBe("t1/builder");
    expect((await screen.findByTestId("seat-detail")).textContent).toContain("builder");
  });

  /** `R-P10`. A held handover is shown whole with its composer, and the two
   *  buttons are the daemon's release and drop; restart asks the daemon. */
  it("shows the seat's last handover whole, delivers or discards it, and offers a restart", async () => {
    board();
    act(() =>
      useStore.setState({
        gauSeat: "t1/builder",
        teamSends: [
          sends({
            log: [
              {
                id: "h1",
                team_id: "t1",
                at: Date.now() / 1000 - 30,
                kind: "handover",
                to: "builder",
                text: "# Handover\n\nThe planner wrote docs/plan.md.",
                why: "handover on start",
                status: "held",
                hop: 0,
                note: "composed by fake-1 @ localhost — read it, then release it to the seat",
                composed_by: "fake-1 @ localhost",
              },
            ],
          }),
        ],
      }),
    );
    const h = await screen.findByTestId("handover");
    expect(h.textContent).toContain("handover · handover on start");
    expect(h.textContent).toContain("composed by fake-1 @ localhost");
    expect(h.textContent).toContain("The planner wrote docs/plan.md.");
    fireEvent.click(screen.getByText("Deliver"));
    expect(sent).toContainEqual({ cmd: "send_release", team_id: "t1", id: "h1" });
    fireEvent.click(screen.getByText("Discard"));
    expect(sent).toContainEqual({ cmd: "send_drop", team_id: "t1", id: "h1" });
    fireEvent.click(screen.getByText("Restart with handover"));
    expect(sent).toContainEqual({ cmd: "seat_restart", team_id: "t1", seat: "builder" });
  });

  /** `R-J132`. The board is a pane of the main dock, raised or closed by the
   *  chord's action; a pick opens the seat's card on it, and no pane
   *  (`R-P19`); an attach adds a terminal pane; the Attention list stays, and
   *  so does the selection. */
  it("lives in the main tile tree: the chord raises or closes it, a pick opens a card, an attach adds a terminal", async () => {
    seed();
    useStore.setState({ selected: "elsewhere", firstSnapshot: true });
    const { default: App } = await import("@/App");
    render(<App />);
    const dock = getDock()!;
    expect(dock).toBeTruthy();
    expect(dock.getPanel(BOARD_PANE)).toBeUndefined();
    expect(useStore.getState().teamBoardOpen).toBe(false);

    act(() => toggleTeamBoard());
    expect(dock.getPanel(BOARD_PANE)).toBeDefined();
    expect(useStore.getState().teamBoardOpen).toBe(true);
    await screen.findByTestId("pulse");
    // Beside it, not under it: the Attention list is still on screen.
    expect(screen.getByRole("listbox", { name: "Attention" })).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("card-planner"));
    expect(dock.getPanel(SEAT_PANE)).toBeUndefined();
    expect((await screen.findByTestId("seat-detail")).textContent).toContain("planner");
    expect(useStore.getState().selected).toBe("elsewhere");

    fireEvent.click(screen.getByText("Attach terminal"));
    expect(dock.getPanel("seat-term:t1/planner")).toBeDefined();
    expect(await screen.findByTestId("terminal-seat-term:t1/planner")).toBeInTheDocument();
    // A second attach raises the one pane rather than adding another.
    fireEvent.click(screen.getByText("Attach terminal"));
    expect(dock.panels.filter((p) => p.id.startsWith("seat-term:"))).toHaveLength(1);

    // The terminal is in front: the chord raises the board, then closes it.
    act(() => toggleTeamBoard());
    expect(dock.activePanel?.id).toBe(BOARD_PANE);
    expect(useStore.getState().teamBoardOpen).toBe(true);
    act(() => toggleTeamBoard());
    expect(dock.getPanel(BOARD_PANE)).toBeUndefined();
    expect(useStore.getState().teamBoardOpen).toBe(false);
    // The terminal is a pane of its own and stays.
    expect(dock.getPanel("seat-term:t1/planner")).toBeDefined();
  });

  /** `R-P19`. A layout saved while the seat was a pane still restores — the
   *  id stays registered — and the retired pane is closed on the way in. */
  it("restores a layout saved with the old seat pane, without the pane", async () => {
    seed();
    const { default: App } = await import("@/App");
    render(<App />);
    const dock = getDock()!;
    act(() => {
      dock.addPanel({ id: SEAT_PANE, component: SEAT_PANE, title: "Seat" });
    });
    const saved = JSON.stringify(dock.toJSON());
    cleanup();
    localStorage.setItem("mogeung.layout", saved);
    render(<App />);
    const again = getDock()!;
    expect(again.getPanel(SEAT_PANE)).toBeUndefined();
    expect(again.panels.length).toBeGreaterThan(0);
  });

  /** `R-P15`. The quick way down is on the board, per team: Wind down asks
   *  every seat for its note, Stop now ends them at once; while it runs the
   *  strip says how far along, and afterwards how it went. */
  it("winds a team down from the board and shows how it is going", async () => {
    board();
    await screen.findByTestId("pulse");
    fireEvent.click(screen.getByText("Wind down"));
    expect(sent).toContainEqual({ cmd: "team_wind_down", id: "t1" });
    fireEvent.click(screen.getByText("Stop now"));
    expect(sent).toContainEqual({ cmd: "team_stop", id: "t1" });
    const now = Math.floor(Date.now() / 1000);
    act(() =>
      useStore.setState({
        teamSends: [
          sends({
            wind_down: {
              started: now - 30,
              deadline: now + 150,
              seats: [
                { seat: "planner", status: "asked", at: now - 30 },
                { seat: "builder", status: "noted", at: now - 5, note: "Parser done; tests next." },
              ],
            },
          }),
        ],
      }),
    );
    const strip = await screen.findByTestId("wind-down");
    expect(strip.textContent).toContain("winding down · 1 of 2 answered, 1 with a note · 2:");
    expect(strip.textContent).toContain("builder · left a note — “Parser done; tests next.”");
    expect(screen.getByText("Wind down").closest("button")).toBeDisabled();
    act(() =>
      useStore.setState({
        teamSends: [
          sends({
            wind_down: {
              started: now - 200,
              deadline: now - 20,
              seats: [
                { seat: "planner", status: "timed_out", at: now - 20 },
                { seat: "builder", status: "noted", at: now - 100, note: "Parser done; tests next." },
              ],
            },
          }),
        ],
      }),
    );
    expect(screen.getByTestId("wind-down").textContent).toContain("wound down — 1 of 2 left a note · planner timed out, builder left a note");
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
