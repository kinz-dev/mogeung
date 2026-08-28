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
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import type { ClientMsg, Health, ModelHealth } from "@/wire/types";
import { ChatTool, threadAsMarkdown } from "./ChatTool";

const proxied = (forwards: string[], admin?: string): Partial<Health> => ({
  proxy: {
    state: { state: "hosting", port: 8717 },
    url: "http://127.0.0.1:8717/v1",
    admin_url: admin ?? null,
    forwards_to: forwards,
  },
});

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
    useStore.setState({
      chat: [],
      health: null,
      conversationId: null,
      chatHistory: null,
      chatHistoryRefusal: null,
      showChatHistory: false,
      send: (m) => void sent.push(m),
    });
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

/**
 * The three gestures are three different lifetimes. `R-O9`, ADR-0032.
 *
 * They look alike and they are not, which is exactly the confusion worth
 * pinning: **new** forgets which thread this window is in, **clear** empties
 * the panel and stays in the thread, and only the ✕ in the history deletes
 * anything. A test that let *clear* start sending a fresh conversation id
 * would have shipped a button that silently orphans your last answer.
 */
describe("keeping and finding conversations", () => {
  const ask = (text: string) => act(() => useStore.getState().askModel(text));
  const lastChat = () =>
    [...sent].reverse().find((m) => m.cmd === "model_chat") as
      | Extract<ClientMsg, { cmd: "model_chat" }>
      | undefined;

  beforeEach(() => useStore.setState({ health: health({}) }));

  it("mints a conversation on the first ask, not on open", () => {
    render(<ChatTool />);
    expect(useStore.getState().conversationId).toBeNull();
    expect(sent.some((m) => m.cmd === "model_chat")).toBe(false);

    ask("first");
    const id = useStore.getState().conversationId;
    expect(id).toBeTruthy();
    expect(lastChat()?.conversation).toBe(id);
  });

  it("keeps asking in the same conversation until you start a new one", () => {
    ask("first");
    const first = useStore.getState().conversationId;
    ask("second");
    expect(lastChat()?.conversation).toBe(first);

    act(() => useStore.getState().newConversation());
    expect(useStore.getState().conversationId).toBeNull();
    expect(useStore.getState().chat).toEqual([]);

    ask("third");
    expect(useStore.getState().conversationId).not.toBe(first);
    expect(lastChat()?.conversation).not.toBe(first);
  });

  /**
   * The one that would have been a real bug. `clear` reads like a delete and
   * is not: it empties the panel and leaves you in the thread, so the next
   * question continues the conversation the daemon already has rather than
   * beginning a second one that has lost its first half.
   */
  it("clearing the panel stays in the conversation", () => {
    ask("first");
    const id = useStore.getState().conversationId;
    act(() => useStore.getState().clearChat());
    expect(useStore.getState().chat).toEqual([]);
    expect(useStore.getState().conversationId).toBe(id);
    ask("again");
    expect(lastChat()?.conversation).toBe(id);
  });

  it("asks for the list when the history is opened, every time", () => {
    render(<ChatTool />);
    expect(sent.some((m) => m.cmd === "chat_list")).toBe(false);
    act(() => useStore.setState({ showChatHistory: true }));
    expect(sent.filter((m) => m.cmd === "chat_list")).toHaveLength(1);
    act(() => useStore.setState({ showChatHistory: false }));
    act(() => useStore.setState({ showChatHistory: true }));
    expect(sent.filter((m) => m.cmd === "chat_list")).toHaveLength(2);
  });

  /**
   * `null` is *not asked yet* and `[]` is *asked, and there are none*. An
   * empty list rendered for the first reads as data loss.
   */
  it("tells looking apart from empty", () => {
    useStore.setState({ showChatHistory: true, chatHistory: null });
    const { rerender } = render(<ChatTool />);
    expect(screen.getByText(/looking/)).toBeInTheDocument();

    act(() => useStore.setState({ chatHistory: [] }));
    rerender(<ChatTool />);
    expect(screen.getByText(/no conversations yet/)).toBeInTheDocument();
  });

  it("opens a conversation and goes on asking in it", () => {
    useStore.setState({
      showChatHistory: true,
      chatHistory: [
        { id: "old", title: "why is the queue empty", turns: 4, created: 1, updated: 2 },
      ],
    });
    render(<ChatTool />);
    fireEvent.click(screen.getByTitle("why is the queue empty"));
    expect(sent).toContainEqual({ cmd: "chat_load", id: "old" });

    // The daemon answers with the turns, and the panel is now *in* that
    // thread — asking again continues it rather than forking a copy.
    act(() =>
      useStore.getState().ingest({
        ev: "chat_conversation",
        id: "old",
        turns: [
          { role: "user", content: "why is the queue empty" },
          { role: "assistant", content: "because" },
        ],
      }),
    );
    expect(useStore.getState().conversationId).toBe("old");
    expect(useStore.getState().chat).toHaveLength(2);
    ask("and now");
    expect(lastChat()?.conversation).toBe("old");
  });

  it("forgets one conversation on the daemon, and only from the history", () => {
    useStore.setState({
      showChatHistory: true,
      chatHistory: [{ id: "old", title: "a thread", turns: 2, created: 1, updated: 2 }],
    });
    render(<ChatTool />);
    fireEvent.click(screen.getByTitle(/forget this conversation/));
    expect(sent).toContainEqual({ cmd: "chat_delete", id: "old" });
  });

  /** The daemon's own words, not a second version composed here. */
  it("shows why there is no history rather than an empty list", () => {
    useStore.setState({
      showChatHistory: true,
      chatHistory: [],
      chatHistoryRefusal: "not keeping conversations — `chat_history = false`",
    });
    render(<ChatTool />);
    expect(screen.getByText(/chat_history = false/)).toBeInTheDocument();
    expect(screen.queryByText(/no conversations yet/)).not.toBeInTheDocument();
  });
});

/**
 * The sentence that stands in for a gate mogeung cannot have. `R-O10`.
 *
 * With its own llmproxy in front, `model.host` is `127.0.0.1`, so ADR-0031's
 * consent gate passes without asking while prompts may be going to a vendor.
 * mogeung cannot gate that — routing is per request and a target can fail over
 * — so the panel says where instead. These pin that it is said, that it is only
 * said when true, and that it never claims the bytes stay home.
 */
describe("saying where a proxy forwards", () => {
  // Its own reset: the suite's other `beforeEach` hooks are scoped to their
  // own `describe`, so without this the panel starts with whatever thread the
  // previous test left behind and the empty-state hint never renders.
  beforeEach(() => {
    sent.length = 0;
    useStore.setState({ chat: [], showChatHistory: false, send: (m) => void sent.push(m) });
  });

  // The line is split by the <Mono> holding the host, so a plain text matcher
  // sees two nodes and neither of them whole.
  const line = () =>
    document.body.textContent?.replace(/\s+/g, " ") ?? "";

  it("names every host the proxy may forward to", () => {
    useStore.setState({
      health: { ...health({ host: "127.0.0.1", remote: false }), ...proxied(["api.anthropic.com"]) } as Health,
    });
    render(<ChatTool />);
    expect(line()).toContain("may forward to");
    expect(screen.getByText("api.anthropic.com")).toBeInTheDocument();
  });

  /**
   * A proxy whose providers are all on this machine forwards nowhere, and
   * claiming otherwise would be the line that teaches you to stop reading it.
   * The proxy itself is still announced — that is what explains why the model
   * under each answer changes from question to question.
   */
  it("announces the proxy but claims no forwarding when there is none", () => {
    useStore.setState({
      health: { ...health({ host: "127.0.0.1", remote: false }), ...proxied([]) } as Health,
    });
    render(<ChatTool />);
    expect(line()).toContain("via mogeung's proxy");
    expect(line()).not.toContain("may forward to");
  });

  /**
   * The whole reason the button exists. `R-O10`.
   *
   * llmproxy's admin interface binds a **random** port, so before this nobody
   * could reach it. With admin off there is no URL — and then there must be no
   * button, because a control that goes nowhere is worse than an absent one.
   */
  it("offers the admin interface only when there is one", () => {
    useStore.setState({
      health: {
        ...health({ host: "127.0.0.1", remote: false }),
        ...proxied([], "http://127.0.0.1:41235/"),
      } as Health,
    });
    const { unmount } = render(<ChatTool />);
    expect(screen.getByTitle(/admin interface/)).toBeInTheDocument();
    unmount();

    useStore.setState({
      health: { ...health({ host: "127.0.0.1", remote: false }), ...proxied([]) } as Health,
    });
    render(<ChatTool />);
    expect(screen.queryByTitle(/admin interface/)).not.toBeInTheDocument();
  });

  /** No proxy at all is the ordinary case and must read exactly as before. */
  it("says nothing when there is no proxy", () => {
    useStore.setState({ health: health({}) });
    render(<ChatTool />);
    expect(line()).not.toContain("may forward to");
  });

  /**
   * The empty state must not claim the endpoint is the machine it is proxied
   * from — `127.0.0.1` there would read as *this stays local*, which is the
   * precise misreading this whole row exists to prevent.
   */
  it("does not present the loopback proxy as the endpoint", () => {
    useStore.setState({
      health: { ...health({ host: "127.0.0.1", remote: false }), ...proxied(["api.anthropic.com"]) } as Health,
    });
    render(<ChatTool />);
    expect(line()).toContain("mogeung's own proxy");
    expect(line()).not.toContain("at 127.0.0.1");
  });
});
