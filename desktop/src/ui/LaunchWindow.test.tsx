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
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LaunchWindow } from "@/ui/LaunchWindow";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { PeerState } from "@/wire/types";

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

  /** `R-I25`. opencode is offered, and its line is `agent_command`'s: `--auto`. */
  it("offers opencode, started with its own --auto", () => {
    open();
    fireEvent.click(screen.getByTitle(/^start opencode/));
    expect(screen.getByText("--auto")).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/repo" } });
    fireEvent.click(screen.getByText(/open an opencode terminal/));
    expect(launches()).toEqual([
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "opencode", headless: false },
    ]);
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
 * Typing an absolute path was the slowest part of this window (`R-J78`), and
 * the picker that fixed it showed the wrong disk (`R-J142`).
 *
 * Asked 2026-10-16: with a followed machine chosen, the browse button still
 * opened the OS picker on this computer. The browser is now drawn here and
 * asked of the daemon whose disk it is, so these pin the wire as much as the
 * screen: the question names the chosen machine, an answer for another
 * machine or an older question is not shown, stepping in asks again, and a
 * daemon too old to know the verb is said to be — not left spinning.
 */
describe("browsing for the folder", () => {
  const sent = (): unknown[] => (useStore.getState() as unknown as { sent: unknown[] }).sent ?? [];
  const peer: PeerState = {
    id: "p1",
    name: "devbox",
    url: "ws://devbox:7717/ws",
    state: "up",
    machine_id: "m2",
    host: "devbox",
    version: "0.1.0",
    sessions: 0,
    allow_writes: true,
    detail: null,
    since_ms: 0,
  };
  const answer = (over: Record<string, unknown>) =>
    act(() =>
      useStore.getState().ingest({ ev: "folders", asked: "~", path: "/home/you", parent: "/home", dirs: [], ...over } as never),
    );

  beforeEach(() => {
    useStore.setState({
      browse: null,
      send: (m: unknown) => useStore.setState({ sent: [...sent(), m] } as never),
    } as never);
  });
  afterEach(() => useStore.setState({ peers: null }));

  it("asks the watched daemon, at what is typed, and puts the folder you use into the box", () => {
    open();
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "~/proj" } });
    fireEvent.click(screen.getByTitle("browse for a folder on this machine"));
    expect(sent()).toEqual([{ cmd: "browse_folders", path: "~/proj" }]);

    answer({ asked: "~/proj", path: "/home/you", dirs: ["projects", ".config", "notes"] });
    // Hidden folders wait for the checkbox.
    expect(screen.queryByText(".config")).toBeNull();
    fireEvent.click(screen.getByText("projects"));
    expect(sent()[1]).toEqual({ cmd: "browse_folders", path: "/home/you/projects" });

    answer({ asked: "/home/you/projects", path: "/home/you/projects", parent: "/home/you", dirs: ["mogeung"] });
    fireEvent.click(screen.getByText("use this folder"));
    expect(screen.getByPlaceholderText("~/projects/foo")).toHaveValue("/home/you/projects");
    expect(screen.queryByTestId("folder-browser")).toBeNull();
  });

  it("asks the followed machine you chose, and shows only that machine's answer", () => {
    useStore.setState({ peers: [peer] });
    open();
    fireEvent.click(screen.getByText("devbox"));
    fireEvent.click(screen.getByTitle("browse for a folder on devbox"));
    expect(sent()).toEqual([{ cmd: "browse_folders", path: "~", machine: "m2" }]);

    // This machine's disk, late: the wrong picker's answer.
    answer({ dirs: ["local-only"] });
    expect(screen.queryByText("local-only")).toBeNull();
    answer({ machine: "m2", dirs: ["remote-repo"] });
    expect(screen.getByText("remote-repo")).toBeInTheDocument();
  });

  it("drops an answer to a folder you have already left", () => {
    open();
    fireEvent.click(screen.getByTitle("browse for a folder on this machine"));
    answer({ dirs: ["a"] });
    fireEvent.click(screen.getByText("a"));
    answer({ asked: "~", dirs: ["stale"] });
    expect(screen.queryByText("stale")).toBeNull();
  });

  it("says a daemon too old to browse is too old, rather than waiting on it", () => {
    useStore.setState({ peers: [peer] });
    open();
    fireEvent.click(screen.getByText("devbox"));
    fireEvent.click(screen.getByTitle("browse for a folder on devbox"));
    act(() =>
      useStore.getState().ingest({
        ev: "error",
        message: "bad command: unknown variant `browse_folders`, expected one of `subscribe`, …",
      } as never),
    );
    expect(screen.getByText(/devbox's mogeung is older than this window/)).toBeInTheDocument();
  });

  it("leaves the box alone when you close the browser without choosing", () => {
    open();
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/half/typed" } });
    fireEvent.click(screen.getByTitle("browse for a folder on this machine"));
    answer({ asked: "/half/typed", path: "/half", dirs: ["x"] });
    fireEvent.click(screen.getByTitle("close the folder browser"));
    expect(screen.getByPlaceholderText("~/projects/foo")).toHaveValue("/half/typed");
    expect(useStore.getState().browse).toBeNull();
  });
});

/**
 * Starting on a followed machine. `R-J135`.
 *
 * The daemon forwards a launch that names a machine and forces it headless
 * there; this pins the half the window owns — the picker appears only with a
 * follow that is up, a follow with changes off is grey and says where to turn
 * them on, the wire carries the machine and `headless: true` whatever the
 * checkbox said, and the recent repos are that machine's.
 */
describe("starting on a followed machine", () => {
  const launches = (): unknown[] => (useStore.getState() as unknown as { sent: unknown[] }).sent ?? [];
  const peer = (over: Partial<PeerState> = {}): PeerState => ({
    id: "p1",
    name: "devbox",
    url: "ws://devbox:7717/ws",
    state: "up",
    machine_id: "m2",
    host: "devbox",
    version: "0.1.0",
    sessions: 2,
    allow_writes: true,
    detail: null,
    since_ms: 0,
    ...over,
  });

  beforeEach(() => {
    useStore.setState({
      send: (m: unknown) => useStore.setState({ sent: [...launches(), m] } as never),
    } as never);
  });
  afterEach(() => useStore.setState({ peers: null, sessions: {}, savedConnections: [] }));

  // `R-J140`, asked 2026-10-14 after `R-J139`: the picker said the host the
  // machine announces, not the name its saved connection was given.
  it("names a followed machine as you named its connection, the host only in the title", () => {
    useStore.setState({
      peers: [peer({ name: "ws://devbox:7717/ws", host: "Kinz-Mac-mini.local" })],
      savedConnections: [{ id: "p1", name: "mac-m4p", url: "ws://devbox:7717/ws" }],
    });
    open();
    expect(screen.queryByText("Kinz-Mac-mini.local")).toBeNull();
    const choice = screen.getByText("mac-m4p").closest("button")!;
    expect(choice.title).toContain("start on mac-m4p (Kinz-Mac-mini.local)");
    fireEvent.click(choice);
    expect(screen.getByText(/it starts on mac-m4p and joins the queue/)).toBeInTheDocument();
  });

  it("offers no machine picker without a follow that is up", () => {
    useStore.setState({ peers: [peer({ state: "down" })] });
    open();
    expect(screen.queryByText("this machine")).toBeNull();
    expect(screen.queryByText("devbox")).toBeNull();
  });

  it("starts there, headless whatever the checkbox said, and says so everywhere it speaks", () => {
    useStore.setState({ peers: [peer()] });
    open();
    expect(screen.getByText("this machine").closest("button")!.getAttribute("aria-selected")).toBe("true");

    fireEvent.click(screen.getByText("devbox"));

    const headless = screen.getByLabelText("headless — no terminal window") as HTMLInputElement;
    expect(headless.checked).toBe(true);
    expect(headless.disabled).toBe(true);
    expect(screen.getByText(/under tmux on devbox — no terminal window/)).toBeInTheDocument();
    expect(screen.getByText(/it starts on devbox and joins the queue/)).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/repo" } });
    fireEvent.click(screen.getByText(/start a headless claude on devbox/));

    expect(launches()).toEqual([
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "claude_code", headless: true, machine: "m2" },
    ]);
  });

  it("greys a follow with changes off, says where to turn them on, and starts here", () => {
    useStore.setState({ peers: [peer({ allow_writes: false })] });
    open();
    const tab = screen.getByText("devbox").closest("button")!;
    expect(tab).toBeDisabled();
    expect(tab.title).toMatch(/allow changes/);

    fireEvent.click(tab);
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/repo" } });
    fireEvent.click(screen.getByText(/open a claude terminal/));

    expect(launches()).toEqual([
      { cmd: "launch_terminal", dir: "/repo", worktree: false, source: "claude_code", headless: false },
    ]);
  });

  it("keeps a favourites list per machine, and the star files a path under the machine it was typed for", () => {
    useStore.setState({ peers: [peer()] });
    // After `open`, which resets the preferences to their defaults.
    open(["/here/kept"]);
    act(() => useStore.getState().setScoped({ favouriteDirs: ["/there/kept"] }, "m2"));
    expect(screen.getByText("/here/kept")).toBeInTheDocument();
    expect(screen.queryByText("/there/kept")).toBeNull();

    fireEvent.click(screen.getByText("devbox"));
    expect(screen.getByText("/there/kept")).toBeInTheDocument();
    expect(screen.queryByText("/here/kept")).toBeNull();

    // Starred while devbox is chosen: devbox's list grows, this machine's does not.
    fireEvent.change(screen.getByPlaceholderText("~/projects/foo"), { target: { value: "/there/new" } });
    fireEvent.click(screen.getByTitle("keep this folder in the list below"));
    expect(useStore.getState().scoped("m2").favouriteDirs).toEqual(["/there/kept", "/there/new"]);
    expect(useStore.getState().scoped().favouriteDirs).toEqual(["/here/kept"]);

    fireEvent.click(screen.getByText("this machine"));
    expect(screen.getByText("/here/kept")).toBeInTheDocument();
    expect(screen.queryByText("/there/new")).toBeNull();
  });

  it("lists the repos mirrored from that machine, not this one's", () => {
    const session = (id: string, repo: string, machine: string | null) =>
      ({ id, repo_root: repo, origin: machine ? { machine_id: machine, host: "devbox" } : null }) as never;
    useStore.setState({
      peers: [peer()],
      sessions: { a: session("a", "/here/repo", null), b: session("@m2:b", "/there/repo", "m2") },
    });
    open();
    expect(screen.getByText("/here/repo")).toBeInTheDocument();
    expect(screen.queryByText("/there/repo")).toBeNull();

    fireEvent.click(screen.getByText("devbox"));

    expect(screen.getByText("/there/repo")).toBeInTheDocument();
    expect(screen.queryByText("/here/repo")).toBeNull();
  });
});
