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
});
