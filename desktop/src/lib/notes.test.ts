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
  destinationFor,
  insertTask,
  removeTask,
  taskLine,
  noteFromConversation,
  noteFromTurn,
  textOf,
  transcriptFilename,
  transcriptMarkdown,
} from "@/lib/notes";
import type { Note, Session, TranscriptEvent, EventKind } from "@/wire/types";

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

/**
 * `R-L14`: a new line goes where you can see it. Every add used to append
 * under whatever heading happened to be last, which is how things got
 * mis-filed; now it goes right under the title, or to the end of the section
 * you picked. Still one line in one document.
 */
describe("where a new task goes", () => {
  it("lands under the title, before the first heading", () => {
    expect(insertTask("# Tasks\n\n## Work\n\n- [ ] first\n", "second", null)).toBe(
      "# Tasks\n\n- [ ] second\n\n## Work\n\n- [ ] first\n",
    );
  });

  it("joins a list that already sits under the title", () => {
    expect(insertTask("# Tasks\n\n- [ ] first\n", "second", null)).toBe("# Tasks\n\n- [ ] second\n- [ ] first\n");
  });

  it("starts an empty document, and one with no title at the top", () => {
    expect(insertTask("", "x", null)).toBe("- [ ] x\n");
    expect(insertTask("some prose\n", "x", null)).toBe("- [ ] x\n\nsome prose\n");
  });

  it("goes to the end of the section it was aimed at, keeping the next heading apart", () => {
    const body = "## Work\n\n- [ ] a\n\n## Home\n\n- [ ] b\n";
    expect(insertTask(body, "c", { name: "Work", occurrence: 0 })).toBe("## Work\n\n- [ ] a\n- [ ] c\n\n## Home\n\n- [ ] b\n");
    expect(insertTask("## Work\n- [ ] a\n## Home\n", "c", { name: "Work", occurrence: 0 })).toBe(
      "## Work\n- [ ] a\n- [ ] c\n\n## Home\n",
    );
  });

  /** A heading used twice is two sections, and the address says which. */
  it("tells the second run of a heading from the first", () => {
    const body = "## Work\n\n- [ ] a\n\n## Work\n\n- [ ] b\n";
    expect(insertTask(body, "c", { name: "Work", occurrence: 1 })).toBe("## Work\n\n- [ ] a\n\n## Work\n\n- [ ] b\n- [ ] c\n");
  });

  it("does not take a heading inside a code fence for a section", () => {
    const body = "```\n## Work\n```\n\n## Work\n\n- [ ] a\n";
    expect(insertTask(body, "c", { name: "Work", occurrence: 0 })).toBe("```\n## Work\n```\n\n## Work\n\n- [ ] a\n- [ ] c\n");
  });

  it("falls back to the top when the section is not there any more", () => {
    expect(insertTask("# T\n\n- [ ] a\n", "c", { name: "Gone", occurrence: 0 })).toBe("# T\n\n- [ ] c\n- [ ] a\n");
  });
});

/** `R-L13`: the editor scrolls to the task's line, read the way the daemon counts them. */
describe("which line a task is", () => {
  it("skips quotations and indented code, like the parser", () => {
    const body = "intro\n\n- [ ] a\n> - [ ] no\n    - [ ] code\n- [ ] b\n";
    expect(taskLine(body, 0)).toBe(2);
    expect(taskLine(body, 1)).toBe(5);
    expect(taskLine(body, 2)).toBeNull();
  });

  it("counts a nested task under a list, and nothing inside a fence", () => {
    const body = "- [ ] a\n    - [ ] nested\n```\n- [ ] fenced\n```\n- [ ] b\n";
    expect(taskLine(body, 1)).toBe(1);
    expect(taskLine(body, 2)).toBe(5);
  });
});

/** `R-L16`: with a session selected, a task belongs to that session's repository. */
describe("where the box aims", () => {
  const n = (id: string, body: string, over: Partial<Note> = {}): Note => ({ id, body, created: 1, updated: 1, ...over }) as Note;
  const s1 = { id: "s1", repo_root: "/r/proj", title: "proj work" } as Session;

  it("aims at the picked folder or section first", () => {
    const d = destinationFor({
      notes: [n("a", "# A")],
      sessions: { s1 },
      selected: "s1",
      target: { noteId: "a", title: "A", section: { name: "Work", occurrence: 0 } },
    });
    expect(d).toMatchObject({ label: "A › Work", noteId: "a", create: null });
  });

  it("creates the repository's own Tasks note, tagged, the first time", () => {
    const d = destinationFor({ notes: [], sessions: { s1 }, selected: "s1", target: null });
    expect(d.label).toBe("Tasks — proj");
    expect(d.noteId).toBeNull();
    expect(d.create?.body("x")).toBe("# Tasks — proj\n\n- [ ] x\n");
    expect(d.create).toMatchObject({ session_id: "s1", repo: "/r/proj" });
  });

  it("finds that note again by its title and tag", () => {
    const d = destinationFor({
      notes: [n("t", "# Tasks — proj\n\n- [ ] a", { repo: "/r/proj" })],
      sessions: { s1 },
      selected: "s1",
      target: null,
    });
    expect(d).toMatchObject({ noteId: "t", create: null });
  });

  it("aims at the plain Tasks note with nothing selected", () => {
    const d = destinationFor({ notes: [n("t", "# Tasks")], sessions: {}, selected: null, target: null });
    expect(d).toMatchObject({ label: "Tasks", noteId: "t" });
  });
});

/** `R-L18`: a task goes out of its document with the lines nested under it. */
describe("taking a task out", () => {
  it("removes a leaf and leaves its neighbours", () => {
    expect(removeTask("# T\n\n- [ ] a\n- [ ] b\n- [ ] c\n", 1)).toBe("# T\n\n- [ ] a\n- [ ] c\n");
  });

  it("takes the nested sub-tasks with a parent, two spaces or four", () => {
    expect(removeTask("- [ ] a\n  - [ ] a1\n    - [ ] a1x\n- [ ] b\n", 0)).toBe("- [ ] b\n");
    expect(removeTask("- [ ] a\n    - [ ] a1\n- [ ] b\n", 0)).toBe("- [ ] b\n");
  });

  it("stops at the next task of the same depth", () => {
    expect(removeTask("- [ ] a\n  - [ ] a1\n  - [ ] a2\n- [ ] b\n", 1)).toBe("- [ ] a\n  - [ ] a2\n- [ ] b\n");
  });

  it("does not leave two blank lines where the block was", () => {
    expect(removeTask("## H\n\n- [ ] a\n\n## I\n\n- [ ] b\n", 0)).toBe("## H\n\n## I\n\n- [ ] b\n");
  });

  it("empties a document that held only that task, and refuses an ord it does not have", () => {
    expect(removeTask("- [ ] only\n", 0)).toBe("");
    expect(removeTask("- [ ] only\n", 3)).toBeNull();
  });
});

