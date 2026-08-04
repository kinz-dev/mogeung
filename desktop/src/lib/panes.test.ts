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

  it("raises the Transcript for a marked turn, and points it at the turn", () => {
    const { api, setActive } = fakeDock(["transcript"]);
    setDock(api);

    jumpToTurn("session-a", 42);

    expect(setActive).toHaveBeenCalled();
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

    showPane("transcript", "Transcript");

    expect(addPanel).toHaveBeenCalledWith({
      id: "transcript",
      component: "transcript",
      title: "Transcript",
    });
  });

  it("raises the Code pane when a file is opened from anywhere", () => {
    const { api, setActive } = fakeDock(["code"]);
    setDock(api);

    openFile("session-a", "src/main.rs", { pin: true });

    expect(setActive).toHaveBeenCalled();
  });
});
