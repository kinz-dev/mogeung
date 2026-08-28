/**
 * The chat panel says *why* it is shut. `R-O5`.
 *
 * The refusals it renders are the daemon's own words — ADR-0030 decides
 * whether a model may be asked in one place, and a window that composed its
 * own version of the reason would be a second place that can disagree. What is
 * pinned here is that the reason is shown at all: a box that is simply dead is
 * the state that gets reported as a mogeung bug.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import type { ClientMsg, Health, ModelHealth } from "@/wire/types";
import { ChatTool, threadAsMarkdown } from "./ChatTool";

const health = (model: Partial<ModelHealth> | null): Health =>
  ({
    alerts: [],
    unknown_types: [],
    versions_seen: [],
    model: model && {
      configured: true,
      host: "spark-7ecc",
      model: "qwen3.8-sglang",
      remote: true,
      allowed: true,
      chat_allowed: true,
      refusal: null,
      last_error: null,
      last_ok_ms: null,
      ...model,
    },
  }) as unknown as Health;

const sent: ClientMsg[] = [];

describe("the chat panel", () => {
  beforeEach(() => {
    sent.length = 0;
    useStore.setState({ chat: [], health: null, send: (m) => void sent.push(m) });
  });

  it("takes a question when the daemon says it may", () => {
    useStore.setState({ health: health({}) });
    render(<ChatTool />);
    const box = screen.getByLabelText("ask the model");
    expect(box).not.toBeDisabled();
    fireEvent.change(box, { target: { value: "what does -w do?" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(sent.some((m) => m.cmd === "model_chat")).toBe(true);
  });

  /** Shift+Enter is a newline in every chat box there has ever been. */
  it("does not send on Shift+Enter", () => {
    useStore.setState({ health: health({}) });
    render(<ChatTool />);
    const box = screen.getByLabelText("ask the model");
    fireEvent.change(box, { target: { value: "half a thought" } });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    expect(sent.some((m) => m.cmd === "model_chat")).toBe(false);
  });

  it("renders the daemon's refusal verbatim rather than a blank panel", () => {
    useStore.setState({
      health: health({ allowed: false, refusal: "refusing to send anything to spark-7ecc" }),
    });
    render(<ChatTool />);
    expect(screen.getByText(/refusing to send anything to spark-7ecc/)).toBeTruthy();
    expect(screen.getByLabelText("ask the model")).toBeDisabled();
  });

  /** ADR-0030 clause 4. The endpoint is fine; the bind is not. */
  it("is shut when the bind is public, however the endpoint is configured", () => {
    useStore.setState({
      health: health({ chat_allowed: false, refusal: "chat is refused on a daemon bound beyond loopback" }),
    });
    render(<ChatTool />);
    expect(screen.getByText(/bound beyond loopback/)).toBeTruthy();
    expect(screen.getByLabelText("ask the model")).toBeDisabled();
  });

  /** Nothing configured is the ordinary state of a fresh install and must not
   *  read as a failure. */
  it("says there is no model rather than showing an error", () => {
    useStore.setState({ health: health({ configured: false, allowed: false, refusal: "no model configured" }) });
    render(<ChatTool />);
    expect(screen.getByText(/no model configured/)).toBeTruthy();
  });

  /** A daemon built before pillar O sends no row at all. That is a different
   *  state from "configured and refused" and says so. */
  it("distinguishes a daemon that has never heard of models", () => {
    useStore.setState({ health: health(null) });
    render(<ChatTool />);
    expect(screen.getByText(/predates pillar O/)).toBeTruthy();
  });

  it("copies the thread into a note, which is the whole of its persistence", () => {
    useStore.setState({
      health: health({}),
      chat: [
        { id: "1:you", role: "user", content: "a question" },
        { id: "1", role: "assistant", content: "| a | table |", model: "m", elapsed_ms: 10 },
      ],
    });
    render(<ChatTool />);
    fireEvent.click(screen.getByTitle("copy this conversation into a note"));
    const note = sent.find((m) => m.cmd === "note_save");
    expect(note).toBeTruthy();
    if (note?.cmd !== "note_save") throw new Error("unreachable");
    // An empty id is how `NoteSave` mints a new note.
    expect(note.id).toBe("");
    expect(note.body).toContain("a question");
    expect(note.body).toContain("| a | table |");
  });
});

describe("threadAsMarkdown", () => {
  /** `R-L2` learnt this once already: quoting with `>` is safe and useless —
   *  a table becomes a quoted table and a fenced block a quoted fence. What
   *  was said has to arrive as what it was. */
  it("carries the text verbatim rather than quoting it", () => {
    const md = threadAsMarkdown(
      [{ id: "1", role: "assistant", content: "```sh\ngit diff -w\n```", model: "m" }],
      new Date("2026-08-28T09:00:00Z"),
    );
    expect(md).toContain("```sh\ngit diff -w\n```");
    expect(md).not.toContain("> ```");
  });

  it("leaves out a question still in flight", () => {
    const md = threadAsMarkdown(
      [
        { id: "1:you", role: "user", content: "asked" },
        { id: "1", role: "assistant", content: "", pending: true },
      ],
      new Date("2026-08-28T09:00:00Z"),
    );
    expect(md).toContain("asked");
    expect(md.split("\n").filter((l) => l.startsWith("**")).length).toBe(1);
  });
});
