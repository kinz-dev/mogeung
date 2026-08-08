/**
 * What makes a diagram affordable in a pane that keeps moving. `R-J23`.
 *
 * `FilePane.mermaid.test.tsx` owns the wiring — a fence routed, a code block
 * left alone, the sanitiser on. This owns the three properties that let the
 * same component live in the **Transcript**, where the pane re-renders as a
 * live session speaks, scrolling unmounts and remounts turns, and the last
 * answer grows a character at a time.
 *
 * Each is written as the thing it prevents, because each was measured before
 * it was built: a re-parse costs ~56ms, and the failure is not an error —
 * it is a pane that gets slower the more diagrams you have scrolled past.
 *
 * **The transcript pane itself is not mounted here.** Its virtualiser measures
 * a container jsdom gives no height, so it renders no rows at all — a test
 * around it would assert nothing while looking thorough. What is asserted is
 * the component that carries the guarantee, driven the way that pane drives it.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { useState } from "react";
import { Mermaid, __clearMermaidCache } from "@/ui/Mermaid";

const render_ = vi.fn(async (id: string, chart: string) => {
  if (chart.includes("!!broken")) throw new Error("Parse error on line 2");
  return { svg: `<svg data-id="${id}"><title>${chart}</title></svg>` };
});
const initialize = vi.fn();
vi.mock("mermaid", () => ({ default: { initialize, render: render_ } }));

const CHART = "graph TD\n  A-->B";

/**
 * Let the render land. The parse sits behind a `setTimeout` — zero for a
 * diagram that arrived whole, `SETTLE_MS` for one still changing — so
 * flushing microtasks is not enough; each pass has to yield a macrotask too.
 */
const settle = async () => {
  for (let i = 0; i < 3; i++) {
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
  }
};

beforeEach(() => {
  cleanup();
  __clearMermaidCache();
  render_.mockClear();
  initialize.mockClear();
  vi.useRealTimers();
});

describe("a diagram in a pane that keeps moving", () => {
  /**
   * **The guard.** A turn re-renders for reasons that have nothing to do with
   * its diagram — a note edited three turns up, a highlight moving, an events
   * tick. None of them may re-parse it.
   */
  it("is not re-parsed when unrelated content around it changes", async () => {
    function Host() {
      const [tick, setTick] = useState(0);
      return (
        <div>
          <button onClick={() => setTick(tick + 1)}>tick</button>
          <span data-testid="tick">{tick}</span>
          <Mermaid chart={CHART} theme="dark" />
        </div>
      );
    }
    render(<Host />);
    await settle();
    expect(render_).toHaveBeenCalledTimes(1);

    for (let i = 0; i < 5; i++) {
      await act(async () => screen.getByText("tick").click());
    }
    await settle();

    expect(screen.getByTestId("tick").textContent).toBe("5");
    expect(render_).toHaveBeenCalledTimes(1);
  });

  /**
   * The virtualiser's case: scrolling a long transcript unmounts a turn and
   * mounts a fresh one on the way back. Without the cache that is a re-parse
   * per pass, so the pane would get slower the more you scrolled.
   */
  it("survives an unmount and remount without parsing again", async () => {
    const first = render(<Mermaid chart={CHART} theme="dark" />);
    await settle();
    expect(render_).toHaveBeenCalledTimes(1);
    first.unmount();

    render(<Mermaid chart={CHART} theme="dark" />);
    // Painted from cache on the first frame — before any promise settles.
    expect(screen.getByTestId("mermaid")).toBeInTheDocument();
    await settle();
    expect(render_).toHaveBeenCalledTimes(1);
  });

  /**
   * Cached per theme, or a flip would serve a dark diagram to a light pane —
   * and drawn **immediately** rather than after the settle period, because a
   * theme flip is a thing the user just did, not text still arriving.
   */
  it("draws again, at once, when the theme changes", async () => {
    const { rerender } = render(<Mermaid chart={CHART} theme="dark" />);
    await settle();
    rerender(<Mermaid chart={CHART} theme="light" />);
    await settle();

    expect(render_).toHaveBeenCalledTimes(2);
    expect(initialize).toHaveBeenLastCalledWith(expect.objectContaining({ theme: "default" }));
  });

  /**
   * The streaming turn. An assistant answer arrives a character at a time, so
   * a fence is incomplete for as long as it takes to type — every intermediate
   * state is a failed parse and 56ms nobody owes. It waits for quiet, and only
   * the settled diagram is drawn.
   */
  it("waits for a changing diagram to settle rather than parsing every keystroke", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { rerender } = render(<Mermaid chart="graph TD" theme="dark" />);
    await settle();
    expect(render_).toHaveBeenCalledTimes(1); // arrived whole as far as it knows

    for (const partial of ["graph TD\n ", "graph TD\n  A", "graph TD\n  A--", "graph TD\n  A-->B"]) {
      rerender(<Mermaid chart={partial} theme="dark" />);
      await act(async () => {
        vi.advanceTimersByTime(50); // still typing
      });
    }
    expect(render_).toHaveBeenCalledTimes(1);

    await act(async () => {
      vi.advanceTimersByTime(500); // quiet
    });
    await settle();
    expect(render_).toHaveBeenCalledTimes(2);
    expect(render_).toHaveBeenLastCalledWith(expect.any(String), "graph TD\n  A-->B");
    vi.useRealTimers();
  });

  /**
   * A diagram that has drawn once keeps its drawing when an edit does not
   * parse. Replacing a good diagram with a red box on every keystroke is worse
   * than being a moment out of date — and the failure is still said, quietly.
   */
  it("keeps the last good drawing when an edit stops parsing", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { rerender } = render(<Mermaid chart={CHART} theme="dark" />);
    await settle();

    rerender(<Mermaid chart={`${CHART}\n  !!broken`} theme="dark" />);
    await act(async () => {
      vi.advanceTimersByTime(500);
    });
    await settle();

    expect(screen.getByTestId("mermaid")).toBeInTheDocument();
    expect(screen.getByText(/showing the last drawing that did/i)).toBeInTheDocument();
    vi.useRealTimers();
  });
});
