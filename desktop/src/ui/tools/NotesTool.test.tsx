/**
 * Searching notes has to say *why* something matched.
 *
 * The filter reads the whole body, so a note can match on its fourth line while
 * its first says something else — and a list that always previews line one then
 * looks like it matched at random.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { NotesTool, previewOf } from "./NotesTool";

describe("previewOf", () => {
  const body = "a title line\nsomething else\negress vs ingress definition\ntrailing";

  it("shows the first line when nothing is being searched", () => {
    expect(previewOf(body, "")).toBe("a title line");
  });

  it("shows the line that matched, not the first", () => {
    expect(previewOf(body, "ingress")).toBe("egress vs ingress definition");
  });

  it("is case-insensitive, like the filter that produced the row", () => {
    expect(previewOf(body, "EGRESS")).toBe("egress vs ingress definition");
  });

  /** A row is only in the list because it matched somewhere, but the preview
   * must still be something rather than blank if that ever stops holding. */
  it("falls back to the first line rather than showing nothing", () => {
    expect(previewOf(body, "nowhere")).toBe("a title line");
    expect(previewOf("", "x")).toBe("");
  });
});

/**
 * A note is mostly *read*, and since the copy buttons landed most of what is in
 * one arrived as markdown — a table, a fenced block, a heading. Showing that as
 * source shows you the thing you copied before it meant anything.
 */
describe("reading a note rather than its source", () => {
  const note = {
    id: "n1",
    body: "# a heading\n\n| Env | Shape |\n|---|---|\n| a | b |",
    created: 0,
    updated: 0,
  };

  beforeEach(() => {
    useStore.setState({ notes: [note] as never });
    useStore.getState().setPrefs({ notesMarkdown: true });
  });

  it("renders the markdown when the box is ticked", async () => {
    render(<NotesTool />);
    fireEvent.click(screen.getByText("# a heading"));
    expect(await screen.findByRole("heading", { name: "a heading" })).toBeInTheDocument();
    expect(screen.getByRole("table")).toBeInTheDocument();
  });

  it("shows the source when it is not, because you cannot type into a rendering", async () => {
    useStore.getState().setPrefs({ notesMarkdown: false });
    render(<NotesTool />);
    fireEvent.click(screen.getByText("# a heading"));
    expect(await screen.findByPlaceholderText("write something")).toHaveValue(note.body);
    expect(screen.queryByRole("heading", { name: "a heading" })).toBeNull();
  });

  /** A blank panel where you just pressed **+** reads as a broken button. */
  it("shows the editor for an empty note whatever the preference says", async () => {
    useStore.setState({ notes: [{ ...note, body: "" }] as never });
    render(<NotesTool />);
    fireEvent.click(screen.getByText("empty"));
    expect(await screen.findByPlaceholderText("write something")).toBeInTheDocument();
  });
});

/**
 * A marked turn is not a note.
 *
 * The remark on a bookmark is a *name for the mark* — a line saying why you
 * stopped there — and it shares the `notes` table only because ADR-0015 decided
 * one table. It belongs beside its turn in Bookmarks, and it was filling the
 * scratchpad with one-line remarks about conversations.
 */
describe("what belongs in the scratchpad", () => {
  const doc = { id: "d1", body: "a document", created: 0, updated: 2, session_id: "s1", seq: null };
  const mark = { id: "m1", body: "the bit that broke", created: 0, updated: 3, session_id: "s1", seq: 12 };
  const bare = { id: "m2", body: "", created: 0, updated: 4, session_id: "s1", seq: 13 };

  beforeEach(() => {
    useStore.setState({ notes: [doc, mark, bare] as never });
  });

  it("lists documents and leaves marked turns to Bookmarks", () => {
    render(<NotesTool />);
    expect(screen.getByText("a document")).toBeInTheDocument();
    expect(screen.queryByText("the bit that broke")).toBeNull();
  });

  it("counts documents, not marks, beside the filter", () => {
    render(<NotesTool />);
    fireEvent.change(screen.getByLabelText("find in notes"), { target: { value: "document" } });
    // One of one document — not one of three, which is what counting the whole
    // table said and made the filter look broken.
    expect(screen.getByText("1/1")).toBeInTheDocument();
  });

  it("says nothing is written when every note is a mark", () => {
    useStore.setState({ notes: [mark, bare] as never });
    render(<NotesTool />);
    expect(screen.getByText("nothing written yet")).toBeInTheDocument();
  });
});

/**
 * `Ctrl+S` saves the open note — and only from inside Notes.
 *
 * The scope is the whole request: `Ctrl+S` means *save* in every editor anyone
 * arrives from, so a window-wide binding would claim it on behalf of a panel
 * that is usually shut. These fail without the focus check, which is the part
 * that is easy to write and easy to write wrongly.
 */
describe("Ctrl+S in the Notes panel", () => {
  const note = { id: "n1", body: "before", created: 0, updated: 0, session_id: null, seq: null };
  let sent: unknown[] = [];

  beforeEach(() => {
    sent = [];
    useStore.setState({ notes: [note] as never, send: ((m: unknown) => sent.push(m)) as never });
    useStore.getState().setPrefs({ notesMarkdown: false });
  });

  const openAndEdit = async (text: string) => {
    render(<NotesTool />);
    fireEvent.click(screen.getByText("before"));
    const box = await screen.findByPlaceholderText("write something");
    fireEvent.change(box, { target: { value: text } });
    return box;
  };

  const pressSave = (target: Element | Window) =>
    fireEvent.keyDown(target, { key: "s", ctrlKey: true });

  it("saves the note being edited", async () => {
    const box = await openAndEdit("after");
    box.focus();
    pressSave(box);
    expect(sent).toEqual([
      { cmd: "note_save", id: "n1", body: "after", session_id: null, seq: null, repo: null },
    ]);
  });

  /**
   * The whole point of the scoping. Focus outside Notes and the key belongs to
   * whatever *is* focused — the Code pane wants it next, and a global binding
   * would have taken it before anyone asked.
   */
  it("does nothing when the focus is somewhere else", async () => {
    // A real element outside the panel, and the move is asserted: focusing
    // `document.body` would have left `activeElement` wherever it already was
    // in jsdom, and the test would pass without exercising the check at all.
    const outside = document.createElement("button");
    document.body.appendChild(outside);
    await openAndEdit("after");
    outside.focus();
    expect(document.activeElement).toBe(outside);

    pressSave(outside);
    expect(sent).toEqual([]);
    outside.remove();
  });

  /** Same rule the button follows by being disabled — one answer, two callers. */
  it("does not write a note that has not changed", async () => {
    render(<NotesTool />);
    fireEvent.click(screen.getByText("before"));
    const box = await screen.findByPlaceholderText("write something");
    box.focus();
    pressSave(box);
    expect(sent).toEqual([]);
  });
});
