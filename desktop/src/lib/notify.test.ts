/**
 * When do we speak?
 *
 * The whole value of a notifier is in what it does *not* say. These are the
 * cases `notify.rs` was built around, asserted again here because the two
 * implementations must agree — a client that re-announced a standing queue
 * would teach you to dismiss banners, and then the one that mattered goes with
 * the rest.
 */

import { describe, expect, it } from "vitest";
import { diffQueue } from "@/lib/notify";
import type { AttentionItem, AttentionReason, SessionId } from "@/wire/types";

const item = (id: string, reason: AttentionReason, detail = "…"): AttentionItem => ({
  session_id: id,
  reason,
  score: 1,
  detail,
});

const label = (id: SessionId) => `label:${id}`;
const empty = new Map<SessionId, AttentionReason>();

describe("what the queue is worth announcing", () => {
  it("speaks on the transition into needing you", () => {
    const { out } = diffQueue(empty, [item("a", "awaiting_permission", "Bash: rm -rf")], label);
    expect(out).toEqual([
      { sessionId: "a", title: "Needs your approval", body: "label:a — Bash: rm -rf" },
    ]);
  });

  it("says nothing about a state that is merely continuing", () => {
    const first = diffQueue(empty, [item("a", "awaiting_input")], label);
    const second = diffQueue(first.next, [item("a", "awaiting_input")], label);
    expect(second.out).toEqual([]);
  });

  it("speaks again when the same session needs you for a different reason", () => {
    const first = diffQueue(empty, [item("a", "awaiting_input")], label);
    const second = diffQueue(first.next, [item("a", "failed")], label);
    expect(second.out.map((n) => n.title)).toEqual(["Session failed"]);
  });

  it("ignores a session that is merely running", () => {
    const { out } = diffQueue(empty, [item("a", "running")], label);
    expect(out).toEqual([]);
  });

  /**
   * Dropping out of the queue is what makes a session announceable again. The
   * seen-state is rebuilt from the queue rather than merged into, which is the
   * one line that gives this behaviour.
   */
  it("forgets a session that left the queue, so its next call is heard", () => {
    const first = diffQueue(empty, [item("a", "awaiting_input")], label);
    const gone = diffQueue(first.next, [], label);
    const again = diffQueue(gone.next, [item("a", "awaiting_input")], label);
    expect(again.out).toHaveLength(1);
  });

  /**
   * A limit is not something being interrupted can help with, and the daemon
   * has no line for it either. It still counts as needing you in the queue.
   */
  it("has nothing to say about a rate limit", () => {
    const { out, next } = diffQueue(empty, [item("a", "rate_limited")], label);
    expect(out).toEqual([]);
    // Still remembered, or it would be re-examined on every snapshot.
    expect(next.get("a")).toBe("rate_limited");
  });
});
