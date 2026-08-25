/**
 * Which agent CLI a session is. `R-J49`.
 *
 * Asked for 2026-08-25, once the window watched more than one: with Claude
 * Code, Codex and Qwen Code in the same queue, every row said what the session
 * was *doing* and nothing said what it *was*. The Info tool has carried a
 * source chip since the Codex adapter landed, but that is one session at a
 * time and behind a click — the queue is where you are choosing between them.
 *
 * **The icon alone, since the same day.** It shipped as an icon *and* the word,
 * on the argument that three small glyphs are three grey smudges until you have
 * learnt them — and the answer, on seeing it, was that the word was the part to
 * drop. Which is right for the job this does: you are not *reading* which CLI a
 * row is, you are noticing it while reading something else, and a fifth piece
 * of text on a crowded row competes with the four that are there to be read.
 * The name moves to hover, which is where something you need once — while you
 * are learning the glyph — belongs.
 *
 * **Two of them are the real marks**, given by URL the same day and vendored in
 * [`AgentGlyphs`]. That answers the objection the word was covering for: a
 * stand-in has to be learnt, a logo is recognised, and Claude's keeps its own
 * orange rather than taking `sourceColor`'s — which is why the wrapper *sets* a
 * colour and the glyph is free to ignore it. The rest are still lucide
 * stand-ins, chosen for silhouette, and they obey it.
 *
 * **The table is keyed by string rather than by `SessionSource`**, which is the
 * same bet `sourceLabel` makes and for the same reason: the daemon can be newer
 * than this client. An unknown source draws a neutral glyph and names itself on
 * hover, so a CLI this build has never heard of says *what it is* instead of
 * quietly wearing Claude's mark.
 */

import type * as React from "react";
import { Cpu, Gem, Terminal } from "lucide-react";
import { cn } from "@/lib/cn";
import { ClaudeGlyph, QwenGlyph } from "@/ui/AgentGlyphs";
import { sourceColor, sourceLabel, type SessionSource } from "@/wire/types";

/**
 * Sized by the caller, like lucide's own — which is what lets a vendored mark
 * and a stand-in sit in the same table.
 */
export type Glyph = React.ComponentType<{ className?: string }>;

/** `gemini` is here ahead of any adapter for it: it costs a line, and the row
 *  it saves from the fallback is the one someone will read first. */
const ICON: Record<string, Glyph> = {
  claude_code: ClaudeGlyph,
  codex: Terminal,
  qwen_code: QwenGlyph,
  gemini: Gem,
};

export function sourceIcon(source: string): Glyph {
  return ICON[source] ?? Cpu;
}

export function SourceMark({ source, className }: { source: SessionSource; className?: string }) {
  const Icon = sourceIcon(source);
  const label = sourceLabel(source);
  return (
    <span
      // `title` rather than a tooltip component: this sits inside a virtualised
      // list of hundreds of rows, and a portal per row is a cost the browser
      // pays whether or not you ever hover one. It is also the only place the
      // name survives now, so it is not decoration — see the header note.
      title={`a ${label} session`}
      aria-label={`${label} session`}
      role="img"
      className={cn("flex shrink-0 items-center", className)}
      style={{ color: sourceColor(source) }}
    >
      {/* A size up from the icon that sat beside the word: with nothing next to
          it to lend it weight, 10px reads as a speck of dust on the row. */}
      <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden />
    </span>
  );
}
