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
