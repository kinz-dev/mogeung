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
