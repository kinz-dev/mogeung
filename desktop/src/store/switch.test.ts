/**
 * Switching daemons, at the store. `R-J102`.
 *
 * Reported 2026-09-22: *"when I use the UI to switch to another session, I
 * can't switch it back to the local session — I think the mogeungd will be
 * killed once I connect to a remote session"*. The daemon was alive, answering
 * 401: since step 7 it is token-gated, the token was composed once at launch,
 * and the switch back dialled the saved local entry, which has none. The
 * switch away had also written the remote's token into `localStorage`.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";

beforeEach(() => {
  localStorage.clear();
  useStore.setState({
    daemonStatus: { mode: "hosting", bound: "0.0.0.0:7717", port: 7717, token: "mine" },
    localAddr: "127.0.0.1:7717",
  } as never);
});

describe("switching daemons", () => {
  it("dials this machine's own daemon with its token on the way back", () => {
    useStore.getState().setUrl("ws://10.0.0.39:7717/ws?token=theirs");
    useStore.getState().setUrl("ws://127.0.0.1:7717/ws");
    expect(useStore.getState().url).toBe("ws://127.0.0.1:7717/ws?token=mine");
  });

  it("remembers the address and never the token", () => {
    useStore.getState().setUrl("ws://10.0.0.39:7717/ws?token=theirs");
    expect(localStorage.getItem("mogeung.url")).toBe("ws://10.0.0.39:7717/ws");
    useStore.getState().setUrl("ws://127.0.0.1:7717/ws");
    expect(localStorage.getItem("mogeung.url")).toBe("ws://127.0.0.1:7717/ws");
  });

  it("still dials a remote with the token it was handed", () => {
    useStore.getState().setUrl("ws://10.0.0.39:7717/ws?token=theirs");
    expect(useStore.getState().url).toBe("ws://10.0.0.39:7717/ws?token=theirs");
  });
});
