/**
 * The Git tool window, region by region. `R-D26`.
 *
 * A fake socket records what the window asks, and the assertions are the
 * exact `ClientMsg` — the shape `GitPane.test.tsx` set. What is pinned here:
 * the default view asks for every ref; a branch click scopes the log and
 * checks nothing out; a hash goes to `git_show`; a file picked in the
 * inspector filters the diff without a round trip; the session toggle narrows
 * the rows; and a page from a daemon that does not echo the `R-D27` fields
 * is kept, where one that echoes a different scope is dropped.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { GitPane } from "@/panes/GitPane";
import { useStore, emptyGit } from "@/store";
import type { ClientMsg, CommitInfo, FileChange, RefsInfo } from "@/wire/types";

// jsdom has no layout, so the real virtualiser measures a viewport of zero
// and renders nothing. Every row is rendered instead; these tests say nothing
// about windowing.
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

const sent: ClientMsg[] = [];
const cmds = (name: string) => sent.filter((m) => (m as { cmd: string }).cmd === name);

const commit = (sha: string, summary: string, touches = false): CommitInfo => ({
  sha: sha.padEnd(40, "0"),
  short: sha.slice(0, 7),
  summary,
  author: "keith",
  epoch: 1_788_912_000,
  refs: [],
  parents: [],
  touches_session: touches,
});

const file = (path: string, hunks: number): FileChange => ({
  path,
  old_path: null,
  status: "modified",
  insertions: hunks,
  deletions: 0,
  hunks: Array.from({ length: hunks }, (_, i) => ({
    anchor: `${path}#${i}`,
    header: `@@ -${i + 1} +${i + 1} @@`,
    lines: [`+line ${i}`],
    insertions: 1,
    deletions: 0,
    flags: [],
    score: 0,
    reviewed: false,
  })),
  flags: [],
  score: 0,
  truncated: false,
});

const refs: RefsInfo = {
  head: "main",
  head_sha: "abc1234",
  branches: [
    { name: "main", sha: "abc1234", current: true, upstream: "origin/main", ahead: 0, behind: 0, epoch: 0 },
    { name: "git-fetch", sha: "def5678", current: false, upstream: null, ahead: 0, behind: 0, epoch: 0 },
  ],
  tags: [],
  remotes: [{ name: "origin", url: "git@github.com:kinz-dev/mogeung.git" }],
  fetch_epoch: null,
  remote_branches: [{ name: "origin/main", sha: "abc1234", current: false, upstream: null, ahead: 0, behind: 0, epoch: 0 }],
};

function show(git: Partial<ReturnType<typeof emptyGit>> = {}) {
  sent.length = 0;
  useStore.setState({
    selected: "s1",
    sessions: { s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "s", alive: true } },
    git: {
      s1: {
        ...emptyGit(),
        commits: [commit("aaaaaaa1", "feat: the one this session made", true), commit("bbbbbbb2", "docs: someone else's")],
        done: true,
        logAsked: true,
        refs,
        status: [],
        ...git,
      },
    },
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  return render(<GitPane />);
}

beforeEach(() => cleanup());

describe("the log", () => {
  it("asks for every ref by default, and only once", () => {
    useStore.setState({
      selected: "s1",
      sessions: { s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "s", alive: true } },
      git: {},
      send: ((m: ClientMsg) => sent.push(m)) as never,
    } as never);
    sent.length = 0;
    render(<GitPane />);
    expect(cmds("git_log")).toEqual([
      { cmd: "git_log", session_id: "s1", skip: 0, limit: 100, rev: null, grep: null, author: null, path: null, pickaxe: null, all: true, since: null, until: null },
    ]);
  });

  it("draws one row per commit and selects on click", () => {
    show();
    const rows = screen.getAllByRole("option");
    expect(rows.map((r) => r.getAttribute("data-sha"))).toEqual([commit("aaaaaaa1", "").sha, commit("bbbbbbb2", "").sha]);
    fireEvent.click(rows[1]);
    expect(cmds("git_show")).toEqual([{ cmd: "git_show", session_id: "s1", sha: commit("bbbbbbb2", "").sha }]);
    expect(rows[1]).toHaveAttribute("aria-selected", "true");
  });

  it("goes to the commit when a hash is typed, and filters otherwise", () => {
    show();
    const box = screen.getByPlaceholderText(/text or hash/);
    fireEvent.change(box, { target: { value: "fa64b61" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(cmds("git_show")).toEqual([{ cmd: "git_show", session_id: "s1", sha: "fa64b61" }]);
    expect(cmds("git_log")).toEqual([]);

    fireEvent.change(box, { target: { value: "author:kinz path:docs the word" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(cmds("git_log")).toMatchObject([{ grep: "the word", author: "kinz", path: "docs", pickaxe: null, all: true }]);
  });

  it("narrows to this session's commits without asking git", () => {
    show();
    fireEvent.click(screen.getByRole("button", { name: /session/ }));
    const rows = screen.getAllByRole("option");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent("feat: the one this session made");
    expect(cmds("git_log")).toEqual([]);
  });
});

describe("the branch pane", () => {
  it("scopes the log on click, and checks nothing out", () => {
    show();
    fireEvent.click(screen.getByText("git-fetch"));
    expect(cmds("git_log")).toMatchObject([{ rev: "git-fetch", all: false, skip: 0 }]);
    expect(cmds("git_switch")).toEqual([]);
    expect(cmds("git_branch_create")).toEqual([]);
  });

  it("groups remotes under their remote and names HEAD", () => {
    show();
    expect(screen.getByText("origin")).toBeInTheDocument();
    expect(screen.getByText("HEAD →")).toBeInTheDocument();
  });
});

describe("the inspector", () => {
  const diff = [file("crates/a.rs", 2), file("docs/b.md", 1)];

  it("shows one file's diff on selection, without a round trip", () => {
    const sel = commit("aaaaaaa1", "").sha;
    show({ selected: sel, diff, detail: { author: "keith", committer: "keith", epoch: 1, commit_epoch: 1, parents: [], refs: [], message: "feat: x\n\nbody", branches: ["main"] } });
    // Details first: no hunks on screen, the message is.
    expect(document.querySelectorAll("[data-hunk]")).toHaveLength(0);
    expect(screen.getByText("body")).toBeInTheDocument();
    sent.length = 0;
    fireEvent.click(screen.getByText("b.md"));
    expect(document.querySelectorAll("[data-hunk]")).toHaveLength(1);
    expect(sent).toEqual([]);
    fireEvent.click(screen.getByText("all files"));
    expect(document.querySelectorAll("[data-hunk]")).toHaveLength(3);
  });

  it("wears the header the daemon sent — emails, signature, branches", () => {
    show({
      selected: commit("aaaaaaa1", "").sha,
      diff,
      detail: {
        author: "keith",
        committer: "GitHub",
        epoch: 1,
        commit_epoch: 2,
        parents: ["p1"],
        refs: [],
        message: "feat: x",
        branches: ["main", "origin/main", "a", "b"],
        author_email: "k@example.com",
        committer_email: "noreply@github.com",
        signature: "B",
      },
    });
    expect(screen.getByText(/k@example.com/)).toBeInTheDocument();
    expect(screen.getByText(/noreply@github.com/)).toBeInTheDocument();
    expect(screen.getByText(/bad signature/)).toBeInTheDocument();
    expect(screen.getByText(/show all 4/)).toBeInTheDocument();
  });
});

describe("a page's echo", () => {
  it("keeps a page from a daemon that does not echo the R-D27 fields, and drops one that echoes a different scope", () => {
    show({ commits: [] });
    const { ingest } = useStore.getState();
    const page = (extra: object) =>
      ({ ev: "git_commits", session_id: "s1", skip: 0, commits: [commit("aaaaaaa1", "x")], done: true, rev: null, grep: null, author: null, path: null, pickaxe: null, ...extra }) as never;
    act(() => ingest(page({ all: false })));
    expect(useStore.getState().git.s1.commits).toEqual([]);
    act(() => ingest(page({})));
    expect(useStore.getState().git.s1.commits.map((c) => c.summary)).toEqual(["x"]);
  });
});
