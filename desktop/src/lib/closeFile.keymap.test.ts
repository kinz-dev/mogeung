/**
 * `Ctrl+F4` closes the file you are reading. Asked for 2026-08-07.
 *
 * The action is exercised through `ACTIONS` rather than by dispatching a key at
 * a mounted `App`, because what is worth pinning here is *which pane it acts
 * on* — that it closes a file and refuses to touch anything else. `keymap.test`
 * already owns the question of whether a chord reaches an action at all, and it
 * pays a mounted window for the privilege.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { ACTIONS } from "@/lib/keymap";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";

const action = ACTIONS.find((a) => a.id === "file.close")!;

const tab = (path: string) => ({ path, rev: null, pinned: true, content: "x", truncated: false, gotoLine: null });

/** A dock with one panel forward, and a `close` we can watch. */
function dockWith(activeId: string | undefined) {
  const close = vi.fn();
  const api = {
    activePanel: activeId ? { id: activeId } : undefined,
    getPanel: (id: string) => (id === activeId ? { api: { close } } : undefined),
    panels: [],
  } as unknown as DockviewApi;
  setDock(api);
  return { api, close };
}

beforeEach(() => {
  useStore.setState({
    selected: "s1",
    explorer: { s1: { ...emptyExplorer(), open: [tab("src/main.rs"), tab("README.md")] as never } },
  });
});

describe("Ctrl+F4", () => {
  it("is what the action is bound to", () => {
    expect(action.keys).toEqual(["Control+F4"]);
  });

  it("closes the file pane that is forward, and only that one", () => {
    const { api, close } = dockWith(filePaneId("s1", "README.md", null));
    action.run(api);

    expect(close).toHaveBeenCalledOnce();
    expect(useStore.getState().explorer.s1.open.map((t) => t.path)).toEqual(["src/main.rs"]);
  });

  /**
   * Every other pane in the centre is permanent, and an Agent pane's close is a
   * *detach* — not what anyone presses a close-the-document chord for. So with
   * an agent forward this does nothing at all, rather than the nearest thing.
   */
  it("does nothing with an agent forward", () => {
    const { api, close } = dockWith("agent");
    action.run(api);

    expect(close).not.toHaveBeenCalled();
    expect(useStore.getState().explorer.s1.open).toHaveLength(2);
  });

  it("does nothing with no pane forward at all", () => {
    const { api, close } = dockWith(undefined);
    action.run(api);

    expect(close).not.toHaveBeenCalled();
    expect(useStore.getState().explorer.s1.open).toHaveLength(2);
  });

  /**
   * It closes the **file**, not merely the pane. `Alt+C` raises the most
   * recently opened file out of this same list, and a pane that closed without
   * clearing the tab would let it reopen what you just dismissed.
   */
  it("forgets the file, so Alt+C cannot raise it again", () => {
    const { api } = dockWith(filePaneId("s1", "src/main.rs", null));
    action.run(api);

    const open = useStore.getState().explorer.s1.open;
    expect(open.some((t) => t.path === "src/main.rs")).toBe(false);
  });
});
