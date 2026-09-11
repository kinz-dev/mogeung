/**
 * The first day's asks, 2026-09-11, after the window was installed:
 *
 * 1. *"make the grid line clearer so that I can resize the column, and make
 *    all column resizable"* — a header row with dividers, widths in the
 *    preferences.
 * 2. *"a keymap Ctrl+D for opening the diff in the main window. Double click
 *    the file to open that file in the main window"* — the diff pane on
 *    Ctrl+D, the file at that commit on a double-click.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { GitPane } from "@/panes/GitPane";
import { setDock } from "@/lib/panes";
import { useStore, emptyGit } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { ClientMsg, CommitInfo, FileChange } from "@/wire/types";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 22,
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ key: index, index, start: index * 22 + 22, size: 22 })),
    measureElement: () => {},
    scrollToIndex: () => {},
  }),
}));

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

const sha = "fa64b61".padEnd(40, "0");
const commit = (): CommitInfo => ({ sha, short: "fa64b61", summary: "fix: x", author: "keith", epoch: 1, refs: [], parents: [], touches_session: false });
const file = (path: string, status: FileChange["status"] = "modified"): FileChange => ({
  path,
  old_path: null,
  status,
  insertions: 1,
  deletions: 0,
  hunks: [{ anchor: `${path}#0`, header: "@@ -1 +1 @@", lines: ["+x"], insertions: 1, deletions: 0, flags: [], score: 0, reviewed: false }],
  flags: [],
  score: 0,
  truncated: false,
});

const added: { id: string; component: string; title?: string }[] = [];
const sent: ClientMsg[] = [];

function show(git: Partial<ReturnType<typeof emptyGit>> = {}) {
  added.length = 0;
  sent.length = 0;
  setDock({ getPanel: () => undefined, panels: [], addPanel: (p: { id: string; component: string; title?: string }) => added.push(p), activeGroup: undefined } as never);
  useStore.setState({
    selected: "s1",
    sessions: { s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "s", alive: true } },
    prefs: defaultPrefs(),
    explorer: {},
    git: { s1: { ...emptyGit(), commits: [commit()], done: true, logAsked: true, status: [], ...git } },
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  return render(<GitPane />);
}

beforeEach(() => cleanup());

describe("the log's columns", () => {
  it("has a header row with a divider per fixed column, and a drag writes the width", () => {
    show();
    expect(screen.getByRole("row", { name: "columns" })).toHaveTextContent(/subject.*author.*date.*marks/);
    const divider = screen.getByRole("separator", { name: /resize the author column/ });
    fireEvent.mouseDown(divider, { clientX: 500 });
    // The divider sits on the subject's right edge; dragging it left widens
    // the author column, which is the one it borders.
    fireEvent.mouseMove(window, { clientX: 470 });
    fireEvent.mouseUp(window);
    expect(useStore.getState().prefs.gitLogColumns).toMatchObject({ author: 126, date: 84, marks: 56 });
    expect(useStore.getState().prefs.gitLogColumns.graph).not.toBeNull();
  });

  it("never drags a column below its floor", () => {
    show();
    fireEvent.mouseDown(screen.getByRole("separator", { name: /resize the marks column/ }), { clientX: 100 });
    fireEvent.mouseMove(window, { clientX: 900 });
    fireEvent.mouseUp(window);
    expect(useStore.getState().prefs.gitLogColumns.marks).toBe(24);
  });

  it("draws the saved widths", () => {
    show();
    // A store write outside React's own event needs `act` to be flushed
    // before the DOM is read — the same rule the ingest tests follow.
    act(() => useStore.setState({ prefs: { ...defaultPrefs(), gitLogColumns: { graph: 30, author: 150, date: 84, marks: 56 } } } as never));
    const header = screen.getByRole("row", { name: "columns" });
    expect(header.style.gridTemplateColumns).toBe("30px minmax(120px, 1fr) 150px 84px 56px");
  });
});

describe("out of the dock", () => {
  const detail = { author: "k", committer: "k", epoch: 1, commit_epoch: 1, parents: [], refs: [], message: "fix: x", branches: [] };

  it("Ctrl+D in the inspector opens the focused file's diff as a pane", () => {
    show({ selected: sha, diff: [file("a.rs"), file("b.rs")], detail, selectedFile: "b.rs" });
    fireEvent.keyDown(screen.getByLabelText("the selected commit"), { key: "d", ctrlKey: true });
    expect(added).toEqual([{ id: `diff:s1:${sha}:b.rs`, component: "diff", title: "b.rs" }]);
  });

  it("Ctrl+D in the log opens the selected commit, every file", () => {
    show({ selected: sha, diff: [file("a.rs")], detail });
    fireEvent.keyDown(screen.getByRole("listbox", { name: "commits" }), { key: "D", ctrlKey: true });
    expect(added).toEqual([{ id: `diff:s1:${sha}:*`, component: "diff", title: `${sha.slice(0, 8)} · all files` }]);
  });

  it("a double-click on a file opens the file at that commit, and a deleted one at the parent", () => {
    show({ selected: sha, diff: [file("a.rs"), file("gone.rs", "deleted")], detail });
    fireEvent.doubleClick(screen.getByText("a.rs"));
    expect(added.at(-1)).toMatchObject({ id: `file:s1:${sha}:a.rs`, component: "file" });
    fireEvent.doubleClick(screen.getByText("gone.rs"));
    expect(added.at(-1)).toMatchObject({ id: `file:s1:${sha}^:gone.rs`, component: "file" });
    // A double-click is not a diff pane.
    expect(added.every((a) => a.component === "file")).toBe(true);
  });
});
