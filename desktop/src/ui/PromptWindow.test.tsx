/**
 * The drafted follow-up prompt. `R-O7`, [ADR-0034](../../../docs/decisions/0034-the-draft-is-a-chat-ask.md).
 *
 * Four properties, and none of them is that a model can be asked something.
 * They are the four the ADR is about:
 *
 * - the draft **is not kept** — the ask names no conversation, which the daemon
 *   reads as *do not store this*;
 * - the draft's answer **never lands in the chat panel**, though it comes back
 *   through the chat's own door;
 * - the **raw concatenation is one click away**, so what the draft dropped can
 *   be seen;
 * - and the clipboard gets **what is on screen**, never the other one.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import type { ClientMsg, ModelHealth } from "@/wire/types";
import { PromptWindow } from "./PromptWindow";

const sent: ClientMsg[] = [];
const written: string[] = [];

const model = (over: Partial<ModelHealth> = {}): ModelHealth => ({
  configured: true,
  host: "127.0.0.1",
  model: "qwen",
  remote: false,
  allowed: true,
  chat_allowed: true,
  refusal: null,
  last_error: null,
  last_ok_ms: null,
  ...over,
});

/** The daemon's health row, which is what says whether the button may be pressed. */
const withModel = (m: ModelHealth | null) =>
  // Only the field the window reads: `Health` is thirty fields wide and a
  // fixture of all of them would pin nothing this test is about.
  ({ model: m } as unknown as NonNullable<ReturnType<typeof useStore.getState>["health"]>);

const askId = () => {
  const msg = sent.filter((m) => m.cmd === "model_chat").at(-1);
  if (!msg || msg.cmd !== "model_chat") throw new Error("nothing was asked");
  return msg;
};

const answers = (id: string, text: string) =>
  act(() =>
    useStore
      .getState()
      .ingest({ ev: "model_reply", id, text, error: null, model: "qwen", elapsed_ms: 900 }),
  );

const draftIt = () => fireEvent.click(screen.getByText("draft with the model"));

/**
 * By role rather than by its words: the button says *copied* for a second and
 * a half afterwards, so a test that clicks it twice by text finds nothing the
 * second time.
 */
const copyButton = () => screen.getByRole("button", { name: /clipboard|copied/i });

beforeEach(() => {
  cleanup();
  sent.length = 0;
  written.length = 0;
  vi.stubGlobal("navigator", {
    clipboard: {
      writeText: (t: string) => {
        written.push(t);
        return Promise.resolve();
      },
    },
  });
  useStore.setState({
    showPrompt: true,
    promptDraft: null,
    chat: [],
    conversationId: null,
    health: withModel(model()),
    flagged: [
      {
        sessionId: "s",
        path: "src/auth.rs",
        header: "@@ -10,7 +10,9 @@",
        note: "this leaks on the error path",
        body: ["-old", "+new"],
      },
    ],
    send: (m) => void sent.push(m),
  });
});

describe("drafting the follow-up prompt", () => {
  it("asks nothing until it is asked to", () => {
    render(<PromptWindow />);
    expect(sent).toHaveLength(0);
  });

  /**
   * The whole of ADR-0034's storage clause, and it is one absent field. A
   * draft is something you copy, not a conversation you come back to.
   */
  it("names no conversation, so the daemon keeps nothing", () => {
    render(<PromptWindow />);
    draftIt();
    expect(askId().conversation).toBeUndefined();
    expect(askId().messages[0].content).toContain("this leaks on the error path");
  });

  /**
   * It travels through `model_chat` because that is the one free-form door
   * this protocol has (ADR-0031 clause 2) — which makes *this* the test that
   * matters: the answer must not surface in a conversation somebody is reading.
   */
  it("keeps the answer out of the chat panel", () => {
    render(<PromptWindow />);
    draftIt();
    answers(askId().id, "Please fix the leak in src/auth.rs.");
    expect(useStore.getState().chat).toHaveLength(0);
    expect(useStore.getState().promptDraft?.text).toBe("Please fix the leak in src/auth.rs.");
  });

  it("shows the draft, and the raw text one click away", () => {
    render(<PromptWindow />);
    draftIt();
    answers(askId().id, "Please fix the leak in src/auth.rs.");
    expect(screen.getByText(/Please fix the leak/)).toBeInTheDocument();

    fireEvent.click(screen.getByText("raw"));
    // What the draft was written from, unchanged — including the hunk the
    // draft may have decided not to mention.
    expect(screen.getByText(/1\. `src\/auth\.rs`/)).toBeInTheDocument();
  });

  /** A window that shows one text and copies another is worse than no draft. */
  it("copies what is on screen", () => {
    render(<PromptWindow />);
    draftIt();
    answers(askId().id, "Please fix the leak in src/auth.rs.");
    fireEvent.click(copyButton());
    expect(written.at(-1)).toBe("Please fix the leak in src/auth.rs.");

    fireEvent.click(screen.getByText("raw"));
    fireEvent.click(copyButton());
    expect(written.at(-1)).toContain("@@ -10,7 +10,9 @@");
  });

  /**
   * A refusal is the daemon's sentence, not the window's guess — there is one
   * place that decides whether a model may be asked.
   */
  it("says why the button is shut rather than offering a draft it cannot make", () => {
    useStore.setState({
      health: withModel(model({ chat_allowed: false, refusal: "not on a public bind" })),
    });
    render(<PromptWindow />);
    const button = screen.getByText("draft with the model").closest("button")!;
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "not on a public bind");
  });

  /** A failed draft leaves the raw text, which is what the window was for. */
  it("shows a failure without taking the prompt away", () => {
    render(<PromptWindow />);
    draftIt();
    act(() =>
      useStore.getState().ingest({
        ev: "model_reply",
        id: askId().id,
        text: null,
        error: "the endpoint refused",
        model: "",
        elapsed_ms: 0,
      }),
    );
    expect(screen.getByText("the endpoint refused")).toBeInTheDocument();
    expect(screen.getByText(/1\. `src\/auth\.rs`/)).toBeInTheDocument();
  });
});
