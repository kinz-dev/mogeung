/**
 * The chat panel. `R-O5`, pillar O.
 *
 * A generic assistant for quick questions — no repository context and no tools,
 * which is the whole of the first cut. It is the one place in this window that
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

import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { History, MessageSquarePlus, NotebookPen, Trash2, X } from "lucide-react";
import { useStore } from "@/store";
import type { ChatMessage } from "@/store";
import { Dim, Empty, IconButton, Mono } from "@/ui/primitives";
import { interactive } from "@/ui/styles";
import { cn } from "@/lib/cn";
import { stamp } from "@/lib/format";

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

/** One row. Split out so the pending and error shapes are visible at a glance. */
function Turn({ m }: { m: ChatMessage }) {
  if (m.role === "user") {
    return (
      <div className="border-l-2 border-[var(--blue)] pl-2 text-sm whitespace-pre-wrap">
        {m.content}
      </div>
    );
  }
  if (m.pending) {
    // A local model can take a minute. Showing nothing for a minute is
    // indistinguishable from a broken panel, which is how it gets reported.
    return <Dim className="text-sm italic">thinking…</Dim>;
  }
  if (m.error) {
    return (
      <div className="rounded-sm border border-[var(--red)] px-2 py-1 text-sm text-[var(--red)]">
        {m.error}
      </div>
    );
  }
  return (
    <div className="text-sm">
      {/* Rendered, not raw: an answer is mostly prose with the occasional
          fenced block, and source is not what you asked to read. */}
      <div className="prose-mogeung">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{m.content}</ReactMarkdown>
      </div>
      {(m.model || m.elapsed_ms !== undefined) && (
        <Dim className="mt-1 block text-2xs">
          {m.model}
          {m.elapsed_ms !== undefined ? ` · ${(m.elapsed_ms / 1000).toFixed(1)}s` : ""}
        </Dim>
      )}
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
  const [draft, setDraft] = useState("");
  const bottom = useRef<HTMLDivElement>(null);

  const model = health?.model ?? null;
  const proxy = health?.proxy ?? null;

  /**
   * Where prompts actually go. `R-O10`.
   *
   * With mogeung's own llmproxy in front, `model.host` is `127.0.0.1` and
   * ADR-0031's consent gate passes without asking — while the proxy may be
   * forwarding to a vendor. mogeung cannot gate that (routing is decided per
   * request, and a target can fail over), so a gate would be a promise it
   * could not keep. It says where instead, and says it *here* rather than only
   * in the Health view, because this is the box you type the prompt into.
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

  useEffect(() => {
    // Not while the list is up: scrolling a thread you are not looking at
    // steals the list's own scroll position on every keystroke elsewhere.
    if (!showHistory) bottom.current?.scrollIntoView({ block: "end" });
  }, [chat, showHistory]);

  // Re-asked every time the list is opened rather than kept in step: it moves
  // on every answered exchange, including ones asked in another window, and a
  // list that was right when you last looked is the one that sends you hunting
  // for a conversation that is not where it says.
  useEffect(() => {
    if (showHistory) send({ cmd: "chat_list" });
  }, [showHistory, send]);

  const waiting = useMemo(() => chat.some((m) => m.pending), [chat]);

  const submit = () => {
    if (!draft.trim() || !usable) return;
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
        </div>
      </div>

      {showHistory ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <HistoryList />
        </div>
      ) : (
        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-2 py-2">
          {chat.length === 0 && (
            <Empty
              hint={
                usable
                  ? `${model?.model ?? "the endpoint's default"} at ${
                      proxy?.url ? "mogeung's own proxy" : model?.host
                    }`
                  : undefined
              }
            >
              {usable ? "ask anything" : "no model"}
            </Empty>
          )}
          {chat.map((m) => (
            <Turn key={m.id} m={m} />
          ))}
          <div ref={bottom} />
        </div>
      )}

      {/* The reason, verbatim from the daemon. The window does not compose its
          own version: there is one place that decides whether a model may be
          asked, and this renders what it said. */}
      {/* Not a warning and not styled as one: adding a forwarding provider to
          the proxy config is a deliberate act, and nagging about a decision
          somebody made on purpose is how a line stops being read. It is here
          rather than only in the Health view because this is the box the
          prompt is typed into. */}
      {usable && !showHistory && forwardsTo.length > 0 && (
        <div className="border-t border-[var(--border)] px-2 py-1 text-2xs text-[var(--dim)]">
          via mogeung's proxy — may forward to <Mono>{forwardsTo.join(", ")}</Mono>
        </div>
      )}

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
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            // Enter sends and Shift+Enter breaks the line — the way every chat
            // box works, and the reason this is not an `ACTIONS` entry: it must
            // fire here and nowhere else.
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          disabled={!usable}
          spellCheck={false}
          rows={3}
          aria-label="ask the model"
          placeholder={usable ? "ask anything  (Enter to send)" : "unavailable"}
          className="w-full resize-none rounded-sm bg-[var(--bg)] px-2 py-1 text-sm outline-none disabled:opacity-50"
        />
        <div className="flex items-center gap-1">
          {waiting && <Dim className="text-2xs">waiting…</Dim>}
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
