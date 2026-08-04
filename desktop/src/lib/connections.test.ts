/**
 * The daemon list is hand-editable localStorage, which means it is also
 * hand-corruptible. Losing every bookmark because one row is malformed is the
 * failure the prefs loader already refuses, and this refuses it the same way.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { defaultName, loadConnections, saveConnections, withCurrent } from "@/lib/connections";

describe("the stored connections", () => {
  beforeEach(() => localStorage.clear());

  it("round-trips", () => {
    saveConnections([{ name: "dev box", url: "ws://devbox:7717/ws" }]);
    expect(loadConnections()).toEqual([{ name: "dev box", url: "ws://devbox:7717/ws" }]);
  });

  it("keeps the good rows when one is malformed", () => {
    localStorage.setItem(
      "mogeung.connections",
      JSON.stringify([{ url: "ws://a/ws", name: "a" }, { nope: 1 }, null, "string"]),
    );
    expect(loadConnections()).toEqual([{ name: "a", url: "ws://a/ws" }]);
  });

  it("survives a file that is not JSON at all", () => {
    localStorage.setItem("mogeung.connections", "{{{");
    expect(loadConnections()).toEqual([]);
  });

  it("names a row that has no name after its address", () => {
    localStorage.setItem("mogeung.connections", JSON.stringify([{ url: "ws://a/ws" }]));
    expect(loadConnections()[0].name).toBe("ws://a/ws");
    expect(defaultName("ws://devbox:7717/ws")).toBe("devbox:7717");
  });

  /**
   * The window must never open on an empty list while plainly connected to
   * something — the entry you most want to keep is the one you never typed.
   */
  it("puts the address in use at the top when it is not already there", () => {
    const list = withCurrent([{ name: "a", url: "ws://a/ws" }], "ws://localhost:7717/ws");
    expect(list[0]).toEqual({ name: "localhost:7717", url: "ws://localhost:7717/ws" });
    expect(list).toHaveLength(2);
  });

  it("does not duplicate the address in use", () => {
    const list = withCurrent([{ name: "here", url: "ws://a/ws" }], "ws://a/ws");
    expect(list).toHaveLength(1);
  });
});
