/**
 * The log's filters, and the two ways they were being thrown away. `R-D12`.
 *
 * Reported 2026-08-22: *"I have entered a jira number and I am sure it is
 * pretty unique in the commit comment. But still it doesn't filter out the
 * other items."* Both halves of that are the client's fault — the daemon runs
 * `--fixed-strings -i --grep=…` and echoes the whole scope back precisely so a
 * superseded answer can be dropped.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { GitPane } from "@/panes/GitPane";
import { useStore, emptyGit } from "@/store";
import type { ClientMsg, CommitInfo } from "@/wire/types";

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

const commit = (sha: string, summary: string): CommitInfo =>
  ({
    sha,
    short: sha.slice(0, 7),
    summary,
    author: "keith",
    epoch: 1_784_995_957,
    refs: [],
    parents: [],
    touches_session: false,
  }) as never;

function show(
  commits: CommitInfo[] = [commit("aaaaaaa1", "PROJ-1 the one you want"), commit("bbbbbbb2", "something else")],
  applied: Partial<{ grep: string; author: string; path: string; pickaxe: string }> = {},
) {
  sent.length = 0;
  useStore.setState({
    selected: "s1",
    sessions: { s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "s", alive: true } },
    git: { s1: { ...emptyGit(), commits, done: true, logAsked: true, ...applied } },
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  return render(<GitPane />);
}

const logs = () => sent.filter((m) => (m as { cmd: string }).cmd === "git_log");

beforeEach(() => cleanup());

describe("filtering the log", () => {
  /**
   * **The bug.** `askLog` empties the list so the filtered page can replace it,
   * and the pane's one-door fetch reads an empty list as "nobody has asked yet"
   * — so every filtered query fired a second, *unfiltered* one behind it, whose
   * answer landed last and put the whole log back.
   */
  it("asks for the filtered log, and does not ask for the whole log again", () => {
    show();

    fireEvent.change(screen.getByPlaceholderText(/filter messages/), {
      target: { value: "PROJ-1" },
    });
    fireEvent.keyDown(screen.getByPlaceholderText(/filter messages/), { key: "Enter" });

    expect(logs()).toEqual([
      {
        cmd: "git_log",
        session_id: "s1",
        skip: 0,
        limit: 100,
        rev: null,
        grep: "PROJ-1",
        author: null,
        path: null,
        pickaxe: null,
      },
    ]);
  });

  /**
   * The second half, and the one that made it look like the filter was ignored
   * rather than slow: an answer for a query nobody is waiting for any more must
   * be dropped. The daemon echoes the scope back for exactly this.
   */
  it("drops a page that answers a different filter", () => {
    show();
    fireEvent.change(screen.getByPlaceholderText(/filter messages/), {
      target: { value: "PROJ-1" },
    });
    fireEvent.keyDown(screen.getByPlaceholderText(/filter messages/), { key: "Enter" });

    const { ingest } = useStore.getState();
    act(() =>
      ingest({
      ev: "git_commits",
      session_id: "s1",
      skip: 0,
      commits: [commit("aaaaaaa1", "PROJ-1 the one you want")],
      done: true,
      rev: null,
      grep: "PROJ-1",
      author: null,
      path: null,
        pickaxe: null,
      } as never),
    );
    // The unfiltered answer, arriving late.
    act(() =>
      ingest({
      ev: "git_commits",
      session_id: "s1",
      skip: 0,
      commits: [commit("aaaaaaa1", "PROJ-1 the one you want"), commit("bbbbbbb2", "something else")],
      done: true,
      rev: null,
        grep: null,
        author: null,
        path: null,
        pickaxe: null,
      } as never),
    );

    expect(useStore.getState().git.s1.commits.map((c) => c.sha)).toEqual(["aaaaaaa1"]);
  });

  /** A filter that matches nothing says so, rather than sitting on "reading
   *  the log…" — which reads as a query that never came back. */
  it("says when nothing matches, instead of looking like it is still loading", () => {
    show([]);
    expect(screen.getByText(/no commit matches/)).toBeInTheDocument();
  });
});

/**
 * The affordance half of the same report. A filter that has been typed but not
 * run looks *identical* to a filter that has been ignored — so the pane says
 * which of the two it is, and names the filter the list actually answers.
 */
describe("what the list is answering", () => {
  it("says a typed filter has not been run yet", () => {
    show();
    fireEvent.change(screen.getByPlaceholderText(/filter messages/), {
      target: { value: "PROJ-1" },
    });

    expect(screen.getByText(/press Enter to run/)).toBeInTheDocument();
  });

  it("names the filter in force, and clears every one of them at once", () => {
    show([commit("aaaaaaa1", "PROJ-1")], { grep: "PROJ-1", author: "keith" });

    expect(screen.getByText(/message ~ PROJ-1/)).toBeInTheDocument();

    sent.length = 0;
    fireEvent.click(screen.getByText("clear"));

    // Every filter dropped in one query, rather than the box you can see and
    // not the two behind the funnel.
    expect(logs()).toEqual([
      {
        cmd: "git_log",
        session_id: "s1",
        skip: 0,
        limit: 100,
        rev: null,
        grep: null,
        author: null,
        path: null,
        pickaxe: null,
      },
    ]);
  });
});

/**
 * Every hook this pane has must run for **every** session, including the ones
 * it refuses to draw for. Caught in the running window rather than here: the
 * filter-sync ref went in below `if (!s.repo_root) return`, and selecting a
 * session outside a repository took the whole window to a blank page with
 * *"Rendered more hooks than during the previous render"*.
 */
describe("a session that is not in a repository", () => {
  it("says so, and does not change how many hooks ran", () => {
    sent.length = 0;
    useStore.setState({
      selected: "s2",
      sessions: { s2: { id: "s2", cwd: "/tmp/loose", repo_root: null, title: "s2", alive: true } },
      git: {},
      send: ((m: ClientMsg) => sent.push(m)) as never,
    } as never);
    const { rerender } = render(<GitPane />);
    expect(screen.getByText(/not in a git repo/)).toBeInTheDocument();

    // The same component instance, now on a session that *is* a repository.
    useStore.setState({
      selected: "s1",
      sessions: { s1: { id: "s1", cwd: "/repo", repo_root: "/repo", title: "s", alive: true } },
      git: { s1: { ...emptyGit(), commits: [commit("aaaaaaa1", "PROJ-1")], done: true, logAsked: true } },
    } as never);
    expect(() => rerender(<GitPane />)).not.toThrow();
  });
});
