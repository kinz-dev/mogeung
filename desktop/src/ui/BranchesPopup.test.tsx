/**
 * The Branches popup. `R-D32`.
 *
 * One box over branches *and* actions, and the two rows that are not routing:
 * *Update Project* is a fetch and merges nothing, and *Checkout Tag or
 * Revision* is the only thing in the window that detaches HEAD.
 *
 * The guard is the assertion that matters most: a checkout from here goes
 * behind `R-D21`'s live-session warning, because a popup that skipped it would
 * be a way around a guard the pane has.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { BranchesPopup } from "@/ui/BranchesPopup";
import { useStore, emptyGit } from "@/store";
import type { ClientMsg, RefsInfo, Session } from "@/wire/types";

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

const refs: RefsInfo = {
  head: "main",
  head_sha: "abc1234def",
  branches: [
    { name: "main", sha: "abc1234", current: true, upstream: "origin/main", ahead: 1, behind: 2, epoch: 0 },
    { name: "claude/linear-contract", sha: "def5678", current: false, upstream: null, ahead: 0, behind: 0, epoch: 0 },
  ],
  tags: [{ name: "v1.4.0", sha: "abc1234", epoch: 0 }],
  remotes: [{ name: "origin", url: "git@example.com:x/y.git" }],
  fetch_epoch: null,
  remote_branches: [],
};

const session = (over: Partial<Session> = {}): Session =>
  ({ id: "s1", cwd: "/repo", repo_root: "/repo", title: "the session", alive: false, ...over }) as unknown as Session;

/** `live` puts a second session in the same worktree — `R-D21`'s trigger. */
function open(opts: { live?: boolean; repo?: boolean } = {}) {
  sent.length = 0;
  const sessions: Record<string, Session> = {
    s1: session(opts.repo === false ? { repo_root: null } : {}),
  };
  if (opts.live) sessions.s2 = session({ id: "s2", title: "an agent at work", alive: true });
  useStore.setState({
    selected: "s1",
    sessions,
    git: { s1: { ...emptyGit(), refs } },
    gitPopup: "branches",
    notices: [],
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  return render(<BranchesPopup />);
}

const box = () => screen.getByPlaceholderText(/Search for branches/);
const type = (v: string) => fireEvent.change(box(), { target: { value: v } });

beforeEach(() => cleanup());

describe("the branches popup", () => {
  it("lists the actions, the repository and the refs in one place", () => {
    open();
    expect(screen.getByText(/Update Project/)).toBeInTheDocument();
    expect(screen.getByText("New Branch…")).toBeInTheDocument();
    expect(screen.getByText("Checkout Tag or Revision…")).toBeInTheDocument();
    // Grouped on `/` the way IntelliJ's is, so the row reads as the leaf
    // under a `claude` folder rather than as the whole ref.
    expect(screen.getByText("claude")).toBeInTheDocument();
    expect(screen.getByText("linear-contract")).toBeInTheDocument();
    expect(screen.getByText("Tags")).toBeInTheDocument();
    // The repository line, with the branch it is on and its drift.
    expect(screen.getByText("repo")).toBeInTheDocument();
    expect(screen.getByText("↑1 ↓2")).toBeInTheDocument();
  });

  /** One box over both, which is what the placeholder promises. */
  it("searches branches and actions together", () => {
    open();
    type("contract");
    expect(screen.getByText("linear-contract")).toBeInTheDocument();
    expect(screen.queryByText("New Branch…")).not.toBeInTheDocument();

    type("branch");
    expect(screen.getByText("New Branch…")).toBeInTheDocument();
    expect(screen.queryByText("linear-contract")).not.toBeInTheDocument();
  });

  it("checks a branch out on Enter when nothing is live in the worktree", () => {
    open();
    type("contract");
    fireEvent.keyDown(screen.getByRole("dialog", { name: "Branches" }), { key: "Enter" });
    expect(sent).toEqual([
      { cmd: "git_switch", session_id: "s1", name: "claude/linear-contract", detach: false },
    ]);
    expect(useStore.getState().gitPopup).toBeNull();
  });

  /**
   * `R-D21`. The worktree is shared, so checking out moves the files under an
   * agent that is reading them — and only the window knows a session is live.
   */
  it("names the live session and sends nothing until that is confirmed", () => {
    open({ live: true });
    type("contract");
    fireEvent.click(screen.getByText("linear-contract"));
    expect(sent).toEqual([]);
    expect(screen.getByRole("dialog", { name: /while sessions are running/ })).toBeInTheDocument();
    expect(screen.getByText("an agent at work")).toBeInTheDocument();
    fireEvent.click(screen.getByText(/Check out claude\/linear-contract/));
    expect(sent).toEqual([
      { cmd: "git_switch", session_id: "s1", name: "claude/linear-contract", detach: false },
    ]);
  });

  /** The one daemon gap either screen had, and the only detach in the window. */
  it("detaches for a tag or a revision, and only there", () => {
    open();
    fireEvent.click(screen.getByText("Checkout Tag or Revision…"));
    fireEvent.change(screen.getByPlaceholderText(/v1\.4\.0/), { target: { value: "v1.4.0" } });
    fireEvent.click(screen.getByText("Checkout"));
    expect(sent).toEqual([{ cmd: "git_switch", session_id: "s1", name: "v1.4.0", detach: true }]);
  });

  it("creates a branch from HEAD, switching to it when asked", () => {
    open();
    fireEvent.click(screen.getByText("New Branch…"));
    fireEvent.change(screen.getByPlaceholderText("feature/thing"), { target: { value: "feature/x" } });
    fireEvent.click(screen.getByText("Create"));
    expect(sent).toEqual([
      { cmd: "git_branch_create", session_id: "s1", name: "feature/x", switch_to: true },
    ]);
  });

  it("fetches from the row that says it merges nothing", () => {
    open();
    fireEvent.click(screen.getByText(/Update Project/));
    expect(sent).toEqual([{ cmd: "git_fetch", session_id: "s1" }]);
  });

  it("refuses push here too, with the same sentence", () => {
    open();
    fireEvent.click(screen.getByText("Push…"));
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/never publishes/);
    expect(sent).toEqual([]);
  });

  it("says so rather than drawing an empty tree outside a repository", () => {
    open({ repo: false });
    expect(screen.getByText(/not in a git repository/)).toBeInTheDocument();
  });
});
