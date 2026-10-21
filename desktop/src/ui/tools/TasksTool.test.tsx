/**
 * The task list is a **view**, and the document is the thing. `R-L3`,
 * ADR-0015 — and since 2026-10-21 the panel of feature 0052 (`R-L12`–`R-L17`).
 *
 * The assertion that matters most is still the one about direction: ticking a
 * box sends `task_set`, which rewrites the markdown, and the derived table is
 * rebuilt from what the document then says. The box now flips at once, but
 * the store's list is untouched until the broadcast: a drawing, never a second
 * record.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { TasksTool } from "@/ui/tools/TasksTool";
import type { Note, Session, Task } from "@/wire/types";

const sent: unknown[] = [];

const note = (id: string, body: string, over: Partial<Note> = {}): Note =>
  ({ id, body, created: 1, updated: 1, ...over }) as Note;

const task = (over: Partial<Task> = {}): Task => ({
  note_id: "n1",
  ord: 0,
  text: "write the thing",
  done: false,
  ...over,
});

const saves = () => sent.filter((m) => (m as { cmd?: string }).cmd === "note_save") as { id: string; body: string; session_id: string | null; repo: string | null }[];
const sets = () => sent.filter((m) => (m as { cmd?: string }).cmd === "task_set");

beforeEach(() => {
  cleanup();
  sent.length = 0;
  useStore.setState({
    prefs: defaultPrefs(),
    notes: [note("n1", "# Plan\n\n- [ ] write the thing\n")],
    tasks: [],
    tasksLoaded: true,
    closedToday: 0,
    closed: [],
    pendingTicks: {},
    pendingAdd: null,
    justAdded: null,
    noteOpenId: null,
    noteReveal: null,
    sessions: {},
    selected: null,
    focusRail: null,
    send: ((m: unknown) => sent.push(m)) as never,
  });
});

describe("the tasks panel", () => {
  it("asks for the list when it opens", () => {
    render(<TasksTool />);
    expect(sent).toContainEqual({ cmd: "task_list" });
  });

  /** *Not asked yet* must not look like *none*: the old panel flashed the empty state on every open. */
  it("shows only the box until the list has arrived", () => {
    useStore.setState({ tasksLoaded: false });
    render(<TasksTool />);
    expect(screen.getByLabelText("add a task")).toBeInTheDocument();
    expect(screen.queryByText(/no tasks yet/i)).not.toBeInTheDocument();
  });

  it("says there are none, and what to do, in plain words", () => {
    render(<TasksTool />);
    expect(screen.getByText(/no tasks yet/i)).toBeInTheDocument();
    expect(screen.getByText(/press Enter/)).toBeInTheDocument();
  });

  /** `R-L12`: the document is the folder, and its headings sit inside it. */
  it("draws each note as a folder with its headings inside, newest note first", () => {
    useStore.setState({
      notes: [note("n1", "# Plan\n\n## Work\n\n- [ ] filed", { created: 2 }), note("n2", "Dentist on Thursday\n\n- [ ] loose", { created: 1 })],
      tasks: [task({ note_id: "n1", text: "filed", group: "Work" }), task({ note_id: "n2", text: "loose" })],
    });
    const { container } = render(<TasksTool />);

    expect(screen.getByRole("button", { name: "Plan" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Work" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dentist on Thursday" })).toBeInTheDocument();
    const text = container.textContent ?? "";
    expect(text.indexOf("Plan")).toBeLessThan(text.indexOf("Dentist"));
    // The headingless note's task sits under its own folder, after it.
    expect(text.indexOf("Dentist on Thursday")).toBeLessThan(text.indexOf("loose"));
  });

  it("keeps two notes with the same heading as two folders", () => {
    useStore.setState({
      notes: [note("n1", "# A"), note("n2", "# B")],
      tasks: [task({ note_id: "n1", text: "one", group: "Work" }), task({ note_id: "n2", text: "two", group: "Work" })],
    });
    render(<TasksTool />);
    expect(screen.getAllByRole("button", { name: "Work" })).toHaveLength(2);
  });

  it("shows the note's repository on the folder", () => {
    useStore.setState({ notes: [note("n1", "# Plan", { repo: "/home/me/proj" })], tasks: [task()] });
    render(<TasksTool />);
    expect(screen.getByText("proj")).toBeInTheDocument();
  });

  it("survives a task whose document has gone", () => {
    useStore.setState({ tasks: [task({ note_id: "vanished" })], notes: [] });
    render(<TasksTool />);
    expect(screen.getByRole("button", { name: "a note that was deleted" })).toBeInTheDocument();
  });

  /**
   * **The direction that matters.** A tick is a request to rewrite the
   * document. The box flips at once (`R-L13`), the store's list does not.
   */
  it("ticking sends task_set, flips the box at once, keeps the row a moment, and changes the list not at all", () => {
    useStore.setState({ tasks: [task()] });
    render(<TasksTool />);

    const box = screen.getByRole("checkbox", { name: "mark done: write the thing" });
    expect(box).toHaveAttribute("aria-checked", "false");
    fireEvent.click(box);

    expect(sent).toContainEqual({ cmd: "task_set", note_id: "n1", ord: 0, done: true });
    expect(screen.getByRole("checkbox", { name: "mark open: write the thing" })).toHaveAttribute("aria-checked", "true");
    expect(useStore.getState().tasks[0].done).toBe(false);
  });

  it("goes back to what the document says when the daemon refuses", () => {
    useStore.setState({ tasks: [task()] });
    render(<TasksTool />);
    fireEvent.click(screen.getByRole("checkbox", { name: "mark done: write the thing" }));
    expect(useStore.getState().pendingTicks).toEqual({ "n1:0": true });

    act(() => useStore.setState({ pendingTicks: {} }));
    expect(screen.getByRole("checkbox", { name: "mark done: write the thing" })).toHaveAttribute("aria-checked", "false");
  });

  /** The P0 of the critique: a near miss on the tick used to open the note. */
  it("does nothing when the words are clicked", () => {
    useStore.setState({ tasks: [task()] });
    render(<TasksTool />);
    fireEvent.click(screen.getByText("write the thing"));
    expect(useStore.getState().noteOpenId).toBeNull();
    expect(sets()).toEqual([]);
  });

  /** *"Where did I write this"* is one explicit click, and it names the line. */
  it("opens the document a task lives in, at its line, with the Notes tool", () => {
    useStore.setState({ tasks: [task({ ord: 3 })] });
    useStore.getState().setPrefs({ rail: [] });
    render(<TasksTool />);

    fireEvent.click(screen.getAllByLabelText("open in Notes")[0]);

    expect(useStore.getState().noteOpenId).toBe("n1");
    expect(useStore.getState().noteReveal).toEqual({ id: "n1", ord: 3 });
    expect(useStore.getState().prefs.rail).toContain("notes");
  });

  it("keeps done tasks behind the count in the footer, shown in place on request", () => {
    useStore.setState({ tasks: [task(), task({ ord: 1, text: "done one", done: true })] });
    render(<TasksTool />);

    expect(screen.queryByText("done one")).not.toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: "1 done" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(toggle);
    expect(screen.getByText("done one")).toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(within(screen.getByRole("status")).getByText("1 open")).toBeInTheDocument();
  });

  it("unticking a done one asks for the opposite", () => {
    useStore.setState({ tasks: [task({ done: true })] });
    render(<TasksTool />);
    fireEvent.click(screen.getByRole("button", { name: "1 done" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "mark open: write the thing" }));
    expect(sent).toContainEqual({ cmd: "task_set", note_id: "n1", ord: 0, done: false });
  });

  /** `R-L17`: the one thing the markdown cannot say, and now a list rather than only a count. */
  it("opens the closures of the day with their notes", () => {
    useStore.setState({
      tasks: [task(), task({ ord: 1, text: "shipped", done: true })],
      closedToday: 1,
      closed: [{ note_id: "n1", text: "shipped", at: 1_760_000_000 }],
    });
    render(<TasksTool />);

    const btn = screen.getByRole("button", { name: "1 closed today" });
    expect(btn).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(btn);
    const list = screen.getByLabelText("closed today");
    expect(within(list).getByText("shipped")).toBeInTheDocument();
    expect(within(list).getByText(/Plan ·/)).toBeInTheDocument();
  });

  it("says so plainly when nothing was closed", () => {
    useStore.setState({ tasks: [task()], closedToday: 0 });
    render(<TasksTool />);
    expect(screen.getByText("nothing closed today")).toBeInTheDocument();
  });

  it("puts the cursor in the box when the panel is opened by its chord", () => {
    render(<TasksTool />);
    act(() => useStore.setState({ focusRail: "tasks" }));
    expect(screen.getByLabelText("add a task")).toHaveFocus();
    expect(useStore.getState().focusRail).toBeNull();
  });
});

describe("making a task", () => {
  /**
   * **It writes a document**, because ADR-0015 says there is no task outside a
   * `- [ ]` line. An add that did anything else would be a second kind of task.
   */
  it("creates the Tasks document the first time, with the line in it", () => {
    useStore.setState({ notes: [] });
    render(<TasksTool />);

    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "buy milk" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(saves()[0]).toMatchObject({ id: "", body: "# Tasks\n\n- [ ] buy milk\n", session_id: null, repo: null });
  });

  /** `R-L14`: the top of the list, not under whatever heading was last. */
  it("puts a new task under the title, before the first heading", () => {
    useStore.setState({ notes: [note("t1", "# Tasks\n\n## Work\n\n- [ ] first\n")] });
    render(<TasksTool />);

    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "second" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(saves()[0]).toMatchObject({ id: "t1", body: "# Tasks\n\n- [ ] second\n\n## Work\n\n- [ ] first\n" });
  });

  it("puts it at the end of the section you picked, and says so on the box", () => {
    useStore.setState({
      notes: [note("t1", "# Tasks\n\n## Work\n\n- [ ] first\n")],
      tasks: [task({ note_id: "t1", text: "first", group: "Work" })],
    });
    render(<TasksTool />);

    fireEvent.click(screen.getByRole("button", { name: "Work" }));
    expect(screen.getByLabelText("where a new task goes: Tasks › Work")).toBeInTheDocument();

    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "second" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(saves()[0]).toMatchObject({ id: "t1", body: "# Tasks\n\n## Work\n\n- [ ] first\n- [ ] second\n" });
  });

  /** The text is the one thing not to lose: it stays until the broadcast carries it. */
  it("keeps the text, disabled, until the task comes back, then clears", () => {
    useStore.setState({ notes: [] });
    render(<TasksTool />);

    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "buy milk" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(box).toHaveValue("buy milk");
    expect(box).toBeDisabled();
    expect(useStore.getState().pendingAdd).toEqual({ text: "buy milk", noteId: null });

    // The broadcast arrives with the task in it.
    act(() => useStore.setState({ tasks: [task({ note_id: "t1", text: "buy milk" })], pendingAdd: null, justAdded: "t1:0" }));
    expect(box).toHaveValue("");
    expect(box).not.toBeDisabled();
  });

  it("gives the text back when the add came back refused", () => {
    useStore.setState({ notes: [] });
    render(<TasksTool />);
    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "buy milk" } });
    fireEvent.keyDown(box, { key: "Enter" });

    act(() => useStore.setState({ pendingAdd: null }));
    expect(box).toHaveValue("buy milk");
    expect(box).not.toBeDisabled();
  });

  it("sends nothing for an empty box", () => {
    render(<TasksTool />);
    fireEvent.keyDown(screen.getByLabelText("add a task"), { key: "Enter" });
    expect(saves()).toEqual([]);
  });
});

/** `R-L16`: tasks know their session — tagged on add, shown, and filterable. */
describe("tasks and the selected session", () => {
  const s1 = { id: "s1", repo_root: "/r/proj", title: "proj work" } as Session;

  it("aims the box at the repository's own note, created tagged the first time", () => {
    useStore.setState({ notes: [], sessions: { s1 }, selected: "s1" });
    render(<TasksTool />);
    expect(screen.getByLabelText("where a new task goes: Tasks — proj")).toBeInTheDocument();

    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "x" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(saves()[0]).toMatchObject({ id: "", body: "# Tasks — proj\n\n- [ ] x\n", session_id: "s1", repo: "/r/proj" });
  });

  it("filters to the notes tagged with the selected session's repository", () => {
    useStore.setState({
      notes: [note("a", "# A", { repo: "/r/proj" }), note("b", "# B")],
      tasks: [task({ note_id: "a", text: "mine" }), task({ note_id: "b", text: "other" })],
      sessions: { s1 },
      selected: "s1",
    });
    render(<TasksTool />);

    fireEvent.click(screen.getByRole("tab", { name: "this session" }));
    expect(screen.getByText("mine")).toBeInTheDocument();
    expect(screen.queryByText("other")).not.toBeInTheDocument();
    expect(screen.getByText(/1 note without this session/)).toBeInTheDocument();
    expect(within(screen.getByRole("status")).getByText("1 open")).toBeInTheDocument();
  });

  it("offers the filter only while a session is selected", () => {
    useStore.setState({ tasks: [task()] });
    render(<TasksTool />);
    expect(screen.getByRole("tab", { name: "this session" })).toBeDisabled();
  });
});

describe("nesting and grouping", () => {
  it("indents a nested task by its depth", () => {
    useStore.setState({ tasks: [task({ ord: 0, text: "parent" }), task({ ord: 1, text: "child", depth: 1 })] });
    render(<TasksTool />);

    const pad = (label: string) => screen.getByText(label).closest("[data-task-key]")!.getAttribute("style") ?? "";
    expect(pad("parent")).toContain("padding-left: 20px");
    expect(pad("child")).toContain("padding-left: 36px");
  });

  /** Document order, not alphabetical — the order you wrote them in. */
  it("keeps the headings in the order the document has them", () => {
    useStore.setState({
      tasks: [task({ ord: 0, text: "z-first", group: "Zebra" }), task({ ord: 1, text: "a-second", group: "Apple" })],
    });
    const { container } = render(<TasksTool />);
    const text = container.textContent ?? "";
    expect(text.indexOf("Zebra")).toBeLessThan(text.indexOf("Apple"));
  });

  it("folds a sub-task away and says what is left under it", () => {
    useStore.setState({ tasks: [task({ ord: 0, text: "ship it" }), task({ ord: 1, text: "write it", depth: 1 })] });
    render(<TasksTool />);

    fireEvent.click(screen.getByLabelText("fold the sub-tasks of ship it"));

    expect(screen.queryByText("write it")).not.toBeInTheDocument();
    expect(screen.getByText(/1 open under/)).toBeInTheDocument();
  });

  it("shuts a section, hiding its tasks but not itself", () => {
    useStore.setState({ tasks: [task({ ord: 0, text: "ship it", group: "Work" }), task({ ord: 1, text: "milk", group: "Groceries" })] });
    render(<TasksTool />);

    fireEvent.click(screen.getByLabelText("fold Work"));

    expect(screen.queryByText("ship it")).not.toBeInTheDocument();
    expect(screen.getByLabelText("unfold Work")).toBeInTheDocument();
    expect(screen.getByText("milk")).toBeInTheDocument();
  });

  /**
   * **No cascade.** Each line is its own checkbox in the document, and ticking
   * a parent's children would write lines the user never ticked.
   */
  it("ticking a parent does not tick its children", () => {
    useStore.setState({ tasks: [task({ ord: 0, text: "ship it" }), task({ ord: 1, text: "write it", depth: 1 })] });
    render(<TasksTool />);
    fireEvent.click(screen.getByRole("checkbox", { name: "mark done: ship it" }));
    expect(sets()).toEqual([{ cmd: "task_set", note_id: "n1", ord: 0, done: true }]);
  });

  it("keeps a ticked parent while an open child needs it", () => {
    useStore.setState({ tasks: [task({ ord: 0, text: "parent", done: true }), task({ ord: 1, text: "child", depth: 1 })] });
    render(<TasksTool />);
    expect(screen.getByText("parent")).toBeInTheDocument();
    expect(screen.getByText("child")).toBeInTheDocument();
  });

  /** A folder with everything done says so rather than vanishing. */
  it("says a folder is all done rather than hiding it", () => {
    useStore.setState({ tasks: [task({ ord: 0, text: "milk", group: "Groceries", done: true })] });
    render(<TasksTool />);
    expect(screen.getAllByText("all 1 done").length).toBeGreaterThan(0);
    expect(screen.queryByText("milk")).not.toBeInTheDocument();
  });
});
