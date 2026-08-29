/**
 * The command box. `R-O12`, `A41`.
 *
 * The three refusals, which are the feature:
 *
 * - **your line is never read** — the ask carries the sentence you typed and
 *   nothing from the terminal, because a prefix can carry `export TOKEN=…`;
 * - **it writes, it never runs** — accepting hands the text to the pty with no
 *   Enter, and that is ADR-0003's 2026-08-29 amendment one level down;
 * - **a draft says it is a draft** — and one that deletes or elevates says
 *   that too, since the hazard here is a plausible line one keypress from a
 *   real shell.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import type { ClientMsg, ModelHealth } from "@/wire/types";
import { CommandBox } from "@/ui/CommandBox";

const sent: ClientMsg[] = [];
const accepted: string[] = [];

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

const health = (m: ModelHealth) =>
  ({ model: m }) as unknown as NonNullable<ReturnType<typeof useStore.getState>["health"]>;

const show = () =>
  render(<CommandBox repo="/w/mogeung" onAccept={(c) => void accepted.push(c)} />);

const box = () => screen.getByLabelText("ask for a command");

const ask = (question: string) => {
  fireEvent.change(box(), { target: { value: question } });
  fireEvent.keyDown(box(), { key: "Enter" });
};

const askId = () => {
  const m = sent.filter((x) => x.cmd === "model_chat").at(-1);
  if (!m || m.cmd !== "model_chat") throw new Error("nothing was asked");
  return m;
};

const answers = (text: string) =>
  act(() =>
    useStore.getState().ingest({
      ev: "model_reply",
      id: askId().id,
      text,
      error: null,
      model: "qwen",
      elapsed_ms: 700,
    } as never),
  );

beforeEach(() => {
  cleanup();
  sent.length = 0;
  accepted.length = 0;
  useStore.setState({
    showCommandBox: true,
    commandDraft: null,
    chat: [],
    health: health(model()),
    send: (m: ClientMsg) => void sent.push(m),
  } as never);
});

describe("asking for a command in words", () => {
  it("sends the sentence and the directory, and nothing else", () => {
    show();
    ask("grep xyz and sort by the first column");
    const msg = askId();
    expect(msg.messages[0].content).toContain("grep xyz and sort by the first column");
    expect(msg.messages[0].content).toContain("/w/mogeung");
    // Not a conversation: a command you asked for is not a thread to return to.
    expect(msg.conversation).toBeUndefined();
  });

  it("shows the command it was given, marked as never having run", () => {
    show();
    ask("grep xyz and sort by the first column");
    answers("```bash\ngrep -rn xyz . | sort -k1\n```");
    expect(screen.getByText("grep -rn xyz . | sort -k1")).toBeInTheDocument();
    expect(screen.getByText(/never run here/)).toBeInTheDocument();
  });

  /** The whole of ADR-0003's amendment, one level down: it writes, you run. */
  it("puts the command in the line and never runs it", () => {
    show();
    ask("list the files");
    answers("ls -la");
    fireEvent.click(screen.getByText(/put it in the line/));
    expect(accepted).toEqual(["ls -la"]);
    // Closed, and the draft forgotten — nothing here is kept.
    expect(useStore.getState().showCommandBox).toBe(false);
    expect(useStore.getState().commandDraft).toBeNull();
  });

  it("accepts with the same Enter that asked", () => {
    show();
    ask("list the files");
    answers("ls -la");
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(accepted).toEqual(["ls -la"]);
  });

  /** Marked, never blocked — mogeung does not run it and cannot make it safe. */
  it("marks a command that deletes or elevates", () => {
    show();
    ask("wipe the build directory");
    answers("rm -rf ./build");
    expect(screen.getByText(/deletes, overwrites or elevates/)).toBeInTheDocument();
  });

  /** A made-up command is worse than none, so a refusal stays a refusal. */
  it("says so when the model would not write one", () => {
    show();
    ask("rewrite the whole application in rust");
    answers("NO");
    expect(screen.getByText(/would not write one command/)).toBeInTheDocument();
    expect(screen.queryByText(/put it in the line/)).not.toBeInTheDocument();
  });

  it("keeps the answer out of the chat panel it borrowed the door from", () => {
    show();
    ask("list the files");
    answers("ls -la");
    expect(useStore.getState().chat).toHaveLength(0);
  });

  it("is shut, with the daemon's reason, when there is no model", () => {
    useStore.setState({
      health: health(model({ chat_allowed: false, refusal: "not on a public bind" })),
    } as never);
    show();
    expect(box()).toBeDisabled();
    expect(box()).toHaveAttribute("placeholder", "not on a public bind");
  });
});
