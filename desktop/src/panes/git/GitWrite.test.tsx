/**
 * The write half of the Git tool window. `R-D28`, A26's test.
 *
 * Every control sends its verb once, as the exact `ClientMsg`; discard and
 * drop send nothing until confirmed; a blank commit message never leaves the
 * window; a checkout warns only when an agent is live in the worktree.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { GitPane } from "@/panes/GitPane";
import { useStore, emptyGit } from "@/store";
import type { ClientMsg, RefsInfo, StatusEntry } from "@/wire/types";

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
const writes = () => sent.filter((m) => /^git_(stage|unstage|discard|commit|switch|branch_create|stash_|resolve)/.test((m as { cmd: string }).cmd));

const entry = (path: string, over: Partial<StatusEntry> = {}): StatusEntry => ({ path, staged: false, unstaged: true, state: " M", conflicted: false, ...over });

const refs: RefsInfo = {
  head: "main",
  head_sha: "abc1234",
  branches: [
    { name: "main", sha: "abc1234", current: true, upstream: null, ahead: 0, behind: 0, epoch: 0 },
    { name: "git-fetch", sha: "def5678", current: false, upstream: null, ahead: 0, behind: 0, epoch: 0 },
  ],
  tags: [],
  remotes: [],
  fetch_epoch: null,
  remote_branches: [],
};

function show(git: Partial<ReturnType<typeof emptyGit>> = {}, live = false) {
  sent.length = 0;
  useStore.setState({
    selected: "s1",
    sessions: {
      s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "the session", alive: live },
    },
    git: {
      s1: {
        ...emptyGit(),
        commits: [],
        done: true,
        logAsked: true,
        refs,
        status: [entry("a.rs"), entry("b.rs", { staged: true, unstaged: false, state: "M " }), entry("new.txt", { state: "??" })],
        ...git,
      },
    },
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  const r = render(<GitPane />);
  return r;
}

const openTab = (name: RegExp) => fireEvent.click(screen.getByRole("tab", { name }));

beforeEach(() => cleanup());

describe("Local changes", () => {
  it("stages and unstages by checkbox, one verb per click", () => {
    show();
    openTab(/Local changes/);
    fireEvent.click(screen.getByLabelText("stage a.rs"));
    fireEvent.click(screen.getByLabelText("unstage b.rs"));
    fireEvent.click(screen.getByLabelText("stage new.txt"));
    expect(writes()).toEqual([
      { cmd: "git_stage", session_id: "s1", paths: ["a.rs"] },
      { cmd: "git_unstage", session_id: "s1", paths: ["b.rs"] },
      { cmd: "git_stage", session_id: "s1", paths: ["new.txt"] },
    ]);
  });

  it("discards nothing until the dialog that names the file is confirmed", () => {
    show();
    openTab(/Local changes/);
    fireEvent.contextMenu(screen.getByTitle("a.rs"));
    fireEvent.click(screen.getByText("Discard changes…"));
    expect(writes()).toEqual([]);
    expect(screen.getByRole("dialog", { name: /Discard this change/ })).toBeInTheDocument();
    expect(screen.getAllByText("a.rs").length).toBeGreaterThan(1);
    fireEvent.click(screen.getByText("Discard it"));
    expect(writes()).toEqual([{ cmd: "git_discard", session_id: "s1", paths: ["a.rs"] }]);
  });

  it("commits what is staged with the message, the amend flag and the trailer, and never a blank", () => {
    show();
    openTab(/Local changes/);
    const commit = screen.getByRole("button", { name: "Commit" });
    expect(commit).toBeDisabled();
    fireEvent.change(screen.getByLabelText("commit message"), { target: { value: "   " } });
    expect(commit).toBeDisabled();
    fireEvent.change(screen.getByLabelText("commit message"), { target: { value: "fix: the thing" } });
    fireEvent.click(commit);
    expect(writes()).toEqual([{ cmd: "git_commit", session_id: "s1", message: "fix: the thing", amend: false, session_trailer: true }]);
  });

  it("resolves a conflicted file from its three sides", () => {
    show({ status: [entry("c.rs", { conflicted: true, state: "UU" })], selectedPath: "c.rs", conflict: { path: "c.rs", base: "b", ours: "o", theirs: "t", truncated: false } });
    openTab(/Local changes/);
    fireEvent.click(screen.getByRole("button", { name: "take theirs" }));
    fireEvent.click(screen.getByRole("button", { name: "mark resolved" }));
    expect(writes()).toEqual([
      { cmd: "git_resolve", session_id: "s1", path: "c.rs", side: "theirs" },
      { cmd: "git_resolve", session_id: "s1", path: "c.rs", side: "mine" },
    ]);
  });
});

describe("branches", () => {
  it("checks out straight away when nothing is live in the worktree", () => {
    show({}, false);
    fireEvent.contextMenu(screen.getByText("git-fetch"));
    fireEvent.click(screen.getByText(/Check out — move/));
    expect(writes()).toEqual([{ cmd: "git_switch", session_id: "s1", name: "git-fetch" }]);
  });

  it("names the live session and waits for a confirm when one is", () => {
    show({}, true);
    fireEvent.contextMenu(screen.getByText("git-fetch"));
    fireEvent.click(screen.getByText(/Check out — move/));
    expect(writes()).toEqual([]);
    expect(screen.getByRole("dialog", { name: /Check out git-fetch/ })).toHaveTextContent("the session");
    fireEvent.click(screen.getByText("Check out anyway"));
    expect(writes()).toEqual([{ cmd: "git_switch", session_id: "s1", name: "git-fetch" }]);
  });

  it("creates a branch from HEAD, checking it out when asked", () => {
    show();
    fireEvent.click(screen.getByRole("button", { name: /new branch from HEAD/ }));
    fireEvent.change(screen.getByLabelText("branch name"), { target: { value: "feat/x" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(writes()).toEqual([{ cmd: "git_branch_create", session_id: "s1", name: "feat/x", switch_to: true }]);
  });
});

describe("stashes", () => {
  it("pushes with the message and the untracked flag, pops on the menu, and drops only after asking", () => {
    show({ stashes: [{ index: 0, message: "WIP on main", epoch: 1 }] });
    openTab(/Stash/);
    fireEvent.change(screen.getByLabelText("stash message"), { target: { value: "park it" } });
    fireEvent.click(screen.getByRole("button", { name: "Stash all" }));
    fireEvent.contextMenu(screen.getByText("WIP on main"));
    fireEvent.click(screen.getByText(/^Pop/));
    fireEvent.contextMenu(screen.getByText("WIP on main"));
    fireEvent.click(screen.getByText("Drop…"));
    expect(writes()).toEqual([
      { cmd: "git_stash_push", session_id: "s1", message: "park it", include_untracked: true },
      { cmd: "git_stash_pop", session_id: "s1", index: 0 },
    ]);
    fireEvent.click(screen.getByText("Drop it"));
    expect(writes().at(-1)).toEqual({ cmd: "git_stash_drop", session_id: "s1", index: 0 });
  });
});
