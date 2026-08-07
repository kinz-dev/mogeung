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
    expect(screen.getByText("nothing running")).toBeInTheDocument();
  });

  /**
   * Live only, asked for after the first look at it. A tile earns its square by
   * being something that might change while you watch; a finished session never
   * will, and a grid of inert squares is a grid you stop scanning.
   */
  it("shows only sessions that are still running", () => {
    board({
      queue: [item("a", "idle"), item("b", "needs_review")],
      sessions: [session("a"), session("b", { alive: false })],
    });
    expect(screen.getByText(/1 session\b/)).toBeInTheDocument();
    expect(screen.queryByText("session b")).toBeNull();
  });

  it("says nothing is running rather than nothing exists", () => {
    board({ queue: [item("a", "needs_review")], sessions: [session("a", { alive: false })] });
    expect(screen.getByText("nothing running")).toBeInTheDocument();
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

  /**
   * Fewer tiles once the wall went live-only, so each one can afford to say
   * more. The rule the card follows: only what is **non-zero** — a tile of
   * `0 turns · 0f +0 −0` reads as broken rather than as quiet.
   */
  it("carries the work a session has actually done", () => {
    board({
      queue: [item("a", "running")],
      sessions: [
        session("a", {
          repo_root: "/home/me/projects/mogeung",
          turns: 12,
          files_changed: 3,
          insertions: 140,
          deletions: 22,
          tokens_out: 48200,
        }),
      ],
    });
    expect(screen.getByText("mogeung")).toBeInTheDocument();
    expect(screen.getByText("12 turns")).toBeInTheDocument();
    expect(screen.getByText(/3f/)).toBeInTheDocument();
    expect(screen.getByText(/48\.2k out|48k out/)).toBeInTheDocument();
  });

  it("says nothing about counts that are zero", () => {
    board({ queue: [item("a", "running")], sessions: [session("a", { turns: 0, files_changed: 0 })] });
    expect(screen.queryByText(/turns/)).toBeNull();
    expect(screen.queryByText(/0f/)).toBeNull();
  });

  /**
   * The number that changes what you do next. A prompt waited on for four
   * minutes is a different situation from the same prompt four seconds old,
   * and nothing else on the card tells them apart.
   */
  it("says how long a waiting session has been waiting", () => {
    const since = new Date(Date.now() - 4 * 60 * 1000).toISOString();
    board({
      queue: [item("a", "awaiting_permission")],
      sessions: [session("a", { status_since: since })],
    });
    expect(screen.getByText(/^for /)).toBeInTheDocument();
  });

  it("keeps that number off a session that wants nothing", () => {
    const since = new Date(Date.now() - 4 * 60 * 1000).toISOString();
    board({ queue: [item("a", "running")], sessions: [session("a", { status_since: since })] });
    expect(screen.queryByText(/^for /)).toBeNull();
  });

  it("flags a collision, a loop and a rate limit where they exist", () => {
    board({
      queue: [item("a", "running")],
      sessions: [
        session("a", {
          collisions: [{ other: "b", path: "src/main.rs" }] as never,
          loop_signal: "Edit src/main.rs ×6",
          limit_hit_at: new Date().toISOString(),
        }),
      ],
    });
    expect(screen.getByText("1 collision")).toBeInTheDocument();
    expect(screen.getByText("thrashing")).toBeInTheDocument();
    expect(screen.getByText("limit")).toBeInTheDocument();
  });

  /**
   * The **whole card**, not a dot. `R-B42` already learnt this about the
   * queue's 4px bar: the point of a colour is knowing which session this is
   * without reading, and something you have to look for has not done that.
   */
  it("tints the whole card with the colour you gave the session", () => {
    useStore.setState({
      prefs: { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), tags: { a: "red" } } } },
      showWall: true,
      selected: null,
      queue: [item("a", "running")],
      sessions: { a: session("a") },
    });
    render(<WallOverlay />);
    const card = screen.getByRole("button", { name: /session a/ });
    expect(card.className).toContain("mogeung-tag-row");
    expect(card.style.getPropertyValue("--tag-bg")).not.toBe("");
  });

  it("leaves an untagged card on the plain surface", () => {
    board({ queue: [item("a", "running")], sessions: [session("a")] });
    const card = screen.getByRole("button", { name: /session a/ });
    expect(card.className).not.toContain("mogeung-tag-row");
  });

  it("skips a queue entry whose session it has never seen", () => {
    board({ queue: [item("ghost", "idle"), item("a", "idle")], sessions: [session("a")] });
    expect(screen.getByText(/1 session\b/)).toBeInTheDocument();
  });
});
