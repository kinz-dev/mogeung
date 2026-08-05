/**
 * Turning a conversation into something you keep. `R-L2`, `R-L3`.
 *
 * A transcript belongs to a session and dies with it — forget the session and
 * the reading is gone. A note is the opposite: the `notes` table carries
 * `session_id` and `seq` as **tags rather than parents** (no foreign key, no
 * cascade, stated in `store.rs`), so a note survives the thing it was written
 * against. That asymmetry is the whole feature. Copying a turn into a note is
 * how you move something across that line, deliberately, at the moment you
 * realise it matters — which is exactly when a bookmark is not enough.
 *
 * Everything here is a pure string function. The window sends the result as an
 * ordinary `note_save`; the daemon has no idea a conversation was involved,
 * and nothing new was added to the wire for this.
 */

import type { Session, TranscriptEvent } from "@/wire/types";
import { sessionLabel } from "@/wire/types";

/** How a turn is labelled in a note. Short, because it prefixes every block. */
function speaker(ev: TranscriptEvent): string {
  switch (ev.kind.t) {
    case "user_prompt":
      return "You";
    case "assistant_text":
      return "Agent";
    case "thinking":
      return "Agent (thinking)";
    case "tool_use":
      return `Tool · ${ev.kind.name}`;
    case "tool_result":
      return ev.kind.is_error ? "Tool result (error)" : "Tool result";
    case "result":
      return "Result";
    case "notice":
      return "Notice";
    case "init":
      return "Session start";
  }
}

/** The text a turn actually carries, or `null` for the ones that carry none. */
export function textOf(ev: TranscriptEvent): string | null {
  switch (ev.kind.t) {
    case "user_prompt":
    case "assistant_text":
    case "thinking":
      return ev.kind.text;
    case "tool_use":
      return ev.kind.summary;
    case "tool_result":
      return ev.kind.preview;
    case "result":
      return ev.kind.text;
    case "notice":
      return ev.kind.message;
    case "init":
      // Deliberately nothing: `init` is metadata, and a note that opens with
      // "model: …, tool_count: 42" buries the reason you copied anything.
      return null;
  }
}

/**
 * An ISO-ish stamp that sorts and reads. Local, because a note is personal.
 *
 * Takes both spellings the window has: a transcript event's `ts` is an ISO
 * string off the wire, and `Date.now()` is epoch milliseconds.
 */
function when(ts: string | number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * One turn, as a note.
 *
 * The heading carries the session and the turn so the note still means
 * something after the transcript is gone — which is the case this exists for.
 * A blockquote rather than a fence: the text is usually prose and often
 * already contains fences of its own, and nesting those is how a note ends up
 * rendering as one enormous code block.
 */
export function noteFromTurn(session: Session | null, ev: TranscriptEvent): string {
  const label = session ? sessionLabel(session) : ev.session_id.slice(0, 8);
  const body = textOf(ev) ?? "";
  const quoted = body
    .split("\n")
    .map((l) => (l.trim() ? `> ${l}` : ">"))
    .join("\n");
  return [`# ${label} · turn ${ev.seq}`, "", `**${speaker(ev)}** · ${when(ev.ts)}`, "", quoted, ""].join("\n");
}

/** How many turns a copied conversation carries before it is cut. */
export const CONVERSATION_LIMIT = 200;

/**
 * A whole conversation, as a note.
 *
 * **Capped, and it says so in the note.** A long session runs to thousands of
 * turns and hundreds of kilobytes; pasting that into a scratchpad produces
 * something nobody will ever read and a mirror file nobody will ever open. The
 * last `CONVERSATION_LIMIT` turns are kept — the end of a conversation is the
 * part you are copying it for — and the note states what was dropped rather
 * than quietly appearing complete. A note that lies about being the whole
 * conversation is worse than no note.
 */
export function noteFromConversation(session: Session | null, events: TranscriptEvent[]): string {
  const label = session ? sessionLabel(session) : "conversation";
  const carried = events.filter((e) => textOf(e) !== null);
  const dropped = Math.max(0, carried.length - CONVERSATION_LIMIT);
  const kept = carried.slice(-CONVERSATION_LIMIT);

  const head = [`# ${label}`, "", `Copied ${when(Date.now())} · ${kept.length} turn(s)`];
  if (dropped > 0) {
    head.push("", `*Earlier ${dropped} turn(s) not copied — this is the tail of the conversation.*`);
  }

  const blocks = kept.map((ev) => {
    const body = textOf(ev) ?? "";
    const quoted = body
      .split("\n")
      .map((l) => (l.trim() ? `> ${l}` : ">"))
      .join("\n");
    return [`### ${speaker(ev)} · turn ${ev.seq}`, "", quoted].join("\n");
  });

  return [...head, "", ...blocks, ""].join("\n");
}
