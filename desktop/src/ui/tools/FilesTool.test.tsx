/**
 * The Files tree from the keyboard, and a click that does not open. `R-J122`.
 *
 * Both asked 2026-10-08: *"which user single mouse click on a file, don't auto
 * open it (make this a checkbox … default is don't auto open it)"* and *"when
 * the files is in focus, I can just type a file name and it can fuzzy search
 * and show the matching files in the tree"*.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilesTool } from "@/ui/tools/FilesTool";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { focusOwns } from "@/lib/keymap";

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

const TREE = ["README.md", "src/lib/explorer.ts", "src/lib/search.ts", "src/ui/tools/FilesTool.tsx"];

let sent: { cmd: string }[] = [];

function setup(opts: { clickOpens?: boolean; tree?: string[] | null } = {}) {
  sent = [];
  useStore.setState({
    prefs: { ...defaultPrefs(), filesClickOpens: opts.clickOpens ?? false, rail: ["files"] },
    selected: "s1",
    sessions: { s1: { id: "s1", cwd: "/r", repo_root: "/r", title: "s1", alive: true, machine_id: "m" } as never },
    explorer: {
      s1: {
        ...emptyExplorer(),
        workspace: { dirs: [], missing: [], hints: [] } as never,
        dirs: {
          "": [
            { name: "src", is_dir: true },
            { name: "README.md", is_dir: false },
          ],
        } as never,
        treePaths: opts.tree === undefined ? TREE : opts.tree,
      },
    },
    send: ((m: { cmd: string }) => sent.push(m)) as never,
  });
  render(<FilesTool />);
}

const opened = () => useStore.getState().explorer.s1.open.map((t) => `${t.path}${t.pinned ? " (pinned)" : ""}`);
const tree = () => screen.getByLabelText("files — type to search");
const type = (text: string) => {
  for (const key of text) fireEvent.keyDown(tree(), { key });
};
const rowNames = () =>
  Array.from(tree().querySelectorAll("[title]")).map((el) => el.getAttribute("title"));

beforeEach(() => cleanup());

describe("a single click on a file", () => {
  it("does not open it by default", () => {
    setup();
    fireEvent.click(screen.getByTitle("README.md"));
    expect(opened()).toEqual([]);
  });

  /** Opening never needs the box: a double click pins whichever way it is set. */
  it("still opens on a double click, pinned", () => {
    setup();
    fireEvent.doubleClick(screen.getByTitle("README.md"));
    expect(opened()).toEqual(["README.md (pinned)"]);
  });

  it("previews it when open on click is ticked", () => {
    setup({ clickOpens: true });
    fireEvent.click(screen.getByTitle("README.md"));
    expect(opened()).toEqual(["README.md"]);
  });

  it("is a box you can tick, and it is remembered", () => {
    setup();
    fireEvent.click(screen.getByLabelText("open on click"));
    expect(useStore.getState().prefs.filesClickOpens).toBe(true);
  });

  /** Selected but not open: Enter is the keyboard's double click. */
  it("leaves a cursor that Enter opens", () => {
    setup();
    fireEvent.click(screen.getByTitle("README.md"));
    fireEvent.keyDown(tree(), { key: "Enter" });
    expect(opened()).toEqual(["README.md (pinned)"]);
  });
});

describe("typing at the tree", () => {
  it("shows the matching files as a tree, folders open above them", () => {
    setup();
    type("srch");
    expect(rowNames()).toEqual(["src", "src/lib", "src/lib/search.ts"]);
    expect(screen.getByText("1 match")).toBeTruthy();
  });

  /**
   * The walk, not the rows on screen: `src` is collapsed and its files have
   * never been listed, and they are found anyway.
   */
  it("searches folders that were never expanded", () => {
    setup();
    expect(screen.queryByTitle("src/ui/tools/FilesTool.tsx")).toBeNull();
    type("files");
    expect(screen.getByTitle("src/ui/tools/FilesTool.tsx")).toBeTruthy();
  });

  it("asks for the walk when it does not have one", () => {
    setup({ tree: null });
    type("x");
    expect(sent.map((m) => m.cmd)).toContain("list_tree");
    expect(screen.getAllByText(/walking the worktree/).length).toBeGreaterThan(0);
  });

  it("opens the best match on Enter, pinned, and lands the plain tree on it", () => {
    setup();
    type("explorer");
    fireEvent.keyDown(tree(), { key: "Enter" });
    expect(opened()).toEqual(["src/lib/explorer.ts (pinned)"]);
    // The search is gone and the file is revealed where it lives.
    expect(screen.queryByText(/match/)).toBeNull();
    expect(useStore.getState().explorer.s1.expanded).toEqual(expect.arrayContaining(["src", "src/lib"]));
  });

  /** The arrows walk the matches and skip the folders drawn above them. */
  it("moves between matches with the arrows", () => {
    setup();
    // `search.ts` has `ts` earliest, so the cursor starts there.
    type("ts");
    fireEvent.keyDown(tree(), { key: "ArrowUp" });
    fireEvent.keyDown(tree(), { key: "Enter" });
    expect(opened()).toEqual(["src/lib/explorer.ts (pinned)"]);
    // Down from `search.ts` passes `ui/` and `tools/` without stopping.
    type("ts");
    fireEvent.keyDown(tree(), { key: "ArrowDown" });
    fireEvent.keyDown(tree(), { key: "Enter" });
    expect(opened()).toContain("src/ui/tools/FilesTool.tsx (pinned)");
  });

  it("goes back a letter on Backspace, and away on Escape", () => {
    setup();
    type("zzq");
    expect(screen.getByText("no match")).toBeTruthy();
    fireEvent.keyDown(tree(), { key: "Backspace" });
    expect(screen.getByText("zz")).toBeTruthy();
    fireEvent.keyDown(tree(), { key: "Escape" });
    expect(screen.queryByText("zz")).toBeNull();
    expect(screen.getByTitle("README.md")).toBeTruthy();
  });

  /**
   * The keymap listens in capture, so a key the tree wants has to be granted
   * by `focusOwns` — or `[` typed into a file name switches pane, and the
   * arrows move the queue.
   */
  it("owns the characters and the arrows while it has focus", () => {
    setup();
    tree().focus();
    expect(focusOwns("[")).toBe(true);
    expect(focusOwns("ArrowDown")).toBe(true);
    // Nothing to clear yet, so Escape stays the window's.
    expect(focusOwns("Escape")).toBeFalsy();
    type("a");
    expect(focusOwns("Escape")).toBe(true);
  });
});
