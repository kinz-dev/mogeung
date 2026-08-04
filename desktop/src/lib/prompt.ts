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
