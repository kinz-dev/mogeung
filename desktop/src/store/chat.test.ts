/**
 * The chat panel's reducer. `R-O5`.
 *
 * Three properties the panel depends on and none of which is obvious from the
 * code: an ask is two rows and one of them is pending, an answer is matched by
 * **id** rather than by position, and an error is shown without becoming part
 * of what gets sent back next time.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStore } from "@/store";
import type { ClientMsg } from "@/wire/types";

const sent: ClientMsg[] = [];

const lastAsk = () => {
  const msg = sent.filter((m) => m.cmd === "model_chat").at(-1);
  if (!msg || msg.cmd !== "model_chat") throw new Error("nothing was asked");
  return msg;
};

const answer = (id: string, text: string) =>
  useStore.getState().ingest({ ev: "model_reply", id, text, error: null, model: "m", elapsed_ms: 12 });

const fail = (id: string, error: string) =>
  useStore.getState().ingest({ ev: "model_reply", id, text: null, error, model: "", elapsed_ms: 0 });

describe("the chat reducer", () => {
  beforeEach(() => {
    sent.length = 0;
    useStore.setState({ chat: [], send: (m) => void sent.push(m) });
  });

  it("shows the question and a pending answer before anything comes back", () => {
    useStore.getState().askModel("what does -w do?");
    const chat = useStore.getState().chat;
    expect(chat.map((m) => m.role)).toEqual(["user", "assistant"]);
    expect(chat[0].content).toBe("what does -w do?");
    expect(chat[1].pending).toBe(true);
    expect(lastAsk().messages).toEqual([{ role: "user", content: "what does -w do?" }]);
  });

  it("ignores an empty ask rather than sending a blank turn", () => {
    useStore.getState().askModel("   \n ");
    expect(useStore.getState().chat).toHaveLength(0);
    expect(sent).toHaveLength(0);
  });

  /** Nothing stops a second question while the first is still out, and the
   *  answers may come back in either order. Position would put one under the
   *  wrong question; the id cannot. */
  it("files two answers by id, whatever order they arrive in", () => {
    useStore.getState().askModel("first");
    const a = lastAsk().id;
    useStore.getState().askModel("second");
    const b = lastAsk().id;
    expect(a).not.toBe(b);

    answer(b, "the second answer");
    answer(a, "the first answer");

    const byId = Object.fromEntries(useStore.getState().chat.map((m) => [m.id, m]));
    expect(byId[a].content).toBe("the first answer");
    expect(byId[b].content).toBe("the second answer");
    expect(useStore.getState().chat.some((m) => m.pending)).toBe(false);
  });

  it("carries the conversation so far, so the model has the thread", () => {
    useStore.getState().askModel("first");
    answer(lastAsk().id, "an answer");
    useStore.getState().askModel("second");
    expect(lastAsk().messages).toEqual([
      { role: "user", content: "first" },
      { role: "assistant", content: "an answer" },
      { role: "user", content: "second" },
    ]);
  });

  /** The failure stays on screen — you want to see what went wrong — but
   *  sending it back would teach the model that mogeung's own error messages
   *  are turns in the conversation. */
  it("keeps a failed turn visible and out of the next request", () => {
    useStore.getState().askModel("first");
    fail(lastAsk().id, "could not reach the model");
    expect(useStore.getState().chat.at(-1)?.error).toBe("could not reach the model");

    useStore.getState().askModel("second");
    expect(lastAsk().messages).toEqual([{ role: "user", content: "second" }]);
  });

  it("clears the thread on request, because that is the whole retention policy", () => {
    useStore.getState().askModel("first");
    useStore.getState().clearChat();
    expect(useStore.getState().chat).toEqual([]);
  });

  /** A reply for a question this window never asked — a stale socket, another
   *  window's id — must not invent a row. */
  it("drops an answer to a question it never asked", () => {
    useStore.getState().askModel("mine");
    answer("someone-elses-id", "not for you");
    expect(useStore.getState().chat.some((m) => m.content === "not for you")).toBe(false);
    expect(useStore.getState().chat).toHaveLength(2);
  });

  it("keeps the thread out of persisted preferences", () => {
    useStore.getState().askModel("something private");
    expect(JSON.stringify(useStore.getState().prefs)).not.toContain("something private");
  });
});

/** Ids only have to be unique within one window, and `randomUUID` is absent on
 *  an insecure origin — so the fallback is a real path, not decoration. */
describe("request ids without crypto.randomUUID", () => {
  it("still gives every ask its own id", () => {
    // Unconditionally: `crypto.randomUUID` is always defined in the type
    // environment, so guarding the spy on it made `tsc` reject the file — and
    // `vitest` does not typecheck, so it passed here and failed the install
    // build instead.
    vi.spyOn(globalThis.crypto, "randomUUID").mockReturnValue(undefined as never);
    sent.length = 0;
    useStore.setState({ chat: [], send: (m) => void sent.push(m) });
    useStore.getState().askModel("one");
    useStore.getState().askModel("two");
    const ids = sent.filter((m) => m.cmd === "model_chat").map((m) => (m as { id: string }).id);
    expect(new Set(ids).size).toBe(2);
    // And that they came from the fallback rather than from a `randomUUID`
    // the spy failed to reach — otherwise this test passes without ever
    // exercising the path it is named after.
    expect(ids.every((id) => id.startsWith("chat-"))).toBe(true);
    vi.restoreAllMocks();
  });
});

/**
 * Streaming. `R-O11`.
 *
 * The property that matters is that the **reply is the truth**: chunks are an
 * early view, so a dropped one has to be repaired rather than leaving a hole
 * nobody can see. These pin that, and that chunks land on the right question
 * when two are in flight.
 */
describe("an answer that arrives in pieces", () => {
  beforeEach(() => {
    useStore.setState({ chat: [], health: null, send: () => {} });
  });

  const pending = (id: string, content = "") =>
    useStore.setState({
      chat: [{ id, role: "assistant" as const, content, pending: true }],
    });

  it("appends each chunk to the turn it belongs to", () => {
    pending("q1");
    useStore.getState().ingest({ ev: "model_chunk", id: "q1", delta: "Hel" });
    useStore.getState().ingest({ ev: "model_chunk", id: "q1", delta: "lo" });
    expect(useStore.getState().chat[0].content).toBe("Hello");
    expect(useStore.getState().chat[0].pending).toBe(true);
  });

  /** Two questions can be out at once and their chunks interleave. */
  it("does not put one answer's text into another", () => {
    useStore.setState({
      chat: [
        { id: "q1", role: "assistant", content: "", pending: true },
        { id: "q2", role: "assistant", content: "", pending: true },
      ],
    });
    useStore.getState().ingest({ ev: "model_chunk", id: "q2", delta: "two" });
    useStore.getState().ingest({ ev: "model_chunk", id: "q1", delta: "one" });
    expect(useStore.getState().chat.map((m) => m.content)).toEqual(["one", "two"]);
  });

  /**
   * The repair. A chunk lost on the way leaves the streamed text short, and
   * the reply replaces it wholesale rather than appending — otherwise the
   * answer on screen would be quietly missing a piece.
   */
  it("the final reply is the truth, not the sum of the chunks", () => {
    pending("q1", "Hel");
    useStore.getState().ingest({
      ev: "model_reply",
      id: "q1",
      text: "Hello, world",
      error: null,
      model: "qwen",
      elapsed_ms: 900,
    });
    const [m] = useStore.getState().chat;
    expect(m.content).toBe("Hello, world");
    expect(m.pending).toBe(false);
    expect(m.model).toBe("qwen");
  });

  /** A chunk for a turn that already finished must not reopen it. */
  it("ignores a chunk that arrives after the reply", () => {
    pending("q1");
    useStore.getState().ingest({
      ev: "model_reply", id: "q1", text: "done", error: null, model: "m", elapsed_ms: 1,
    });
    useStore.getState().ingest({ ev: "model_chunk", id: "q1", delta: " late" });
    expect(useStore.getState().chat[0].content).toBe("done");
  });
});
