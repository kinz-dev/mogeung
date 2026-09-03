/**
 * The picker behind `Ctrl+Alt+Shift+Insert`. `R-L5`.
 *
 * Picking a language asks the daemon for a file with that extension and
 * nothing else — the pane opens when the answer comes back, which
 * `scratch.test.ts` covers. Picking an existing name opens it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Palette } from "@/ui/Palette";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { setDock } from "@/lib/panes";

const send = vi.fn();
const addPanel = vi.fn();

function open(names: string[] = []) {
  useStore.setState({
    send,
    prefs: defaultPrefs(),
    paletteOpen: true,
    paletteMode: "scratch",
    scratch: { names, files: {} },
  });
  setDock({ getPanel: () => undefined, panels: [], addPanel, activeGroup: undefined } as never);
  return render(<Palette dock={{ current: null }} />);
}

beforeEach(() => {
  send.mockReset();
  addPanel.mockReset();
});
afterEach(() => cleanup());

describe("the scratch picker", () => {
  it("asks for the list on open and offers Java first", () => {
    open();
    expect(send).toHaveBeenCalledWith({ cmd: "scratch_list" });
    const items = screen.getAllByRole("option").map((el) => el.textContent);
    expect(items[0]).toMatch(/Java/);
  });

  it("picking a language creates a file with that extension and closes", () => {
    open();
    fireEvent.click(screen.getByText("SQL"));
    expect(send).toHaveBeenCalledWith({ cmd: "scratch_create", ext: "sql" });
    expect(useStore.getState().paletteOpen).toBe(false);
    // Not opened here: the name is the daemon's to choose.
    expect(addPanel).not.toHaveBeenCalled();
  });

  it("lists what already exists and opens it as its own pane", () => {
    open(["scratch-2.md", "scratch-1.java"]);
    fireEvent.click(screen.getByText("scratch-1.java"));
    expect(addPanel).toHaveBeenCalledWith(
      expect.objectContaining({ id: "scratch:scratch-1.java", component: "scratch", title: "scratch-1.java" }),
    );
    expect(send.mock.calls.some(([m]) => m.cmd === "scratch_create")).toBe(false);
  });
});
