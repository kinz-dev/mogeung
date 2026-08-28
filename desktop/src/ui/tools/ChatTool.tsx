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
 * **It stores nothing.** The conversation lives in the store, in memory, and
 * dies with the window — not in `prefs`, so it never reaches `localStorage`,
 * and not on the daemon, so there is no table to forget. That is not
 * minimalism: [A37](../../../docs/product/assumptions.md) is `A27`'s shape with
 * a stronger incumbent — a chat window competing with a chat window — so this
 * is built to be cheap to remove if the fortnight says it is not used.
 * Persistence, when you want it, is `R-L2`'s gesture: copy the thread into a
 * note, which is a document with a lifetime of its own.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { NotebookPen, Trash2 } from "lucide-react";
import { useStore } from "@/store";
import type { ChatMessage } from "@/store";
import { Dim, Empty, IconButton } from "@/ui/primitives";

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

export function ChatTool() {
  const chat = useStore((s) => s.chat);
  const askModel = useStore((s) => s.askModel);
  const clearChat = useStore((s) => s.clearChat);
  const send = useStore((s) => s.send);
  const health = useStore((s) => s.health);
  const [draft, setDraft] = useState("");
  const bottom = useRef<HTMLDivElement>(null);

  const model = health?.model ?? null;
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
    bottom.current?.scrollIntoView({ block: "end" });
  }, [chat]);

  const waiting = useMemo(() => chat.some((m) => m.pending), [chat]);

  const submit = () => {
    if (!draft.trim() || !usable) return;
    askModel(draft);
    setDraft("");
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-2 py-2">
        {chat.length === 0 && (
          <Empty hint={usable ? `${model?.model ?? "the endpoint's default"} at ${model?.host}` : undefined}>
            {usable ? "ask anything" : "no model"}
          </Empty>
        )}
        {chat.map((m) => (
          <Turn key={m.id} m={m} />
        ))}
        <div ref={bottom} />
      </div>

      {/* The reason, verbatim from the daemon. The window does not compose its
          own version: there is one place that decides whether a model may be
          asked, and this renders what it said. */}
      {!usable && (
        <div className="border-t border-[var(--border)] px-2 py-2 text-2xs text-[var(--dim)]">
          {unknown
            ? "this daemon does not know about models — it predates pillar O"
            : (model?.refusal ?? "no model configured")}
        </div>
      )}

      <div className="flex shrink-0 flex-col gap-1 border-t border-[var(--border)] p-2">
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
