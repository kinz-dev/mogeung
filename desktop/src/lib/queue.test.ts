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
import { visibleQueue } from "@/lib/queue";
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

function ask(opts: {
  queue: AttentionItem[];
  sessions: Session[];
  scope?: "needs_you" | "live" | "all";
  filter?: string;
  scoped?: Partial<ScopedPrefs>;
}) {
  return visibleQueue({
    queue: opts.queue,
    sessions: Object.fromEntries(opts.sessions.map((s) => [s.id, s])),
    scope: opts.scope ?? "all",
    filter: opts.filter ?? "",
    scoped: { ...emptyScoped(), ...opts.scoped },
  }).map((r) => r.session.id);
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
