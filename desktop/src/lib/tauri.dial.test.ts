/**
 * The URL a window dials its own daemon on. `R-I17` step 7.
 *
 * The file may move the port and name a token; the shell reports both with
 * the hosting status, and the window has to dial what was actually bound —
 * or it asks a port nobody is on with no token, and the board stays empty
 * with the daemon running fine beside it.
 */

import { describe, expect, it } from "vitest";
import { dialFor } from "./tauri";

const HERE = "ws://127.0.0.1:7717/ws";

describe("dialling the window's own daemon", () => {
  it("is the plain address when the shell bound the plain thing", () => {
    expect(dialFor({ mode: "hosting", bound: "127.0.0.1:7717", port: 7717, token: null }, HERE)).toBe(HERE);
    expect(dialFor({ mode: "attached", pid: 1, claude_home: null, token: null }, HERE)).toBe(HERE);
    expect(dialFor({ mode: "none", reason: null }, HERE)).toBe(HERE);
  });

  it("carries the token the file names, for a hosted and an attached daemon alike", () => {
    expect(dialFor({ mode: "hosting", bound: "0.0.0.0:7717", port: 7717, token: "s3cret" }, HERE)).toBe(
      "ws://127.0.0.1:7717/ws?token=s3cret",
    );
    expect(dialFor({ mode: "attached", pid: 1, claude_home: null, token: "s3cret" }, HERE)).toBe(
      "ws://127.0.0.1:7717/ws?token=s3cret",
    );
  });

  it("follows the port the file moved the daemon to", () => {
    expect(dialFor({ mode: "hosting", bound: "0.0.0.0:7800", port: 7800, token: "s3cret" }, HERE)).toBe(
      "ws://127.0.0.1:7800/ws?token=s3cret",
    );
  });

  it("does not double a token already in the address", () => {
    const already = "ws://127.0.0.1:7717/ws?token=typed";
    expect(dialFor({ mode: "hosting", bound: "0.0.0.0:7717", port: 7717, token: "s3cret" }, already)).toBe(already);
  });
});
