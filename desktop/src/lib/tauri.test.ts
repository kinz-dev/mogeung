import { describe, expect, it } from "vitest";
import { localAddrOf } from "./tauri";

describe("localAddrOf", () => {
  /**
   * The rule that stops a window pointed at another machine from starting a
   * daemon here. It is the same line the egui client draws at `--url`: being
   * told where to look is a decision already made, and hosting a local daemon
   * would be answering a question nobody asked.
   */
  it("refuses to host for a daemon on another machine", () => {
    expect(localAddrOf("ws://10.0.0.39:7717/ws")).toBeNull();
    expect(localAddrOf("ws://devbox.local:7717/ws")).toBeNull();
  });

  it("recognises loopback in the spellings it actually arrives in", () => {
    expect(localAddrOf("ws://127.0.0.1:7717/ws")).toBe("127.0.0.1:7717");
    expect(localAddrOf("ws://localhost:7717/ws")).toBe("127.0.0.1:7717");
  });

  /**
   * An `ssh -L` tunnel makes a *remote* daemon answer on `127.0.0.1`, so this
   * reads as local and the window will try to bind — and correctly fail,
   * because the tunnel already holds the port. Winning the bind is the only
   * thing that makes us the daemon, which is precisely why the test is the
   * bind rather than a guess about the address.
   */
  it("treats a tunnelled port as local, and lets the bind settle it", () => {
    expect(localAddrOf("ws://127.0.0.1:7717/ws")).toBe("127.0.0.1:7717");
  });

  it("defaults the port when the URL omits one", () => {
    expect(localAddrOf("ws://127.0.0.1/ws")).toBe("127.0.0.1:7717");
  });

  it("returns null rather than throwing on a malformed URL", () => {
    expect(localAddrOf("not a url")).toBeNull();
  });
});
