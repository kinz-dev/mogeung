/**
 * One press, one pane. `R-B52`.
 *
 * Reported 2026-08-07: *"I have a screen at the middle but when I move left or
 * right, the focus flips between the left session and the right session and
 * never comes back to the middle"*. The geometry was right — `spatial.test.ts`
 * already pinned it — and the wiring was not: the chord was handled by the
 * keymap **and** by the terminal's own key handler, so one keystroke ran the
 * move twice and stepped straight over the middle pane.
 *
 * The fix moved the *action* to the keymap alone and left the terminal
 * swallowing only. What these pin is the property that was broken rather than
 * the arrangement of the fix: a move goes one step, and it lands somewhere you
 * can type.
 *
 * **What they do not do, said plainly: reproduce the defect.** The two call
 * sites were the keymap and xterm's key handler, and in jsdom the second one
 * does not exist — `TerminalView` renders "the terminal needs the desktop
 * build" without ever constructing an `Xterm`, so there is nothing to fire
 * twice. A test that cannot see the second caller cannot catch a second caller
 * arriving again. These are the guard against the *geometry* regressing and
 * against a move that forgets to bring the keyboard; the wiring is held by
 * `defaultPrevented` being the single rule for "the window already claimed
 * this", which is one line in one place rather than a list to keep in sync.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { movePane, setDock } from "@/lib/panes";

/** Three panes in a row: `a | b | c`, `b` in the middle. */
function threeInARow(activeId: string) {
  const setActive = vi.fn();
  const groups = ["a", "b", "c"].map((id, i) => {
    const element = document.createElement("div");
    element.getBoundingClientRect = () =>
      ({ left: i * 100, right: i * 100 + 100, top: 0, bottom: 100 }) as DOMRect;
    return { id, element, api: { setActive: () => setActive(id) } };
  });
  const api = {
    groups,
    activeGroup: groups.find((g) => g.id === activeId),
  } as unknown as DockviewApi;
  setDock(api);
  return { setActive };
}

beforeEach(() => {
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    cb(0);
    return 0;
  });
});

describe("moving one pane at a time", () => {
  /** The bug, exactly: from the left pane, "right" must stop at the middle. */
  it("lands on the middle pane rather than stepping over it", () => {
    const { setActive } = threeInARow("a");
    movePane("right");
    expect(setActive).toHaveBeenCalledTimes(1);
    expect(setActive).toHaveBeenCalledWith("b");
  });

  it("does the same coming back the other way", () => {
    const { setActive } = threeInARow("c");
    movePane("left");
    expect(setActive).toHaveBeenCalledWith("b");
  });

  it("stops at the edge rather than wrapping", () => {
    const { setActive } = threeInARow("a");
    movePane("left");
    expect(setActive).not.toHaveBeenCalled();
  });

  it("does nothing when there is no active group to move from", () => {
    setDock({ groups: [], activeGroup: undefined } as unknown as DockviewApi);
    expect(() => movePane("right")).not.toThrow();
  });

  /**
   * Landing means the terminal, not the tab. Activating the group alone would
   * leave the keyboard behind, so the next thing typed would go to the pane you
   * just left — this feature's own failure, running backwards.
   */
  it("puts the keyboard in the pane it arrived at", () => {
    const setActive = vi.fn();
    const withTerminal = document.createElement("div");
    const textarea = document.createElement("textarea");
    textarea.className = "xterm-helper-textarea";
    withTerminal.appendChild(textarea);
    document.body.appendChild(withTerminal);

    const groups = [
      (() => {
        const el = document.createElement("div");
        el.getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 100 }) as DOMRect;
        return { id: "a", element: el, api: { setActive } };
      })(),
      (() => {
        withTerminal.getBoundingClientRect = () =>
          ({ left: 100, right: 200, top: 0, bottom: 100 }) as DOMRect;
        return { id: "b", element: withTerminal, api: { setActive } };
      })(),
    ];
    setDock({ groups, activeGroup: groups[0] } as unknown as DockviewApi);

    movePane("right");

    expect(document.activeElement).toBe(textarea);
  });
});
