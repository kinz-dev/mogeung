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
const accepted: [string, boolean][] = [];

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
  render(<CommandBox repo="/w/mogeung" onAccept={(c, run) => void accepted.push([c, run])} />);

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
    expect(accepted).toEqual([["ls -la", false]]);
    // Closed, and the draft forgotten — nothing here is kept.
    expect(useStore.getState().showCommandBox).toBe(false);
    expect(useStore.getState().commandDraft).toBeNull();
  });

  it("accepts with the same Enter that asked", () => {
    show();
    ask("list the files");
    answers("ls -la");
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(accepted).toEqual([["ls -la", false]]);
  });

  /**
   * `Alt+Enter` runs it, asked for on 2026-08-29. Two keys rather than one,
   * and the command has been on screen since before either could be pressed.
   *
   * This is not ADR-0003's fence: that one is about text reaching an
   * **agent's** input, and a shell tab is yours (ADR-0011). What moved is this
   * box's own sentence, by request.
   */
  it("runs it on Alt+Enter, and only then", () => {
    show();
    ask("list the files");
    answers("ls -la");
    fireEvent.keyDown(box(), { key: "Enter", altKey: true });
    expect(accepted).toEqual([["ls -la", true]]);
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

  /**
   * The second ask of a session is almost never a different question — it is
   * *without the pipe*, *use ripgrep*. Asked for 2026-08-29.
   */
  it("refines the command already there rather than starting over", () => {
    show();
    ask("grep xyz");
    answers("grep xyz . | sort");
    ask("use ripgrep");
    const msg = askId();
    // The previous command travels with the change, and the prompt says to
    // keep the rest of it.
    expect(msg.messages[0].content).toContain("grep xyz . | sort");
    expect(msg.messages[0].content).toContain("use ripgrep");
    expect(msg.messages[0].content).toContain("Change only what was asked");
  });

  /** Up-arrow walks what you asked, so a variation is one key rather than a retype. */
  it("remembers what you asked", () => {
    show();
    ask("list the files");
    answers("ls -la");
    fireEvent.keyDown(box(), { key: "ArrowUp" });
    expect(box()).toHaveValue("list the files");
  });

  /**
   * A placeholder is the **model** saying the command is unfinished, not
   * mogeung judging it unsafe — so `Alt+Enter` writes it and does not run it.
   */
  it("will not run a command that still has a placeholder", () => {
    show();
    ask("grep something and sort it");
    answers("grep xyz <file> | sort -k1,1");
    // Said before either key can be pressed, not after.
    expect(screen.getByText(/fill it in before running/)).toBeInTheDocument();
    fireEvent.keyDown(box(), { key: "Enter", altKey: true });
    // Written, not run: `run` is false despite the Alt.
    expect(accepted).toEqual([["grep xyz <file> | sort -k1,1", false]]);
  });

  /**
   * The rule that makes both keys unambiguous, and it was a bug first: with an
   * answer on screen, Enter accepted the old command instead of asking for the
   * change that had just been typed.
   */
  it("accepts on an empty box and asks on a full one", () => {
    show();
    ask("list the files");
    answers("ls -la");
    // Something typed: that is the ask, not an accept.
    fireEvent.change(box(), { target: { value: "with sizes" } });
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(accepted).toEqual([]);
    // Nothing typed: that is the accept.
    answers("ls -lah");
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(accepted).toEqual([["ls -lah", false]]);
  });

  /** A second model call, so it is asked for rather than volunteered. */
  it("explains the command only when asked", () => {
    show();
    ask("list the files");
    answers("ls -la");
    expect(sent.filter((m) => m.cmd === "model_chat")).toHaveLength(1);
    fireEvent.click(screen.getByText(/what does it do/));
    const explain = sent.filter((m) => m.cmd === "model_chat").at(-1);
    if (!explain || explain.cmd !== "model_chat") throw new Error("nothing was asked");
    expect(explain.messages[0].content).toContain("Explain this bash command");
    expect(explain.messages[0].content).toContain("ls -la");
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
