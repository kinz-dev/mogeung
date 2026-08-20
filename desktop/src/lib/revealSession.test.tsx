/**
 * Clicking a queue row has to put the session **on screen**. `R-J31`.
 *
 * `select` alone only ever did half of that, and which half depended on state
 * the queue cannot see: a selection is what the *unheld* panes follow, so with
 * every pane held it moved the dock, the rail and the status bar while the
 * centre — the part being looked at — did not change at all. With one pane,
 * held, that is the whole window ignoring the click.
 *
 * All four of these fail against `select`: the first two because nothing is
 * added, the third because something is, and the fourth because a session with
 * every pane already moored would land nowhere at all.
 */

import { describe, expect, it, beforeEach, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { revealSession, setDock } from "@/lib/panes";
import { useStore } from "@/store";

/**
 * A dock that remembers what it holds, unlike `panes.test.tsx`'s — these
 * assertions are about *how many* panes there are afterwards, so the fake has
 * to grow when one is added.
 */
function fakeDock(initial: string[]) {
  const panels = new Map<string, { api: { setActive: ReturnType<typeof vi.fn> } }>();
  const activated: string[] = [];
  for (const id of initial) {
    panels.set(id, { api: { setActive: vi.fn(() => activated.push(id)) } });
  }
  const addPanel = vi.fn((opts: { id: string }) => {
    panels.set(opts.id, { api: { setActive: vi.fn(() => activated.push(opts.id)) } });
  });
  const api = {
    getPanel: (id: string) => panels.get(id),
    addPanel,
    activeGroup: undefined,
    // Dockview's own `panels` array, which `agentSlots` reads now that there
    // is no ceiling to count up to. A getter, so a pane added mid-test is in
    // it — the same reason this fake remembers what it holds at all.
    get panels() {
      return [...panels.keys()].map((id) => ({ id }));
    },
  } as unknown as DockviewApi;
  return { api, addPanel, activated, panels };
}

/** Holds are machine-scoped, and no daemon has identified itself in a test. */
function holds(paneHold: Record<string, string>) {
  useStore.setState({
    prefs: { ...useStore.getState().prefs, scoped: { unknown: { ...useStore.getState().scoped(), paneHold } } },
  });
}

describe("putting a session on screen", () => {
  beforeEach(() => {
    useStore.setState({ selected: null, notices: [] });
    holds({});
  });

  /**
   * The report, exactly: one Agent pane, anchored, and a click on another
   * session with nowhere to land.
   */
  it("splits a pane when the only one is held on somebody else", () => {
    const { api, addPanel } = fakeDock(["agent"]);
    setDock(api);
    holds({ agent: "s1" });

    revealSession("s2");

    expect(addPanel).toHaveBeenCalledWith(expect.objectContaining({ id: "agent:2", component: "agent" }));
    expect(useStore.getState().selected).toBe("s2");
    // **Anchored on arrival** since `R-J35`. It used to come unheld and be
    // pointed at `s2` by the selection, which looks the same in this frame and
    // stops looking the same on the next queue click.
    expect(useStore.getState().scoped().paneHold["agent:2"]).toBe("s2");
  });

  /**
   * A session with a home already has one. Splitting here would put the same
   * agent in two panes, which is the arrangement `R-B49` exists *not* to make.
   */
  it("raises the pane already holding that session rather than adding another", () => {
    const { api, addPanel, activated } = fakeDock(["agent", "agent:2"]);
    setDock(api);
    holds({ agent: "s1", "agent:2": "s2" });

    revealSession("s2");

    expect(addPanel).not.toHaveBeenCalled();
    expect(activated).toEqual(["agent:2"]);
    expect(useStore.getState().selected).toBe("s2");
  });

  /**
   * The case that must stay cheap. Two unheld panes both follow the selection,
   * so a click is a selection and nothing else — a split per click would reach
   * the ceiling in four.
   *
   * Still the rule with the ceiling gone (`R-J35`) and with new panes arriving
   * anchored: what makes this cheap is that the pane which came back with the
   * layout is *not* anchored, so there is always something following the queue
   * until you moor it yourself.
   */
  it("adds nothing while a pane is still free to follow the selection", () => {
    const { api, addPanel } = fakeDock(["agent", "agent:2"]);
    setDock(api);
    holds({ agent: "s1" });

    revealSession("s3");

    expect(addPanel).not.toHaveBeenCalled();
    expect(useStore.getState().selected).toBe("s3");
  });

  /**
   * Four held panes used to be the end of the road: the click was refused with
   * a notice, because the ceiling was four. `R-J35` took the ceiling out, so
   * the fifth is a pane like any other — and it, too, arrives anchored.
   */
  it("keeps going past the four panes that used to be the ceiling", () => {
    const ids = Array.from({ length: 4 }, (_, i) => (i === 0 ? "agent" : `agent:${i + 1}`));
    const { api, addPanel } = fakeDock(ids);
    setDock(api);
    holds(Object.fromEntries(ids.map((id, i) => [id, `s${i}`])));

    revealSession("late");

    expect(addPanel).toHaveBeenCalledWith(expect.objectContaining({ id: "agent:5", component: "agent" }));
    expect(useStore.getState().scoped().paneHold["agent:5"]).toBe("late");
    expect(useStore.getState().notices).toEqual([]);
  });
});
