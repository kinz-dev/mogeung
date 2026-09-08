/**
 * The half of a pop-out that has no window in it. `R-B55`,
 * [ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md).
 *
 * Popping out **moves** a pane: it closes here as the window opens there, so
 * closing that window has to put one back. Two ways that goes wrong and neither
 * is visible from the popout's side:
 *
 * - It comes back showing **the wrong session**. The pane returns for the
 *   session it was watching, which by then is very likely not the selection —
 *   that is the whole reason it was worth detaching. A pane that returned
 *   following the queue would look like a different pane arriving.
 * - It comes back **twice**. Open the session again by hand while its popout is
 *   still up, then close the popout, and a second anchored pane appears for a
 *   session already on screen.
 */

import { describe, expect, it, beforeEach, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { returnAgentPane, setDock } from "@/lib/panes";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";

function fakeDock(existing: string[]) {
  const addPanel = vi.fn((p: { id: string }) => existing.push(p.id));
  const api = {
    get panels() {
      return existing.map((id) => ({ id }));
    },
    getPanel: (id: string) => (existing.includes(id) ? { api: { setActive: vi.fn() } } : undefined),
    addPanel,
    activeGroup: undefined,
  } as unknown as DockviewApi;
  return { api, addPanel, existing };
}

const SESSION = "0b3f9c2a-1d4e-4f77-9a2b-6c5d8e1f0a3b";

/** The hold map is machine-scoped, so a machine has to be set for it to land. */
function freshStore() {
  useStore.setState({
    prefs: defaultPrefs(),
    machineId: "machine-a",
    selected: "some-other-session",
    scopedState: { "machine-a": emptyScoped() },
  } as never);
}

describe("taking a pane back when its window closes", () => {
  beforeEach(() => {
    localStorage.clear();
    freshStore();
  });

  it("adds a pane anchored to the session that was popped out", () => {
    const { api, addPanel } = fakeDock([]);
    setDock(api);

    const id = returnAgentPane(SESSION);

    expect(id).toBe("agent");
    expect(addPanel).toHaveBeenCalledTimes(1);
    expect(useStore.getState().scoped().paneHold["agent"]).toBe(SESSION);
  });

  /**
   * The anchor is the point. Without it the pane follows `selected`, which is
   * some other session by now — so the pane you got back would be showing
   * something you did not ask for.
   */
  it("does not come back following the selection", () => {
    const { api } = fakeDock([]);
    setDock(api);

    returnAgentPane(SESSION);

    expect(useStore.getState().scoped().paneHold["agent"]).not.toBe(
      useStore.getState().selected,
    );
  });

  it("takes the next free slot when one pane is already open", () => {
    const { api, addPanel } = fakeDock(["agent"]);
    setDock(api);

    const id = returnAgentPane(SESSION);

    expect(id).toBe("agent:2");
    expect(addPanel).toHaveBeenCalledTimes(1);
  });

  /** Closing a popout for a session you have since reopened by hand. */
  it("does not add a second pane for a session already anchored", () => {
    const { api, addPanel } = fakeDock(["agent"]);
    setDock(api);
    const { scoped, setScoped } = useStore.getState();
    setScoped({ paneHold: { ...scoped().paneHold, agent: SESSION } });

    const id = returnAgentPane(SESSION);

    expect(id).toBe("agent");
    expect(addPanel).not.toHaveBeenCalled();
  });
});
