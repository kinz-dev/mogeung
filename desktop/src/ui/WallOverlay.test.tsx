/**
 * The wall. `R-B50`.
 *
 * Its one claim over the queue is that **it does not move** — a ranked list
 * reorders, so a row is somewhere new every time you look and spatial memory
 * never forms. That claim is a sort order, and a sort order is exactly the kind
 * of thing a later "improvement" quietly reverts to `score`, so it is the first
 * thing pinned here.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { WallOverlay } from "@/ui/WallOverlay";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import type { AttentionItem, Session } from "@/wire/types";

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    name: null,
    last_prompt: null,
    cwd: "/tmp/repo",
    repo_root: "/tmp/repo",
    git_branch: "main",
    alive: true,
    last_event_at: new Date().toISOString(),
    last_activity: null,
    recent_tools: [],
    error: null,
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    ...extra,
  }) as unknown as Session;

const item = (id: string, reason: AttentionItem["reason"], score = 0): AttentionItem => ({
  session_id: id,
  reason,
  score,
  detail: "",
});

function board(opts: { queue: AttentionItem[]; sessions: Session[]; hidden?: string[] }) {
  useStore.setState({
    prefs: { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), hidden: opts.hidden ?? [] } } },
    showWall: true,
    selected: null,
    queue: opts.queue,
    sessions: Object.fromEntries(opts.sessions.map((s) => [s.id, s])),
  });
  render(<WallOverlay />);
}

beforeEach(() => cleanup());

describe("the wall", () => {
  it("shows nothing at all until it is opened", () => {
    useStore.setState({ showWall: false, queue: [item("a", "idle")], sessions: { a: session("a") } });
    render(<WallOverlay />);
    expect(screen.queryByRole("dialog", { name: "the wall" })).toBeNull();
  });

  /**
   * The whole point. `b` outranks `a` in the queue and must still sit second on
   * the wall — a tile that moves when a session changes state is a tile you
   * have to find again, which is the problem this exists to solve.
   */
  it("orders tiles by a stable key, never by the queue's score", () => {
    board({
      queue: [item("b", "awaiting_permission", 90), item("a", "idle", 1)],
      sessions: [session("a"), session("b")],
    });
    const names = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    expect(names[0]).toContain("session a");
    expect(names[1]).toContain("session b");
  });

  it("counts what is waiting, using the queue's own verdict", () => {
    board({
      queue: [item("a", "awaiting_permission"), item("b", "running"), item("c", "failed")],
      sessions: [session("a"), session("b"), session("c")],
    });
    expect(screen.getByText(/3 sessions · 2 waiting/)).toBeInTheDocument();
  });

  it("says so rather than showing an empty grid", () => {
    board({ queue: [], sessions: [] });
    expect(screen.getByText("nothing to show")).toBeInTheDocument();
  });

  /** One list, one opinion. A session hidden from the queue is hidden here. */
  it("honours the sessions you have hidden", () => {
    board({
      queue: [item("a", "idle"), item("b", "idle")],
      sessions: [session("a"), session("b")],
      hidden: ["b"],
    });
    expect(screen.getByText(/1 session\b/)).toBeInTheDocument();
    expect(screen.queryByText("session b")).toBeNull();
  });

  it("goes to a session when its tile is clicked, and leaves", () => {
    board({ queue: [item("a", "idle")], sessions: [session("a")] });
    fireEvent.click(screen.getByRole("button", { name: /session a/ }));
    expect(useStore.getState().selected).toBe("a");
    expect(useStore.getState().showWall).toBe(false);
  });

  it("leaves on Escape without changing the selection", () => {
    board({ queue: [item("a", "idle")], sessions: [session("a")] });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(useStore.getState().showWall).toBe(false);
    expect(useStore.getState().selected).toBeNull();
  });

  /**
   * The tile is a contact sheet, not a terminal — the row's cheap build. What
   * it shows comes from the snapshot the window already has, so opening the
   * wall costs no fetches at all.
   */
  it("shows what a session is doing, from data already streamed", () => {
    board({
      queue: [item("a", "running")],
      sessions: [session("a", { last_activity: "editing src/main.rs", recent_tools: ["Edit", "Bash"] })],
    });
    expect(screen.getByText("editing src/main.rs")).toBeInTheDocument();
    expect(screen.getByText("Edit · Bash")).toBeInTheDocument();
  });

  it("wears the queue's own label for why it wants you", () => {
    board({ queue: [item("a", "awaiting_permission")], sessions: [session("a")] });
    expect(screen.getByText("APPROVE")).toBeInTheDocument();
  });

  it("skips a queue entry whose session it has never seen", () => {
    board({ queue: [item("ghost", "idle"), item("a", "idle")], sessions: [session("a")] });
    expect(screen.getByText(/1 session\b/)).toBeInTheDocument();
  });
});
