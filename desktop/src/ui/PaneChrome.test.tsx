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
import { usePaneTitle } from "@/ui/PaneChrome";
import { AgentPane } from "@/panes/AgentPane";
import { useStore } from "@/store";
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

  /** The Code pane is not bindable in a way worth naming — it says what it is. */
  it("leaves a pane that is not the Agent alone", () => {
    render(<Title id="code" />);
    expect(screen.getByTestId("title")).toHaveTextContent("Agent");
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
