/**
 * A popout is decided by a query string, and a query string is the one input
 * here that a person can edit by hand — the window is reachable from the dev
 * tools, and the shell's own checks are upstream of a URL that someone may then
 * change. `R-B55`,
 * [ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md).
 *
 * So `readPopout` re-checks what `popout.rs` already checked. The two are
 * deliberately the same rule in two places rather than one rule trusted twice:
 * the Rust side is protecting the URL it builds, and this side is protecting
 * what it renders from a URL it did not build.
 */

import { describe, expect, it, vi } from "vitest";

const tauri = vi.hoisted(() => ({ on: false }));
vi.mock("@/lib/tauri", () => ({ isTauri: () => tauri.on }));

import { readPopout, POPPABLE } from "@/lib/popout";

describe("deciding whether this window is a popout", () => {
  it("reads the kind and the session", () => {
    expect(readPopout("?popout=agent&session=0b3f9c2a")).toEqual({
      kind: "agent",
      session: "0b3f9c2a",
    });
  });

  it("is not a popout with no parameters at all", () => {
    expect(readPopout("")).toBeNull();
    expect(readPopout("?url=ws://localhost:7717/ws")).toBeNull();
  });

  /** Half a popout is not a popout; it is a main window with a stray parameter. */
  it("needs both halves", () => {
    expect(readPopout("?popout=agent")).toBeNull();
    expect(readPopout("?session=0b3f9c2a")).toBeNull();
  });

  /**
   * The kind becomes a component choice. An unknown one would render nothing
   * with no explanation, which is the worst of the available failures.
   */
  it("refuses a pane kind that cannot be popped out", () => {
    expect(readPopout("?popout=git&session=abc")).toBeNull();
    expect(readPopout("?popout=&session=abc")).toBeNull();
    expect(readPopout("?popout=../../etc&session=abc")).toBeNull();
  });

  it("refuses a session id that is not one", () => {
    expect(readPopout("?popout=agent&session=")).toBeNull();
    expect(readPopout("?popout=agent&session=" + "x".repeat(129))).toBeNull();
    // `URLSearchParams` decodes, so a session that arrived encoded is checked
    // as what it actually is rather than as what it looked like.
    expect(readPopout("?popout=agent&session=a%20b")).toBeNull();
    expect(readPopout("?popout=agent&session=a%2Fb")).toBeNull();
  });

  it("accepts the shape a real session id has", () => {
    const id = "0b3f9c2a-1d4e-4f77-9a2b-6c5d8e1f0a3b";
    expect(readPopout(`?popout=agent&session=${id}`)).toEqual({ kind: "agent", session: id });
  });

  /** The two allowlists have to agree, or the shell opens what this refuses. */
  it("lists the same poppable kinds the shell does", () => {
    expect([...POPPABLE]).toEqual(["agent"]);
  });
});

describe("asking for a popout in a browser tab", () => {
  it("says it did not happen rather than pretending", async () => {
    tauri.on = false;
    const { openPopout } = await import("@/lib/popout");
    await expect(openPopout("agent", "abc")).resolves.toBe(false);
  });
});
