/**
 * Does the whole window mount without throwing?
 *
 * Not a screenshot test and not pretending to be one — it cannot tell you
 * whether anything *looks* right. What it does catch is the class of failure
 * that takes the entire app down and shows a blank page: a bad hook order, a
 * component that resolved to `undefined` through a circular import, a store
 * selector that throws before the first message arrives.
 *
 * The socket is stubbed rather than opened, because the point is that the
 * window renders **before** the daemon has said anything — `R-J7`'s "an empty
 * board during the first scan is indistinguishable from a broken one" is a
 * state that has to work, not one to skip in tests.
 */

import { describe, expect, it, vi, beforeAll } from "vitest";
import { render, screen } from "@testing-library/react";
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
});

describe("the window", () => {
  /**
   * The regression that shipped a blank window.
   *
   * `scoped()` returns the stored preferences object itself — it must, or the
   * selector re-renders for ever — so a field added *after* a file was written
   * is `undefined` at every read site. `tags` was such a field: the first
   * `scoped.tags[id]` threw, React unwound the tree, and the window came up
   * blank for anyone whose preferences predated the feature.
   *
   * A file with a scoped entry from before is the whole test. It fails without
   * the completion in `loadPrefs`.
   */
  it("mounts on a preferences file written before the newest field existed", async () => {
    localStorage.setItem(
      "mogeung.prefs",
      JSON.stringify({
        scoped: {
          unknown: { hidden: [], pinned: [], labels: { s1: "mine" }, editorWrap: [], bookmarks: [] },
        },
      }),
    );
    const { default: App } = await import("./App");
    const { useStore } = await import("@/store");
    // A row on screen is the whole point: the throw is in `QueueRow`, so a
    // window with an empty queue proves nothing. This is why the first version
    // of this test passed against the bug it was written for.
    useStore.setState({
      prefs: (await import("@/store/prefs")).loadPrefs(),
      sessions: {
        s1: {
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
        } as unknown as Session,
      },
      queue: [{ session_id: "s1", reason: "awaiting_input", score: 1, detail: "waiting" }],
      firstSnapshot: true,
    });
    expect(() => render(<App />)).not.toThrow();

    // Put the store back. These tests share one module instance, and the next
    // one asserts on the *first scan* state — which a seeded session silently
    // turns into a different test.
    useStore.setState({ sessions: {}, queue: [], firstSnapshot: false });
    localStorage.clear();
  });

  it("mounts with no daemon connected", async () => {
    const { default: App } = await import("./App");
    expect(() => render(<App />)).not.toThrow();
  });

  it("says the board is still being read rather than showing an empty one", async () => {
    const { default: App } = await import("./App");
    render(<App />);
    // `R-J7`: before the first snapshot, "nothing needs you" and "we have not
    // looked yet" must not be the same sentence.
    expect(await screen.findByText(/reading the first scan/i)).toBeInTheDocument();
  });
});
