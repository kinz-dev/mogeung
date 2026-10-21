/**
 * The follow message and the follow's words. `R-I21`.
 *
 * The one that matters most is the first: the daemon keeps the address it is
 * sent and shows it back in its follow list, so a token left inside the
 * address would be displayed by every window that lists the follows.
 */

import { describe, expect, it } from "vitest";
import { followMsg, followOf, machineNames, peerLine } from "@/lib/federation";
import type { PeerState } from "@/wire/types";

describe("following a saved connection", () => {
  it("takes a token out of an address that still carries one", () => {
    const m = followMsg({ id: "b", name: "", url: "ws://devbox:7717/ws?token=6f1c" });
    expect(m).toEqual({
      cmd: "follow",
      id: "b",
      name: "ws://devbox:7717/ws",
      url: "ws://devbox:7717/ws",
      token: "6f1c",
      allow_writes: true,
    });
  });

  it("finds a follow by the connection's id, or by its address when the id moved", () => {
    const p = { id: "old", url: "ws://devbox:7717/ws", state: "up" } as PeerState;
    expect(followOf({ id: "b", name: "", url: "ws://devbox:7717/ws?token=x" }, [p])).toBe(p);
    expect(followOf({ id: "b", name: "", url: "ws://other:7717/ws" }, [p])).toBeNull();
    expect(followOf({ id: "b", name: "", url: "ws://devbox:7717/ws" }, null)).toBeNull();
  });

  it("says a quiet peer's sessions are kept but last known", () => {
    const p = { state: "down", sessions: 3, detail: "went silent" } as PeerState;
    expect(peerLine(p)).toBe("following — not answering; 3 sessions kept, greyed, last known (went silent)");
  });
});

// `R-J139`: what a followed machine is called on the Attention list.
describe("naming a followed machine", () => {
  const peer = (over: Partial<PeerState>) =>
    ({ id: "c1", name: "mac-m4p", url: "ws://192.168.1.20:7717/ws", state: "up", machine_id: "m-mac", host: "Kinz-Mac-mini.local", ...over }) as PeerState;

  it("is the saved connection's name as it reads now, not the one the follow was sent with", () => {
    const saved = [{ id: "c1", name: "mac-m4p", url: "ws://192.168.1.20:7717/ws" }];
    expect(machineNames([peer({ name: "ws://192.168.1.20:7717/ws" })], saved)).toEqual({ "m-mac": "mac-m4p" });
    // Found by address when the follow's id is not the entry's.
    expect(machineNames([peer({ id: "other" })], [{ ...saved[0], name: "studio" }])).toEqual({ "m-mac": "studio" });
  });

  it("falls back to the follow's name, but never to a bare address, and skips a machine not yet heard", () => {
    expect(machineNames([peer({})], [])).toEqual({ "m-mac": "mac-m4p" });
    expect(machineNames([peer({ name: "ws://192.168.1.20:7717/ws" })], [])).toEqual({});
    expect(machineNames([peer({ machine_id: null })], [])).toEqual({});
    expect(machineNames(null, [])).toEqual({});
  });
});
