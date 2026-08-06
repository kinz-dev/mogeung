/**
 * Which way the conversation runs, and what must never run that way.
 *
 * The reversal is a display choice. The trap it sets is that the same array
 * feeds the note and the exported file, and a conversation written down
 * backwards has answers before their questions — so these assert the two stay
 * separate.
 */

import { describe, expect, it } from "vitest";
import { newestIndex, orderedTurns, visibleTurns } from "@/lib/turns";
import type { EventKind, TranscriptEvent } from "@/wire/types";

const ev = (seq: number, kind: EventKind): TranscriptEvent => ({
  session_id: "s",
  seq,
  ts: "2026-08-06T09:00:00Z",
  kind,
});

const conversation = [
  ev(1, { t: "user_prompt", text: "why is this slow?" }),
  ev(2, { t: "thinking", text: "considering" }),
  ev(3, { t: "assistant_text", text: "because it forks" }),
];

describe("which turns are shown", () => {
  it("drops the reasoning blocks when they are not wanted", () => {
    expect(visibleTurns(conversation, false).map((e) => e.seq)).toEqual([1, 3]);
    expect(visibleTurns(conversation, true).map((e) => e.seq)).toEqual([1, 2, 3]);
  });

  it("has nothing to show before the events arrive", () => {
    expect(visibleTurns(undefined, true)).toEqual([]);
  });
});

describe("which way they run", () => {
  it("reads oldest first by default", () => {
    expect(orderedTurns(conversation, false).map((e) => e.seq)).toEqual([1, 2, 3]);
  });

  it("puts the latest turn at the top when asked", () => {
    expect(orderedTurns(conversation, true).map((e) => e.seq)).toEqual([3, 2, 1]);
  });

  /**
   * The trap. `reverse()` mutates, and the array handed here is the one the
   * note and the export read — so an in-place reversal would silently write
   * every copied conversation backwards, and only when the toggle was on.
   */
  it("never reverses the array it was given", () => {
    const visible = visibleTurns(conversation, true);
    orderedTurns(visible, true);
    expect(visible.map((e) => e.seq)).toEqual([1, 2, 3]);
  });
});

describe("where the newest turn is", () => {
  it("is the last row normally and the first when reversed", () => {
    expect(newestIndex(3, false)).toBe(2);
    expect(newestIndex(3, true)).toBe(0);
  });

  /** `-1` so "nothing to scroll to" is distinguishable from "scroll to the
   *  top", which is a real index. */
  it("says there is nowhere to go when the list is empty", () => {
    expect(newestIndex(0, false)).toBe(-1);
    expect(newestIndex(0, true)).toBe(-1);
  });
});
