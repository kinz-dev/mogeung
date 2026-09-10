/**
 * The task list is a **view**, and the document is the thing. `R-L3`,
 * ADR-0015.
 *
 * The assertion that matters most is the one about direction: ticking a box
 * here sends `task_set`, which rewrites the markdown, and the derived table is
 * rebuilt from what the document then says. There is deliberately no path that
 * marks a row locally — a list that could disagree with its documents is the
 * second source of truth the ADR exists to refuse, and it would look right on
 * screen while being wrong on disk.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { TasksTool } from "@/ui/tools/TasksTool";
import type { Note, Task } from "@/wire/types";

const sent: unknown[] = [];

const note = (id: string, body: string): Note =>
  ({ id, body, created: 1, updated: 1 }) as Note;

const task = (over: Partial<Task> = {}): Task => ({
  note_id: "n1",
  ord: 0,
  text: "write the thing",
  done: false,
  ...over,
});

beforeEach(() => {
  cleanup();
  sent.length = 0;
  useStore.setState({
    prefs: defaultPrefs(),
    notes: [note("n1", "# Plan\n\n- [ ] write the thing\n")],
    tasks: [],
    closedToday: 0,
    noteOpenId: null,
    send: ((m: unknown) => sent.push(m)) as never,
  });
});

describe("the tasks panel", () => {
  it("asks for the list when it opens", () => {
    render(<TasksTool />);
    expect(sent).toContainEqual({ cmd: "task_list" });
  });

  it("says there are none, and what makes one", () => {
    render(<TasksTool />);
    expect(screen.getByText(/no tasks/i)).toBeInTheDocument();
    expect(screen.getByText(/a checkbox in a document/i)).toBeInTheDocument();
  });

  it("lists the open ones with the document they are in", () => {
    useStore.setState({ tasks: [task()] });
    render(<TasksTool />);

    expect(screen.getByText("write the thing")).toBeInTheDocument();
    // The document's first line, with the heading marker off: a label is not a
    // rendering, and a document has no title field to read instead.
    expect(screen.getByText("Plan")).toBeInTheDocument();
  });

  /**
   * **The direction that matters.** A tick is a request to rewrite the
   * document; nothing here marks the row itself.
   */
  it("ticking sends task_set and changes nothing locally", () => {
    useStore.setState({ tasks: [task()] });
    render(<TasksTool />);

    fireEvent.click(screen.getByLabelText("close write the thing"));

    expect(sent).toContainEqual({
      cmd: "task_set",
      note_id: "n1",
      ord: 0,
      done: true,
    });
    expect(useStore.getState().tasks[0].done).toBe(false);
  });

  it("unticking a closed one asks for the opposite", () => {
    useStore.setState({ tasks: [task({ done: true })] });
    render(<TasksTool />);
    fireEvent.click(screen.getByText(/show 1 done/i));

    fireEvent.click(screen.getByLabelText("reopen write the thing"));

    expect(sent).toContainEqual({
      cmd: "task_set",
      note_id: "n1",
      ord: 0,
      done: false,
    });
  });

  /**
   * A checklist that keeps its corpses at eye level stops being a list of what
   * to do.
   */
  it("keeps closed tasks behind a count rather than in the list", () => {
    useStore.setState({ tasks: [task(), task({ ord: 1, text: "done one", done: true })] });
    render(<TasksTool />);

    expect(screen.queryByText("done one")).not.toBeInTheDocument();
    expect(screen.getByText(/show 1 done/i)).toBeInTheDocument();

    fireEvent.click(screen.getByText(/show 1 done/i));
    expect(screen.getByText("done one")).toBeInTheDocument();
  });

  /** The one thing the markdown cannot say, so the one number worth the strip. */
  it("says how many were closed today", () => {
    useStore.setState({ tasks: [task()], closedToday: 3 });
    render(<TasksTool />);
    expect(screen.getByText("3 closed today")).toBeInTheDocument();
  });

  it("says so plainly when nothing was closed", () => {
    useStore.setState({ tasks: [task()], closedToday: 0 });
    render(<TasksTool />);
    expect(screen.getByText("nothing closed today")).toBeInTheDocument();
  });

  /**
   * *"Where did I write this"* has to be one click, and it must not set state
   * behind a panel that is shut — the `R-J31` failure, one panel over.
   */
  it("opens the document a task lives in, and the Notes tool with it", () => {
    useStore.setState({ tasks: [task()] });
    useStore.getState().setPrefs({ rail: [] });
    render(<TasksTool />);

    fireEvent.click(screen.getByText("write the thing"));

    expect(useStore.getState().noteOpenId).toBe("n1");
    expect(useStore.getState().prefs.rail).toContain("notes");
  });

  /** The box and the row are different questions and must not be one target. */
  it("ticking does not also open the document", () => {
    useStore.setState({ tasks: [task()] });
    render(<TasksTool />);

    fireEvent.click(screen.getByLabelText("close write the thing"));

    expect(useStore.getState().noteOpenId).toBeNull();
  });

  /** A note can be deleted while its history keeps the closure. */
  it("survives a task whose document has gone", () => {
    useStore.setState({ tasks: [task({ note_id: "vanished" })], notes: [] });
    render(<TasksTool />);
    expect(screen.getByText(/a document that has gone/)).toBeInTheDocument();
  });
});

describe("making a task", () => {
  /**
   * Reported 2026-09-10: *"I can't get any task display. it always show no
   * tasks"* — and the panel was right, because none of the user's fourteen
   * notes held a checkbox and **nothing in the window could put one there**.
   * A list of checkboxes you can only fill by knowing markdown and finding
   * another panel first is a list that reads as broken.
   */
  it("offers a box even when there is nothing to list", () => {
    render(<TasksTool />);
    expect(screen.getByLabelText("add a task")).toBeInTheDocument();
    expect(screen.getByText(/no tasks yet/i)).toBeInTheDocument();
  });

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

    const save = sent.find((m) => (m as { cmd?: string }).cmd === "note_save") as {
      id: string;
      body: string;
    };
    expect(save.id).toBe("");
    expect(save.body).toBe("# Tasks\n\n- [ ] buy milk\n");
  });

  it("appends to the Tasks document once it exists", () => {
    useStore.setState({ notes: [note("t1", "# Tasks\n\n- [ ] first\n")] });
    render(<TasksTool />);

    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "second" } });
    fireEvent.keyDown(box, { key: "Enter" });

    const save = sent.find((m) => (m as { cmd?: string }).cmd === "note_save") as {
      id: string;
      body: string;
    };
    expect(save.id).toBe("t1");
    expect(save.body).toBe("# Tasks\n\n- [ ] first\n- [ ] second\n");
  });

  /** A document that ends mid-line after every addition is one you notice. */
  it("keeps the document ending in a single newline", () => {
    useStore.setState({ notes: [note("t1", "# Tasks\n\n- [ ] first\n\n\n")] });
    render(<TasksTool />);

    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "second" } });
    fireEvent.keyDown(box, { key: "Enter" });

    const save = sent.find((m) => (m as { cmd?: string }).cmd === "note_save") as { body: string };
    expect(save.body).toBe("# Tasks\n\n- [ ] first\n- [ ] second\n");
  });

  it("sends nothing for an empty box", () => {
    render(<TasksTool />);
    fireEvent.keyDown(screen.getByLabelText("add a task"), { key: "Enter" });
    expect(sent.some((m) => (m as { cmd?: string }).cmd === "note_save")).toBe(false);
  });

  it("clears the box so the next task can be typed straight away", () => {
    render(<TasksTool />);
    const box = screen.getByLabelText("add a task");
    fireEvent.change(box, { target: { value: "one" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(box).toHaveValue("");
  });
});
