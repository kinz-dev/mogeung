/**
 * The rail's open list. `R-J33`.
 *
 * Every one of these fails against the code as it stood on 2026-08-19, where
 * `prefs.rail` was a single tool and opening one closed the last.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { closeRail, openRail, toggleRail } from "@/lib/rail";
import { defaultPrefs, railList } from "@/store/prefs";
import { useStore } from "@/store";

describe("what the rail has open", () => {
  it("adds a tool beside the one already open", () => {
    expect(openRail(["files"], "notes")).toEqual(["files", "notes"]);
  });

  /** Strip order, whichever order they were opened in — the stack must not
   *  rearrange itself around a click. */
  it("keeps strip order rather than the order you opened them in", () => {
    expect(openRail(["notes"], "files")).toEqual(["files", "notes"]);
    expect(openRail(openRail(["bookmarks"], "search"), "files")).toEqual(["files", "search", "bookmarks"]);
  });

  it("never holds a tool twice", () => {
    expect(openRail(["files", "notes"], "files")).toEqual(["files", "notes"]);
  });

  it("toggles both ways, and is its own inverse", () => {
    expect(toggleRail(["files"], "search")).toEqual(["files", "search"]);
    expect(toggleRail(toggleRail(["files"], "search"), "search")).toEqual(["files"]);
  });

  it("closing leaves the rest of the stack alone", () => {
    expect(closeRail(["files", "search", "notes"], "search")).toEqual(["files", "notes"]);
  });
});

describe("reading a saved rail", () => {
  /** Every preferences file written before 2026-08-19 says this. */
  it("takes the old single tool as a stack of one", () => {
    expect(railList("files")).toEqual(["files"]);
  });

  it("takes a collapsed rail, however it was spelled", () => {
    expect(railList(null)).toEqual([]);
    expect(railList(undefined)).toEqual([]);
    expect(railList("")).toEqual([]);
  });

  /**
   * Forgiving, because `Prefs` is one `JSON.parse`: a tool name from a newer
   * build has to cost that name and not every setting in the file.
   */
  it("drops a tool this build does not have, and keeps the ones it does", () => {
    expect(railList(["files", "timeline", 7, null])).toEqual(["files"]);
  });

  it("puts a saved stack back in strip order", () => {
    expect(railList(["notes", "files"])).toEqual(["files", "notes"]);
  });
});

/**
 * The store's toggle, which is what both the chord and the strip icon call.
 * `R-J82`.
 *
 * `toggleRail` above decides *which tools are open*; this decides *whether the
 * one that just opened should take the keyboard*. They are separate because
 * closing must not aim focus at a panel that is no longer on screen.
 */
describe("toggling a rail tool from the store", () => {
  beforeEach(() => {
    useStore.setState({ prefs: defaultPrefs(), focusRail: null });
    useStore.getState().setPrefs({ rail: [] });
  });

  it("signals the tool it opened", () => {
    useStore.getState().toggleRailTool("chat");
    expect(useStore.getState().prefs.rail).toContain("chat");
    expect(useStore.getState().focusRail).toBe("chat");
  });

  /**
   * The half that would be easy to get wrong. Closing a panel has to hand the
   * keyboard back; signalling here would point it at markup that has just been
   * unmounted, and on the *next* open the value would already be set and the
   * consumer would not re-run.
   */
  it("says nothing when it closed one", () => {
    useStore.getState().toggleRailTool("chat");
    useStore.setState({ focusRail: null });

    useStore.getState().toggleRailTool("chat");
    expect(useStore.getState().prefs.rail).not.toContain("chat");
    expect(useStore.getState().focusRail).toBeNull();
  });
});
