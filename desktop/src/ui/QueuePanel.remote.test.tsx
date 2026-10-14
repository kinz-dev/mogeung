/**
 * Another machine's sessions in this Attention list. `R-I21`, feature 0047.
 *
 * The spec's words: *marked remote, grouped under their machine — this
 * machine first*. And *a followed machine that goes quiet greys out*. Pinned
 * against the real window, because the row is a Radix clone and a chip or a
 * class that never reaches it fails silently — see `QueuePanel.tag.test.tsx`.
 */

import { describe, expect, it, vi, beforeAll, afterEach } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
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



const mk = (id: string, title: string, origin?: Session["origin"]) =>
  ({
    id,
    title,
    cwd: "/tmp/repo",
    repo_root: "/tmp/repo",
    alive: true,
    tmux_target: "work:0.0",
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
    origin,
  }) as unknown as Session;

async function board(stale = false) {
  cleanup();
  const { default: App } = await import("@/App");
  const { useStore } = await import("@/store");
  const { defaultPrefs } = await import("@/store/prefs");
  const remote = mk("@mac-id:r1", "the remote one", { machine_id: "mac-id", host: "mac", stale });
  useStore.setState({
    prefs: { ...defaultPrefs(), scope: "all" },
    daemon: { host: "ubuntu", version: "0.1.0", claude_home: "/h", pid: 1, federation: 1 },
    sessions: { l1: mk("l1", "the local one"), [remote.id]: remote },
    // The remote one outranks the local one — and is still drawn second.
    queue: [
      { session_id: remote.id, reason: stale ? "idle" : "awaiting_input", score: 1, detail: "waiting" },
      { session_id: "l1", reason: "running", score: 0, detail: "" },
    ],
    firstSnapshot: true,
    selected: null,
  } as never);
  render(<App />);
  const rows = screen.getAllByRole("option");
  const find = (t: string) => rows.find((el) => el.textContent?.includes(t))!;
  return { local: find("the local one"), remote: find("the remote one") };
}

afterEach(async () => {
  const { useStore } = await import("@/store");
  useStore.setState({ sessions: {}, queue: [], firstSnapshot: false, selected: null, daemon: null, peers: null, savedConnections: [] });
  localStorage.clear();
});

describe("a remote session in the queue", () => {
  it("is grouped under its machine, this machine first, and says where it runs", async () => {
    const { local, remote } = await board();
    expect(screen.getByText("this machine · ubuntu")).toBeInTheDocument();
    const heading = screen.getByText("mac");
    // This machine's heading, then its row, then the other machine's.
    expect(local.compareDocumentPosition(heading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(heading.compareDocumentPosition(remote) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(remote.textContent).toContain("on mac");
    expect(local.textContent).not.toContain("on ");
    expect(remote.dataset.stale).toBeUndefined();
  });

  // `R-J139`, reported 2026-10-14: the heading was the host the Mac
  // announces, not the name its saved connection was given.
  it("is headed by the name you gave its connection, and follows a rename", async () => {
    const { useStore } = await import("@/store");
    const { saveConnections } = await import("@/lib/connections");
    useStore.setState({
      peers: [
        { id: "c1", name: "ws://192.168.1.20:7717/ws", url: "ws://192.168.1.20:7717/ws", state: "up", machine_id: "mac-id", host: "mac", sessions: 1, allow_writes: false, since_ms: 0 },
      ],
    });
    await board();
    await act(() => saveConnections([{ id: "c1", name: "mac-m4p", url: "ws://192.168.1.20:7717/ws" }]));
    expect(screen.getByText("mac-m4p")).toBeInTheDocument();
    expect(screen.queryByText("mac")).toBeNull();
    const remote = screen.getAllByRole("option").find((el) => el.textContent?.includes("the remote one"))!;
    expect(remote.textContent).toContain("on mac-m4p");

    await act(() => saveConnections([{ id: "c1", name: "studio", url: "ws://192.168.1.20:7717/ws" }]));
    expect(screen.getByText("studio")).toBeInTheDocument();
  });

  it("greys out when its machine goes quiet", async () => {
    const { remote } = await board(true);
    expect(remote.dataset.stale).toBe("true");
    expect(remote.className).toContain("opacity-55");
    expect(screen.getByText(/not heard from/)).toBeInTheDocument();
  });
});
