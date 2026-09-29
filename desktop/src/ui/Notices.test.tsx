/**
 * Warnings must announce themselves and then get out of the way.
 *
 * The behaviour this replaces waited for a click, which is right for something
 * you must act on and wrong for a daemon saying "that session is not in a git
 * repository" — a fact, not a task.
 */

import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { capNotices, useStore, type Notice } from "@/store";
import { NoticeButton, NoticesWindow, Toasts } from "@/ui/Notices";

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

beforeAll(() => vi.stubGlobal("WebSocket", FakeSocket));
beforeEach(() => useStore.setState({ notices: [], noticesOpen: false }));

describe("a bookmark whose session is gone", () => {
  /**
   * A bookmark outlives the session it marks — that is the point of it
   * (ADR-0015). So clicking one whose session is no longer watched has to
   * *say* so: silently swallowing the click reads as the panel having lost
   * track of things, which is exactly how it was reported.
   */
  it("says so rather than swallowing the click", async () => {
    const { BookmarksTool } = await import("@/ui/tools/BookmarksTool");
    act(() => {
      useStore.setState({
        sessions: {},
        notes: [{ id: "b1", body: "egress vs ingress", created: 1, updated: 2, session_id: "gone", seq: 4 }],
      });
    });
    render(<BookmarksTool />);
    act(() => screen.getByText(/egress vs ingress/i).click());
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/no longer being watched/i);
  });
});

describe("a warning", () => {
  it("appears without being asked, and leaves without being told", async () => {
    vi.useFakeTimers();
    try {
      const { default: App } = await import("@/App");
      render(<App />);

      act(() => {
        useStore.getState().pushError("that session is not in a git repo");
      });
      expect(screen.getByText(/not in a git repo/i)).toBeInTheDocument();

      // Nobody touches it.
      act(() => {
        vi.advanceTimersByTime(16_000);
      });
      expect(screen.queryByText(/not in a git repo/i)).not.toBeInTheDocument();

      // ...but it is not *lost*. The count is what makes "I saw something
      // flash past" recoverable instead of a memory test.
      expect(useStore.getState().notices).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps its unread mark until the log is opened", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    act(() => {
      useStore.getState().pushError("one");
      useStore.getState().pushError("two");
    });
    expect(useStore.getState().notices.filter((n) => !n.seen)).toHaveLength(2);

    act(() => useStore.getState().markNoticesSeen());
    expect(useStore.getState().notices.filter((n) => !n.seen)).toHaveLength(0);
    // Seen is not gone: the log is a log.
    expect(useStore.getState().notices).toHaveLength(2);
  });

  /** A daemon failing in a loop must not grow the log without bound, and the
   * fiftieth copy of a message says nothing the first did not. */
  it("caps the log rather than growing forever", async () => {
    act(() => {
      for (let i = 0; i < 120; i++) useStore.getState().pushError(`boom ${i}`);
    });
    const notices = useStore.getState().notices;
    expect(notices).toHaveLength(50);
    expect(notices[notices.length - 1].text).toBe("boom 119");
  });
});

/**
 * Only what went wrong is a warning. `R-J114`.
 *
 * Reported 2026-09-25: *check for updates* said up to date, and the status
 * bar's warning count went red with it listed in the Warnings window. Every
 * `info` notice did the same — *copied*, *bookmark set*, *sent to* — in a
 * window whose empty state reads *nothing has gone wrong*.
 */
describe("what counts as a warning", () => {
  function mount() {
    render(
      <>
        <Toasts />
        <NoticeButton />
        <NoticesWindow />
      </>,
    );
  }

  beforeEach(() => {
    cleanup();
    useStore.setState({ syncs: [] } as never);
  });

  it("an info notice is a toast, and is neither counted nor logged as a warning", () => {
    mount();
    act(() => useStore.getState().pushNotice("up to date — this build is main's newest commit, 4e9ad7e"));
    expect(screen.getByText(/up to date/)).toBeInTheDocument();
    // No count in the status bar at all: nothing has gone wrong.
    expect(screen.queryByTitle(/^\d+ (unread of \d+|in the log)$/)).toBeNull();
    // And its toast does not promise a log it is not in.
    expect(screen.getByTitle("dismiss")).toBeInTheDocument();
  });

  it("an error is a toast, a red count, and a line in the Warnings window", () => {
    mount();
    act(() => {
      useStore.getState().pushNotice("copied", "info");
      useStore.getState().pushError("could not check for updates: fatal: not a git repository");
    });
    const count = screen.getByTitle("1 unread of 1");
    expect(count).toHaveTextContent("1");
    fireEvent.click(count);
    const log = screen.getByRole("dialog", { name: "Warnings" });
    expect(log).toHaveTextContent("1 recorded");
    expect(log).toHaveTextContent("not a git repository");
    expect(log).not.toHaveTextContent("copied");
  });

  it("a burst of successes cannot push a failure out of the log", () => {
    const n = (id: number, kind: Notice["kind"]): Notice => ({ id, text: `${kind} ${id}`, kind, at: id, seen: false });
    const all = [n(1, "error"), ...Array.from({ length: 200 }, (_, i) => n(i + 2, "info"))];
    const kept = capNotices(all);
    expect(kept[0]).toMatchObject({ id: 1, kind: "error" });
    expect(kept.filter((x) => x.kind === "info")).toHaveLength(10);
    // Newest kept, in order.
    expect(kept.at(-1)?.id).toBe(201);
  });
});
