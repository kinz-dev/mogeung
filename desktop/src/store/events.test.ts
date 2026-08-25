/**
 * The events reducer, pinned where its cost used to live.
 *
 * The original folded every message with a linear scan per event and then
 * re-sorted **every** session's array — so selecting a long session froze the
 * window quadratically, and each live tick changed the identity of arrays the
 * message never touched, re-rendering panes watching other sessions. These
 * tests pin the behaviours that fix depends on: order kept without a global
 * re-sort, replacement by `seq`, and identity preserved for bystanders.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";
import type { TranscriptEvent } from "@/wire/types";

const ev = (session: string, seq: number, text = `t${seq}`): TranscriptEvent => ({
  session_id: session,
  seq,
  ts: new Date(2026, 0, 1, 0, 0, seq).toISOString(),
  kind: { t: "assistant_text", text },
});

const ingest = (events: TranscriptEvent[]) =>
  useStore.getState().ingest({ ev: "events", events });

const seqs = (session: string) =>
  (useStore.getState().events[session] ?? []).map((e) => e.seq);

describe("the events reducer", () => {
  beforeEach(() => useStore.setState({ events: {} }));

  it("appends a live tail in order without disturbing it", () => {
    ingest([ev("a", 1), ev("a", 2)]);
    ingest([ev("a", 3)]);
    expect(seqs("a")).toEqual([1, 2, 3]);
  });

  it("files replayed history into place", () => {
    ingest([ev("a", 5), ev("a", 9)]);
    ingest([ev("a", 7), ev("a", 1)]);
    expect(seqs("a")).toEqual([1, 5, 7, 9]);
  });

  it("treats a re-sent seq as an update, not a duplicate", () => {
    ingest([ev("a", 1, "first"), ev("a", 2)]);
    ingest([ev("a", 1, "revised")]);
    expect(seqs("a")).toEqual([1, 2]);
    const first = useStore.getState().events.a[0];
    expect(first.kind.t === "assistant_text" && first.kind.text).toBe("revised");
  });

  it("leaves untouched sessions' arrays identical, so their panes stay quiet", () => {
    ingest([ev("a", 1), ev("b", 1)]);
    const before = useStore.getState().events.b;
    ingest([ev("a", 2)]);
    expect(useStore.getState().events.b).toBe(before);
    expect(useStore.getState().events.a).not.toBe(before);
  });

  it("keeps a mixed message per-session sorted", () => {
    ingest([ev("a", 2), ev("b", 4), ev("a", 1), ev("b", 3)]);
    expect(seqs("a")).toEqual([1, 2]);
    expect(seqs("b")).toEqual([3, 4]);
  });

  /**
   * The cap (`EVENTS_CAP`, 5000): every session's live tail lands in this
   * store whether or not the window ever looks at it, and before the cap a
   * long-lived window held 52k events / 22 MB of JSON with no way down. The
   * newest events survive — the end every pane reads from — and bystander
   * sessions keep their identity, so the cap costs nothing when it does not
   * bind.
   */
  it("caps a session's events at 5000, keeping the newest", () => {
    const batch = (from: number, n: number) =>
      Array.from({ length: n }, (_, i) => ev("a", from + i));
    ingest(batch(1, 4000));
    ingest([ev("b", 1)]);
    const bystander = useStore.getState().events.b;
    ingest(batch(4001, 2000));

    const held = seqs("a");
    expect(held.length).toBe(5000);
    expect(held[0]).toBe(1001);
    expect(held[held.length - 1]).toBe(6000);
    // A session the message did not push past the cap is untouched.
    expect(useStore.getState().events.b).toBe(bystander);
  });
});
