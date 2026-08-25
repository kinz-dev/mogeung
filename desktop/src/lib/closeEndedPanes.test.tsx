/**
 * When the agent is gone, its pane goes with it — anchored or not.
 *
 * Asked for directly: a held pane whose session ended used to sit there saying
 * *"that session has ended — drop the anchor to follow the queue again"*, and
 * clearing it took two deliberate gestures for a thing you did not choose. The
 * anchor keeps a pane pointed at one session; it was never meant to keep it
 * pointed at a session that no longer exists.
 *
 * The two tests that matter are the ones about restraint: `/clear` ends a
 * session too, and a *finished* session is something you open on purpose.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { closePanesFor, setDock } from "@/lib/panes";
import { onSessionsEnded, useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { Session } from "@/wire/types";

const session = (id: string, patch: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/repo",
    repo_root: "/repo",
    pid: 4242,
    alive: true,
    source: "claude_code",
    started_at: "2026-08-25T10:00:00.000Z",
    last_event_at: "2026-08-25T10:05:00.000Z",
    touched_files: [],
    ...patch,
  }) as Session;

/** A dock whose panels record their own closure. */
function fakeDock(existing: string[]) {
  const closed: string[] = [];
  const api = {
    getPanel: (id: string) =>
      existing.includes(id) ? { api: { close: () => closed.push(id), setActive: vi.fn() } } : undefined,
    addPanel: vi.fn(),
  } as unknown as DockviewApi;
  return { api, closed };
}

function setup(opts: { panes: string[]; hold: Record<string, string>; sessions: Session[] }) {
  const dock = fakeDock(opts.panes);
  setDock(dock.api);
  const sessions: Record<string, Session> = {};
  for (const s of opts.sessions) sessions[s.id] = s;
  useStore.setState({ sessions, selected: null, prefs: { ...defaultPrefs() } } as never);
  useStore.getState().setScoped({ paneHold: { ...opts.hold } });
  return dock;
}

describe("closing a pane whose session ended", () => {
  beforeEach(() => {
    localStorage.clear();
    onSessionsEnded(null);
    setDock(null as unknown as DockviewApi);
    useStore.setState({ prefs: { ...useStore.getState().prefs, scoped: {} } } as never);
  });

  it("closes an anchored pane, and lets go of the anchor with it", () => {
    const dock = setup({
      panes: ["agent", "agent:2"],
      hold: { "agent:2": "gone" },
      sessions: [session("gone", { alive: false })],
    });

    closePanesFor(["gone"]);

    expect(dock.closed).toEqual(["agent:2"]);
    // The hold has to go too, or opening into that slot again arrives already
    // anchored to a session that ended last week.
    expect(useStore.getState().scoped().paneHold["agent:2"]).toBeUndefined();
  });

  /** The anchor is the thing that used to save it. That is the whole ask. */
  it("does not spare a pane just because it is anchored", () => {
    const dock = setup({
      panes: ["agent"],
      hold: { agent: "gone" },
      sessions: [session("gone", { alive: false })],
    });

    closePanesFor(["gone"]);

    expect(dock.closed).toEqual(["agent"]);
  });

  /**
   * An unheld pane is not bound to anything — it shows the selection and has
   * already moved on. Closing it would take away a view that was never pointed
   * at this session.
   */
  it("leaves an unanchored pane alone", () => {
    const dock = setup({
      panes: ["agent", "agent:2"],
      hold: {},
      sessions: [session("gone", { alive: false })],
    });

    closePanesFor(["gone"]);

    expect(dock.closed).toEqual([]);
  });

  it("leaves panes anchored to other sessions alone", () => {
    const dock = setup({
      panes: ["agent", "agent:2"],
      hold: { agent: "alive-one", "agent:2": "gone" },
      sessions: [session("alive-one"), session("gone", { alive: false })],
    });

    closePanesFor(["gone"]);

    expect(dock.closed).toEqual(["agent:2"]);
  });

  /** A file pane is a document, not an agent. It outlives the session. */
  it("does not close a file pane bound to the session", () => {
    const filePane = "file:gone:src/main.rs";
    const dock = setup({
      panes: ["agent", filePane],
      hold: { [filePane]: "gone" },
      sessions: [session("gone", { alive: false })],
    });

    closePanesFor(["gone"]);

    expect(dock.closed).toEqual([]);
  });

  it("does nothing when there is no dock yet", () => {
    useStore.getState().setScoped({ paneHold: { agent: "gone" } });
    expect(() => closePanesFor(["gone"])).not.toThrow();
  });
});

describe("noticing that a session ended", () => {
  beforeEach(() => {
    localStorage.clear();
    onSessionsEnded(null);
    useStore.setState({
      sessions: {},
      selected: null,
      prefs: { ...useStore.getState().prefs, scoped: {} },
    } as never);
  });

  /** The transition, which is the only thing that should fire this. */
  it("reports a session that was alive and now is not", () => {
    const seen: string[][] = [];
    onSessionsEnded((ids) => seen.push([...ids]));
    const st = useStore.getState();

    st.ingest({ ev: "session_updated", session: session("a") } as never);
    expect(seen).toEqual([]);

    st.ingest({ ev: "session_updated", session: session("a", { alive: false }) } as never);
    expect(seen).toEqual([["a"]]);
  });

  /**
   * The guard that makes this safe to wire to a close. Sessions load from the
   * daemon's store with `alive: false` and have their liveness re-derived on
   * the first scan — so firing on the *state* rather than the transition would
   * close a pane a moment before being told its agent is running.
   */
  it("says nothing about a session that was never seen alive", () => {
    const seen: string[][] = [];
    onSessionsEnded((ids) => seen.push([...ids]));

    useStore.getState().ingest({
      ev: "session_updated",
      session: session("cold", { alive: false }),
    } as never);

    expect(seen).toEqual([]);
  });

  /** Once. A window that re-reported it on every tick would re-close a pane
   *  you had deliberately opened again. */
  it("reports an ending only once", () => {
    const seen: string[][] = [];
    onSessionsEnded((ids) => seen.push([...ids]));
    const st = useStore.getState();

    st.ingest({ ev: "session_updated", session: session("a") } as never);
    st.ingest({ ev: "session_updated", session: session("a", { alive: false }) } as never);
    st.ingest({ ev: "session_updated", session: session("a", { alive: false }) } as never);

    expect(seen).toEqual([["a"]]);
  });

  /**
   * **The one that would have broken `/clear`.** A cleared session ends and its
   * successor starts on the same pid; succession moves the held pane forward
   * onto the heir. If the ending were reported before that ran — or if the
   * pane were closed on the raw `alive` flag — the pane succession is about to
   * re-aim would be gone instead, which is `R-J15` with the terminal missing
   * rather than blank.
   */
  it("hands the pane to the successor instead of closing it", () => {
    const dock = fakeDock(["agent"]);
    setDock(dock.api);
    onSessionsEnded(closePanesFor);
    const st = useStore.getState();
    st.setScoped({ paneHold: { agent: "old" } });

    st.ingest({ ev: "session_updated", session: session("old", { pid: 900 }) } as never);
    // `/clear`: same pid, new id, and the old one goes quiet.
    st.ingest({
      ev: "snapshot",
      sessions: [
        session("old", { pid: 900, alive: false }),
        session("new", { pid: 900, started_at: "2026-08-25T10:06:00.000Z" }),
      ],
      queue: [],
    } as never);

    // The ending of `old` is real and is reported; what matters is that
    // succession has already re-aimed the pane by then, so there is nothing
    // still anchored to `old` for the close to find.
    expect(useStore.getState().scoped().paneHold["agent"]).toBe("new");
    expect(dock.closed).toEqual([]);
  });

  /** Pruned outright, with no later message to notice it in. */
  it("reports a session removed after it had been running", () => {
    const seen: string[][] = [];
    onSessionsEnded((ids) => seen.push([...ids]));
    const st = useStore.getState();

    st.ingest({ ev: "session_updated", session: session("a") } as never);
    st.ingest({ ev: "session_removed", session_id: "a" } as never);

    expect(seen).toEqual([["a"]]);
  });
});
