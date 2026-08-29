/**
 * Ask why a hunk changed. `R-O4`, and the three answers `--bin why` measured.
 *
 * What is pinned here is **not** that a model can be asked something. It is
 * that the three outcomes stay three outcomes: the turns say why, the turns do
 * not say why, and no conversation covers the file at all. The harness that
 * gated this row found the middle one to be the most common — 5 of 14 moments
 * had a reason — so a panel that dressed *the turns do not say* as a failure,
 * or dressed *read from the diff* as provenance, would be wrong most of the
 * time in the direction that matters.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore, askKey } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { ClientMsg, FileChange, ModelHealth } from "@/wire/types";
import { DiffList } from "@/ui/DiffView";

const sent: ClientMsg[] = [];

const file = (): FileChange =>
  ({
    path: "src/auth.rs",
    old_path: null,
    status: "modified",
    insertions: 1,
    deletions: 0,
    hunks: [
      {
        anchor: "a1",
        header: "@@ -10,7 +10,9 @@",
        lines: ["+let retries = 3;"],
        insertions: 1,
        deletions: 0,
        flags: [],
        score: 0,
        reviewed: false,
      },
    ],
    flags: [],
    score: 0,
    truncated: false,
  }) as unknown as FileChange;

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

const health = (m: ModelHealth | null) =>
  ({ model: m }) as unknown as NonNullable<ReturnType<typeof useStore.getState>["health"]>;

const ask = () =>
  fireEvent.click(screen.getByTitle(/why did this change/));

const askId = () => {
  const msg = sent.filter((m) => m.cmd === "model_chat").at(-1);
  if (!msg || msg.cmd !== "model_chat") throw new Error("nothing was asked");
  return msg;
};

/** A reply from the daemon, carrying the provenance the daemon decided. */
const reply = (over: Partial<Extract<import("@/wire/types").ServerMsg, { ev: "model_reply" }>>) =>
  act(() =>
    useStore.getState().ingest({
      ev: "model_reply",
      id: askId().id,
      text: null,
      error: null,
      model: "qwen",
      elapsed_ms: 800,
      ...over,
    } as never),
  );

beforeEach(() => {
  cleanup();
  sent.length = 0;
  useStore.setState({
    prefs: defaultPrefs(),
    diffAnswers: {},
    chat: [],
    health: health(model()),
    send: (m: ClientMsg) => void sent.push(m),
  } as never);
});

describe("asking a hunk why it changed", () => {
  it("asks nothing until it is asked to, and then carries ids and a question", () => {
    render(<DiffList files={[file()]} sessionId="s1" />);
    expect(sent).toHaveLength(0);

    ask();
    fireEvent.keyDown(screen.getByLabelText(/ask about @@/), { key: "Enter" });

    const msg = askId();
    // Ids and a question. The client has never read the transcript and could
    // not send the turns if it wanted to — ADR-0030 clause 1.
    expect(msg.about).toEqual({ session_id: "s1", path: "src/auth.rs", anchor: "a1" });
    expect(msg.messages[0].content).toBe("Why did this change?");
    // Not a conversation: an answer about a hunk is not a thread to come back to.
    expect(msg.conversation).toBeUndefined();
  });

  it("shows the answer and citations that open the Transcript there", () => {
    render(<DiffList files={[file()]} sessionId="s1" />);
    ask();
    fireEvent.keyDown(screen.getByLabelText(/ask about @@/), { key: "Enter" });
    reply({
      text: "They asked for the retries to be backed off.",
      basis: "turns",
      cites: [
        { line: 12, role: "user", timestamp: "2026-08-01T10:00:00Z", preview: "back them off" },
      ],
    });

    expect(screen.getByText(/retries to be backed off/)).toBeInTheDocument();
    fireEvent.click(screen.getByText(/you · line 12/));
    // `focusEventTs`, which is how the Transcript opens at a moment — a
    // transcript line number is not a place any client can navigate to.
    expect(useStore.getState().focusEventTs).toBe("2026-08-01T10:00:00Z");
  });

  /**
   * The majority case in `--bin why`'s corpus. It has to read as an answer,
   * because a panel that cries failure five times out of nine is reporting a
   * bug that is not there.
   */
  it("says plainly when the turns do not contain the reason", () => {
    render(<DiffList files={[file()]} sessionId="s1" />);
    ask();
    fireEvent.keyDown(screen.getByLabelText(/ask about @@/), { key: "Enter" });
    reply({ text: "", basis: "unanswered", cites: [] });

    expect(screen.getByText(/do not say why/)).toBeInTheDocument();
  });

  /** An answer read from the diff is never passed off as provenance. */
  it("labels an answer read from the code alone", () => {
    render(<DiffList files={[file()]} sessionId="s1" />);
    ask();
    fireEvent.keyDown(screen.getByLabelText(/ask about @@/), { key: "Enter" });
    reply({ text: "It raises the retry count to three.", basis: "code", cites: [] });

    expect(screen.getByText(/no conversation covering this file/)).toBeInTheDocument();
  });

  /**
   * The rule `--bin why` bought: nearest-in-time answers cited one human turn
   * against twelve of the assistant's. The **daemon** decides this; the window
   * only has to render it, and this test is what keeps it rendering it.
   */
  it("marks an answer that rests on the assistant narrating itself", () => {
    render(<DiffList files={[file()]} sessionId="s1" />);
    ask();
    fireEvent.keyDown(screen.getByLabelText(/ask about @@/), { key: "Enter" });
    reply({
      text: "The file was changed because the assistant then wrote the file.",
      basis: "turns",
      narration: true,
      cites: [
        { line: 40, role: "assistant", timestamp: "2026-08-01T10:00:00Z", preview: "writing it now" },
      ],
    });

    expect(screen.getByText(/treat it as narration/)).toBeInTheDocument();
  });

  it("keeps the answer out of the chat panel it borrowed the door from", () => {
    render(<DiffList files={[file()]} sessionId="s1" />);
    ask();
    fireEvent.keyDown(screen.getByLabelText(/ask about @@/), { key: "Enter" });
    reply({ text: "because they asked", basis: "turns", cites: [] });

    expect(useStore.getState().chat).toHaveLength(0);
    expect(useStore.getState().diffAnswers[askKey("s1", "src/auth.rs", "a1")].text).toBe(
      "because they asked",
    );
  });

  /**
   * The degrade that would otherwise be silent. An older daemon ignores
   * `about` entirely — the field is `serde(default)` — and answers the
   * question as ordinary chat, with no transcript behind it. Showing that
   * text where provenance is promised is the failure the labels exist to
   * prevent, so it is withheld rather than dressed.
   */
  it("withholds an answer from a daemon that never read a transcript", () => {
    render(<DiffList files={[file()]} sessionId="s1" />);
    ask();
    fireEvent.keyDown(screen.getByLabelText(/ask about @@/), { key: "Enter" });
    reply({ text: "In general, retry counts are raised to improve resilience." });

    expect(screen.getByText(/predates/)).toBeInTheDocument();
    expect(screen.queryByText(/improve resilience/)).not.toBeInTheDocument();
  });

  /** No model is a state, not an error, and the daemon's own sentence says why. */
  it("offers no ask when there is no model, and says the daemon's reason", () => {
    useStore.setState({
      health: health(model({ chat_allowed: false, refusal: "not on a public bind" })),
    } as never);
    render(<DiffList files={[file()]} sessionId="s1" />);
    const button = screen.getByTitle("not on a public bind");
    expect(button).toBeDisabled();
  });
});
