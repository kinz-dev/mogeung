/**
 * A folder you keep has to survive the window closing, removing one has to not
 * launch it (`R-J45`), and the launch has to start the CLI you picked
 * (`R-J51`).
 *
 * Both of these are wiring rather than logic — [`lib/favourites.ts`](../lib/favourites.ts)
 * has its own tests for the list itself. What is asserted here is the pair of
 * joins the list cannot check on its own: that the star writes through
 * `setScoped`, so the entry is in `localStorage` and not merely in this
 * render's state; and that the ✕ inside a clickable row stops the row's own
 * handler. The second is the one with previous form in this codebase — the
 * diff's blast-radius button folded the file it was asking about — and it
 * fails silently, by setting the directory field on the way to deleting the
 * row you were pointing at.
 */

import { describe, expect, it, beforeEach, afterEach } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LaunchWindow } from "@/ui/LaunchWindow";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

function open(favourites: string[] = []) {
  useStore.setState({ prefs: defaultPrefs(), showLaunch: true, sent: [] } as never);
  // Through the store's own setter, so the key is the one `scoped()` reads
  // with no daemon connected rather than one this test picked.
  useStore.getState().setScoped({ favouriteDirs: favourites });
  render(<LaunchWindow />);
}

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  useStore.setState({ showLaunch: false });
});

describe("keeping a folder in the New session window", () => {
  it("says the list is empty rather than showing nothing at all", () => {
    open();
    expect(screen.getByText(/none yet/)).toBeInTheDocument();
  });

  it("writes what you starred into the preferences, not just onto the screen", () => {
    open();

    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), {
      target: { value: "~/projects/mogeung/" },
    });
    fireEvent.click(screen.getByTitle("keep this folder in the list below"));

    // Normalised on the way in — the trailing slash does not reach the file.
    expect(useStore.getState().scoped().favouriteDirs).toEqual(["~/projects/mogeung"]);
    const saved = JSON.parse(localStorage.getItem("mogeung.prefs") ?? "{}");
    expect(saved.scoped.unknown.favouriteDirs).toEqual(["~/projects/mogeung"]);
  });

  it("removes on the ✕ without the row's own click setting the directory", () => {
    open(["~/projects/one", "~/projects/two"]);

    fireEvent.click(screen.getByTitle("stop keeping ~/projects/one"));

    expect(useStore.getState().scoped().favouriteDirs).toEqual(["~/projects/two"]);
    // The field is what the row's handler would have filled. Empty says the
    // click stopped where it was meant to.
    expect(screen.getByPlaceholderText("~/projects/foo")).toHaveValue("");
  });

  it("offers to unkeep a folder that is already kept, rather than keeping it twice", () => {
    open(["~/projects/one"]);

    fireEvent.click(screen.getByText("~/projects/one"));
    fireEvent.click(screen.getByTitle("stop keeping this folder"));

    expect(useStore.getState().scoped().favouriteDirs).toEqual([]);
  });
});

/**
 * Which agent, and what it costs. `R-J51`.
 *
 * The third test is the one worth having: the yolo flag differs per CLI, and a
 * warning that keeps saying `--dangerously-skip-permissions` while starting
 * `qwen` is worse than no warning — it is a sentence about a flag that is not
 * being passed.
 */
describe("choosing which CLI to start", () => {
  const launches = (): unknown[] =>
    (useStore.getState() as unknown as { sent: unknown[] }).sent ?? [];

  beforeEach(() => {
    // A `send` that records rather than reaching a daemon there is none of.
    useStore.setState({
      send: (m: unknown) => useStore.setState({ sent: [...launches(), m] } as never),
    } as never);
  });

  it("starts Claude Code by default, naming the source on the wire", () => {
    open();
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/repo" } });
    fireEvent.click(screen.getByText(/open a claude terminal/));

    expect(launches()).toEqual([
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "claude_code" },
    ]);
  });

  it("starts the CLI you picked, and remembers it for next time", () => {
    open();
    fireEvent.click(screen.getByTitle(/^start qwen/));
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/repo" } });
    fireEvent.click(screen.getByText(/open a qwen terminal/));

    expect(launches()).toEqual([
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "qwen_code" },
    ]);
    // Written through to the preferences, not merely to this render.
    expect(useStore.getState().prefs.launchSource).toBe("qwen_code");
  });

  it("quotes the flag the chosen CLI is actually started with", () => {
    open();
    expect(screen.getByText("--dangerously-skip-permissions")).toBeInTheDocument();

    fireEvent.click(screen.getByTitle(/^start qwen/));

    expect(screen.getByText("--approval-mode yolo")).toBeInTheDocument();
    expect(screen.queryByText("--dangerously-skip-permissions")).toBeNull();
  });

  /** mogeung has no recipe for starting it; a dead control invites "why not". */
  it("does not offer Codex at all", () => {
    open();
    expect(screen.queryByTitle(/^start codex/)).toBeNull();
  });
});
