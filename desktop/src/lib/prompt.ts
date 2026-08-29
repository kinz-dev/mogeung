/**
 * The follow-up prompt. mogeung writes it; **you** paste it.
 *
 * A port of `build_prompt`, and the one feature whose whole design is a refusal:
 * nothing here is ever sent to a session. Sending it would be steering, which is
 * exactly what made v0.1 worse than a terminal
 * ([ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md)). What the
 * window does instead is assemble what you flagged while reading a diff into
 * text you can copy — the clipboard is deliberately the widest part of the pipe,
 * because a human is on the other end of it.
 */

export interface FlaggedHunk {
  sessionId: string;
  path: string;
  header: string;
  note: string;
  /** Only the changed lines, for quoting back. */
  body: string[];
}

export function buildPrompt(note: string, flagged: readonly FlaggedHunk[]): string {
  let out = "";
  if (note.trim()) out += `${note.trim()}\n\n`;
  out += "Please look at the following, which I flagged while reviewing:\n";
  flagged.forEach((f, i) => {
    out += `\n${i + 1}. \`${f.path}\` ${f.header}\n`;
    if (f.note.trim()) out += `   ${f.note.trim()}\n`;
    if (f.body.length > 0) {
      out += "```diff\n";
      for (const l of f.body) out += `${l}\n`;
      out += "```\n";
    }
  });
  return out;
}

/** The changed lines of a hunk — context is what you can look up yourself. */
export function changedLines(lines: readonly string[]): string[] {
  return lines.filter((l) => l.startsWith("+") || l.startsWith("-"));
}

/**
 * The most of one hunk the model is shown when it drafts. `R-O7`.
 *
 * Thirty lines is most of a hunk and all of an ordinary one. The draft is an
 * *instruction*, not a review: the model has to see enough to name what it is
 * pointing at, and nothing beyond that earns its place in the prompt.
 */
export const DRAFT_LINES_PER_HUNK = 30;

/**
 * The whole ask's line budget, shared out between the flagged hunks.
 *
 * `R-O3` bought this lesson at 78 seconds and no answer at all: a per-item cap
 * alone is not a bound, because the item count is unbounded. Flagging is done
 * by hand and rarely runs past a handful — but *rarely* is not a limit, and a
 * draft that fails on the day someone flags forty hunks fails on the day it
 * was most wanted.
 */
export const TOTAL_DRAFT_LINES = 400;

/** Below this a hunk says nothing, so it is quoted whole or not at all. */
export const MIN_DRAFT_LINES_PER_HUNK = 4;

/** How much of each hunk to show, for this many flags. */
export function draftLinesPerHunk(count: number): number {
  if (count === 0) return DRAFT_LINES_PER_HUNK;
  return Math.min(
    DRAFT_LINES_PER_HUNK,
    Math.max(MIN_DRAFT_LINES_PER_HUNK, Math.floor(TOTAL_DRAFT_LINES / count)),
  );
}

/**
 * What is asked of the model so that it drafts the instruction. `R-O7`.
 *
 * **Composed here rather than in the daemon, and that is the decision rather
 * than a convenience** ([ADR-0034](../../../docs/decisions/0034-the-draft-is-a-chat-ask.md)).
 * What travels is a question in the shape of every other question the chat
 * panel asks, so the wire grows no second free-form family — ADR-0031 clause 2
 * keeps `ModelChat` as the single exception — and the daemon relays a string it
 * does not keep and does not know is a prompt.
 *
 * The output contract is the load-bearing part: what comes back is pasted into
 * an agent's terminal, so a model that opens with *"Here is a draft:"* has put
 * a sentence into somebody's session that they did not write. It is told to
 * answer with the instruction and nothing else.
 */
export function draftAsk(note: string, flagged: readonly FlaggedHunk[]): string {
  let s =
    "Below is what a reviewer flagged while reading a diff, in their own words. " +
    "Write ONE instruction they can paste to the coding agent that made this change.\n\n" +
    "Rules:\n" +
    "- Answer with the instruction itself and nothing else. No preamble, no sign-off, " +
    "no explanation of what you did.\n" +
    "- Address the agent directly, and name each file and what to do to it.\n" +
    "- Say only what the reviewer's notes ask for. Do not invent work they did not " +
    "mention, and do not soften an ask into a suggestion.\n" +
    "- Where they left no note, say what you can see the change needs, or leave that " +
    "hunk out — a made-up reason is worse than a shorter instruction.\n" +
    "- Keep it short enough to read in one go.\n\n";
  if (note.trim()) s += `What the reviewer wants done, verbatim:\n${note.trim()}\n\n`;
  s += "What they flagged:\n";
  const budget = draftLinesPerHunk(flagged.length);
  flagged.forEach((f, i) => {
    s += `\n${i + 1}. ${f.path} ${f.header}\n`;
    if (f.note.trim()) s += `   their note: ${f.note.trim()}\n`;
    const body = f.body.slice(0, budget);
    if (body.length > 0) {
      s += "```diff\n";
      for (const l of body) s += `${l}\n`;
      if (f.body.length > body.length) s += `… ${f.body.length - body.length} more changed line(s)\n`;
      s += "```\n";
    }
  });
  return s;
}
