/**
 * The folder, on the status bar. `R-J48`.
 *
 * These moved here from `PaneChrome.test.tsx` with the thing they test — they
 * were written for `PaneCwd`, the left header action added on 2026-08-07, and
 * the two that matter are unchanged by the move: it is `cwd` and not
 * `repo_root`, and a path too long for the row loses whole leading segments
 * rather than its tail.
 *
 * What is new is the last one. Down here the folder shares a row with *"4m
 * since last event"*, and the ask was explicitly that the two not read as one
 * grey phrase — so the frame is a tested property rather than a styling detail
 * that the next person to tidy the row is free to drop.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { StatusBar } from "@/ui/StatusBar";
import { TooltipProvider } from "@/ui/primitives";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { Session } from "@/wire/types";

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/tmp/repo",
    repo_root: "/tmp/repo",
    alive: true,
    source: "claude_code",
    turns: 3,
    tool_calls: 4,
    tokens_in: 10,
    tokens_out: 20,
    files_changed: 0,
    insertions: 0,
    deletions: 0,
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
    ...extra,
  }) as unknown as Session;

const bar = () =>
  render(
    <TooltipProvider>
      <StatusBar />
    </TooltipProvider>,
  );

beforeEach(() => {
  cleanup();
  useStore.setState({ prefs: defaultPrefs(), selected: "s1", sessions: { s1: session("s1") }, health: null });
});

describe("the folder on the status bar", () => {
  it("names the directory the session was started in", () => {
    useStore.setState({ sessions: { s1: session("s1", { cwd: "/home/kinz/projects/mogeung" }) } });
    bar();
    expect(screen.getByTitle("started in /home/kinz/projects/mogeung")).toBeInTheDocument();
  });

  /**
   * `cwd`, not `repo_root`. They differ exactly when a session was started in a
   * subdirectory, which is the case where being told is worth the width.
   */
  it("shows where the CLI was run, not the repository it found", () => {
    useStore.setState({ sessions: { s1: session("s1", { cwd: "/repo/crates/daemon", repo_root: "/repo" }) } });
    bar();
    expect(screen.getByTitle("started in /repo/crates/daemon")).toBeInTheDocument();
  });

  /**
   * Whole leading segments come off; the tail is the half that identifies it.
   * The budget is **64**, asked for 2026-08-25 — `dirTail`'s default of 34 was
   * picked for a pane header competing with the tabs, and this row is wider.
   */
  it("shortens a long path from the front, keeping the whole one on hover", () => {
    const cwd = `/home/kinz/work/${"deeply/".repeat(12)}nested/checkout`;
    useStore.setState({ sessions: { s1: session("s1", { cwd }) } });
    bar();
    const chip = screen.getByTitle(`started in ${cwd}`);
    expect(chip).toHaveTextContent("…/");
    expect(chip).not.toHaveTextContent(cwd);
  });

  /** The 56-character path that fitted nowhere before now fits whole. */
  it("shows a path the old 34-character budget would have cut", () => {
    const cwd = "/home/kinz/work/very/deeply/nested/checkout/of/something";
    expect(cwd.length).toBeGreaterThan(34);
    expect(cwd.length).toBeLessThanOrEqual(64);
    useStore.setState({ sessions: { s1: session("s1", { cwd }) } });
    bar();
    expect(screen.getByTitle(`started in ${cwd}`)).toHaveTextContent(cwd);
  });

  /** The ask: it must not read as more of the sentence beside it. */
  it("is framed, so it is a different kind of thing from the elapsed time", () => {
    bar();
    const chip = screen.getByTitle("started in /tmp/repo");
    expect(chip.className).toContain("border");
    expect(chip.className).toContain("font-mono");
  });

  it("says nothing when no session is selected", () => {
    useStore.setState({ selected: null });
    bar();
    expect(screen.queryByTitle(/started in/)).toBeNull();
  });
});
