/**
 * One header, and it says which session. `R-B49`.
 *
 * With two Agent panes up, a tab reading `AGENT | AGENT` identifies neither —
 * and the pane used to draw a second header saying the same word underneath it.
 * These pin the two halves of that: the tab tracks the session, and a held pane
 * with nothing left to show says so rather than reading as empty.
 */

import { describe, expect, it, beforeEach } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { PaneScope } from "@/lib/paneScope";
import { PaneActions, PaneCwd, usePaneTitle } from "@/ui/PaneChrome";
import { AgentPane } from "@/panes/AgentPane";
import { useStore } from "@/store";
import { filePaneId } from "@/lib/panes";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import type { Session } from "@/wire/types";

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/tmp/repo",
    repo_root: "/tmp/repo",
    alive: true,
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
    ...extra,
  }) as unknown as Session;

function scopedState(patch: { paneHold?: Record<string, string>; labels?: Record<string, string> }) {
  useStore.setState({
    prefs: { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), ...patch } } },
  });
}

function Title({ id }: { id: string }) {
  const { text, held } = usePaneTitle(id, "Agent");
  return <div data-testid="title">{`${text}${held ? " ⚓" : ""}`}</div>;
}

beforeEach(() => {
  cleanup();
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    sessions: { s1: session("s1"), s2: session("s2") },
  });
});

describe("the Agent tab", () => {
  it("names the session it is showing, not the pane", () => {
    render(<Title id="agent" />);
    expect(screen.getByTestId("title")).not.toHaveTextContent("Agent");
  });

  it("prefers the label you gave the session", () => {
    scopedState({ labels: { s1: "the migration" } });
    render(<Title id="agent" />);
    expect(screen.getByTestId("title")).toHaveTextContent("the migration");
  });

  it("follows the selection, and stops when the pane is held", () => {
    render(<Title id="agent" />);
    act(() => {
      useStore.setState({ selected: "s2" });
    });
    expect(screen.getByTestId("title")).toHaveTextContent("session s2");

    act(() => {
      scopedState({ paneHold: { agent: "s1" } });
      useStore.setState({ selected: "s2" });
    });
    expect(screen.getByTestId("title")).toHaveTextContent("⚓");
    expect(screen.getByTestId("title")).toHaveTextContent("session s1");
  });

  /**
   * Reverting to `Agent` here would read as the pane coming loose, when what
   * actually happened is that the thing it was watching ended.
   */
  it("keeps naming a held session after it has gone", () => {
    scopedState({ paneHold: { "agent:2": "gone-abc123" } });
    render(<Title id="agent:2" />);
    expect(screen.getByTestId("title")).toHaveTextContent("ended");
    expect(screen.getByTestId("title")).toHaveTextContent("⚓");
  });

  /** A pane with no naming rule of its own keeps whatever dockview gave it. */
  it("leaves a pane it has no opinion about alone", () => {
    render(<Title id="somethingelse" />);
    expect(screen.getByTestId("title")).toHaveTextContent("Agent");
  });
});

/**
 * A file pane names its file, and needs nothing to do it. `R-B53`.
 *
 * The Code tab used to look this up in the store — *the focused half of the
 * internal split's active tab* — because one pane showed many files. One pane
 * is one file now, so the id already carries the answer and the tab cannot be
 * wrong about which file it is over.
 */
describe("a file tab", () => {
  it("names the file from its own id, with no store lookup at all", () => {
    useStore.setState({ explorer: {} });
    render(<Title id={filePaneId("s1", "src/main.rs", null)} />);
    expect(screen.getByTestId("title")).toHaveTextContent("main.rs");
  });

  it("keeps naming its own file while another session is selected", () => {
    render(<Title id={filePaneId("s1", "src/main.rs", null)} />);
    act(() => {
      useStore.setState({ selected: "s2" });
    });
    expect(screen.getByTestId("title")).toHaveTextContent("main.rs");
  });

  it("tells a revision apart from the worktree twin of the same path", () => {
    render(<Title id={filePaneId("s1", "src/main.rs", "abc1234")} />);
    expect(screen.getByTestId("title")).toHaveTextContent("main.rs");
  });

  /** A path with a colon in it must not be cut short by the id's separators. */
  it("survives a path that contains a colon", () => {
    render(<Title id={filePaneId("s1", "weird:name.rs", null)} />);
    expect(screen.getByTestId("title")).toHaveTextContent("weird:name.rs");
  });
});

/**
 * The branch a session is on, in its pane header. Asked for 2026-08-07.
 *
 * `PaneActions` is rendered by dockview per group rather than by a pane, so it
 * is exercised directly here — what matters is which session it reads and what
 * it does when there is no branch to name.
 */
describe("the branch on the pane header", () => {
  const actions = (extra: Partial<Session>) => {
    useStore.setState({ selected: "s1", sessions: { s1: session("s1", extra) } });
    return render(
      <PaneActions
        activePanel={{ id: "agent" } as never}
        containerApi={{ getPanel: () => undefined } as never}
        api={{} as never}
        group={{} as never}
        panels={[]}
        isGroupActive
      />,
    );
  };

  it("names the branch the session is working on", () => {
    actions({ git_branch: "feature/ENG-441" });
    expect(screen.getByTitle("on branch feature/ENG-441")).toBeInTheDocument();
    expect(screen.getByText("feature/ENG-441")).toBeInTheDocument();
  });

  /**
   * `git_branch` is null for a directory that is not a repository *and* for a
   * detached HEAD. Naming a branch in either would be a small lie, so the row
   * carries one fewer thing instead.
   */
  it("says nothing at all when there is no branch", () => {
    actions({ git_branch: null });
    expect(screen.queryByTitle(/on branch/)).toBeNull();
  });

  /** A held pane names *its* branch, not the selected session's. */
  it("follows the pane's own session when the pane is held", () => {
    useStore.setState({
      prefs: { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), paneHold: { agent: "s2" } } } },
      selected: "s1",
      sessions: {
        s1: session("s1", { git_branch: "main" }),
        s2: session("s2", { git_branch: "the-held-one" }),
      },
    });
    render(
      <PaneActions
        activePanel={{ id: "agent" } as never}
        containerApi={{ getPanel: () => undefined } as never}
        api={{} as never}
        group={{} as never}
        panels={[]}
        isGroupActive
      />,
    );
    expect(screen.getByText("the-held-one")).toBeInTheDocument();
    expect(screen.queryByText("main")).toBeNull();
  });
});

/**
 * The directory a session was started in, on the left of the same header.
 * Asked for 2026-08-07.
 */
describe("the folder path on the pane header", () => {
  const cwdHeader = (paneId = "agent") =>
    render(
      <PaneCwd
        activePanel={{ id: paneId } as never}
        containerApi={{ getPanel: () => undefined } as never}
        api={{} as never}
        group={{} as never}
        panels={[]}
        isGroupActive
      />,
    );

  it("names the directory the session was started in", () => {
    useStore.setState({ selected: "s1", sessions: { s1: session("s1", { cwd: "/home/kinz/projects/mogeung" }) } });
    cwdHeader();
    expect(screen.getByTitle("started in /home/kinz/projects/mogeung")).toBeInTheDocument();
  });

  /**
   * `cwd`, not `repo_root`. They differ exactly when a session was started in a
   * subdirectory, which is the case where being told is worth the width.
   */
  it("shows where claude was run, not the repository it found", () => {
    useStore.setState({
      selected: "s1",
      sessions: { s1: session("s1", { cwd: "/repo/crates/daemon", repo_root: "/repo" }) },
    });
    cwdHeader();
    expect(screen.getByTitle("started in /repo/crates/daemon")).toBeInTheDocument();
  });

  /** A held pane names *its* session's directory, as its branch and tab do. */
  it("follows the pane's own session when the pane is held", () => {
    useStore.setState({
      prefs: { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), paneHold: { agent: "s2" } } } },
      selected: "s1",
      sessions: {
        s1: session("s1", { cwd: "/one" }),
        s2: session("s2", { cwd: "/the/held/one" }),
      },
    });
    cwdHeader();
    expect(screen.getByTitle("started in /the/held/one")).toBeInTheDocument();
  });

  /** Every group gets this container; only an Agent pane has a directory. */
  it("says nothing on a pane that is not an agent", () => {
    cwdHeader("changes");
    expect(screen.queryByTitle(/started in/)).toBeNull();
  });
});

describe("a held pane whose session ended", () => {
  it("says so, rather than asking you to select a session you already selected", () => {
    scopedState({ paneHold: { "agent:2": "gone-abc123" } });
    render(
      <PaneScope id="agent:2">
        <AgentPane />
      </PaneScope>,
    );
    expect(screen.getByText(/that session has ended/i)).toBeInTheDocument();
    expect(screen.queryByText(/select a session/i)).toBeNull();
  });

  it("still asks an unheld empty pane to select one", () => {
    useStore.setState({ selected: null });
    render(
      <PaneScope id="agent">
        <AgentPane />
      </PaneScope>,
    );
    expect(screen.getByText(/select a session/i)).toBeInTheDocument();
  });
});

/**
 * The pane drew its own `PaneHeader` until `R-B49` — 28px of uppercase `AGENT`
 * directly under dockview's 30px uppercase `AGENT`. The merge is only real if
 * the second one is actually gone, and a stray re-add would be invisible in a
 * screenshot review of a single pane.
 */
describe("the merged header", () => {
  it("leaves the pane drawing no header of its own", () => {
    render(
      <PaneScope id="agent">
        <AgentPane />
      </PaneScope>,
    );
    expect(screen.queryByText("Agent")).toBeNull();
    expect(screen.queryByText(/raise its window/i)).toBeNull();
  });
});
