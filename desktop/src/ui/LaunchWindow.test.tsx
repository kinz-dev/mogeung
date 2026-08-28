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

import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

// The OS picker, which has no OS here. Mocked at the module rather than at the
// plugin, because `chooseFolder` is the seam: it already decides between a
// shell picker and a typed prompt, and this window only has to ask it.
const picked = vi.hoisted(() => ({ value: null as string | null }));
vi.mock("@/lib/tauri", async (orig) => ({
  ...(await orig<typeof import("@/lib/tauri")>()),
  chooseFolder: vi.fn(async () => picked.value),
}));

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
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "claude_code", headless: false },
    ]);
  });

  it("starts the CLI you picked, and remembers it for next time", () => {
    open();
    fireEvent.click(screen.getByTitle(/^start qwen/));
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/repo" } });
    fireEvent.click(screen.getByText(/open a qwen terminal/));

    expect(launches()).toEqual([
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "qwen_code", headless: false },
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

  /**
   * Codex is offered since `R-J72` — and started with flags that are
   * deliberately **not** its CLI's most dangerous ones.
   *
   * `--dangerously-bypass-approvals-and-sandbox` is the exact analogue of the
   * other two rows and it also turns off a sandbox, which neither sibling CLI
   * has to give up. This asserts the quoted line, because the quote is a
   * promise about what the daemon runs: `agent_command` passes exactly these
   * flags, and the two are only kept honest by both being pinned.
   */
  it("offers Codex, and quotes flags that keep its sandbox", () => {
    open();
    fireEvent.click(screen.getByTitle(/^start codex/));

    expect(
      screen.getByText("--ask-for-approval never --sandbox workspace-write"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/dangerously-bypass/)).toBeNull();
    expect(screen.queryByText("--dangerously-skip-permissions")).toBeNull();
  });
});

/**
 * Headless, and the dialog telling the truth about it. `R-J61`.
 *
 * The wire flag is the feature; the texts are the part that can rot. A button
 * reading "open a … terminal" above a launch that opens no window would be
 * `R-J51`'s stale-warning bug in a new coat — a sentence about a thing that
 * is not being done.
 */
describe("starting headless", () => {
  const launches = (): unknown[] =>
    (useStore.getState() as unknown as { sent: unknown[] }).sent ?? [];

  beforeEach(() => {
    useStore.setState({
      send: (m: unknown) => useStore.setState({ sent: [...launches(), m] } as never),
    } as never);
  });

  it("sends headless on the wire, and remembers the choice", () => {
    open();
    fireEvent.click(screen.getByLabelText("headless — no terminal window"));
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/repo" } });
    fireEvent.click(screen.getByText(/start a headless claude/));

    expect(launches()).toEqual([
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "claude_code", headless: true },
    ]);
    // Written through to the preferences, not merely to this render.
    expect(useStore.getState().prefs.launchHeadless).toBe(true);
  });

  it("stops promising a terminal window once none will open", () => {
    open();
    expect(screen.getByText(/in your terminal/)).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("headless — no terminal window"));

    expect(screen.getByText(/under tmux — no terminal window/)).toBeInTheDocument();
    expect(screen.queryByText(/in your terminal/)).toBeNull();
    expect(screen.queryByText(/open a claude terminal/)).toBeNull();
    expect(screen.getByText(/start a headless claude/)).toBeInTheDocument();
  });
});

/**
 * A headless launch must not create a session nobody can see. `R-J74`.
 *
 * Reported 2026-08-26: *"I can see the process in ps command. But that doesn't
 * appear in the mogeung."* Codex asks whether it may work in a directory the
 * first time it sees one, and opens no thread until you answer — so headless
 * gave the prompt no window to appear in, and the session was invisible to the
 * user and to the daemon alike, which reads Codex's own bookkeeping.
 *
 * The daemon refuses this too. These pin the half you read *before* clicking,
 * because a refusal that arrives after the session failed to appear is a worse
 * version of the same sentence.
 */
describe("headless into a directory codex has not been trusted in", () => {
  /** `open()` plus the daemon's answer about which directories codex may use. */
  function openWithTrust(trusted: string[] | undefined) {
    useStore.setState({
      prefs: { ...defaultPrefs(), launchHeadless: true },
      showLaunch: true,
      sent: [],
      health: {
        agents: [
          { source: "codex", present: true, threads: 0, error: null, unknown: [], trusted_dirs: trusted },
        ],
      },
    } as never);
    render(<LaunchWindow />);
  }

  const chooseCodexIn = (dir: string) => {
    fireEvent.click(screen.getByTitle(/^start codex/));
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: dir } });
  };

  it("says so, and stops promising headless, before you click", () => {
    openWithTrust(["/home/kinz"]);
    chooseCodexIn("/home/kinz/projects/mogeung");

    expect(screen.getByText(/has not been trusted in this directory/i)).toBeInTheDocument();
    // The preference is still on, so the button is the thing that must not lie.
    expect(screen.getByRole("button", { name: /open a codex terminal/i })).toBeInTheDocument();
  });

  /** Exact-path, because that is how Codex matches — the reported case was a
   *  *subdirectory* of a directory that had been trusted. */
  it("is satisfied only by the exact directory", () => {
    openWithTrust(["/home/kinz/projects/mogeung"]);
    chooseCodexIn("/home/kinz/projects/mogeung");

    expect(screen.queryByText(/has not been trusted/i)).toBeNull();
    expect(screen.getByRole("button", { name: /start a headless codex/i })).toBeInTheDocument();
  });

  /** A daemon that cannot say warns about nothing rather than guessing; its own
   *  refusal is the backstop. */
  it("stays quiet when the daemon does not send the list", () => {
    openWithTrust(undefined);
    chooseCodexIn("/home/kinz/projects/mogeung");
    expect(screen.queryByText(/has not been trusted/i)).toBeNull();
  });

  /** Claude has no per-directory trust and must not inherit the warning. */
  it("does not warn for a CLI with no such notion", () => {
    openWithTrust(["/somewhere/else"]);
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), {
      target: { value: "/home/kinz/projects/mogeung" },
    });
    expect(screen.queryByText(/has not been trusted/i)).toBeNull();
  });
});

/**
 * Typing an absolute path was the slowest part of this window. `R-J78`.
 *
 * Two properties, and the second is the one that fails quietly: the picked
 * folder has to land in the box *and* a declined picker has to leave what was
 * already there alone — a cancel that blanks the field is worse than no picker,
 * because it destroys work in the act of doing nothing.
 */
describe("browsing for the folder", () => {
  it("puts what you picked into the box", async () => {
    picked.value = "/home/you/projects/foo";
    open();
    await act(async () => {
      fireEvent.click(screen.getByTitle("browse for a folder"));
    });
    await waitFor(() =>
      expect(screen.getByPlaceholderText("~/projects/foo")).toHaveValue("/home/you/projects/foo"),
    );
  });

  it("leaves the box alone when you cancel", async () => {
    picked.value = null;
    open();
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), {
      target: { value: "/half/typed" },
    });
    await act(async () => {
      fireEvent.click(screen.getByTitle("browse for a folder"));
    });
    expect(screen.getByPlaceholderText("~/projects/foo")).toHaveValue("/half/typed");
  });
});
