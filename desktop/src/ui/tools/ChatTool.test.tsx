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

  /**
   * Alt+Enter is the same newline — asked 2026-10-01. It used to send, being
   * "Enter without Shift", and the browser inserts nothing for it, so the
   * newline has to land where the caret is, not at the end.
   */
  it("breaks the line at the caret on Alt+Enter rather than sending", () => {
    useStore.setState({ health: health({}) });
    render(<ChatTool />);
    const box = screen.getByLabelText("ask the model") as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "first second" } });
    box.setSelectionRange(5, 6); // the space between the words
    fireEvent.keyDown(box, { key: "Enter", altKey: true });
    expect(sent.some((m) => m.cmd === "model_chat")).toBe(false);
    expect(box.value).toBe("first\nsecond");
    expect(box.selectionStart).toBe(6);
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
/**
 * ADR-0033 clause 6's disclosure, after it was cut back. `R-O10`.
 *
 * The standing line under the thread was removed on 2026-08-28 at the owner's
 * request. These now pin the thinner promise that replaced it: the hosts are
 * still reachable, on the admin button's tooltip, and the empty state still
 * refuses to present a loopback proxy as though the endpoint were local.
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

  it("names every host the proxy may forward to, on the admin button", () => {
    useStore.setState({
      health: {
        ...health({ host: "127.0.0.1", remote: false }),
        ...proxied(["api.anthropic.com"], "http://127.0.0.1:41235/"),
      } as Health,
    });
    render(<ChatTool />);
    // Not a standing line any more — a tooltip. Thinner than clause 6 asked
    // for, and pinned so it is not thinned further by accident.
    expect(screen.getByTitle(/may forward to api\.anthropic\.com/)).toBeInTheDocument();
    expect(line()).not.toContain("may forward to");
  });

  /**
   * A proxy whose providers are all on this machine forwards nowhere, and
   * claiming otherwise would be the line that teaches you to stop reading it.
   * The proxy itself is still announced — that is what explains why the model
   * under each answer changes from question to question.
   */
  it("claims no forwarding when there is none", () => {
    useStore.setState({
      health: {
        ...health({ host: "127.0.0.1", remote: false }),
        ...proxied([], "http://127.0.0.1:41235/"),
      } as Health,
    });
    render(<ChatTool />);
    expect(screen.getByTitle(/admin interface/)).toBeInTheDocument();
    expect(screen.queryByTitle(/may forward to/)).not.toBeInTheDocument();
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
    expect(line()).not.toContain("proxy");
    expect(screen.queryByTitle(/admin interface/)).not.toBeInTheDocument();
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

/**
 * Opening the panel puts the cursor in the box. `R-J82`.
 *
 * The second test is the one that matters: the panel also mounts when the
 * window starts with it already open, and focusing then would take the
 * keyboard away from the queue on every launch. That is why this is driven by
 * a signal from the toggle rather than by `autoFocus` on the textarea, and it
 * is the property an `autoFocus` would silently break.
 */
describe("opening the chat panel", () => {
  beforeEach(() => {
    useStore.setState({ chat: [], focusRail: null, showChatHistory: false, health: health({}) });
  });

  it("puts the cursor in the box when the toggle opened it", () => {
    render(<ChatTool />);
    expect(document.activeElement).not.toBe(screen.getByLabelText("ask the model"));

    act(() => useStore.setState({ focusRail: "chat" }));
    expect(document.activeElement).toBe(screen.getByLabelText("ask the model"));
    // Cleared, or re-opening would not fire again: the value would already be
    // "chat" and the effect would not re-run.
    expect(useStore.getState().focusRail).toBeNull();
  });

  it("does not take the keyboard just because it rendered", () => {
    render(<ChatTool />);
    expect(document.activeElement).toBe(document.body);
  });

  it("leaves another tool's turn alone", () => {
    render(<ChatTool />);
    act(() => useStore.setState({ focusRail: "files" }));
    expect(document.activeElement).not.toBe(screen.getByLabelText("ask the model"));
    expect(useStore.getState().focusRail).toBe("files");
  });
});

/**
 * Reading an answer while it is still arriving. `R-J112`, asked 2026-09-25:
 * *"when I scroll back and want to read the message when it is still
 * generating, it always auto scroll to the bottom, it prevent me from
 * reading"*.
 *
 * jsdom lays nothing out, so the thread is given a height and a `scrollTop`
 * that behaves as a browser's does: clamped to the content, and announcing a
 * change with a `scroll` event — which is what the panel reads to tell a
 * reader's scroll from its own.
 */
describe("reading while it answers", () => {
  const VIEW = 200;
  let content = 1000;
  const question = { id: "q", role: "user" as const, content: "explain the retry loop" };
  const answer = (text: string, pending = true) => ({ id: "a", role: "assistant" as const, content: text, pending });

  function thread() {
    const el = screen.getByLabelText("conversation");
    let top = 0;
    Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => content });
    Object.defineProperty(el, "clientHeight", { configurable: true, get: () => VIEW });
    Object.defineProperty(el, "scrollTop", {
      configurable: true,
      get: () => top,
      set: (v: number) => {
        const next = Math.max(0, Math.min(v, content - VIEW));
        if (next === top) return;
        top = next;
        el.dispatchEvent(new Event("scroll"));
      },
    });
    return el;
  }
  /** More of the answer: the thread grows, and the store says so. */
  const more = (el: HTMLElement, text: string, by = 100) => {
    content += by;
    act(() => useStore.setState({ chat: [question, answer(text)] }));
    return el.scrollTop;
  };
  const end = () => content - VIEW;
  const jump = () => screen.queryByRole("button", { name: /jump to the latest/ });

  beforeEach(() => {
    content = 1000;
    sent.length = 0;
    useStore.setState({
      chat: [question, answer("First,")],
      health: health({}),
      conversationId: null,
      showChatHistory: false,
      send: (m) => void sent.push(m),
    });
    render(<ChatTool />);
  });

  it("follows the answer while you are at the end", () => {
    const el = thread();
    el.scrollTop = end();
    expect(more(el, "First, the loop")).toBe(end());
    expect(more(el, "First, the loop retries")).toBe(end());
    expect(jump()).toBeNull();
  });

  it("scrolling up to read stops it following, and says the answer is still coming", () => {
    const el = thread();
    el.scrollTop = end();
    el.scrollTop = 300;
    expect(more(el, "First, the loop")).toBe(300);
    expect(more(el, "First, the loop retries")).toBe(300);
    expect(jump()).toHaveTextContent("still answering — jump to the latest");
  });

  /** A trackpad scrolls a few pixels at a time; each one has to count. */
  it("a small move up lets go too", () => {
    const el = thread();
    el.scrollTop = end();
    el.scrollTop = end() - 8;
    const held = el.scrollTop;
    expect(more(el, "First, the loop")).toBe(held);
  });

  it("jump to the latest picks it up again", () => {
    const el = thread();
    el.scrollTop = end();
    el.scrollTop = 300;
    more(el, "First, the loop");
    fireEvent.click(jump()!);
    expect(el.scrollTop).toBe(end());
    expect(jump()).toBeNull();
    expect(more(el, "First, the loop retries")).toBe(end());
  });

  it("asking the next question follows its answer, wherever you had scrolled to", () => {
    const el = thread();
    el.scrollTop = end();
    el.scrollTop = 300;
    act(() => useStore.setState({ chat: [question, answer("done.", false)] }));
    const box = screen.getByLabelText("ask the model");
    fireEvent.change(box, { target: { value: "and the backoff?" } });
    content += 80;
    fireEvent.keyDown(box, { key: "Enter" });
    expect(sent.some((m) => m.cmd === "model_chat")).toBe(true);
    expect(el.scrollTop).toBe(end());
    expect(jump()).toBeNull();
  });

  /**
   * Text that shrinks as it re-lays out pulls the view up to the new end —
   * a scroll up that nobody made, and must not read as letting go.
   */
  it("text shrinking at the end is not you scrolling away", () => {
    const el = thread();
    el.scrollTop = end();
    const was = el.scrollTop;
    content -= 100;
    // What the browser does to a view past the new end: clamps it, and says so.
    el.scrollTop = was;
    expect(el.scrollTop).toBe(end());
    expect(more(el, "First, the loop")).toBe(end());
    expect(jump()).toBeNull();
  });
});

/**
 * Stop, asked 2026-10-01 — `R-J118`. The daemon side (hanging up on the model,
 * keeping what arrived) is `model.rs`'s test; what is asserted here is the
 * window's half: a button that exists only while something is out, asks for
 * every answer still out, and a stopped answer that says so and is handled
 * right as context for the next question.
 */
describe("stopping an answer", () => {
  const ask = (text: string) => act(() => useStore.getState().askModel(text));
  const lastChat = () =>
    [...sent].reverse().find((m) => m.cmd === "model_chat") as
      | Extract<ClientMsg, { cmd: "model_chat" }>
      | undefined;
  const reply = (id: string, text: string, stopped: boolean) =>
    act(() =>
      useStore.getState().ingest({ ev: "model_reply", id, text, error: null, model: "m", elapsed_ms: 900, stopped }),
    );

  beforeEach(() => {
    sent.length = 0;
    useStore.setState({
      chat: [],
      health: health({}),
      conversationId: null,
      showChatHistory: false,
      send: (m) => void sent.push(m),
    });
  });

  it("offers stop only while an answer is out, and stops every one", () => {
    render(<ChatTool />);
    expect(screen.queryByTitle(/stop the answer/)).toBeNull();
    ask("first");
    const first = lastChat()!.id;
    ask("second");
    const second = lastChat()!.id;
    fireEvent.click(screen.getByTitle(/stop the answer/));
    expect(sent.filter((m) => m.cmd === "model_stop")).toEqual([
      { cmd: "model_stop", id: first },
      { cmd: "model_stop", id: second },
    ]);
    reply(first, "half an ans", true);
    reply(second, "", true);
    expect(screen.queryByTitle(/stop the answer/)).toBeNull();
  });

  it("marks a stopped answer, and says so when nothing had arrived", () => {
    render(<ChatTool />);
    ask("first");
    const first = lastChat()!.id;
    ask("second");
    const second = lastChat()!.id;
    reply(first, "half an ans", true);
    reply(second, "", true);
    expect(screen.getByText("half an ans")).toBeInTheDocument();
    expect(screen.getByText(/· stopped/)).toBeInTheDocument();
    expect(screen.getByText("stopped before any answer arrived")).toBeInTheDocument();
  });

  /**
   * What you read is what the next question follows on from; a question that
   * got no answer at all goes, both halves, as a failed one does.
   */
  it("keeps a part answer as context and drops an empty one with its question", () => {
    render(<ChatTool />);
    ask("first");
    reply(lastChat()!.id, "half an ans", true);
    ask("second");
    reply(lastChat()!.id, "", true);
    ask("third");
    expect(lastChat()!.messages).toEqual([
      { role: "user", content: "first" },
      { role: "assistant", content: "half an ans" },
      { role: "user", content: "third" },
    ]);
  });
});

/**
 * What the sandboxed agent did, and which of the two answered. `R-O14`.
 *
 * An agent's answer and a plain one must never be mistaken for each other:
 * the line under each says which, and a plain answer to an ask that wanted
 * the agent carries the daemon's reason.
 */
describe("an answer from the agent", () => {
  const ask = (text: string) => act(() => useStore.getState().askModel(text));
  const lastId = () =>
    ([...sent].reverse().find((m) => m.cmd === "model_chat") as Extract<ClientMsg, { cmd: "model_chat" }>).id;

  beforeEach(() => {
    sent.length = 0;
    useStore.setState({
      chat: [],
      health: health({ ask_agent: "opencode 1.19.2 · sandboxed" }),
      conversationId: null,
      showChatHistory: false,
      send: (m) => void sent.push(m),
    });
  });

  it("says before you ask that the agent will answer", () => {
    render(<ChatTool />);
    expect(screen.getByText(/opencode 1\.19\.2 · sandboxed, with qwen3\.8-sglang/)).toBeInTheDocument();
  });

  it("shows each step as it happens, then the answer under them", () => {
    render(<ChatTool />);
    ask("summarise https://example.com");
    const id = lastId();
    act(() =>
      useStore.getState().ingest({ ev: "model_step", id, step: { tool: "webfetch", title: "https://example.com", ok: true } }),
    );
    expect(screen.getByText("https://example.com")).toBeInTheDocument();
    expect(screen.getByText(/working…/)).toBeInTheDocument();
    act(() =>
      useStore.getState().ingest({
        ev: "model_reply", id, text: "A placeholder page.", error: null, model: "Auto", elapsed_ms: 4900,
        agent: "opencode 1.19.2 · sandboxed",
        steps: [
          { tool: "webfetch", title: "https://example.com", ok: true },
          { tool: "bash", title: "touch /home/x", ok: false },
        ],
      }),
    );
    const list = screen.getByRole("list", { name: "what the agent did" });
    expect(list.querySelectorAll("li")).toHaveLength(2);
    expect(screen.getByLabelText("failed")).toBeInTheDocument();
    expect(screen.getByText("A placeholder page.")).toBeInTheDocument();
    expect(screen.getByText(/opencode 1\.19\.2 · sandboxed · Auto · 4\.9s/)).toBeInTheDocument();
  });

  it("says why when plain chat answered instead", () => {
    useStore.setState({ health: health({ ask_agent_note: "answered as plain chat: opencode is not installed" }) });
    render(<ChatTool />);
    ask("what is today's date?");
    act(() =>
      useStore.getState().ingest({
        ev: "model_reply", id: lastId(), text: "Thursday.", error: null, model: "m", elapsed_ms: 800,
        agent_note: "answered as plain chat: opencode is not installed",
      }),
    );
    expect(screen.getByText("answered as plain chat: opencode is not installed")).toBeInTheDocument();
    expect(screen.queryByText(/sandboxed/)).toBeNull();
    expect(screen.queryByRole("list", { name: "what the agent did" })).toBeNull();
  });
});
