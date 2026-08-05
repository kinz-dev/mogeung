/**
 * A note has to still make sense after the thing it was copied from is gone.
 *
 * That is the whole reason this feature exists rather than a bookmark, so the
 * cases below are mostly about what survives the trip: who said it, which turn
 * it was, and — when a conversation is too long to carry whole — an honest
 * statement that something was left behind.
 */

import { describe, expect, it } from "vitest";
import { CONVERSATION_LIMIT, noteFromConversation, noteFromTurn, textOf } from "@/lib/notes";
import type { Session, TranscriptEvent, EventKind } from "@/wire/types";

const session = { id: "abc12345-0000", title: "the auth refactor" } as Session;

function ev(seq: number, kind: EventKind, ts = "2026-08-05T14:30:00Z"): TranscriptEvent {
  return { session_id: "abc12345-0000", seq, ts, kind };
}

describe("one turn as a note", () => {
  it("carries who said it and which turn, because the transcript may not survive", () => {
    const note = noteFromTurn(session, ev(12, { t: "user_prompt", text: "why is this slow?" }));
    expect(note).toContain("# the auth refactor · turn 12");
    expect(note).toContain("**You**");
    expect(note).toContain("> why is this slow?");
  });

  it("names the session by id when the session is already gone", () => {
    const note = noteFromTurn(null, ev(3, { t: "assistant_text", text: "because it forks" }));
    expect(note).toContain("# abc12345 · turn 3");
    expect(note).toContain("**Agent**");
  });

  /**
   * Quoting rather than fencing. Agent output is full of code fences already,
   * and a fence around a fence renders as one enormous code block — the note
   * becomes unreadable exactly when it was worth keeping.
   */
  it("quotes rather than fences, so an answer full of code still reads", () => {
    const note = noteFromTurn(session, ev(1, { t: "assistant_text", text: "run:\n\n```sh\nls -l\n```" }));
    expect(note).toContain("> run:");
    expect(note).toContain("> ```sh");
    expect(note).not.toMatch(/^```/m);
  });

  it("keeps blank lines as blank quote lines rather than breaking the quote", () => {
    const note = noteFromTurn(session, ev(1, { t: "user_prompt", text: "one\n\ntwo" }));
    expect(note).toContain("> one\n>\n> two");
  });
});

describe("what a turn actually carries", () => {
  it("reads the text out of every kind that has one", () => {
    expect(textOf(ev(1, { t: "thinking", text: "hmm" }))).toBe("hmm");
    expect(textOf(ev(1, { t: "tool_use", tool_use_id: "t", name: "Bash", summary: "ls" }))).toBe("ls");
    expect(textOf(ev(1, { t: "tool_result", tool_use_id: "t", is_error: false, preview: "out" }))).toBe("out");
    expect(textOf(ev(1, { t: "notice", level: "warn", message: "careful" }))).toBe("careful");
  });

  /** `init` is metadata. A note opening with model and tool counts buries the
   *  reason it was written. */
  it("carries nothing for the metadata turn", () => {
    expect(textOf(ev(0, { t: "init", model: "opus", cwd: "/repo", tool_count: 42 }))).toBeNull();
  });
});

describe("a whole conversation as a note", () => {
  const many = (n: number) =>
    Array.from({ length: n }, (_, i) => ev(i + 1, { t: "assistant_text", text: `line ${i + 1}` }));

  it("keeps every turn when the conversation is short", () => {
    const note = noteFromConversation(session, many(3));
    expect(note).toContain("3 turn(s)");
    expect(note).toContain("> line 1");
    expect(note).toContain("> line 3");
    expect(note).not.toContain("not copied");
  });

  /**
   * The cap is the honest part. A note that silently holds the last 200 turns
   * of a 900-turn session reads as the whole conversation, and the reader has
   * no way to know otherwise — which is the failure this states out loud.
   */
  it("says what it dropped rather than looking complete", () => {
    const note = noteFromConversation(session, many(CONVERSATION_LIMIT + 40));
    expect(note).toContain(`Earlier 40 turn(s) not copied`);
    expect(note).toContain("tail of the conversation");
    // The tail is what is kept: the last turn is present, the first is not.
    expect(note).toContain(`> line ${CONVERSATION_LIMIT + 40}`);
    expect(note).not.toContain("> line 1\n");
  });

  it("leaves out the turns that carry no text at all", () => {
    const note = noteFromConversation(session, [
      ev(0, { t: "init", model: "opus", cwd: "/repo", tool_count: 1 }),
      ev(1, { t: "user_prompt", text: "hello" }),
    ]);
    expect(note).toContain("1 turn(s)");
    expect(note).not.toContain("Session start");
  });
});
