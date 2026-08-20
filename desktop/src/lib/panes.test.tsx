/**
 * Going somewhere has to *take* you there.
 *
 * Clicking a bookmark set the session and the turn and stopped. With the Agent
 * or Git tab forward that is a click with no visible effect — the state was
 * right and the screen never changed, which reads as a broken row rather than
 * as "your destination is behind another tab". The same hole existed for every
 * file opened from outside the Code pane.
 *
 * These fail without the raise: the store ends up correct and the dock is never
 * touched.
 */

import { describe, expect, it, beforeEach, vi } from "vitest";
import { screen } from "@testing-library/react";
import type { DockviewApi } from "dockview";
import { closeAgentPane, filePaneId, jumpToTurn, setDock, showPane, splitAgent } from "@/lib/panes";
import { openFile } from "@/lib/explorer";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";

function fakeDock(existing: string[]) {
  const setActive = vi.fn();
  const addPanel = vi.fn();
  const api = {
    getPanel: (id: string) => (existing.includes(id) ? { api: { setActive } } : undefined),
    addPanel,
  } as unknown as DockviewApi;
  return { api, setActive, addPanel };
}

describe("jumping across panes", () => {
  beforeEach(() => {
    useStore.setState({ selected: null, focusSeq: null, highlightSeq: null });
  });

  /**
   * The Transcript became a **dock tool** on 2026-08-06, so raising it means
   * opening the dock on it rather than activating a tab. The destination
   * changed; the promise — a click takes you there — did not, which is what
   * this still asserts.
   */
  it("raises the Transcript for a marked turn, and points it at the turn", () => {
    const { api } = fakeDock([]);
    setDock(api);
    useStore.getState().setPrefs({ dock: null });

    jumpToTurn("session-a", 42);

    expect(useStore.getState().prefs.dock).toBe("transcript");
    expect(useStore.getState().selected).toBe("session-a");
    expect(useStore.getState().focusSeq).toBe(42);
    expect(useStore.getState().highlightSeq).toBe(42);
  });

  /**
   * `select` clears the focus fields when the session changes, so a jump that
   * published the seq first would wipe it on the way. The order is the fix.
   */
  it("survives the session change that clears the focus fields", () => {
    const { api } = fakeDock(["transcript"]);
    setDock(api);
    useStore.setState({ selected: "session-b" });

    jumpToTurn("session-a", 7);

    expect(useStore.getState().focusSeq).toBe(7);
  });

  it("adds a pane back when it was closed rather than doing nothing", () => {
    const { api, addPanel } = fakeDock([]);
    setDock(api);

    showPane("agent", "Agent");

    expect(addPanel).toHaveBeenCalledWith({
      id: "agent",
      component: "agent",
      title: "Agent",
    });
  });

  /**
   * A dock tool must never be added to the tile tree. Before `showPane` knew
   * the difference, asking for a moved surface added a tab for a component that
   * no longer exists — a dead tab, and a click that did nothing.
   */
  it("never adds a tab for a surface that lives in the dock", () => {
    const { api, addPanel } = fakeDock([]);
    setDock(api);

    for (const id of ["changes", "transcript", "git", "insight", "debt"]) {
      showPane(id, id);
      expect(useStore.getState().prefs.dock).toBe(id);
    }
    expect(addPanel).not.toHaveBeenCalled();
  });

  it("opens a pane for a file opened from anywhere", () => {
    const { api, addPanel } = fakeDock([]);
    setDock(api);

    openFile("session-a", "src/main.rs", { pin: true });

    expect(addPanel).toHaveBeenCalledWith(
      expect.objectContaining({ id: filePaneId("session-a", "src/main.rs", null), component: "file" }),
    );
  });
});

/**
 * One open file, one pane. `R-B53`.
 *
 * These replace the Code pane's lifecycle tests rather than dropping them: the
 * question *"does opening a file put something on screen, and does closing it
 * take it away"* is the same one, asked of a design where the answer is a pane
 * per file instead of one pane that comes and goes.
 */
describe("a file gets a pane of its own", () => {
  it("opens a pane named after the file", async () => {
    const { default: App } = await import("@/App");
    const { getDock } = await import("@/lib/panes");
    const { render, act } = await import("@testing-library/react");
    localStorage.removeItem("mogeung.layout");
    useStore.setState({ selected: "s1", explorer: {} });
    render(<App />);

    await act(async () => {
      openFile("s1", "src/main.rs", { pin: true });
    });

    expect(getDock()?.getPanel(filePaneId("s1", "src/main.rs", null))).toBeDefined();
    expect(screen.getAllByText("main.rs").length).toBeGreaterThan(0);
  });

  it("gives two files two panes rather than two tabs in one", async () => {
    const { default: App } = await import("@/App");
    const { getDock, filePanes } = await import("@/lib/panes");
    const { render, act } = await import("@testing-library/react");
    localStorage.removeItem("mogeung.layout");
    useStore.setState({ selected: "s1", explorer: {} });
    render(<App />);

    await act(async () => {
      openFile("s1", "src/main.rs", { pin: true });
      openFile("s1", "src/lib.rs", { pin: true });
    });

    expect(filePanes(getDock())).toHaveLength(2);
  });

  /**
   * The IntelliJ rule, kept: a file merely browsed past reuses the one unpinned
   * preview. What is new is that the *pane* has to go with it, or the tab would
   * outlive the file it was showing.
   */
  it("reuses the preview pane for an unpinned file", async () => {
    const { default: App } = await import("@/App");
    const { getDock, filePanes } = await import("@/lib/panes");
    const { render, act } = await import("@testing-library/react");
    localStorage.removeItem("mogeung.layout");
    useStore.setState({ selected: "s1", explorer: {} });
    render(<App />);

    await act(async () => {
      openFile("s1", "a.rs");
      openFile("s1", "b.rs");
    });

    expect(filePanes(getDock())).toHaveLength(1);
    expect(getDock()?.getPanel(filePaneId("s1", "b.rs", null))).toBeDefined();
  });

  it("takes the pane away when the file is closed", async () => {
    const { default: App } = await import("@/App");
    const { getDock, filePanes } = await import("@/lib/panes");
    const { closeFile } = await import("@/lib/explorer");
    const { render, act } = await import("@testing-library/react");
    localStorage.removeItem("mogeung.layout");
    useStore.setState({ selected: "s1", explorer: {} });
    render(<App />);

    await act(async () => {
      openFile("s1", "src/main.rs", { pin: true });
    });
    await act(async () => {
      closeFile("s1", "src/main.rs", null);
    });

    expect(filePanes(getDock())).toHaveLength(0);
    expect(useStore.getState().explorer.s1.open).toHaveLength(0);
  });

  /**
   * The reason a `file:` id must never survive a restart: it names a session
   * and a path, and `explorer` is store state rather than preferences, so there
   * is nothing to restore it against.
   */
  it("is stripped from a restored layout rather than resurrected", async () => {
    const { default: App } = await import("@/App");
    const { getDock, filePanes } = await import("@/lib/panes");
    const { render } = await import("@testing-library/react");
    localStorage.setItem(
      "mogeung.layout",
      JSON.stringify({
        grid: { root: { type: "branch", data: [] }, width: 100, height: 100, orientation: "HORIZONTAL" },
        panels: {},
      }),
    );
    useStore.setState({ selected: "s1", explorer: {} });
    render(<App />);
    expect(filePanes(getDock())).toHaveLength(0);
  });
});

/**
 * Closing an Agent pane, slot 1 included. 2026-08-19.
 *
 * The base `agent` pane refused to close until this, and the asymmetry was
 * reported as a bug: with two panes up, the one you are finished with is as
 * often the first as the second, and only the second would go. The guard is
 * gone — these pin that it went, and that the hold went with the pane.
 */
/**
 * Splitting, and what the new pane arrives holding. `R-J35`, asked for
 * 2026-08-20: *"by default please set the anchore = true when openning a new
 * panel"*.
 *
 * Both of these fail against the split as it stood: it added a pane and wrote
 * no hold at all, so two panes followed the queue and the next click collapsed
 * the arrangement you had just made into two views of one session.
 */
describe("splitting an Agent pane", () => {
  function dockWithSplit(existing: string[]) {
    const panels = [...existing];
    const addPanel = vi.fn((opts: { id: string }) => panels.push(opts.id));
    const api = {
      getPanel: (id: string) => (panels.includes(id) ? { api: { setActive: vi.fn() } } : undefined),
      addPanel,
      activeGroup: undefined,
      get panels() {
        return panels.map((id) => ({ id }));
      },
    } as unknown as DockviewApi;
    return { api, addPanel };
  }

  beforeEach(() => {
    useStore.setState({ prefs: defaultPrefs(), selected: null });
  });

  it("moors the new pane on the session you split from", () => {
    const { api } = dockWithSplit(["agent"]);
    setDock(api);
    useStore.setState({ selected: "s1" });

    expect(splitAgent()).toBe("agent:2");
    expect(useStore.getState().scoped().paneHold["agent:2"]).toBe("s1");
  });

  /**
   * The pane you split *from* keeps following the queue, and that is what
   * keeps the queue working: with every pane moored, a click on a new session
   * has to split again to land anywhere.
   */
  it("leaves the pane it split from following the selection", () => {
    const { api } = dockWithSplit(["agent"]);
    setDock(api);
    useStore.setState({ selected: "s1" });

    splitAgent();

    expect(useStore.getState().scoped().paneHold["agent"]).toBeUndefined();
  });

  /** An anchor needs a session. Holding a pane on nothing is not a state. */
  it("writes no anchor when nothing is selected", () => {
    const { api } = dockWithSplit(["agent"]);
    setDock(api);

    splitAgent();

    expect(useStore.getState().scoped().paneHold).toEqual({});
  });
});

describe("closing an Agent pane", () => {
  function dockWithClose(existing: string[]) {
    const close = vi.fn();
    const api = {
      getPanel: (id: string) => (existing.includes(id) ? { api: { close } } : undefined),
      addPanel: vi.fn(),
    } as unknown as DockviewApi;
    return { api, close };
  }

  beforeEach(() => {
    useStore.setState({ prefs: defaultPrefs(), selected: null });
  });

  it("closes the base pane, which used to be the one exception", () => {
    const { api, close } = dockWithClose(["agent"]);
    setDock(api);

    closeAgentPane("agent");

    expect(close).toHaveBeenCalledOnce();
  });

  it("closes an extra pane as it always did", () => {
    const { api, close } = dockWithClose(["agent", "agent:2"]);
    setDock(api);

    closeAgentPane("agent:2");

    expect(close).toHaveBeenCalledOnce();
  });

  /**
   * The hold has to go with the pane whichever slot it was. Left behind, the
   * next split into that number arrives already moored to a session you chose
   * last week — which reads as the split ignoring your selection.
   */
  it("lets go of the session the closed pane was held on", () => {
    const { api } = dockWithClose(["agent"]);
    setDock(api);
    useStore.setState({
      prefs: { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), paneHold: { agent: "s1", "agent:2": "s2" } } } },
    });

    closeAgentPane("agent");

    expect(useStore.getState().scoped().paneHold).toEqual({ "agent:2": "s2" });
  });
});
