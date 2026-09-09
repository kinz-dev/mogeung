/**
 * A pane rebuilds itself when it changes window, and only then. `R-B55`.
 *
 * Reported 2026-09-09, from the running app: *"when I type in that panel, it is
 * not able to show the update. But when I switch the focus to another app and
 * switch it back to mogeung, I can see the word that I typed just now."*
 *
 * dockview moves a group's **DOM** into the popout document while the React
 * tree — and every listener in it — keeps running in the *opener's* context.
 * That is what makes one dockview across two windows possible, and it is also
 * the trap: xterm and Monaco both drive painting from `requestAnimationFrame`
 * on the window they captured when they were built, which is the main one.
 * Focus the popout and the opener stops being the focused window, so its
 * animation frames are throttled and nothing repaints. Alt-tab away and back,
 * the frames resume, and every deferred paint lands at once — which is why the
 * keystrokes looked late rather than lost. React commits throughout, because
 * its scheduler is not rAF-based.
 *
 * Rebuilding against the document the pane is now in fixes it. The half worth
 * testing as hard as the fix is the **restraint**: `onDidLocationChange` fires
 * on every group change, including an ordinary drag between two splits of one
 * window, and remounting there would detach and reattach tmux for nothing.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import type { IDockviewPanelProps } from "dockview";
import { pane } from "@/App";

/** A panel api with the two things the wrapper reads, and a way to move it. */
function fakePanel(startWindow: Window) {
  const listeners: (() => void)[] = [];
  let current = startWindow;
  const api = {
    id: "agent",
    isVisible: true,
    onDidVisibilityChange: () => ({ dispose: () => {} }),
    onDidLocationChange: (cb: () => void) => {
      listeners.push(cb);
      return { dispose: () => {} };
    },
    getWindow: () => current,
  };
  return {
    props: { api } as unknown as IDockviewPanelProps,
    /** What dockview does on a pop-out: the window changes, then the event. */
    moveTo(next: Window) {
      current = next;
      listeners.forEach((cb) => cb());
    },
    /** What it does on an ordinary split: same window, event fires anyway. */
    regroup() {
      listeners.forEach((cb) => cb());
    },
  };
}

/** Counts **constructions** — which is what rebuilding a terminal or an
 *  editor against a new document actually is. */
const mounts = { count: 0 };
function Body() {
  mounts.count += 1;
  return <div data-testid="body" />;
}

const Pane = pane("agent", Body, { scale: false });

beforeEach(() => {
  cleanup();
  mounts.count = 0;
});

describe("a pane that changes window", () => {
  it("rebuilds when it moves to another window", () => {
    const other = { name: "popout" } as unknown as Window;
    const panel = fakePanel(window);
    render(<Pane {...panel.props} />);
    expect(mounts.count).toBe(1);

    act(() => panel.moveTo(other));

    expect(mounts.count).toBe(2);
  });

  /** The restraint. An ordinary split must not cost a tmux reattach. */
  it("does not rebuild when it only changes group", () => {
    const panel = fakePanel(window);
    render(<Pane {...panel.props} />);
    expect(mounts.count).toBe(1);

    act(() => panel.regroup());
    act(() => panel.regroup());

    expect(mounts.count).toBe(1);
  });

  it("rebuilds again when it is docked back", () => {
    const other = { name: "popout" } as unknown as Window;
    const panel = fakePanel(window);
    render(<Pane {...panel.props} />);

    act(() => panel.moveTo(other));
    act(() => panel.moveTo(window));

    expect(mounts.count).toBe(3);
  });
});
