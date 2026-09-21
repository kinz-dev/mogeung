/**
 * More than one repository. `R-D34`.
 *
 * Asked 2026-09-16 from a workspace folder holding four checkouts that one
 * gradle build composes: a session opened in `immix-trading-v3` could be shown
 * that repository and no other, while the sibling it builds against was one
 * `cd` away in the terminal.
 *
 * The rules worth pinning are all about **which** repository a request is
 * about, and what happens to the window when that changes — none of which is
 * visible in a screenshot and all of which is a bug if it is wrong.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { GitPane } from "@/panes/GitPane";
import { useStore, emptyGit, emptyExplorer } from "@/store";
import { pickRepo, repoOf } from "@/lib/gitActions";
import type { ClientMsg, RefsInfo, Session } from "@/wire/types";

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

const OWN = "/ws/immix-trading-v3";
const SIBLING = "/ws/immix-sequencer";

const sent: ClientMsg[] = [];
const git = () => sent.filter((m) => (m as { cmd: string }).cmd.startsWith("git_"));

const refs: RefsInfo = {
  head: "main",
  head_sha: "abc1234",
  branches: [{ name: "main", sha: "abc1234", current: true, upstream: null, ahead: 0, behind: 0, epoch: 0 }],
  tags: [],
  remotes: [],
  fetch_epoch: null,
  remote_branches: [],
};

/** A session in `immix-trading-v3` with the sequencer in its workspace. */
function show(opts: { repos?: string[]; ownRoot?: string | null } = {}) {
  cleanup();
  sent.length = 0;
  const repos = opts.repos ?? [OWN, SIBLING];
  useStore.setState({
    selected: "s1",
    gitTab: "log",
    sessions: {
      s1: {
        id: "s1",
        cwd: OWN,
        repo_root: opts.ownRoot === undefined ? OWN : opts.ownRoot,
        title: "the session",
        alive: true,
      } as unknown as Session,
    },
    git: { s1: { ...emptyGit(), refs, status: [], logAsked: true, done: true } },
    explorer: {
      s1: { ...emptyExplorer(), workspace: { dirs: [SIBLING], missing: [], hints: [], repos } },
    },
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  return render(<GitPane />);
}

beforeEach(() => cleanup());

describe("the repository picker", () => {
  it("names the session's own repository, and lists the ones it may show", () => {
    show();
    const trigger = screen.getByRole("button", { name: /immix-trading-v3/ });
    expect(trigger).toHaveTextContent("immix-trading-v3");
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(screen.getByText("immix-sequencer")).toBeInTheDocument();
    // Its own is marked as such, so "why is this one different" is answered
    // before it is asked.
    expect(screen.getByText("this session's")).toBeInTheDocument();
  });

  /** One repository is not a choice, and a control that does nothing is worse
   *  than no control. */
  it("stays a label when there is nothing to pick", () => {
    show({ repos: [OWN] });
    expect(screen.queryByRole("button", { name: /immix-trading-v3/ })).toBeNull();
    expect(screen.getByText(/immix-trading-v3/)).toBeInTheDocument();
  });

  /**
   * The whole point of the ask: a session opened in the workspace folder is in
   * no repository at all, and used to get a dead end.
   */
  it("serves a session that is not in a repository itself", () => {
    show({ ownRoot: null, repos: [SIBLING] });
    expect(screen.queryByText(/not in a git repo/)).toBeNull();
  });

  it("says what to do when there is no repository anywhere", () => {
    show({ ownRoot: null, repos: [] });
    expect(screen.getByText(/not in a git repo/)).toBeInTheDocument();
    expect(screen.getByText(/add one in Files/)).toBeInTheDocument();
  });
});

describe("picking one", () => {
  it("puts the repository on every request the window makes", () => {
    show();
    act(() => pickRepo("s1", SIBLING));
    expect(repoOf("s1")).toBe(SIBLING);
    // The pane re-asks its three first questions for the new repository.
    expect(git().length).toBeGreaterThan(0);
    for (const m of git()) {
      expect((m as { repo?: string | null }).repo, (m as { cmd: string }).cmd).toBe(SIBLING);
    }
  });

  /** Everything in the slice belongs to the repository it came from. */
  it("clears what belonged to the one before", () => {
    show();
    act(() => {
      useStore.getState().patchGit("s1", { commits: [{ sha: "x", parents: [], subject: "one", author: "t", email: "t@e", epoch: 0, refs: [] }] as never, selected: "x" });
    });
    act(() => pickRepo("s1", SIBLING));
    const st = useStore.getState().git.s1;
    expect(st.commits).toEqual([]);
    expect(st.refs).toBeNull();
    expect(st.status).toBeNull();
    expect(st.selected).toBeNull();
    expect(st.repo).toBe(SIBLING);
  });

  /** Back to its own is `null`, not the path — so a session that never picked
   *  anything and one that picked its way home send the same message. */
  it("goes home as the absence of a repository, not as its path", () => {
    show();
    act(() => pickRepo("s1", SIBLING));
    const trigger = screen.getByRole("button", { name: /immix-sequencer/ });
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(screen.getByText("immix-trading-v3"));
    expect(repoOf("s1")).toBeNull();
  });
});

describe("answers that are not for the repository on screen", () => {
  /**
   * A write verb re-broadcasts the status to **every** window watching the
   * session. Two windows on two repositories of one session would otherwise
   * feed each other — the reason the daemon echoes the repo back at all.
   */
  it("are dropped rather than shown under the wrong name", () => {
    show();
    act(() => pickRepo("s1", SIBLING));
    const deliver = (repo: string | null) =>
      act(() => {
        useStore.getState().ingest({
          ev: "git_local_changes",
          session_id: "s1",
          repo,
          entries: [{ path: `${repo}/a.rs`, staged: false, unstaged: true, state: " M", conflicted: false }],
        } as never);
      });

    deliver(OWN);
    expect(useStore.getState().git.s1.status, "the other repository's status").toBeNull();

    deliver(SIBLING);
    expect(useStore.getState().git.s1.status?.[0]?.path).toBe(`${SIBLING}/a.rs`);
  });
});
