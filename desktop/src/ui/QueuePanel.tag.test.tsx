/**
 * Does a colour tag actually reach the row?
 *
 * The tint is two things arriving together — a class and a `--tag-bg` property
 * — on an element that **Radix clones**: the queue row is a context-menu
 * trigger, so `asChild` merges our props into its own. That path has already
 * cost this codebase once, and its failure mode is the reason this is a test
 * rather than a look: nothing throws, nothing logs, the row simply renders
 * untagged — which is indistinguishable from the complaint the tint was built
 * to answer.
 */

import { describe, expect, it, vi, beforeAll, afterEach } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { Session } from "@/wire/types";

class FakeSocket {
  static OPEN = 1;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  constructor(public url: string) {}
  send() {}
  close() {}
}

beforeAll(() => {
  vi.stubGlobal("WebSocket", FakeSocket);
  // The queue is virtualised, and in jsdom every element is zero by zero — so
  // a list that draws only what fits draws **nothing**. Giving the measurement
  // a size is what makes this a test of the row rather than of jsdom's layout
  // engine, and all three pieces are needed: the observer has to *fire*, and
  // `@tanstack/react-virtual` takes its first rect from `offsetHeight` before
  // any observer runs at all.
  //
  // Worth knowing beyond this file: the window's smoke test renders the same
  // board and its comment says a row on screen is the whole point of it.
  // Without these stubs there is no row, so that test is currently proving
  // less than it says — the throw it was written for is in `QueueRow`.
  class SizedResizeObserver {
    constructor(private cb: ResizeObserverCallback) {}
    observe(el: Element) {
      this.cb(
        [{ target: el, contentRect: { width: 320, height: 800 } } as unknown as ResizeObserverEntry],
        this as unknown as ResizeObserver,
      );
    }
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal("ResizeObserver", SizedResizeObserver);
  for (const prop of ["offsetHeight", "clientHeight"]) {
    Object.defineProperty(HTMLElement.prototype, prop, { configurable: true, get: () => 800 });
  }
  for (const prop of ["offsetWidth", "clientWidth"]) {
    Object.defineProperty(HTMLElement.prototype, prop, { configurable: true, get: () => 320 });
  }
});

const SESSION = {
  id: "s1",
  title: "a session",
  cwd: "/tmp/repo",
  repo_root: "/tmp/repo",
  alive: true,
  touched_files: [],
  collisions: [],
  verify_runs: [],
  claims: [],
  last_event_at: new Date().toISOString(),
} as unknown as Session;

async function board(opts: { tag?: string; selected?: boolean }) {
  // Renders stack otherwise, and a `find` over the result would keep answering
  // from the *first* board — so a loop asserting three states would silently
  // assert the same one three times.
  cleanup();
  const { default: App } = await import("@/App");
  const { useStore } = await import("@/store");
  const { loadPrefs } = await import("@/store/prefs");
  const prefs = loadPrefs();
  useStore.setState({
    prefs: {
      ...prefs,
      scoped: {
        ...prefs.scoped,
        // The key `scoped()` resolves to with no daemon connected.
        unknown: {
          ...(prefs.scoped.unknown ?? {}),
          hidden: [],
          pinned: [],
          labels: {},
          bookmarks: [],
          editorWrap: [],
          tags: opts.tag ? { s1: opts.tag } : {},
        },
      },
    } as typeof prefs,
    sessions: { s1: SESSION },
    queue: [{ session_id: "s1", reason: "awaiting_input", score: 1, detail: "waiting" }],
    firstSnapshot: true,
    selected: opts.selected ? "s1" : null,
  });
  render(<App />);
  return screen.getAllByRole("option").find((el) => el.textContent?.includes("a session"))!;
}

afterEach(async () => {
  const { useStore } = await import("@/store");
  useStore.setState({ sessions: {}, queue: [], firstSnapshot: false, selected: null });
  localStorage.clear();
});

describe("a tagged queue row", () => {
  it("carries the tint through Radix's clone", async () => {
    const row = await board({ tag: "green" });
    expect(row.className).toContain("mogeung-tag-row");
    // The palette variable, not a colour: a hex here would be a value that
    // exists in one theme and was checked against neither.
    expect(row.style.getPropertyValue("--tag-bg")).toBe("var(--tag-green-bg)");
  });

  /**
   * The report that rewrote this rule: withholding the tint meant a tagged row
   * stopped looking tagged the moment you clicked it — which is when you are
   * most likely to be checking you are on the right session. Both signals now,
   * and this is the test that says so.
   */
  it("keeps its colour while selected, and still says it is selected", async () => {
    const row = await board({ tag: "green", selected: true });
    expect(row.className).toContain("mogeung-tag-row");
    expect(row.style.getPropertyValue("--tag-bg")).toBe("var(--tag-green-bg)");
    // Not `--selection-bg`: that would replace the colour, which is the thing
    // being fixed. A ring, and the wash the stylesheet hangs off this
    // attribute — so the attribute is load-bearing, not only assistive.
    expect(row.getAttribute("aria-selected")).toBe("true");
    expect(row.className).toContain("outline-[var(--selection-stroke)]");
    expect(row.className).not.toContain("bg-[var(--selection-bg)]");
  });

  it("gives an untagged row the plain selection surface", async () => {
    const row = await board({ selected: true });
    expect(row.getAttribute("aria-selected")).toBe("true");
    expect(row.className).toContain("bg-[var(--selection-bg)]");
  });

  it("leaves an untagged row on the plain card surface", async () => {
    const row = await board({});
    expect(row.className).not.toContain("mogeung-tag-row");
    expect(row.style.getPropertyValue("--tag-bg")).toBe("");
    expect(row.className).toContain("bg-[var(--row-bg)]");
  });

  /**
   * Three class rules wanted this one property — the card, the tint and the
   * selection — and which won would have come down to the order Tailwind
   * happened to emit them in. Only one may be present at a time.
   */
  it("never asks for two backgrounds at once", async () => {
    for (const opts of [{ tag: "red" }, { selected: true }, { tag: "red", selected: true }]) {
      const row = await board(opts);
      const backgrounds = [
        row.className.includes("bg-[var(--row-bg)]"),
        row.className.includes("mogeung-tag-row"),
        row.className.includes("bg-[var(--selection-bg)]"),
      ].filter(Boolean);
      expect(`${JSON.stringify(opts)} -> ${backgrounds.length}`).toBe(`${JSON.stringify(opts)} -> 1`);
    }
  });
});
