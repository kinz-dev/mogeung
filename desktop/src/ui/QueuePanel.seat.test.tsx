/**
 * A seat is named on its row. `R-P4`.
 *
 * A seat ranks on the same queue as every session you started by hand, and
 * the one thing that must never happen is the two passing for each other —
 * ADR-0003's amendment owns a seat and nothing else, and the row is where
 * that line is visible. The chip carries the team and the seat, from the tag
 * the daemon puts on the row at birth.
 */

import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
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
  // The same three stubs `QueuePanel.tag.test.tsx` explains: the list is
  // virtualised, and in jsdom nothing has a size, so nothing is drawn.
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

const SEAT = {
  id: "seat:t1:builder",
  title: null,
  name: "port/builder — seat, nothing written yet",
  cwd: "/tmp/repo",
  repo_root: "/tmp/repo",
  alive: true,
  tmux_target: "geung-port-builder:0.0",
  touched_files: [],
  collisions: [],
  verify_runs: [],
  claims: [],
  last_event_at: new Date().toISOString(),
  seat: { team_id: "t1", team: "port", seat: "builder" },
} as unknown as Session;

async function board() {
  cleanup();
  const { default: App } = await import("@/App");
  const { useStore } = await import("@/store");
  const { emptyScoped, loadPrefs } = await import("@/store/prefs");
  const prefs = loadPrefs();
  useStore.setState({
    prefs: {
      ...prefs,
      scoped: {
        ...prefs.scoped,
        unknown: { ...emptyScoped(), ...(prefs.scoped.unknown ?? {}), hidden: [], pinned: [], labels: {}, bookmarks: [], editorWrap: [], tags: {} },
      },
    } as typeof prefs,
    sessions: { "seat:t1:builder": SEAT },
    queue: [{ session_id: "seat:t1:builder", reason: "awaiting_input", score: 1, detail: "waiting" }],
    firstSnapshot: true,
    selected: null,
  });
  render(<App />);
  return screen.getAllByRole("option").find((el) => el.textContent?.includes("builder"))!;
}

afterEach(async () => {
  const { useStore } = await import("@/store");
  useStore.setState({ sessions: {}, queue: [], firstSnapshot: false, selected: null });
  localStorage.clear();
});

describe("a seat's queue row", () => {
  it("names its team and seat, and is not called provisional", async () => {
    const row = await board();
    expect(row.textContent).toContain("port/builder");
    expect(row.textContent).not.toContain("not started writing");
  });
});
