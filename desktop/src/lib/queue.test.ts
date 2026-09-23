/**
 * The keyboard walks what the queue is *showing*. `R-J13`.
 *
 * There were two lists. The panel rendered a filtered, re-sorted one; the
 * keyboard walked the raw `queue` straight from the daemon. With a scope on,
 * `j`/`k` and the arrows stepped through sessions that were not on screen, in
 * an order that was not the visible one — reported 2026-08-07 against the
 * `live` filter.
 *
 * These pin the rule rather than the arithmetic: the visible list is one
 * function, and it is the one the keyboard asks.
 */

import { describe, expect, it } from "vitest";
import { groupQueue, hiddenByTmux, queueDetail, visibleQueue, type QueueLine } from "@/lib/queue";
import { emptyScoped, type ScopedPrefs } from "@/store/prefs";
import type { AttentionItem, Session } from "@/wire/types";

const session = (id: string, extra: Partial<Session> = {}): Session =>
  ({
    id,
    title: `session ${id}`,
    name: null,
    last_prompt: null,
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

const item = (id: string, reason: AttentionItem["reason"] = "running"): AttentionItem => ({
  session_id: id,
  reason,
  score: 0,
  detail: "",
});

interface Ask {
  queue: AttentionItem[];
  sessions: Session[];
  scope?: "needs_you" | "live" | "all";
  filter?: string;
  scoped?: Partial<ScopedPrefs>;
  tmuxOnly?: boolean;
}

const view = (opts: Ask) => ({
  queue: opts.queue,
  sessions: Object.fromEntries(opts.sessions.map((s) => [s.id, s])),
  scope: opts.scope ?? "all",
  filter: opts.filter ?? "",
  scoped: { ...emptyScoped(), ...opts.scoped },
  tmuxOnly: opts.tmuxOnly,
});

function ask(opts: Ask) {
  return visibleQueue(view(opts)).map((r) => r.session.id);
}

describe("what the queue is showing", () => {
  /** The reported bug: a dead session between two live ones. */
  it("drops what the live scope hides", () => {
    expect(
      ask({
        queue: [item("a"), item("dead"), item("b")],
        sessions: [session("a"), session("dead", { alive: false }), session("b")],
        scope: "live",
      }),
    ).toEqual(["a", "b"]);
  });

  it("drops what the needs-you scope hides", () => {
    expect(
      ask({
        queue: [item("a", "awaiting_input"), item("b", "running")],
        sessions: [session("a"), session("b")],
        scope: "needs_you",
      }),
    ).toEqual(["a"]);
  });

  it("drops what you hid, and what the text filter excludes", () => {
    expect(
      ask({
        queue: [item("a"), item("b")],
        sessions: [session("a"), session("b")],
        scoped: { hidden: ["b"] },
      }),
    ).toEqual(["a"]);

    expect(
      ask({
        queue: [item("a"), item("b")],
        sessions: [session("a", { title: "the migration" }), session("b", { title: "something else" })],
        filter: "migration",
      }),
    ).toEqual(["a"]);
  });

  /**
   * The half a filter-only fix would have missed. `R-B44` put pins, then
   * colours, then labels above the daemon's rank — so even with nothing
   * filtered out, rank order is *not* visible order.
   */
  it("returns them in the order the panel draws, not the daemon's rank", () => {
    expect(
      ask({
        queue: [item("a"), item("b"), item("c")],
        sessions: [session("a"), session("b"), session("c")],
        scoped: { pinned: ["c"] },
      }),
    ).toEqual(["c", "a", "b"]);
  });

  it("keeps the rank as the tiebreak when nothing is hand-made", () => {
    expect(
      ask({
        queue: [item("b"), item("a"), item("c")],
        sessions: [session("a"), session("b"), session("c")],
      }),
    ).toEqual(["b", "a", "c"]);
  });

  it("skips a queue entry whose session it has never seen", () => {
    expect(ask({ queue: [item("ghost"), item("a")], sessions: [session("a")] })).toEqual(["a"]);
  });

  /**
   * `R-J93`. A session with no pane cannot be attached to, so under the two
   * scopes you work from it is a row you can only read past. The pane, not
   * the process: `tmux_target` is the daemon's one answer to *can this be
   * attached to*.
   */
  describe("sessions with no tmux pane", () => {
    const pane = (id: string, extra: Partial<Session> = {}) =>
      session(id, { tmux_target: "work:0.0", ...extra } as Partial<Session>);

    it("drops them from the needs-you and live scopes", () => {
      for (const scope of ["needs_you", "live"] as const) {
        expect(
          ask({
            queue: [item("a", "awaiting_input"), item("loose", "awaiting_input")],
            sessions: [pane("a"), session("loose")],
            scope,
            tmuxOnly: true,
          }),
        ).toEqual(["a"]);
      }
    });

    it("keeps every one of them under the all scope, which is the way back", () => {
      expect(
        ask({
          queue: [item("a"), item("loose")],
          sessions: [pane("a"), session("loose")],
          scope: "all",
          tmuxOnly: true,
        }),
      ).toEqual(["a", "loose"]);
    });

    it("leaves them alone when the rule is off", () => {
      expect(
        ask({
          queue: [item("a"), item("loose")],
          sessions: [pane("a"), session("loose")],
          scope: "live",
          tmuxOnly: false,
        }),
      ).toEqual(["a", "loose"]);
    });

    /** The panel has to be able to say what it took, or the row just vanishes. */
    it("counts what it held back, and only what the other filters would have shown", () => {
      const opts: Ask = {
        queue: [item("a"), item("loose"), item("dead"), item("mine")],
        sessions: [pane("a"), session("loose"), session("dead", { alive: false }), session("mine")],
        scope: "live",
        tmuxOnly: true,
        scoped: { hidden: ["mine"] },
      };
      // `dead` fails the live scope and `mine` you hid yourself: neither is
      // being held back by this rule, and counting them would offer to show
      // rows that would not appear.
      expect(hiddenByTmux(view(opts))).toBe(1);
      expect(hiddenByTmux(view({ ...opts, scope: "all" }))).toBe(0);
      expect(hiddenByTmux(view({ ...opts, tmuxOnly: false }))).toBe(0);
    });
  });

  it("has nothing to show when everything is filtered out", () => {
    expect(
      ask({
        queue: [item("a")],
        sessions: [session("a", { alive: false })],
        scope: "live",
      }),
    ).toEqual([]);
  });
});

describe("the queue row's clock", () => {
  /**
   * `R-J65`. The daemon used to render the duration into `detail`, which made
   * every waiting row differ from its own previous value on every tick and
   * defeated the gate in front of the queue broadcast. The text is static now
   * and the window appends the clock — so the rendering must still read the
   * way it always did.
   */
  const now = Date.parse("2026-08-26T12:00:00Z");

  it("appends the elapsed time to a row that carries an anchor", () => {
    const it0 = {
      ...item("a", "awaiting_input"),
      detail: "waiting for you",
      since: "2026-08-26T11:54:55Z",
    };
    expect(queueDetail(it0, session("a"), now)).toBe("waiting for you — 5m05s");
  });

  it("counts a snoozed row down to its deadline, not up from an anchor", () => {
    const it0 = { ...item("a"), detail: "snoozed" };
    const s = session("a", { snoozed_until: "2026-08-26T12:04:00Z" } as Partial<Session>);
    expect(queueDetail(it0, s, now)).toBe("snoozed — 4m00s left");
  });

  it("leaves a row with no clock exactly as the daemon wrote it", () => {
    const it0 = { ...item("a", "needs_review"), detail: "22 file(s), +4110 -213 unread" };
    expect(queueDetail(it0, session("a"), now)).toBe("22 file(s), +4110 -213 unread");
  });

  /** A snooze that has lapsed is not a snooze; the anchor takes over again. */
  it("ignores a snooze deadline that has already passed", () => {
    const it0 = {
      ...item("a", "awaiting_input"),
      detail: "waiting for you",
      since: "2026-08-26T11:59:30Z",
    };
    const s = session("a", { snoozed_until: "2026-08-26T11:00:00Z" } as Partial<Session>);
    expect(queueDetail(it0, s, now)).toBe("waiting for you — 30s");
  });
});

/**
 * The list as drawn, with machines in it. `R-I21`, feature 0047.
 *
 * The spec's words: *grouped under their machine — this machine first — and
 * ranked together with local ones by one ranking*.
 */
describe("grouping by machine", () => {
  const remote = (id: string, host: string, extra: Partial<Session> = {}) =>
    session(`@${host}-id:${id}`, { origin: { machine_id: `${host}-id`, host }, ...extra });
  const rowsOf = (sessions: Session[], reasons: Record<string, AttentionItem["reason"]> = {}) =>
    sessions.map((s) => ({ item: item(s.id, reasons[s.id] ?? "running"), session: s }));
  const shape = (lines: QueueLine[]) =>
    lines.map((l) => (l.kind === "header" ? `${"  ".repeat(l.depth)}# ${l.label}` : `${l.session.id}`));

  it("adds no heading while every row is this machine's", () => {
    const lines = groupQueue(rowsOf([session("a"), session("b")]), {
      byRepo: false,
      byMachine: true,
      collapsed: [],
      here: "ubuntu",
    });
    expect(shape(lines)).toEqual(["a", "b"]);
  });

  it("puts this machine first even when a remote session outranks it", () => {
    const rows = rowsOf([remote("w", "mac"), session("a"), remote("x", "mini"), session("b")], {
      "@mac-id:w": "awaiting_input",
    });
    const lines = groupQueue(rows, { byRepo: false, byMachine: true, collapsed: [], here: "ubuntu" });
    expect(shape(lines)).toEqual([
      "# this machine · ubuntu",
      "a",
      "b",
      "# mac",
      "@mac-id:w",
      "# mini",
      "@mini-id:x",
    ]);
    const mac = lines.find((l) => l.kind === "header" && l.label === "mac");
    expect(mac).toMatchObject({ count: 1, needing: 1 });
  });

  it("nests repositories under their machine, keyed apart so one fold does not fold both", () => {
    const rows = rowsOf([session("a"), remote("w", "mac")]);
    const lines = groupQueue(rows, { byRepo: true, byMachine: true, collapsed: [], here: null });
    expect(shape(lines)).toEqual(["# this machine", "  # repo", "a", "# mac", "  # repo", "@mac-id:w"]);
    const keys = lines.flatMap((l) => (l.kind === "header" ? [l.key] : []));
    expect(new Set(keys).size).toBe(keys.length);

    const folded = groupQueue(rows, { byRepo: true, byMachine: true, collapsed: ["machine:mac-id"], here: null });
    expect(shape(folded)).toEqual(["# this machine", "  # repo", "a", "# mac"]);
  });

  it("marks a machine gone quiet as stale", () => {
    const rows = rowsOf([session("a"), remote("w", "mac", { origin: { machine_id: "mac-id", host: "mac", stale: true } })]);
    const lines = groupQueue(rows, { byRepo: false, byMachine: true, collapsed: [], here: null });
    expect(lines.find((l) => l.kind === "header" && l.label === "mac")).toMatchObject({ stale: true });
    expect(lines.find((l) => l.kind === "header" && l.label === "this machine")).not.toHaveProperty("stale");
  });

  it("keeps rank order flat when machines are not grouped — the chip on the row says where", () => {
    const rows = rowsOf([remote("w", "mac"), session("a")]);
    const lines = groupQueue(rows, { byRepo: false, byMachine: false, collapsed: [], here: null });
    expect(shape(lines)).toEqual(["@mac-id:w", "a"]);
  });
});
