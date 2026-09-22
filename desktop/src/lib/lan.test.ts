/**
 * The Found list's decisions, without a shell. `R-I17`.
 *
 * The two that could fail quietly: a row that dials something other than the
 * datagram's source, and a saved connection that is the daemon being heard
 * but does not light up because the spelling differs.
 */

import { describe, expect, it } from "vitest";
import { agoLabel, foundName, hostPortOf, matchesSaved, reachLabel, wsUrlOf, type FoundDaemon } from "./lan";

const found = (over: Partial<FoundDaemon> = {}): FoundDaemon => ({
  machine: "abc123",
  home: "/home/k/.claude",
  host: "devbox",
  addr: "10.0.0.30",
  addrs: ["10.0.0.30", "10.0.0.31"],
  port: 7717,
  version: "0.1.0",
  reach: "lan",
  ssh: null,
  live: 2,
  waiting: 1,
  last_seen_ms: 1_000_000,
  beacons: 4,
  gone: false,
  mine: false,
  ...over,
});

describe("what a found row dials", () => {
  it("is the datagram's source and the announced port", () => {
    expect(wsUrlOf(found())).toBe("ws://10.0.0.30:7717/ws");
    expect(wsUrlOf(found({ addr: "fd00::1", port: 7800 }))).toBe("ws://[fd00::1]:7800/ws");
  });

  it("is named by the hostname it announced, else where it came from", () => {
    expect(foundName(found())).toBe("devbox");
    expect(foundName(found({ host: null }))).toBe("10.0.0.30");
  });
});

describe("a saved connection that is the daemon being heard", () => {
  const saved = (url: string) => ({ id: "x", name: "n", url });

  it("matches on any address it was heard from, and the port", () => {
    expect(matchesSaved(saved("ws://10.0.0.30:7717/ws"), found())).toBe(true);
    expect(matchesSaved(saved("ws://10.0.0.31:7717/ws"), found())).toBe(true);
    expect(matchesSaved(saved("ws://10.0.0.30:7718/ws"), found())).toBe(false);
    expect(matchesSaved(saved("ws://devbox:7717/ws"), found())).toBe(false);
    expect(matchesSaved(saved("not a url"), found())).toBe(false);
  });

  it("reads host and port out of a URL, brackets and all", () => {
    expect(hostPortOf("ws://[fd00::1]:7800/ws")).toBe("fd00::1:7800");
    expect(hostPortOf("wss://devbox/ws")).toBe("devbox:443");
    expect(hostPortOf("")).toBeNull();
  });
});

describe("the words on a row", () => {
  it("say how long ago, on the window's clock", () => {
    expect(agoLabel(1_000_000, 1_012_000)).toBe("12 s ago");
    expect(agoLabel(1_000_000, 1_000_000 + 3 * 60_000)).toBe("3 min ago");
    expect(agoLabel(1_000_000, 1_000_000 + 2 * 3_600_000)).toBe("2 h ago");
    expect(agoLabel(2_000_000, 1_000_000)).toBe("0 s ago");
  });

  it("say how a hand reaches it", () => {
    expect(reachLabel(found())).toMatch(/token/);
    expect(reachLabel(found({ reach: "ssh", ssh: "kinz@devbox" }))).toBe("over ssh — kinz@devbox");
    expect(reachLabel(found({ reach: "local" }))).toMatch(/own machine/);
  });
});
