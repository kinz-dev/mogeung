/**
 * Which sentence the Agent pane refuses with. `R-B49`.
 *
 * Reported 2026-08-11: a pane held on a session showed *"This session is not
 * running under tmux"*, and the session was under tmux — it had ended. Both
 * facts reach the client as `tmux_target: null`, because `state.rs` clears the
 * target when a session dies rather than leaving a stale one, so the tmux
 * branch answered for a case that has nothing to do with tmux. The cost is not
 * the wrong words: it sends you to check a launcher that is working, and on a
 * **held** pane the sentence is fixed on screen, so changing selection does not
 * dislodge it and the pane reads as broken.
 *
 * These pin the *choice*, which is why `TerminalView` is mocked down to the
 * `refusal` it was handed: the real one needs a pty and a Tauri window, and
 * neither has an opinion about which sentence is correct.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { PaneScope } from "@/lib/paneScope";
import type { Session } from "@/wire/types";

// Stands in for the terminal, and only for the part under test: it draws the
// refusal when there is no command, which is `TerminalView`'s own contract.
vi.mock("@/ui/Terminal", () => ({
  TerminalView: ({ command, refusal }: { command: string[] | null; refusal?: React.ReactNode }) =>
    command ? <div data-testid="attached">{command.join(" ")}</div> : <div>{refusal}</div>,
}));

const { AgentPane } = await import("@/panes/AgentPane");

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/repo",
    repo_root: "/repo",
    pid: 26514,
    alive: true,
    started_at: "2026-08-05T17:53:00.000Z",
    last_event_at: "2026-08-07T14:32:00.000Z",
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    tmux_target: "mogeung-app:0.0",
    ...extra,
  }) as unknown as Session;

/** Put one session on the board and select it. */
function show(s: Session): void {
  useStore.setState({ sessions: { [s.id]: s }, selected: s.id });
}

/** Hold the base `agent` pane on a session, the way the anchor does. */
function hold(id: string): void {
  const { scoped, setScoped } = useStore.getState();
  setScoped({ paneHold: { ...scoped().paneHold, agent: id } });
}

/**
 * Mounted inside its `PaneScope`, as dockview mounts it.
 *
 * Not optional decoration: a hold belongs to a **pane**, so `usePaneBinding`
 * reads `usePaneId()` — and outside the provider that is `null`, which makes
 * every pane look unheld and would quietly pass the held test against a
 * following pane.
 */
function renderPane() {
  return render(
    <PaneScope id="agent">
      <AgentPane />
    </PaneScope>,
  );
}

beforeEach(() => {
  cleanup();
  localStorage.clear();
  useStore.setState({
    sessions: {},
    selected: null,
    machineId: "m1",
    daemon: { machine_id: "m1", host: "here", claude_home: "/h", pid: 1, version: "0.1.0" } as never,
    prefs: { ...useStore.getState().prefs, scoped: {} },
  });
});

describe("the Agent pane's refusal names the actual reason", () => {
  /** The bug. A dead session is not a tmux problem, and saying so misdirects. */
  it("says a session has ended rather than blaming tmux", () => {
    show(session("done", { alive: false, tmux_target: null }));
    renderPane();

    expect(screen.getByText(/has ended/i)).toBeTruthy();
    expect(screen.queryByText(/not running under tmux/i)).toBeNull();
    // The launcher is not at fault and must not be named: that advice is what
    // sent the report looking at tmux in the first place.
    expect(screen.queryByText(/yolomo/i)).toBeNull();
  });

  /** The worst version of it, and the one reported. */
  it("tells a held pane that it is held, so the anchor is the way out", () => {
    show(session("live-one"));
    hold("done");
    useStore.setState((st) => ({ sessions: { ...st.sessions, done: session("done", { alive: false, tmux_target: null }) } }));
    renderPane();

    expect(screen.getByText(/has ended/i)).toBeTruthy();
    expect(screen.getByText(/held/i)).toBeTruthy();
    expect(screen.getByText(/drop the anchor/i)).toBeTruthy();
  });

  /** An unheld pane has nothing to drop, so it must not offer the anchor. */
  it("does not mention an anchor a following pane has not dropped", () => {
    show(session("done", { alive: false, tmux_target: null }));
    renderPane();

    expect(screen.queryByText(/drop the anchor/i)).toBeNull();
  });

  /**
   * The case the tmux sentence is *for*, which must survive the reordering —
   * a session that is running, started outside tmux. This is the one where the
   * launcher really is the answer.
   */
  it("still blames tmux when a live session has no pane", () => {
    show(session("bare", { alive: true, tmux_target: null }));
    renderPane();

    expect(screen.getByText(/not running under tmux/i)).toBeTruthy();
    expect(screen.queryByText(/has ended/i)).toBeNull();
  });

  /** A live session under tmux attaches, and says nothing at all. */
  it("attaches a live session and refuses nothing", () => {
    show(session("going"));
    renderPane();

    expect(screen.getByTestId("attached").textContent).toContain("attach-session");
    expect(screen.queryByText(/has ended/i)).toBeNull();
  });
});
