/**
 * Close all, and close all others. `R-J47`.
 *
 * Asked for 2026-08-25. The three tests that matter are not "it closes them":
 * they are about *which* tabs it reaches (the group, never across the sash),
 * *how* each one closes (a file leaves `explorer` too, an Agent pane drops its
 * anchor), and the remove-while-iterating trap — closing a tab mutates the very
 * array `groupPanes` reads, so a naive loop skips every second pane and the
 * failure looks like the menu half-working rather than like a bug.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { closeAllTabs, closeOtherTabs, closeTab } from "@/lib/closeTabs";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

/**
 * A dock of one or two tab groups that records what was closed — and, like the
 * real thing, drops a closed panel out of its group's array.
 */
function fakeDock(groups: string[][]) {
  const closed: string[] = [];
  const live = groups.map((g) => [...g]);
  const panelOf = (id: string) => {
    const group = live.find((g) => g.includes(id));
    if (!group) return undefined;
    return {
      id,
      group: { panels: group.map((pid) => ({ id: pid })) },
      api: {
        close: () => {
          closed.push(id);
          group.splice(group.indexOf(id), 1);
        },
        setActive: vi.fn(),
      },
    };
  };
  const api = { getPanel: panelOf, addPanel: vi.fn() } as unknown as DockviewApi;
  return { api, closed };
}

beforeEach(() => {
  localStorage.clear();
  useStore.setState({ prefs: defaultPrefs(), sessions: {}, selected: null, explorer: {} } as never);
});

const file = (path: string) => filePaneId("s1", path, null);

describe("closing tabs from the tab menu", () => {
  it("closes every tab in the group, itself included", () => {
    const dock = fakeDock([["agent", file("a.rs"), file("b.rs")]]);
    setDock(dock.api);

    closeAllTabs("agent");

    expect(dock.closed).toEqual(["agent", file("a.rs"), file("b.rs")]);
  });

  it("keeps the one you asked from when closing the others", () => {
    const dock = fakeDock([["agent", file("a.rs"), file("b.rs")]]);
    setDock(dock.api);

    closeOtherTabs(file("a.rs"));

    expect(dock.closed).toEqual(["agent", file("b.rs")]);
  });

  /**
   * The whole reason `groupPanes` returns ids read in one go. With the live
   * array walked directly, closing `a` shifts `b` into the slot the loop has
   * already passed and `b` survives.
   */
  it("does not skip a tab because closing its neighbour moved it", () => {
    const dock = fakeDock([[file("a.rs"), file("b.rs"), file("c.rs"), file("d.rs")]]);
    setDock(dock.api);

    closeAllTabs(file("a.rs"));

    expect(dock.closed).toHaveLength(4);
  });

  /** Two Agent panes split side by side are two groups. */
  it("never reaches across the sash into another group", () => {
    const dock = fakeDock([["agent", file("a.rs")], ["agent:2", file("b.rs")]]);
    setDock(dock.api);

    closeAllTabs("agent");

    expect(dock.closed).toEqual(["agent", file("a.rs")]);
  });

  /** An Agent pane's anchor goes with it, or the next pane in that slot
   *  arrives already held on a session you chose last week. */
  it("drops the anchor of an Agent pane it closes", () => {
    const dock = fakeDock([["agent", "agent:2"]]);
    setDock(dock.api);
    useStore.getState().setScoped({ paneHold: { agent: "s1", "agent:2": "s2" } });

    closeAllTabs("agent");

    expect(useStore.getState().scoped().paneHold).toEqual({});
  });

  /** A file pane is also a row in `explorer`'s open list, and both go. */
  it("takes a closed file out of the explorer's open tabs", () => {
    const dock = fakeDock([[file("a.rs"), file("b.rs")]]);
    setDock(dock.api);
    useStore.getState().patchExplorer("s1", {
      open: [
        { path: "a.rs", rev: null },
        { path: "b.rs", rev: null },
      ] as never,
    });

    closeTab(file("a.rs"));

    expect(useStore.getState().explorer.s1?.open.map((t) => t.path)).toEqual(["b.rs"]);
    expect(dock.closed).toEqual([file("a.rs")]);
  });
});
