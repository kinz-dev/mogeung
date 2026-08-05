/**
 * Ctrl+wheel has to survive a pane that handles its own scrolling.
 *
 * The Code pane could not be zoomed for one reason: Monaco's scrollable element
 * consumes the wheel and calls `stopPropagation`, so a bubble-phase listener on
 * the wrapper never ran. Nothing errored — the gesture simply did nothing over
 * that one pane, which is indistinguishable from the feature not existing.
 *
 * The child here does what Monaco does. Both tests fail on a bubble-phase
 * listener.
 */

import { describe, expect, it, beforeEach } from "vitest";
import { render } from "@testing-library/react";
import { ZoomPane } from "@/ui/ZoomPane";
import { useStore } from "@/store";

function greedyChild(el: HTMLElement) {
  // Bubble phase, like Monaco's own handler: it sees the event only if nothing
  // upstream claimed it first.
  el.addEventListener("wheel", (e) => e.stopPropagation());
}

function wheel(el: HTMLElement, init: WheelEventInit) {
  const e = new WheelEvent("wheel", { bubbles: true, cancelable: true, ...init });
  el.dispatchEvent(e);
  return e;
}

describe("a pane's Ctrl+wheel zoom", () => {
  beforeEach(() => {
    const { prefs, setPrefs } = useStore.getState();
    setPrefs({ zoom: { ...prefs.zoom, code: 1, agent: 1 } });
  });

  it("reaches the wrapper even when the content swallows the event", () => {
    const { getByTestId } = render(
      <ZoomPane name="code" scale={false}>
        <div data-testid="editor" />
      </ZoomPane>,
    );
    const child = getByTestId("editor");
    greedyChild(child);

    const e = wheel(child, { ctrlKey: true, deltaY: -100 });

    expect(useStore.getState().prefs.zoom.code).toBeGreaterThan(1);
    // Claimed, or the webview zooms the whole document underneath us.
    expect(e.defaultPrevented).toBe(true);
  });

  it("leaves a bare wheel alone, so the content still scrolls", () => {
    const { getByTestId } = render(
      <ZoomPane name="code" scale={false}>
        <div data-testid="editor" />
      </ZoomPane>,
    );
    const child = getByTestId("editor");

    const e = wheel(child, { deltaY: -100 });

    expect(useStore.getState().prefs.zoom.code ?? 1).toBe(1);
    expect(e.defaultPrevented).toBe(false);
  });

  /**
   * The regression the capture-phase fix caused.
   *
   * Making the wrapper capture meant it ran *before* content that handles its
   * own Ctrl+wheel — so the terminal, which changes its font because xterm
   * measures a cell in device pixels, was CSS-scaled instead and started
   * selecting text at the wrong position. Opting out has to be declared now
   * that stopping propagation from the inside is too late.
   */
  it("leaves content that owns its own zoom alone", () => {
    const { getByTestId } = render(
      <ZoomPane name="agent">
        <div data-owns-zoom data-testid="terminal" />
      </ZoomPane>,
    );

    const e = wheel(getByTestId("terminal"), { ctrlKey: true, deltaY: -100 });

    expect(useStore.getState().prefs.zoom.agent ?? 1).toBe(1);
    // Not claimed either, or the content's own handler never gets to act.
    expect(e.defaultPrevented).toBe(false);
  });

  // The other half of the fix — that `scale={false}` leaves the CSS alone, so
  // Monaco is scaled by its font size and not by both — has no test here on
  // purpose: jsdom does not implement the non-standard `zoom` property at all,
  // so `style.zoom` reads `undefined` whatever the component did. An assertion
  // that cannot distinguish the two behaviours is worse than none.
});
