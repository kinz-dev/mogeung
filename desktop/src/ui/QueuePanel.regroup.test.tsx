/**
 * Rows keep their height when the Attention list regroups. `R-J129`.
 *
 * Reported 2026-10-11 with a screenshot: grouped by machine and repository,
 * the list drew a blank band under some headings and one machine's heading
 * straight across a session's card. The list is virtualised, and the
 * virtualizer remembers each row's measured height by **index** unless it is
 * given a key — so a row that moves index, because a group above it folded
 * or a grouping was toggled, inherits the height of whatever used to sit at
 * that index. React keeps the same element for it, so it is never measured
 * again, and the wrong height stays.
 *
 * Pinned with a jsdom in which a heading is 24px and a card 74px: after every
 * regroup, each row starts exactly where the row above it ends, and the list
 * is exactly as tall as its rows. Both cases fail on a list keyed by index.
 */

import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
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

// Deliberately not the panel's estimates (24 and 74): a measurement equal to
// the estimate is never cached, and the fault lives in the cache. The real
// rows differ from the estimates too — that is why the screenshot shows it.
const HEADING = 28;
const CARD = 92;

beforeAll(() => {
  vi.stubGlobal("WebSocket", FakeSocket);
  // The same stubs `QueuePanel.tag.test.tsx` explains, with one difference: a
  // virtual row is as tall as what it holds, so a heading and a card measure
  // differently — which is the whole subject here.
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
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get(this: HTMLElement) {
      if (!this.hasAttribute("data-index")) return 800;
      return this.querySelector('[role="option"]') ? CARD : HEADING;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 800 });
  for (const prop of ["offsetWidth", "clientWidth"]) {
    Object.defineProperty(HTMLElement.prototype, prop, { configurable: true, get: () => 320 });
  }
});

const mk = (id: string, repo: string) =>
  ({
    id,
    title: id,
    cwd: `/tmp/${repo}`,
    repo_root: `/tmp/${repo}`,
    alive: true,
    tmux_target: "work:0.0",
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
  }) as unknown as Session;

/** Two in `alpha`, one in `beta`, grouped by repository: heading, card, card, heading, card. */
async function board() {
  cleanup();
  const { default: App } = await import("@/App");
  const { useStore } = await import("@/store");
  const { defaultPrefs } = await import("@/store/prefs");
  useStore.setState({
    prefs: { ...defaultPrefs(), scope: "all", groupByRepo: true },
    foldedGroups: [],
    sessions: { a1: mk("a1", "alpha"), a2: mk("a2", "alpha"), b1: mk("b1", "beta") },
    queue: [
      { session_id: "a1", reason: "awaiting_input", score: 3, detail: "waiting" },
      { session_id: "a2", reason: "running", score: 2, detail: "" },
      { session_id: "b1", reason: "awaiting_input", score: 1, detail: "waiting" },
    ],
    firstSnapshot: true,
    selected: null,
  } as never);
  render(<App />);
  return useStore;
}

/** Each drawn row: what it is, where the virtualizer put it, and where the row above it ends. */
function layout() {
  const list = screen.getByRole("listbox", { name: "Attention" });
  const rows = [...list.querySelectorAll<HTMLElement>("[data-index]")].sort(
    (a, b) => Number(a.dataset.index) - Number(b.dataset.index),
  );
  let end = 0;
  const lines = rows.map((row) => {
    const kind = row.querySelector('[role="option"]') ? "card" : "heading";
    const start = end;
    end += row.offsetHeight;
    return { kind, drawnAt: Number(/translateY\((-?[\d.]+)px\)/.exec(row.style.transform)?.[1]), shouldBeAt: start };
  });
  return { lines, height: rows[0]?.parentElement?.style.height, shouldBe: `${end}px` };
}

function expectSeamless() {
  const { lines, height, shouldBe } = layout();
  expect(lines.map((l) => l.drawnAt)).toEqual(lines.map((l) => l.shouldBeAt));
  expect(height).toBe(shouldBe);
  return lines.map((l) => l.kind);
}

afterEach(async () => {
  const { useStore } = await import("@/store");
  useStore.setState({ sessions: {}, queue: [], firstSnapshot: false, selected: null, foldedGroups: [] });
  localStorage.clear();
});

describe("the Attention list regrouped", () => {
  it("folds a group without leaving a blank band under the heading that moves up", async () => {
    await board();
    expect(expectSeamless()).toEqual(["heading", "card", "card", "heading", "card"]);

    // `beta`'s heading moves from the fourth line to the second, where a card
    // was: keyed by index it would be given that card's height, and `b1`
    // would be drawn a card's worth below where the heading ends.
    fireEvent.click(screen.getByRole("button", { name: /alpha.*2 sessions/ }));
    expect(expectSeamless()).toEqual(["heading", "heading", "card"]);
  });

  it("drops the grouping without drawing the second card over the first", async () => {
    const useStore = await board();
    expect(expectSeamless()).toEqual(["heading", "card", "card", "heading", "card"]);

    // `a1` moves from the second line to the first, where `alpha`'s heading
    // was: keyed by index it would be given that heading's height, and `a2`
    // would be drawn across it.
    act(() => useStore.getState().setPrefs({ groupByRepo: false }));
    expect(expectSeamless()).toEqual(["card", "card", "card"]);
  });
});
