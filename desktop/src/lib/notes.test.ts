/**
 * A note has to still make sense after the thing it was copied from is gone.
 *
 * That is the whole reason this feature exists rather than a bookmark, so the
 * cases below are mostly about what survives the trip: who said it, which turn
 * it was, and — when a conversation is too long to carry whole — an honest
 * statement that something was left behind.
 */

import { describe, expect, it } from "vitest";
import {
  CONVERSATION_LIMIT,
  noteFromConversation,
  noteFromTurn,
  textOf,
  transcriptFilename,
  transcriptMarkdown,
} from "@/lib/notes";
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
    expect(note).toContain("why is this slow?");
  });

  /**
   * The whole point of the format, reported 2026-08-06.
   *
   * The first version quoted every line with `> `, which is safe and useless: a
   * table renders as a table *inside a quotation*, a fenced block as a quoted
   * fence, and in a plain editor every line just wears a `>`. What was copied
   * has to arrive as what it was.
   */
  it("copies the markdown verbatim — no quoting, or it cannot render", () => {
    const body = ["## Short answer", "", "| Env | Shape |", "|---|---|", "| a | b |"].join("\n");
    const note = noteFromTurn(session, ev(4, { t: "assistant_text", text: body }));
    expect(note).toContain(body);
    expect(note).not.toMatch(/^>/m);
  });

  it("separates the provenance from the copied text with a rule", () => {
    const note = noteFromTurn(session, ev(4, { t: "assistant_text", text: "the answer" }));
    const lines = note.split("\n");
    const rule = lines.indexOf("---");
    expect(rule).toBeGreaterThan(0);
    // Everything above the rule is ours; everything below is theirs.
    expect(lines.slice(0, rule).join("\n")).toContain("**Agent**");
    expect(lines.slice(rule + 1).join("\n")).toContain("the answer");
  });

  it("names the session by id when the session is already gone", () => {
    const note = noteFromTurn(null, ev(3, { t: "assistant_text", text: "because it forks" }));
    expect(note).toContain("# abc12345 · turn 3");
    expect(note).toContain("**Agent**");
  });

  /** A fenced block has to survive intact — it is most of what gets copied. */
  it("leaves a code fence exactly as it was", () => {
    const body = "run:\n\n```sh\nkubectl get pods\n```";
    const note = noteFromTurn(session, ev(1, { t: "assistant_text", text: body }));
    expect(note).toContain("```sh\nkubectl get pods\n```");
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
    expect(note).toContain("line 1");
    expect(note).toContain("line 3");
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
    expect(note).toContain(`line ${CONVERSATION_LIMIT + 40}`);
    expect(note).not.toMatch(/^line 1$/m);
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

describe("the whole transcript, as a file", () => {
  const many = (n: number) =>
    Array.from({ length: n }, (_, i) => ev(i + 1, { t: "assistant_text", text: `line ${i + 1}` }));

  /**
   * The opposite choice from a copied conversation, and deliberately so. A note
   * is read, so it keeps the tail; an export is *kept*, and one that quietly
   * dropped the beginning would be discovered only when you went looking for
   * the part that is gone.
   */
  it("carries every turn, however long the conversation", () => {
    const note = transcriptMarkdown(session, many(CONVERSATION_LIMIT + 50));
    expect(note).toMatch(/^line 1$/m);
    expect(note).toContain(`line ${CONVERSATION_LIMIT + 50}`);
    expect(note).not.toContain("not copied");
  });

  it("states what the file can honestly claim to be", () => {
    const note = transcriptMarkdown(session, many(2));
    expect(note).toContain("2 turn(s)");
    expect(note).toContain("turns mogeung has read");
  });

  it("carries the facts you need to know which session this was", () => {
    const s = { ...session, repo_root: "/repo", git_branch: "main", cwd: "/repo/sub" } as Session;
    const note = transcriptMarkdown(s, many(1));
    expect(note).toContain("`abc12345-0000`");
    expect(note).toContain("`/repo`");
    expect(note).toContain("`main`");
  });

  it("names the file after the session and the moment", () => {
    const name = transcriptFilename(session, Date.parse("2026-08-06T09:05:00"));
    expect(name).toBe("the-auth-refactor-20260806-0905.md");
  });

  /** The label is agent-written text; the shell sanitises again, but a name
   *  full of punctuation should not arrive there in the first place. */
  it("keeps a hostile title readable", () => {
    const s = { id: "x", title: "fix: ../../etc/passwd!!" } as Session;
    expect(transcriptFilename(s, Date.parse("2026-08-06T09:05:00"))).toBe("fix-etc-passwd-20260806-0905.md");
  });
});
