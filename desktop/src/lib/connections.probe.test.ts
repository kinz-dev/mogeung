/**
 * The Test button's one question: who is there? `R-I19`.
 *
 * Driven with a fake socket, because the answer is in the first frame and
 * the three ways of getting no frame — an error, a close, silence — are the
 * cases a hand needs told apart from "it works".
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { missingToken, probeConnection } from "./connections";

class FakeSocket {
  static last: FakeSocket | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  closed = 0;
  constructor(public url: string) {
    FakeSocket.last = this;
  }
  close() {
    this.closed++;
  }
}
const make = (u: string) => new FakeSocket(u) as unknown as WebSocket;

describe("testing a connection", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("reads who answered out of the first frame, and hangs up", async () => {
    const p = probeConnection("ws://devbox:7717/ws", 4000, make);
    FakeSocket.last!.onmessage!({
      data: JSON.stringify({ kind: "snapshot", daemon: { host: "devbox", version: "0.1.0" }, queue: [1, 2, 3] }),
    });
    await expect(p).resolves.toEqual({ ok: true, host: "devbox", version: "0.1.0", queued: 3 });
    expect(FakeSocket.last!.closed).toBe(1);
  });

  it("says a refusal cannot be told from an unreachable host", async () => {
    const p = probeConnection("ws://devbox:7717/ws", 4000, make);
    FakeSocket.last!.onerror!();
    // The close that follows an error must not answer a second time.
    FakeSocket.last!.onclose!();
    await expect(p).resolves.toMatchObject({ ok: false, reason: expect.stringMatching(/refused|unreachable/) });
  });

  it("gives up after the timeout, saying how long it waited", async () => {
    const p = probeConnection("ws://devbox:7717/ws", 4000, make);
    vi.advanceTimersByTime(4001);
    await expect(p).resolves.toMatchObject({ ok: false, reason: expect.stringMatching(/no answer in 4 s/) });
  });

  it("waits past a frame it cannot read", async () => {
    const p = probeConnection("ws://devbox:7717/ws", 4000, make);
    FakeSocket.last!.onmessage!({ data: "not json" });
    FakeSocket.last!.onmessage!({ data: JSON.stringify({ daemon: { host: "spark" } }) });
    await expect(p).resolves.toMatchObject({ ok: true, host: "spark", version: null, queued: null });
  });

  it("refuses an address a socket cannot dial", async () => {
    const bad = () => {
      throw new Error("SyntaxError: bad url");
    };
    await expect(probeConnection("nope", 4000, bad)).resolves.toMatchObject({ ok: false, reason: expect.stringMatching(/a socket can dial/) });
  });
});

describe("the refusal a socket cannot explain", () => {
  const c = (url: string, token?: string) => ({ id: "x", name: "n", url, token });

  it("is said before dialling a LAN address with no token", () => {
    expect(missingToken(c("ws://10.0.0.39:7717/ws"))).toMatch(/no token saved/);
    expect(missingToken(c("ws://devbox:7717/ws"))).toMatch(/no token saved/);
  });

  it("is not said for loopback, which needs none, nor when a token is there", () => {
    expect(missingToken(c("ws://127.0.0.1:7717/ws"))).toBeNull();
    expect(missingToken(c("ws://localhost:7717/ws"))).toBeNull();
    expect(missingToken(c("ws://[::1]:7717/ws"))).toBeNull();
    expect(missingToken(c("ws://10.0.0.39:7717/ws", "6f1c"))).toBeNull();
    expect(missingToken(c("", undefined))).toBeNull();
  });
});
