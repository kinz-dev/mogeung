/**
 * The tree follows the file you are reading, and the tab says where it is.
 * `R-J24`, `R-J25`.
 *
 * Both asked for together 2026-08-08: *"can you show the full path… can I add
 * a short-cut + right click menu to copy full path"* and *"can we add a button
 * in the files panel to sync view with the in-focus opened file"*.
 *
 * What is pinned here is the half that is easy to get subtly wrong: **which
 * session's worktree a reveal lands in**. A file pane is bound to the session
 * that opened it and the tree shows the selected one, so following across a
 * mismatch would expand directories that do not contain the file — a wrong
 * answer that looks like a working feature.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilesTool } from "@/ui/tools/FilesTool";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { filePaneId } from "@/lib/panes";
import { fullPath } from "@/ui/PaneChrome";

class FakeSocket {
  static OPEN = 1;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  constructor(public url: string) {}
  send() {}
  close() {}
}
vi.stubGlobal("WebSocket", FakeSocket);

const session = (id: string, cwd: string) =>
  ({
    id,
    cwd,
    repo_root: cwd,
    title: id,
    alive: true,
    machine_id: "m",
  }) as never;

function setup(opts: { selected: string; activeFor: string; path: string; follow: boolean }) {
  useStore.setState({
    prefs: { ...defaultPrefs(), filesFollow: opts.follow, rail: ["files"] },
    selected: opts.selected,
    sessions: { s1: session("s1", "/repo/one"), s2: session("s2", "/repo/two") },
    explorer: { s1: emptyExplorer(), s2: emptyExplorer() },
    activePane: filePaneId(opts.activeFor, opts.path, null),
  });
}

const expanded = (id: string) => useStore.getState().explorer[id]?.expanded ?? [];
const revealed = (id: string) => useStore.getState().explorer[id]?.reveal ?? null;

beforeEach(() => cleanup());

describe("the Files tree following the open file", () => {
  it("reveals the active file, expanding the directories above it", () => {
    setup({ selected: "s1", activeFor: "s1", path: "src/ui/deep/thing.ts", follow: true });
    render(<FilesTool />);

    expect(revealed("s1")).toBe("src/ui/deep/thing.ts");
    expect(expanded("s1")).toEqual(expect.arrayContaining(["src", "src/ui", "src/ui/deep"]));
  });

  /** Off is off: the tree stays where you left it. */
  it("does nothing at all when the toggle is off", () => {
    setup({ selected: "s1", activeFor: "s1", path: "src/thing.ts", follow: false });
    render(<FilesTool />);

    expect(revealed("s1")).toBeNull();
    expect(expanded("s1")).toEqual([]);
  });

  /**
   * **The mismatch.** The file forward belongs to another session, so it is
   * not in this tree — following must not expand a path into the wrong
   * worktree just because the strings happen to fit.
   */
  it("stays put when the file forward belongs to another session", () => {
    setup({ selected: "s1", activeFor: "s2", path: "src/other.ts", follow: true });
    render(<FilesTool />);

    expect(revealed("s1")).toBeNull();
    expect(expanded("s1")).toEqual([]);
    expect(expanded("s2")).toEqual([]);
  });

  /** Switching it on reveals at once, rather than waiting for the next file. */
  it("reveals on the way on", () => {
    setup({ selected: "s1", activeFor: "s1", path: "src/now.ts", follow: false });
    render(<FilesTool />);
    expect(revealed("s1")).toBeNull();

    fireEvent.click(screen.getByTitle(/follow the file you are reading/i));

    expect(useStore.getState().prefs.filesFollow).toBe(true);
    expect(revealed("s1")).toBe("src/now.ts");
  });
});

describe("the full path", () => {
  /** Absolute where the root is known — the half a relative path cannot say
   * is *which checkout*, which is the whole point with two worktrees open. */
  it("joins the session's root to the path in the repo", () => {
    expect(fullPath("/repo/one", "src/lib.rs")).toBe("/repo/one/src/lib.rs");
    expect(fullPath("/repo/one/", "src/lib.rs")).toBe("/repo/one/src/lib.rs");
  });

  /** A path that pretends to be absolute is worse than one that admits it is
   * not: an unknown session yields the relative path unchanged. */
  it("stays relative when the root is unknown", () => {
    expect(fullPath(null, "src/lib.rs")).toBe("src/lib.rs");
  });
});
