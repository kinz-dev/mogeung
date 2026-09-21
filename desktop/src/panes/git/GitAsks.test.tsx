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

function show(git: Partial<ReturnType<typeof emptyGit>> = {}, session: string | null = "s1") {
  added.length = 0;
  sent.length = 0;
  setDock({ getPanel: () => undefined, panels: [], addPanel: (p: { id: string; component: string; title?: string }) => added.push(p), activeGroup: undefined } as never);
  useStore.setState({
    selected: session,
    sessions: { s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "s", alive: true } },
    prefs: defaultPrefs(),
    explorer: {},
    git: { s1: { ...emptyGit(), commits: [commit()], done: true, logAsked: true, status: [], ...git } },
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  return render(<GitPane />);
}

beforeEach(() => cleanup());

/**
 * jsdom lays nothing out, so every element is 0 px wide and there is no
 * ResizeObserver. The log reads its own width once, at the grab, to turn a
 * pointer's pixels into shares, and the pane watches its body's to cap the
 * panes beside the log; this lends both a width, and an observer that
 * reports it the moment it is asked to watch.
 */
function withLayout<T>(px: number, run: () => T): T {
  class RO {
    cb: ResizeObserverCallback;
    constructor(cb: ResizeObserverCallback) {
      this.cb = cb;
    }
    observe() {
      this.cb([], this as unknown as ResizeObserver);
    }
    unobserve() {}
    disconnect() {}
  }
  const had = globalThis.ResizeObserver;
  globalThis.ResizeObserver = RO as unknown as typeof ResizeObserver;
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => px });
  try {
    return run();
  } finally {
    delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
    globalThis.ResizeObserver = had;
  }
}

describe("the log's columns", () => {
  /** The width the shares divide: the pane less the graph, which is pixels. */
  const span = (px: number) => px - parseFloat(screen.getByRole("row", { name: "columns" }).style.gridTemplateColumns);

  it("has a header row with a divider between each pair, and a drag moves that boundary alone", () => {
    show();
    expect(screen.getByRole("row", { name: "columns" })).toHaveTextContent(/subject.*author.*date.*marks/);
    withLayout(1018, () => {
      const w = span(1018);
      fireEvent.mouseDown(screen.getByRole("separator", { name: /subject and author/ }), { clientX: 500 });
      // Dragging the subject's right edge 30 px left gives the author those
      // 30 px; date and marks do not move, and the graph is pinned in pixels.
      fireEvent.mouseMove(window, { clientX: 470 });
      fireEvent.mouseUp(window);
      const cols = useStore.getState().prefs.gitLogColumns;
      expect(cols.author).toBeCloseTo(14 + (100 * 30) / w, 1);
      expect(cols.subject).toBeCloseTo(66 - (100 * 30) / w, 1);
      expect(cols).toMatchObject({ date: 12, marks: 8 });
      expect(cols.graph).not.toBeNull();
    });
  });

  it("never drags a column below its floor", () => {
    show();
    withLayout(1018, () => {
      const w = span(1018);
      fireEvent.mouseDown(screen.getByRole("separator", { name: /date and marks/ }), { clientX: 100 });
      fireEvent.mouseMove(window, { clientX: 900 });
      fireEvent.mouseUp(window);
      const cols = useStore.getState().prefs.gitLogColumns;
      expect((cols.marks * w) / 100).toBeCloseTo(24, 0);
      // What marks gave up, date took; the pair's sum is what it was.
      expect(cols.date + cols.marks).toBeCloseTo(20, 1);
    });
  });

  it("draws the shares as fr tracks over their floors, so a resize needs nothing measured", () => {
    show();
    // A store write outside React's own event needs `act` to be flushed
    // before the DOM is read — the same rule the ingest tests follow.
    act(() => useStore.setState({ prefs: { ...defaultPrefs(), gitLogColumns: { graph: 30, subject: 60, author: 20, date: 12, marks: 8 } } } as never));
    const header = screen.getByRole("row", { name: "columns" });
    // Percentages, not fractions: factors that sum below 1 fill only that
    // fraction of the row, and a floored column takes the rest below 1.
    expect(header.style.gridTemplateColumns).toBe("30px minmax(120px, 60fr) minmax(40px, 20fr) minmax(64px, 12fr) minmax(24px, 8fr)");
  });

  it("keeps every column in a narrow pane — they scale, they do not drop out", () => {
    // The first cut dropped the author below 700 px and the date and marks
    // below 480 px; a pane that narrow now draws all five, at their floors.
    withLayout(400, () => {
      show();
      const header = screen.getByRole("row", { name: "columns" });
      expect(header).toHaveTextContent(/subject.*author.*date.*marks/);
      expect(header.style.gridTemplateColumns.split(" minmax(")).toHaveLength(5);
    });
  });
});

describe("the panes' ceilings", () => {
  /** The body's children: branch pane, splitter, log, splitter, inspector. */
  const widths = () => {
    const body = screen.getByRole("listbox", { name: "commits" }).parentElement!.parentElement!;
    return [...body.children].map((c) => (c as HTMLElement).style.width);
  };

  it("measures a body that appears after the first render, so a tool opened before a session still caps the panes", () => {
    // The tool was open before a session was selected — a browser tab does
    // this — and the effect keyed on the ref object had nothing to observe
    // and never looked again: 210 and 360 of a 688-px dock, and 80 px of log.
    withLayout(688, () => {
      show({}, null);
      expect(screen.getByText("select a session")).toBeInTheDocument();
      act(() => useStore.setState({ selected: "s1" }));
      // 28 % of 688 for the branches; the inspector's own floor of 260 beats
      // what the log's 320 would leave it, which is 168.
      expect(widths()).toEqual(["192px", "", "", "", "260px"]);
    });
  });

  it("lets the inspector take everything but the log's floor, not a share of the dock", () => {
    withLayout(2000, () => {
      show();
      act(() => useStore.setState({ prefs: { ...defaultPrefs(), gitInspectorWidth: 1400 } } as never));
      // 2000 less the 210-px branch pane, two 4-px splitters and 320 for the
      // log is 1462: room for the whole 1400, where 42 % would have stopped at 840.
      expect(widths()).toEqual(["210px", "", "", "", "1400px"]);
    });
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
