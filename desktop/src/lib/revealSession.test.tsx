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
 * added, the third because something is, and the fourth because the ceiling
 * was reached in silence.
 */

import { describe, expect, it, beforeEach, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { MAX_AGENT_PANES, revealSession, setDock } from "@/lib/panes";
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
    // The new pane arrives unheld, so the selection is what points it at `s2`.
    expect(useStore.getState().selected).toBe("s2");
    expect(useStore.getState().scoped().paneHold["agent:2"]).toBeUndefined();
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
   * Four held panes is a state you got into deliberately, and the click that
   * cannot be answered says so — silence here is indistinguishable from the
   * bug this row fixed.
   */
  it("says so at the ceiling instead of failing quietly", () => {
    const ids = Array.from({ length: MAX_AGENT_PANES }, (_, i) => (i === 0 ? "agent" : `agent:${i + 1}`));
    const { api, addPanel } = fakeDock(ids);
    setDock(api);
    holds(Object.fromEntries(ids.map((id, i) => [id, `s${i}`])));

    revealSession("late");

    expect(addPanel).not.toHaveBeenCalled();
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/every Agent pane is held/);
  });
});
