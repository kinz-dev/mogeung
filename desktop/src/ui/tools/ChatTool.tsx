/**
 * The chat panel. `R-O5`, pillar O.
 *
 * A generic assistant for quick questions, with no repository context. It had
 * no tools either until `R-O14`: since 2026-10-22 an ask is answered by opencode
 * inside a sandbox where the daemon can build one
 * ([ADR-0046](../../../docs/decisions/0046-an-ask-may-be-answered-by-a-sandboxed-agent.md)),
 * so it can read a page you link and work things out, and the steps it took
 * are drawn under its answer. It is the one place in this window that
 * sends a **free-form string** to the daemon, and
 * [ADR-0030](../../../docs/decisions/0030-a-model-reads-the-evidence.md) clause
 * 4 names it as the exception: the daemon refuses it on a bind beyond loopback,
 * with no flag to open it.
 *
 * **It stored nothing, and since `R-O9` it does.** The first cut kept the
 * conversation in memory and let it die with the window — *no table to
 * forget*, which was a real property and is now deliberately gone. Asked for
 * 2026-08-28 and recorded in
 * [ADR-0032](../../../docs/decisions/0032-the-chat-panel-remembers.md): the
 * daemon keeps every **answered** exchange against a conversation id this
 * window mints on the first question, and the history is how you find one
 * again. `chat_history = false` in the config file is the way back.
 *
 * Three gestures, and they are three different lifetimes, which is why none of
 * them is a rename of another:
 *
 * | | what it does |
 * | --- | --- |
 * | **new** | this window forgets which thread it is in. Nothing is deleted. |
 * | **clear** | empties the panel and stays in the thread. Ask again and it continues. |
 * | **✕ in the history** | forgets one conversation on the daemon, for good. |
 *
 * [A37](../../../docs/product/assumptions.md) is unchanged by any of it — this
 * is still `A27`'s shape with a stronger incumbent, still built to be cheap to
 * remove if the fortnight says it is not used, and `R-L2`'s gesture (copy the
 * thread into a note) is still the way to keep something *on purpose* rather
 * than merely to find it again.
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ArrowDown, ExternalLink, History, MessageSquarePlus, NotebookPen, Square, Trash2, X } from "lucide-react";
import { useStore } from "@/store";
import type { ChatMessage } from "@/store";
import type { AgentStep } from "@/wire/types";
import { Button, Dim, Empty, IconButton } from "@/ui/primitives";
import { interactive } from "@/ui/styles";
import { cn } from "@/lib/cn";
import { stamp } from "@/lib/format";
import { openLocalUrl } from "@/lib/tauri";

/**
 * The thread as markdown, for `R-L2`'s note.
 *
 * Verbatim under a rule, with provenance above it — the shape `R-L2` settled on
 * after quoting with `>` turned a table into a quoted table and a fenced block
 * into a quoted fence. What was said has to arrive as what it was.
 */
export function threadAsMarkdown(chat: ChatMessage[], when: Date): string {
  const lines = [`# Chat — ${when.toISOString().slice(0, 16).replace("T", " ")}`, ""];
  for (const m of chat) {
    if (m.pending) continue;
    if (m.error) {
      lines.push(`**mogeung** — ${m.error}`, "");
      continue;
    }
    lines.push(m.role === "user" ? "**You**" : `**${m.model ?? "the model"}**`, "", m.content, "");
  }
  return lines.join("\n");
}

/**
 * How near the end still counts as *at* the end: a line's worth, for a view
 * that is not quite at the bottom because of rounding at a fractional zoom.
 */
const FOLLOW_SLACK_PX = 24;

/**
 * Should the thread keep following new text, given a scroll that just
 * happened? `R-J112`.
 *
 * **At the very end, yes — whichever way it moved.** Text that shrinks as it
 * re-lays out (a fence closing, a table settling) makes the browser pull the
 * view up to the new end, and that must not read as letting go.
 *
 * **Near the end, only if it moved down.** A move up lets go at once, however
 * small: a slow trackpad scroll arrives a few pixels at a time, and with each
 * streamed chunk snapping the view back while it stayed inside the slack, the
 * reader could never get away.
 */
export function stillFollowing(m: { scrollTop: number; scrollHeight: number; clientHeight: number }, lastTop: number): boolean {
  const gap = m.scrollHeight - m.scrollTop - m.clientHeight;
  return gap <= 1 || (gap <= FOLLOW_SLACK_PX && m.scrollTop >= lastTop);
}

/**
 * What the sandboxed agent did, under its answer. `R-O14`, ADR-0046.
 *
 * A record of actions, never a citation: a fetched page is not evidence that
 * the answer is right (ADR-0031 clause 5), so this is drawn as a quiet list
 * and nothing in it links to anything. A failed step is shown as failed —
 * a fetch that did not land explains a vague answer.
 */
function Steps({ steps }: { steps: AgentStep[] }) {
  if (steps.length === 0) return null;
  return (
    <ul aria-label="what the agent did" className="mb-1 space-y-0.5">
      {steps.map((s, i) => (
        <li key={i} className="flex min-w-0 items-baseline gap-1 text-2xs text-[var(--dim)]">
          <span className={s.ok ? "text-[var(--green)]" : "text-[var(--red)]"} aria-label={s.ok ? "done" : "failed"}>
            {s.ok ? "✓" : "✗"}
          </span>
          <span className="shrink-0 font-mono">{s.tool}</span>
          <span className="truncate font-mono" title={s.title}>
            {s.title}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** One row. Split out so the pending and error shapes are visible at a glance. */
function Turn({ m, now }: { m: ChatMessage; now: number }) {
  if (m.role === "user") {
    return (
      <div className="border-l-2 border-[var(--blue)] pl-2 text-sm whitespace-pre-wrap">
        {m.content}
      </div>
    );
  }
  // Nothing has arrived yet. A local model can take a minute to its first
  // token, and showing nothing for a minute is indistinguishable from a broken
  // panel — which is how it gets reported.
  if (m.pending && !m.content) {
    // With the seconds, because a thinking model at high effort reasons for
    // half a minute before it writes anything and streaming cannot shorten
    // that — there is no answer text yet to stream. A number that moves is
    // the difference between *working* and *hung*.
    const secs = m.started ? Math.floor((now - m.started) / 1000) : 0;
    return (
      <div>
        {/* The agent's steps arrive before its text does, and a list that
            grows is what says it is working rather than hung. */}
        <Steps steps={m.steps ?? []} />
        <Dim className="text-sm italic">
          {m.steps?.length ? "working…" : "thinking…"}
          {secs > 0 ? ` ${secs}s` : ""}
        </Dim>
      </div>
    );
  }
  if (m.error) {
    return (
      <div>
        <Steps steps={m.steps ?? []} />
        <div className="rounded-sm border border-[var(--red)] px-2 py-1 text-sm text-[var(--red)]">
          {m.error}
        </div>
        {m.agentNote && <Dim className="mt-1 block text-2xs">{m.agentNote}</Dim>}
      </div>
    );
  }
  // Stopped before a word arrived: said, rather than an empty answer that
  // reads as the model having nothing to say. `R-J118`.
  if (m.stopped && !m.content) {
    return <Dim className="text-sm italic">stopped before any answer arrived</Dim>;
  }
  return (
    <div className="text-sm">
      <Steps steps={m.steps ?? []} />
      {/* Rendered, not raw: an answer is mostly prose with the occasional
          fenced block, and source is not what you asked to read. */}
      <div className="prose-mogeung">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{m.content}</ReactMarkdown>
      </div>
      {/* Only once it has finished. While streaming there is no elapsed time
          worth printing and the model may not be known yet — a line that
          appears, changes and settles is more distracting than one that
          arrives when the answer does. `R-O11`. */}
      {!m.pending && (m.model || m.elapsed_ms !== undefined) && (
        <Dim className="mt-1 block text-2xs">
          {/* Which of the two answered, first: an agent's answer and a plain
              one read differently, and must never be mistaken for each
              other. `R-O14`. */}
          {m.agent ? `${m.agent} · ` : ""}
          {m.model}
          {m.elapsed_ms !== undefined ? ` · ${(m.elapsed_ms / 1000).toFixed(1)}s` : ""}
          {/* Cut off by you, not finished — the difference between an answer
              and the first half of one. `R-J118`. */}
          {m.stopped && <span className="text-[var(--amber)]"> · stopped</span>}
        </Dim>
      )}
      {/* The daemon's own words, under a plain answer to an ask that wanted
          the agent — so nobody reads it as the agent's. */}
      {!m.pending && m.agentNote && <Dim className="mt-0.5 block text-2xs">{m.agentNote}</Dim>}
    </div>
  );
}

/**
 * The kept conversations. `R-O9`.
 *
 * A list of doors rather than a preview: the title is the first thing you
 * asked, which is what you remember a conversation by, and the turn count
 * distinguishes a one-question aside from an afternoon at a glance. Opening
 * one puts the panel **into** that thread — asking again continues it rather
 * than forking a copy, which is the difference between a history and a
 * graveyard.
 */
function HistoryList() {
  const chats = useStore((s) => s.chatHistory);
  const refusal = useStore((s) => s.chatHistoryRefusal);
  const openConversation = useStore((s) => s.openConversation);
  const current = useStore((s) => s.conversationId);
  const send = useStore((s) => s.send);

  if (refusal) {
    return (
      <div className="px-2 py-2 text-2xs text-[var(--dim)]">{refusal}</div>
    );
  }
  // `null` is *not asked yet* and `[]` is *asked, and there are none*. An
  // empty list rendered for the first is a lie that reads as data loss.
  if (chats === null) return <Dim className="block px-2 py-2 text-2xs">looking…</Dim>;
  if (chats.length === 0) {
    return <Empty hint="a conversation is kept once it has an answer">no conversations yet</Empty>;
  }

  return (
    <div className="py-1">
      {chats.map((c) => (
        <div
          key={c.id}
          className={
            "group flex items-center gap-1 px-2 py-1 hover:bg-[var(--hover)]" +
            (c.id === current ? " bg-[var(--hover)]" : "")
          }
        >
          <button
            type="button"
            // `interactive` rather than a hand-rolled ring: the focus state is
            // the whole point in a window driven from the keyboard, and
            // `styles.test.ts` refuses a button without one.
            className={cn(interactive, "min-w-0 flex-1 rounded-sm text-left")}
            title={c.title}
            onClick={() => openConversation(c.id)}
          >
            <div className="truncate text-sm">{c.title}</div>
            <Dim className="block text-2xs">
              {stamp(c.updated)} · {c.turns} turn{c.turns === 1 ? "" : "s"}
            </Dim>
          </button>
          {/*
            Forgetting is per row and permanent — this is the only gesture in
            the panel that deletes from disk, which is why it is the only one
            wearing an ✕ and why its title says *forget* rather than *clear*.
          */}
          <IconButton
            title="forget this conversation — there is no undo"
            onClick={() => send({ cmd: "chat_delete", id: c.id })}
          >
            <X size={12} />
          </IconButton>
        </div>
      ))}
    </div>
  );
}

export function ChatTool() {
  const chat = useStore((s) => s.chat);
  const askModel = useStore((s) => s.askModel);
  const clearChat = useStore((s) => s.clearChat);
  const newConversation = useStore((s) => s.newConversation);
  const showHistory = useStore((s) => s.showChatHistory);
  const send = useStore((s) => s.send);
  const health = useStore((s) => s.health);
  const focusRail = useStore((s) => s.focusRail);
  const [draft, setDraft] = useState("");
  const thread = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  // Where the caret goes once a typed-in newline has rendered. A controlled
  // textarea given a new value puts the caret at the end, which is wrong for a
  // line broken in the middle of a paragraph.
  const caret = useRef<number | null>(null);
  useLayoutEffect(() => {
    if (caret.current === null || !box.current) return;
    box.current.selectionStart = box.current.selectionEnd = caret.current;
    caret.current = null;
  }, [draft]);
  /**
   * Whether the thread follows new text to its end. `R-J112`, asked
   * 2026-09-25: *"when I scroll back and want to read the message when it is
   * still generating, it always auto scroll to the bottom, it prevent me from
   * reading"*.
   *
   * It used to scroll to the end on every change to the thread, and a
   * streamed answer changes it several times a second. Now it follows only
   * while you are at the end: scroll up and it lets you read, and *jump to the
   * latest* — or asking the next question — picks it up again. A ref, not
   * state: it is read when text arrives and never drawn; `away` is the drawn
   * half.
   */
  const following = useRef(true);
  const lastTop = useRef(0);
  const [away, setAway] = useState(false);

  const follow = () => {
    following.current = true;
    setAway(false);
    const el = thread.current;
    if (el) el.scrollTop = el.scrollHeight;
  };

  const onScroll = () => {
    const el = thread.current;
    if (!el) return;
    following.current = stillFollowing(el, lastTop.current);
    lastTop.current = el.scrollTop;
    setAway(!following.current);
  };

  const model = health?.model ?? null;
  const proxy = health?.proxy ?? null;

  /**
   * Where prompts actually go. `R-O10`, ADR-0033 clause 6.
   *
   * With mogeung's own llmproxy in front, `model.host` is `127.0.0.1` and
   * ADR-0031's consent gate passes without asking — while the proxy may be
   * forwarding to a vendor. mogeung cannot gate that (routing is decided per
   * request, and a target can fail over), so a gate would be a promise it
   * could not keep, and the ADR chose an honest sentence instead.
   *
   * **That sentence is no longer on screen.** It was a standing line under the
   * thread until 2026-08-28, when it was removed at the owner's request as
   * clutter. What survives is the admin button's tooltip below. Recording the
   * change here rather than deleting the reasoning with the markup: the ADR
   * still says mogeung reports where prompts go, and a hover is a thinner
   * version of reporting than the clause had in mind.
   */
  const forwardsTo = proxy?.forwards_to ?? [];
  // A daemon built before pillar `O` sends no row at all, which is a different
  // state from "configured and refused" and must not read as an error.
  const unknown = model === null;
  const usable = !!model && model.configured && model.allowed && model.chat_allowed;

  // The health row is what says *why* the box is shut, so ask for it once on
  // open rather than waiting for the next scan to volunteer it.
  useEffect(() => {
    if (unknown) send({ cmd: "fetch_health" });
  }, [unknown, send]);

  /**
   * Opening the panel puts the cursor in the box. `R-J82`.
   *
   * Driven by the store's `focusRail` signal rather than by an `autoFocus` on
   * the textarea, and the difference is the whole point: the panel also mounts
   * when the window starts with it already open, and grabbing the keyboard
   * then would take it from the queue on every launch. This fires only when
   * the toggle says *you just asked for this*.
   *
   * Cleared on the way out so re-opening fires again — without that the value
   * would already be `"chat"` and the effect would not re-run.
   */
  useEffect(() => {
    if (focusRail !== "chat") return;
    // Focus lands on a disabled box as a no-op, which is the right outcome
    // when there is no model: the refusal is what there is to read.
    box.current?.focus();
    useStore.setState({ focusRail: null });
  }, [focusRail]);

  // Before paint, so following text never shows a frame of the old position.
  // The thread itself is scrolled rather than a marker scrolled into view,
  // which would move every scrolling ancestor too — the rail included.
  useLayoutEffect(() => {
    // The list replaces the thread, and a thread shown again starts at its
    // end, following — where the conversation is, whichever one it now is.
    if (showHistory) {
      following.current = true;
      setAway(false);
      return;
    }
    const el = thread.current;
    if (el && following.current) el.scrollTop = el.scrollHeight;
  }, [chat, showHistory]);

  // Re-asked every time the list is opened rather than kept in step: it moves
  // on every answered exchange, including ones asked in another window, and a
  // list that was right when you last looked is the one that sends you hunting
  // for a conversation that is not where it says.
  useEffect(() => {
    if (showHistory) send({ cmd: "chat_list" });
  }, [showHistory, send]);

  const waiting = useMemo(() => chat.some((m) => m.pending), [chat]);
  // Told apart on purpose: *waiting* is nothing yet, *answering* is text
  // already arriving. Before `R-O11` there was only the first of those.
  const streaming = useMemo(() => chat.some((m) => m.pending && !!m.content), [chat]);

  // Ticks only while something is out, so an idle panel re-renders never.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!waiting) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [waiting]);

  const submit = () => {
    if (!draft.trim() || !usable) return;
    // You asked: the answer is what you want to see, wherever you had
    // scrolled to.
    following.current = true;
    setAway(false);
    askModel(draft);
    setDraft("");
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Above the thread rather than beside the send button: these two change
          *which conversation you are in*, and a control that changes the
          subject belongs at the top of it, not in the row you press to talk. */}
      <div className="flex shrink-0 items-center gap-1 border-b border-[var(--border)] px-2 py-1">
        <Dim className="truncate text-2xs">
          {showHistory ? "conversations" : chat.length === 0 ? "new conversation" : "this conversation"}
        </Dim>
        <div className="ml-auto flex items-center gap-1">
          <IconButton
            title="start a new conversation — the current one is kept"
            disabled={chat.length === 0 && !showHistory}
            onClick={newConversation}
          >
            <MessageSquarePlus size={14} />
          </IconButton>
          <IconButton
            title={showHistory ? "back to the conversation" : "find an old conversation"}
            active={showHistory}
            onClick={() => useStore.setState({ showChatHistory: !showHistory })}
          >
            <History size={14} />
          </IconButton>
          {/*
            llmproxy's admin interface binds a **random** port, so without this
            nobody could reach it. When admin is off there is no URL and no
            button, rather than a button that goes nowhere.

            The hosts the proxy may forward to are in the **tooltip**, which is
            all that is left of ADR-0033 clause 6's disclosure — the standing
            line it asked for was removed at the owner's request on 2026-08-28
            as clutter. That is a deliberate weakening and is recorded as one:
            where a prompt goes is now answerable only on a hover, and only
            while admin is enabled.
          */}
          {proxy?.admin_url && (
            <IconButton
              title={
                `open llmproxy's admin interface (${proxy.admin_url})` +
                (forwardsTo.length > 0 ? ` — may forward to ${forwardsTo.join(", ")}` : "")
              }
              onClick={() => void openLocalUrl(proxy.admin_url!)}
            >
              <ExternalLink size={12} />
            </IconButton>
          )}
        </div>
      </div>

      {showHistory ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <HistoryList />
        </div>
      ) : (
        <div className="relative min-h-0 flex-1">
          <div
            ref={thread}
            onScroll={onScroll}
            aria-label="conversation"
            className="h-full space-y-3 overflow-y-auto px-2 py-2"
          >
            {chat.length === 0 && (
              <Empty
                hint={
                  usable
                    ? `${model?.ask_agent ? `${model.ask_agent}, with ` : ""}${model?.model ?? "the endpoint's default"} at ${
                        proxy?.url ? "mogeung's own proxy" : model?.host
                      }`
                    : undefined
                }
              >
                {usable ? "ask anything" : "no model"}
              </Empty>
            )}
            {chat.map((m) => (
              <Turn key={m.id} m={m} now={now} />
            ))}
          </div>
          {away && chat.length > 0 && (
            <button
              type="button"
              onClick={follow}
              className={cn(
                "absolute bottom-2 left-1/2 flex -translate-x-1/2 items-center gap-1 rounded-full border border-[var(--border)] bg-[var(--bg-raised)] px-2 py-0.5 text-2xs text-[var(--text)] shadow-[var(--elev-2)]",
                interactive,
              )}
            >
              <ArrowDown size={11} />
              {streaming ? "still answering — jump to the latest" : "jump to the latest"}
            </button>
          )}
        </div>
      )}

      {/* The reason, verbatim from the daemon. The window does not compose its
          own version: there is one place that decides whether a model may be
          asked, and this renders what it said. */}
      {!usable && !showHistory && (
        <div className="border-t border-[var(--border)] px-2 py-2 text-2xs text-[var(--dim)]">
          {unknown
            ? "this daemon does not know about models — it predates pillar O"
            : (model?.refusal ?? "no model configured")}
        </div>
      )}

      <div
        className={
          "flex shrink-0 flex-col gap-1 border-t border-[var(--border)] p-2" +
          // Kept mounted while the list is up rather than unmounted: a draft
          // you were half-way through must survive a glance at the history,
          // and unmounting the textarea would throw it away.
          (showHistory ? " hidden" : "")
        }
      >
        <textarea
          ref={box}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            // Enter sends and Shift+Enter breaks the line — the way every chat
            // box works, and the reason this is not an `ACTIONS` entry: it must
            // fire here and nowhere else.
            //
            // **Alt+Enter breaks the line too**, asked 2026-10-01. It used to
            // send, being "Enter without Shift". The browser inserts nothing
            // for it, so the newline is put in by hand, over the selection as
            // Shift+Enter's would be.
            if (e.key !== "Enter" || e.shiftKey) return;
            e.preventDefault();
            if (e.altKey) {
              const t = e.currentTarget;
              const at = t.selectionStart;
              caret.current = at + 1;
              setDraft(t.value.slice(0, at) + "\n" + t.value.slice(t.selectionEnd));
              return;
            }
            submit();
          }}
          disabled={!usable}
          spellCheck={false}
          rows={3}
          aria-label="ask the model"
          placeholder={usable ? "ask anything  (Enter to send, Shift+Enter or Alt+Enter for a new line)" : "unavailable"}
          className="w-full resize-none rounded-sm bg-[var(--bg)] px-2 py-1 text-sm outline-none disabled:opacity-50"
        />
        <div className="flex items-center gap-1">
          {waiting && (
            <Dim className="text-2xs">{streaming ? "answering…" : "waiting…"}</Dim>
          )}
          {/*
            Stop, asked 2026-10-01 (`R-J118`). Every answer still out, because
            a second question can be asked while the first is answering and
            "stop" means stop talking. The daemon hangs up on the model, which
            stops it generating, and the reply comes back with what had arrived.
          */}
          {waiting && (
            <Button
              size="sm"
              variant="outline"
              title="stop the answer here — what has arrived is kept"
              onClick={() => {
                for (const m of chat) if (m.pending) send({ cmd: "model_stop", id: m.id });
              }}
            >
              <Square size={10} className="fill-current" />
              stop
            </Button>
          )}
          <div className="ml-auto flex items-center gap-1">
            <IconButton
              title="copy this conversation into a note"
              disabled={chat.length === 0}
              onClick={() => send({ cmd: "note_save", id: "", body: threadAsMarkdown(chat, new Date()) })}
            >
              <NotebookPen size={14} />
            </IconButton>
            <IconButton title="clear" disabled={chat.length === 0} onClick={clearChat}>
              <Trash2 size={14} />
            </IconButton>
          </div>
        </div>
      </div>
    </div>
  );
}
