/**
 * A sync's balloon, from the press to the answer, through the real store.
 * `R-J109`, asked 2026-09-25: *"once git sync complete, popup a little nice
 * info box saying sync complete (and if there is any warn/error msg)"*.
 *
 * The daemon is played by `ingest`: what it sends back after a `git_fetch` is
 * a `git_fetched`, or an `error` with no address — which is the case worth the
 * most here, because before this a failed fetch left the Git pane's button
 * disabled until a reload and said nothing anywhere else.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { syncRepo } from "@/lib/gitActions";
import { QUIET_MS } from "@/lib/gitSync";
import { emptyGit, useStore } from "@/store";
import { Toasts } from "@/ui/Notices";
import type { Session } from "@/wire/types";

const session = (id: string): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/home/k/projects/mogeung",
    repo_root: "/home/k/projects/mogeung",
    alive: true,
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
  }) as unknown as Session;

const fetched = (over: Record<string, unknown> = {}) =>
  act(() =>
    useStore.getState().ingest({
      ev: "git_fetched",
      session_id: "s1",
      updates: [],
      upstream: "origin/main",
      ahead: 0,
      behind: 0,
      ...over,
    } as never),
  );
const refused = (message: string) => act(() => useStore.getState().ingest({ ev: "error", message } as never));
const balloon = () => screen.getByRole("status", { name: "git sync" });
const fetchesSent = () => useStore.getState().git.s1?.console.filter((r) => r.cmd === "git_fetch").length ?? 0;

beforeEach(() => {
  useStore.setState({
    sessions: { s1: session("s1"), s2: session("s2") },
    selected: "s1",
    git: {},
    syncs: [],
    notices: [],
  } as never);
  render(<Toasts />);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("the balloon", () => {
  it("is up from the press, and says complete with where the branch stands", () => {
    act(() => void syncRepo("s1"));
    expect(balloon()).toHaveTextContent("Syncing mogeung…");
    expect(balloon()).toHaveTextContent("git fetch --all --prune");
    expect(fetchesSent()).toBe(1);
    expect(useStore.getState().git.s1.fetching).toBe(true);

    fetched({ behind: 2 });
    expect(balloon()).toHaveTextContent("Sync complete — mogeung");
    expect(balloon()).toHaveTextContent("2 behind origin/main");
    expect(balloon()).toHaveTextContent("the remotes had nothing new");
    expect(useStore.getState().git.s1.fetching).toBe(false);
  });

  it("lists what moved, and puts git's warnings apart from it", () => {
    act(() => void syncRepo("s1"));
    fetched({
      updates: ["   1a2b3c4..5d6e7f8  main -> origin/main", "warning: redirecting to https://example.test/r.git/"],
    });
    expect(balloon()).toHaveTextContent("Sync complete, with warnings — mogeung");
    expect(within(balloon()).getByRole("list", { name: "warnings" })).toHaveTextContent("warning: redirecting");
    expect(within(balloon()).getByRole("list", { name: "ref updates" })).toHaveTextContent("main -> origin/main");
    expect(balloon()).toHaveTextContent("1 ref updated");
  });

  /**
   * The failure that used to be silent everywhere but the log, and that left
   * the pane's fetch button disabled for good.
   */
  it("says failed with git's own words — once, not also as a toast — and frees the pane's button", () => {
    act(() => void syncRepo("s1"));
    refused("fatal: unable to access 'https://example.test/r.git/': Could not resolve host");
    expect(balloon()).toHaveTextContent("Sync failed — mogeung");
    expect(balloon()).toHaveTextContent("Could not resolve host");
    expect(screen.getAllByText(/Could not resolve host/)).toHaveLength(1);
    // Still in the log, and still on the Console row.
    expect(useStore.getState().notices.map((n) => n.text)).toEqual([
      "fatal: unable to access 'https://example.test/r.git/': Could not resolve host",
    ]);
    expect(useStore.getState().git.s1.console.at(-1)?.error).toMatch(/Could not resolve host/);
    expect(useStore.getState().git.s1.fetching).toBe(false);
  });

  it("an error that is not the fetch's is still an ordinary toast", () => {
    refused("that file is not in this repository");
    expect(screen.queryByRole("status", { name: "git sync" })).toBeNull();
    expect(screen.getByText("that file is not in this repository")).toBeInTheDocument();
  });

  it("a second press while it runs sends nothing, and brings a hidden balloon back", () => {
    act(() => void syncRepo("s1"));
    fireEvent.click(within(balloon()).getByTitle(/hide — the sync carries on/));
    expect(screen.queryByRole("status", { name: "git sync" })).toBeNull();
    act(() => void syncRepo("s1"));
    expect(fetchesSent()).toBe(1);
    expect(balloon()).toHaveTextContent("Syncing mogeung…");
  });

  it("a fetch another window started is that window's to report", () => {
    useStore.setState({ git: { s1: { ...emptyGit(), fetching: true } } } as never);
    fetched();
    expect(screen.queryByRole("status", { name: "git sync" })).toBeNull();
  });

  it("goes on its own once it has said its piece", () => {
    vi.useFakeTimers();
    act(() => void syncRepo("s1"));
    fetched();
    expect(balloon()).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(QUIET_MS + 100));
    expect(screen.queryByRole("status", { name: "git sync" })).toBeNull();
  });

  it("with no session, says which is missing instead of doing nothing", () => {
    act(() => void syncRepo(null));
    expect(screen.queryByRole("status", { name: "git sync" })).toBeNull();
    expect(screen.getByText(/pick a session first/)).toBeInTheDocument();
  });
});
