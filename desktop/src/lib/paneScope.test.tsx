/**
 * Two agents at once. `R-B49`.
 *
 * The splitting was never the hard part — the centre has been a dockview tree
 * since the port, and a tab could always be dragged beside itself. What could
 * not happen is a pane aimed somewhere the queue is not: every pane read
 * `selected`, so two Agent panes were two views of one session.
 *
 * Every test here fails without the binding, and the two that matter most are
 * the ones about a hold that has outlived its session. Falling back to the
 * selection there is the tempting default and it is the dangerous one: the pane
 * would silently show a *different agent* than the one you moored it to, which
 * is the one failure a window full of live terminals must not have.
 */

import { describe, expect, it, beforeEach, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { DockviewApi } from "dockview";
import { PaneScope, paneKind } from "@/lib/paneScope";
import { nextAgentSlot } from "@/lib/panes";
import { togglePaneHold, useSelectedSession, usePaneBinding, useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import type { Session } from "@/wire/types";

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/tmp/repo",
    repo_root: "/tmp/repo",
    alive: true,
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
    ...extra,
  }) as unknown as Session;

/** The scope key `scoped()` resolves to with no daemon connected. */
function setHolds(holds: Record<string, string>): void {
  const prefs = { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), paneHold: holds } } };
  useStore.setState({ prefs });
}

function Probe() {
  const s = useSelectedSession();
  const { held } = usePaneBinding();
  return <div data-testid="probe">{`${s?.id ?? "none"}/${held ? "held" : "follows"}`}</div>;
}

beforeEach(() => {
  cleanup();
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    sessions: { s1: session("s1"), s2: session("s2") },
  });
});

describe("a pane's session", () => {
  it("follows the selection when nothing holds it", () => {
    render(
      <PaneScope id="agent">
        <Probe />
      </PaneScope>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("s1/follows");

    act(() => {
      useStore.setState({ selected: "s2" });
    });
    expect(screen.getByTestId("probe")).toHaveTextContent("s2/follows");
  });

  /** The whole feature, in one assertion: the queue moves and this does not. */
  it("stays where it was held while the selection moves underneath it", () => {
    setHolds({ "agent:2": "s1" });
    render(
      <PaneScope id="agent:2">
        <Probe />
      </PaneScope>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("s1/held");

    act(() => {
      useStore.setState({ selected: "s2" });
    });
    expect(screen.getByTestId("probe")).toHaveTextContent("s1/held");
  });

  it("leaves the other panes following", () => {
    setHolds({ "agent:2": "s1" });
    render(
      <>
        <PaneScope id="agent:2">
          <Probe />
        </PaneScope>
        <PaneScope id="agent">
          <Probe />
        </PaneScope>
      </>,
    );
    act(() => {
      useStore.setState({ selected: "s2" });
    });
    const [pinned, following] = screen.getAllByTestId("probe");
    expect(pinned).toHaveTextContent("s1/held");
    expect(following).toHaveTextContent("s2/follows");
  });

  /**
   * A hold that outlives its session must resolve to **nothing**, not to the
   * selection. The pane then says the session ended; falling through would show
   * another agent's terminal under the name of the one you moored.
   */
  it("resolves to nothing — never to the selection — when the held session has gone", () => {
    setHolds({ "agent:2": "gone" });
    render(
      <PaneScope id="agent:2">
        <Probe />
      </PaneScope>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("none/held");

    act(() => {
      useStore.setState({ selected: "s2" });
    });
    expect(screen.getByTestId("probe")).toHaveTextContent("none/held");
  });

  it("follows the selection again once the hold is let go", () => {
    setHolds({ agent: "s1" });
    render(
      <PaneScope id="agent">
        <Probe />
      </PaneScope>,
    );
    act(() => {
      useStore.setState({ selected: "s2" });
      togglePaneHold("agent");
    });
    expect(screen.getByTestId("probe")).toHaveTextContent("s2/follows");
  });

  /** Outside a pane — the rail, the palette, a dialog — nothing changes. */
  it("is the selection for anything that is not in a pane at all", () => {
    setHolds({ agent: "s1" });
    render(<Probe />);
    act(() => {
      useStore.setState({ selected: "s2" });
    });
    expect(screen.getByTestId("probe")).toHaveTextContent("s2/follows");
  });
});

describe("numbered slots", () => {
  const dockWith = (ids: string[]) =>
    ({ getPanel: (id: string) => (ids.includes(id) ? {} : undefined) }) as unknown as DockviewApi;

  it("starts at the bare `agent`, so a layout saved before this still fits", () => {
    expect(nextAgentSlot(dockWith([]))).toBe("agent");
  });

  it("skips the slots that are taken", () => {
    expect(nextAgentSlot(dockWith(["agent"]))).toBe("agent:2");
    expect(nextAgentSlot(dockWith(["agent", "agent:2"]))).toBe("agent:3");
  });

  /** Close the middle of three and the next split reuses that number. */
  it("fills a gap rather than climbing past it", () => {
    expect(nextAgentSlot(dockWith(["agent", "agent:3"]))).toBe("agent:2");
  });

  /**
   * There was a ceiling at four until 2026-08-20 (`R-J35`), and the reason it
   * gave is still true — every visible pane is a live pty. What changed is who
   * weighs it: a number here applied one limit to a laptop and to a 49-inch
   * screen. Well past where it used to refuse, and past the digit that would
   * sort wrongly as a string.
   */
  it("keeps handing out slots past the four it used to stop at", () => {
    const many = Array.from({ length: 10 }, (_, i) => (i === 0 ? "agent" : `agent:${i + 1}`));
    expect(nextAgentSlot(dockWith(many.slice(0, 4)))).toBe("agent:5");
    expect(nextAgentSlot(dockWith(many))).toBe("agent:11");
  });

  it("reads a slot back to the component that renders it", () => {
    expect(paneKind("agent:3")).toBe("agent");
    expect(paneKind("agent")).toBe("agent");
    expect(paneKind("code")).toBe("code");
  });
});

/**
 * The window has one selection and several panes, so with three agents up the
 * file tabs, the dock and Info all describe whichever session the *queue* last
 * pointed at — not the one you are working in. Clicking the pane you are
 * already looking at is the cheapest way to say "this one", and it is what was
 * asked for.
 */
describe("clicking an Agent pane", () => {
  it("makes its session the current one, so everything else follows", async () => {
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
    vi.stubGlobal("WebSocket", FakeSocket);
    localStorage.removeItem("mogeung.layout");

    const { default: App } = await import("@/App");
    const { splitAgent, getDock } = await import("@/lib/panes");
    useStore.setState({ selected: "s1", sessions: { s1: session("s1"), s2: session("s2") } });
    render(<App />);

    await act(async () => {
      splitAgent();
    });
    // The second pane, held on s2, while the queue still points at s1.
    await act(async () => {
      useStore.getState().setScoped({ paneHold: { "agent:2": "s2" } });
      getDock()?.getPanel("agent")?.api.setActive();
    });
    expect(useStore.getState().selected).toBe("s1");

    await act(async () => {
      getDock()?.getPanel("agent:2")?.api.setActive();
    });
    expect(useStore.getState().selected).toBe("s2");

    // One-way, and this is the half that matters. A pane that moved the
    // selection and then followed it would let go of its own mooring the moment
    // you clicked it — the opposite of what holding is for.
    expect(useStore.getState().scoped().paneHold["agent:2"]).toBe("s2");
  });
});

/**
 * The layout is persisted, so anything session-shaped in it comes back as a tab
 * pointing at something that ended days ago. Numbering the slots is what keeps
 * that impossible, and this is the assertion that keeps it numbered — a future
 * `agent:${session.id}` would look tidier and quietly break restore.
 */
describe("the saved layout", () => {
  it("names panes, never sessions", async () => {
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
    vi.stubGlobal("WebSocket", FakeSocket);
    localStorage.removeItem("mogeung.layout");

    const { default: App } = await import("@/App");
    const { splitAgent } = await import("@/lib/panes");
    render(<App />);

    await act(async () => {
      splitAgent();
    });

    const { getDock } = await import("@/lib/panes");
    const json = JSON.stringify(getDock()?.toJSON() ?? {});
    expect(json).toContain("agent:2");
    expect(json).not.toContain("s1");
    expect(json).not.toContain("s2");
  });
});
