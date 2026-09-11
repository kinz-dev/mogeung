/**
 * A diff in the centre. `R-D30`.
 *
 * The id round-trips; a pane draws from the store's revision cache without
 * asking; a pane whose diff is not cached asks exactly once; and the
 * inspector's button adds the pane to the dock with the kind and title the
 * registry expects.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { DiffPane } from "@/panes/DiffPane";
import { GitPane } from "@/panes/GitPane";
import { PaneScope } from "@/lib/paneScope";
import { diffPaneId, parseDiffPaneId, setDock, showDiffPane } from "@/lib/panes";
import { useStore, emptyGit } from "@/store";
import type { ClientMsg, FileChange } from "@/wire/types";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 22,
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ key: index, index, start: index * 22, size: 22 })),
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

const file = (path: string, hunks: number): FileChange => ({
  path,
  old_path: null,
  status: "modified",
  insertions: hunks,
  deletions: 0,
  hunks: Array.from({ length: hunks }, (_, i) => ({ anchor: `${path}#${i}`, header: `@@ -${i + 1} +${i + 1} @@`, lines: ["+x"], insertions: 1, deletions: 0, flags: [], score: 0, reviewed: false })),
  flags: [],
  score: 0,
  truncated: false,
});

const sent: ClientMsg[] = [];
beforeEach(() => {
  cleanup();
  sent.length = 0;
  useStore.setState({ revDiffs: {}, git: {}, send: ((m: ClientMsg) => sent.push(m)) as never } as never);
});

describe("the id", () => {
  it("round-trips, with colons in the path", () => {
    const id = diffPaneId("s1", "fa64b61", "a/b:c.rs");
    expect(id).toBe("diff:s1:fa64b61:a/b:c.rs");
    expect(parseDiffPaneId(id)).toEqual({ session: "s1", rev: "fa64b61", path: "a/b:c.rs" });
    expect(parseDiffPaneId("file:s1::x")).toBeNull();
    expect(parseDiffPaneId("diff:s1")).toBeNull();
  });
});

describe("the pane", () => {
  it("draws one file from the cache without asking, and every file for *", () => {
    useStore.setState({ revDiffs: { "s1:fa64b61": [file("a.rs", 2), file("b.rs", 1)] } } as never);
    const { unmount } = render(
      <PaneScope id={diffPaneId("s1", "fa64b61", "b.rs")} visible>
        <DiffPane />
      </PaneScope>,
    );
    expect(document.querySelectorAll("[data-hunk]")).toHaveLength(1);
    // The strip and the file header both count; one of each is enough.
    expect(screen.getAllByText(/0\/1 read/).length).toBeGreaterThan(0);
    expect(sent).toEqual([]);
    unmount();
    render(
      <PaneScope id={diffPaneId("s1", "fa64b61", "*")} visible>
        <DiffPane />
      </PaneScope>,
    );
    expect(document.querySelectorAll("[data-hunk]")).toHaveLength(3);
  });

  it("asks once when the cache has nothing, and a range asks for a range", () => {
    render(
      <PaneScope id={diffPaneId("s1", "aaaa..bbbb", "*")} visible>
        <DiffPane />
      </PaneScope>,
    );
    expect(screen.getByText(/reading aaaa\.\.bbbb/)).toBeInTheDocument();
    expect(sent).toEqual([{ cmd: "git_diff_range", session_id: "s1", from: "aaaa", to: "bbbb" }]);
    render(
      <PaneScope id={diffPaneId("s1", "fa64b61", "a.rs")} visible>
        <DiffPane />
      </PaneScope>,
    );
    expect(sent.at(-1)).toEqual({ cmd: "git_show", session_id: "s1", sha: "fa64b61" });
  });

  it("is filled by the answer whether or not the commit is still selected", () => {
    useStore.getState().ingest({ ev: "git_commit_diff", session_id: "s1", sha: "fa64b61", files: [file("a.rs", 1)], detail: null } as never);
    expect(useStore.getState().revDiffs["s1:fa64b61"]).toHaveLength(1);
    expect(useStore.getState().git.s1).toBeUndefined();
  });
});

describe("the inspector's way out", () => {
  it("adds a pane to the dock for the focused file, titled by its path", () => {
    const added: { id: string; component: string; title?: string }[] = [];
    setDock({ getPanel: () => undefined, panels: [], addPanel: (p: { id: string; component: string; title?: string }) => added.push(p), activeGroup: undefined } as never);
    const sha = "fa64b61".padEnd(40, "0");
    useStore.setState({
      selected: "s1",
      sessions: { s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "s", alive: true } },
      git: {
        s1: {
          ...emptyGit(),
          commits: [],
          done: true,
          logAsked: true,
          selected: sha,
          selectedFile: "b.rs",
          diff: [file("a.rs", 1), file("b.rs", 1)],
          detail: { author: "k", committer: "k", epoch: 1, commit_epoch: 1, parents: [], refs: [], message: "m", branches: [] },
          status: [],
        },
      },
    } as never);
    render(<GitPane />);
    fireEvent.click(screen.getByRole("button", { name: /open this diff as a pane/ }));
    expect(added).toEqual([{ id: diffPaneId("s1", sha, "b.rs"), component: "diff", title: "b.rs" }]);
    // Straight through the door as well, for the keyboard.
    showDiffPane("s1", sha, "*");
    expect(added[1]).toMatchObject({ component: "diff", title: `${sha.slice(0, 8)} · all files` });
  });
});
