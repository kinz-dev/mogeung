/**
 * What you are *looking at* survives `/clear`, not just what you named. `R-J15`.
 *
 * Reported 2026-08-07: "when I run /clear the ATTENTION label disappears and
 * the tmux session goes blank — I have to click the session list again". Two
 * halves of one bug. The label was stranded by an ordering that could not
 * order a chain (see `prefs.test.ts`); the blank pane is this file's half —
 * `selected` and every held pane still pointed at an id whose tmux pane died
 * with the old conversation, and nothing moved them.
 *
 * The rule these pin is *once*. A hop that repeats is a window that will not
 * let you read a finished session, which is a worse bug than the one being
 * fixed here — so each predecessor donates its hop a single time.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";
import { emptyScoped } from "@/store/prefs";
import type { Session } from "@/wire/types";

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/repo",
    repo_root: "/repo",
    pid: 26514,
    alive: true,
    started_at: "2026-08-05T17:53:00.000Z",
    last_event_at: "2026-08-07T14:32:00.000Z",
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    tmux_target: "mogeung:1.0",
    ...extra,
  }) as unknown as Session;

/** The `/clear` itself: the old id stops, a fresh one takes the pid over. */
function clearInto(dead: Session, live: Session): void {
  const { ingest } = useStore.getState();
  ingest({ ev: "session_updated", session: { ...dead, alive: false, tmux_target: null } });
  ingest({ ev: "session_updated", session: live });
}

/** Every test uses its own ids: the "hopped once" record outlives a test. */
describe("the view follows a session through /clear", () => {
  beforeEach(() => {
    localStorage.clear();
    useStore.setState({ sessions: {}, selected: null, prefs: { ...useStore.getState().prefs, scoped: {} } });
  });

  it("moves the selection onto the successor", () => {
    const { ingest, select } = useStore.getState();
    ingest({ ev: "snapshot", sessions: [session("a-old")], queue: [] });
    select("a-old");

    clearInto(session("a-old"), session("a-new", { last_event_at: "2026-08-07T14:35:00.000Z" }));

    expect(useStore.getState().selected).toBe("a-new");
  });

  it("takes held panes with it, so a pinned Agent pane stays attached", () => {
    const { ingest, setScoped } = useStore.getState();
    ingest({ ev: "snapshot", sessions: [session("b-old")], queue: [] });
    setScoped({ paneHold: { agent: "b-old", "code-2": "elsewhere" } });

    clearInto(session("b-old"), session("b-new"));

    expect(useStore.getState().scoped().paneHold).toEqual({
      agent: "b-new",
      "code-2": "elsewhere",
    });
  });

  it("leaves you on a finished session you went back to read", () => {
    const { ingest, select } = useStore.getState();
    ingest({ ev: "snapshot", sessions: [session("c-old")], queue: [] });
    select("c-old");
    clearInto(session("c-old"), session("c-new"));
    expect(useStore.getState().selected).toBe("c-new");

    // Now go back to the ended conversation on purpose. Every tick from the
    // daemon still sees a dead session and its live heir — the hop must not
    // fire a second time and drag the window forwards again.
    select("c-old");
    useStore.getState().ingest({ ev: "session_updated", session: session("c-new") });

    expect(useStore.getState().selected).toBe("c-old");
  });

  it("moves nothing while both are alive", () => {
    const { ingest, select } = useStore.getState();
    ingest({ ev: "snapshot", sessions: [session("d-one"), session("d-two")], queue: [] });
    select("d-one");
    ingest({ ev: "session_updated", session: session("d-two") });

    expect(useStore.getState().selected).toBe("d-one");
  });

  it("does not follow a reused pid into another directory", () => {
    const { ingest, select } = useStore.getState();
    ingest({ ev: "snapshot", sessions: [session("e-old")], queue: [] });
    select("e-old");

    clearInto(session("e-old"), session("e-new", { cwd: "/somewhere-else" }));

    expect(useStore.getState().selected).toBe("e-old");
  });

  it("carries the label across in the same pass, so the row keeps its name", () => {
    const { ingest, select, setScoped } = useStore.getState();
    ingest({ ev: "snapshot", sessions: [session("f-old")], queue: [] });
    setScoped({ ...emptyScoped(), labels: { "f-old": "api-work" } });
    select("f-old");

    clearInto(session("f-old"), session("f-new"));

    const st = useStore.getState();
    expect(st.selected).toBe("f-new");
    expect(st.scoped().labels).toEqual({ "f-new": "api-work" });
  });
});
