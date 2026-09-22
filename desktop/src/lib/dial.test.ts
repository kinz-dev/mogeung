/**
 * Which URL a switch dials. `R-J102`.
 *
 * Each of these is a way the window ended up refused by a daemon that was
 * running fine: this machine's own, token-gated since step 7, dialled without
 * its token; a tunnel on loopback handed this machine's token; a remote whose
 * token was only ever in the URL a previous launch remembered.
 */

import { describe, expect, it } from "vitest";
import { composeDial, isOwnDaemon, rememberable, type LocalDaemon } from "./dial";

const HOSTING: LocalDaemon = {
  addr: "127.0.0.1:7717",
  status: { mode: "hosting", bound: "0.0.0.0:7717", port: 7717, token: "mine" },
};
const MAC = { id: "m", name: "mac", url: "ws://10.0.0.39:7717/ws", token: "theirs" };

describe("this machine's own daemon", () => {
  it("is dialled with the token and port the shell reported, whichever way it was reached", () => {
    expect(composeDial("ws://127.0.0.1:7717/ws", HOSTING)).toBe("ws://127.0.0.1:7717/ws?token=mine");
    expect(composeDial("ws://localhost:7717/ws", HOSTING)).toBe("ws://localhost:7717/ws?token=mine");
    const moved: LocalDaemon = {
      addr: "127.0.0.1:7717",
      status: { mode: "hosting", bound: "0.0.0.0:7800", port: 7800, token: "mine" },
    };
    expect(composeDial("ws://127.0.0.1:7717/ws", moved)).toBe("ws://127.0.0.1:7800/ws?token=mine");
    expect(isOwnDaemon("ws://127.0.0.1:7800/ws", moved)).toBe(true);
  });

  it("gets the file's token when the window attached to a daemon started by hand", () => {
    const attached: LocalDaemon = {
      addr: "127.0.0.1:7717",
      status: { mode: "attached", pid: 9, claude_home: null, token: "mine" },
    };
    expect(composeDial("ws://127.0.0.1:7717/ws", attached)).toBe("ws://127.0.0.1:7717/ws?token=mine");
  });

  /** An `ssh -L` tunnel answers on loopback and is somebody else's daemon. */
  it("is not a tunnel on another loopback port, which takes its own saved token", () => {
    const tunnel = { id: "t", name: "devbox via ssh", url: "ws://127.0.0.1:7719/ws", token: "devbox" };
    expect(isOwnDaemon("ws://127.0.0.1:7719/ws", HOSTING)).toBe(false);
    expect(composeDial("ws://127.0.0.1:7719/ws", HOSTING, [tunnel])).toBe("ws://127.0.0.1:7719/ws?token=devbox");
  });

  it("is dialled as it is when the shell said nothing — a browser tab", () => {
    expect(composeDial("ws://127.0.0.1:7717/ws", { addr: null, status: null })).toBe("ws://127.0.0.1:7717/ws");
  });
});

describe("another machine's daemon", () => {
  it("keeps a token it already carries", () => {
    expect(composeDial("ws://10.0.0.39:7717/ws?token=typed", HOSTING, [MAC])).toBe("ws://10.0.0.39:7717/ws?token=typed");
  });

  it("takes the token saved against it when the address came without one", () => {
    expect(composeDial("ws://10.0.0.39:7717/ws", HOSTING, [MAC])).toBe("ws://10.0.0.39:7717/ws?token=theirs");
  });

  it("is dialled bare when nothing is saved against it", () => {
    expect(composeDial("ws://10.0.0.50:7717/ws", HOSTING, [MAC])).toBe("ws://10.0.0.50:7717/ws");
  });
});

describe("what the window remembers", () => {
  it("is the address, never the token", () => {
    expect(rememberable("ws://10.0.0.39:7717/ws?token=theirs")).toBe("ws://10.0.0.39:7717/ws");
    expect(rememberable("ws://127.0.0.1:7717/ws")).toBe("ws://127.0.0.1:7717/ws");
  });
});
