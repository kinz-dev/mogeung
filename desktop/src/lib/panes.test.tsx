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
import { jumpToTurn, setDock, showPane } from "@/lib/panes";
import { openFile } from "@/lib/explorer";
import { useStore } from "@/store";

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

  it("raises the Code pane when a file is opened from anywhere", () => {
    const { api, setActive } = fakeDock(["code"]);
    setDock(api);

    openFile("session-a", "src/main.rs", { pin: true });

    expect(setActive).toHaveBeenCalled();
  });
});

/**
 * The Code pane is the one pane with nothing to say by itself. Every other pane
 * describes the selected session; an empty Code tab is a promise of a file that
 * is not there.
 */
describe("the Code pane comes and goes with its files", () => {
  it("is not a tab when nothing is open, and is one the moment a file opens", async () => {
    const { default: App } = await import("@/App");
    const { render, act } = await import("@testing-library/react");
    useStore.setState({ selected: "s1", explorer: {} });
    render(<App />);

    expect(screen.queryByText("Code")).toBeNull();

    await act(async () => {
      openFile("s1", "src/main.rs", { pin: true });
    });
    expect(screen.getByText("Code")).toBeInTheDocument();

    // ...and goes again when the last file is closed.
    await act(async () => {
      useStore.getState().patchExplorer("s1", { open: [] });
    });
    expect(screen.queryByText("Code")).toBeNull();
  });
});
