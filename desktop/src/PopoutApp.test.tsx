/**
 * A popped-out pane shows the session its URL named. `R-B55`.
 *
 * Reported 2026-09-08, on the first real pop-out: *"the tmux is not showing
 * there. in the pop out window, I can only see a 'select a session' text."*
 *
 * The first cut bound the pane with a **hold**, which reads as the right
 * mechanism — it is what means *"this pane shows this session whatever the
 * queue says"*. It is stored in `scoped()`, which keys on `daemon?.machine_id`,
 * and that is not known when the window opens: the hold was written under
 * `"unknown"` and lost the moment the daemon published its identity, or gated
 * behind a `machineId` that is a different value entirely and never arrived.
 * Either way `usePaneBinding` fell through to `selected`, which a popout has
 * nobody to set — so the pane refused with the empty-state sentence for ever.
 *
 * These assert the binding, not the terminal: `TerminalView` is mocked to the
 * command it was handed, because the real one needs a pty and a Tauri window
 * and has no opinion about which session it was pointed at.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { useStore } from "@/store";
import type { Session } from "@/wire/types";

vi.mock("@/ui/Terminal", () => ({
  TerminalView: ({ id, command }: { id: string; command: string[] | null }) =>
    command ? (
      <div data-testid="attached" data-pty-id={id}>
        {command.join(" ")}
      </div>
    ) : (
      <div data-testid="refused" />
    ),
}));

vi.mock("@/lib/popout", async (orig) => ({
  ...(await orig<typeof import("@/lib/popout")>()),
  closeThisWindow: vi.fn(async () => {}),
}));

const { default: PopoutApp } = await import("@/PopoutApp");

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/repo",
    repo_root: "/repo",
    pid: 26514,
    alive: true,
    started_at: "2026-09-08T10:00:00.000Z",
    last_event_at: "2026-09-08T10:01:00.000Z",
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    tmux_target: "mogeung-app:0.0",
    ...extra,
  }) as unknown as Session;

const SID = "0b3f9c2a";

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

describe("a popped-out pane", () => {
  it("attaches to the session its URL named", async () => {
    useStore.setState({ sessions: { [SID]: session(SID) } });

    render(<PopoutApp popout={{ kind: "agent", session: SID }} />);

    await waitFor(() => expect(screen.getByTestId("attached")).toBeInTheDocument());
    expect(screen.getByTestId("attached").textContent).toContain("mogeung-app:0.0");
    expect(screen.queryByText(/select a session/i)).not.toBeInTheDocument();
  });

  /**
   * The regression in one line. Nothing in this window sets a selection, so a
   * pane that depends on one shows the empty state for ever.
   */
  it("selects that session rather than waiting for something to", async () => {
    useStore.setState({ sessions: { [SID]: session(SID) }, selected: null });

    render(<PopoutApp popout={{ kind: "agent", session: SID }} />);

    await waitFor(() => expect(useStore.getState().selected).toBe(SID));
  });

  /**
   * It must not depend on the daemon having published its identity: that is
   * what the hold did, and it is why the first cut failed.
   */
  it("works before the daemon has said which machine it is", async () => {
    useStore.setState({
      sessions: { [SID]: session(SID) },
      daemon: null as never,
      machineId: null,
    });

    render(<PopoutApp popout={{ kind: "agent", session: SID }} />);

    await waitFor(() => expect(screen.getByTestId("attached")).toBeInTheDocument());
  });

  /** Its own pty, distinct from the pane it was moved out of. */
  it("opens a pty of its own rather than the one the main window used", async () => {
    useStore.setState({ sessions: { [SID]: session(SID) } });

    render(<PopoutApp popout={{ kind: "agent", session: SID }} />);

    await waitFor(() => expect(screen.getByTestId("attached")).toBeInTheDocument());
    const ptyId = screen.getByTestId("attached").getAttribute("data-pty-id");
    expect(ptyId).toBe(`popout:agent:${SID}`);
    expect(ptyId).not.toBe(`agent:${SID}`);
  });

  it("names the session in its title bar", async () => {
    useStore.setState({ sessions: { [SID]: session(SID, { title: "fix the queue" }) } });

    render(<PopoutApp popout={{ kind: "agent", session: SID }} />);

    expect(await screen.findByText("fix the queue")).toBeInTheDocument();
  });
});
