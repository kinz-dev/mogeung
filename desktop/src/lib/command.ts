/**
 * Ask for a shell command in words, get one back. `R-O12`, `A41`.
 *
 * **What is left of `R-O12` after the harness.** The row was filed as
 * completion from the corpus of commands your agents ran; `--bin judge
 * --complete` measured that and it predicted **0 of 57** held-out commands
 * against a shell history's 22, because 11,043 of 11,359 agent commands are
 * distinct — they are long, piped and path-specific, which is the least
 * re-typable text there is. `A40` is `REFUTED` and that half came out. This is
 * the half asked for second, and it rests on a different bet
 * ([A41](../../../docs/product/assumptions.md)): that a command written to
 * order beats typing it, with a coding agent already open two panes away.
 *
 * **Composed here rather than in the daemon**, which is
 * [ADR-0034](../../../docs/decisions/0034-the-draft-is-a-chat-ask.md) clause 1
 * again: the ask travels as an ordinary `model_chat`, so the protocol grows no
 * second free-form family and the bind refusal that guards the first covers
 * this without being written twice.
 *
 * **Your command line is never sent.** Only the sentence you deliberately
 * typed, which is the chat panel's shape — a prefix you are part-way through
 * typing can carry `export TOKEN=…`, and that is why nothing here reads it.
 */

/** What the model is asked, and the contract it has to answer under. */
export function commandAsk(question: string, repo: string | null, shell: string): string {
  const where = repo ? `\nThe working directory is \`${repo}\`.` : "";
  return (
    `Write ONE ${shell} command that does this: ${question.trim()}${where}\n\n` +
    "Rules:\n" +
    "- Answer with the command itself and nothing else. No explanation, no " +
    "backticks, no shell prompt, no leading `$`.\n" +
    "- One line. If it genuinely needs several steps, join them with `&&` or a pipe.\n" +
    "- Prefer the plainest thing that works, and tools that are certainly installed.\n" +
    "- Do not invent file or directory names. Where a path is needed and you have " +
    "not been told one, use an obvious placeholder such as `<file>`.\n" +
    "- If the request is not something a single command can do, answer with the " +
    "single word NO.\n"
  );
}

/**
 * Read the command back out of whatever the model wrote.
 *
 * Forgiving about the wrapping and strict about the shape. Models fence code,
 * prefix a `$`, and occasionally explain themselves despite being asked not to
 * — none of which should reach a terminal, and all of which is cheaper to strip
 * here than to litigate in the prompt.
 */
export function parseCommand(text: string): string {
  let out = text.trim();
  // A fenced block: take its body, not its fence, and not any prose around it.
  const fence = out.match(/```(?:[a-z]*\n)?([\s\S]*?)```/i);
  if (fence) out = fence[1].trim();
  // The first non-empty line. Prose after the command is common; prose *before*
  // it would be caught by the fence above or is the model ignoring the contract,
  // and either way the first line is the only thing that could be a command.
  out = out.split("\n").map((l) => l.trim()).find((l) => l.length > 0) ?? "";
  // A copied prompt, which is the most common wrapping of all.
  out = out.replace(/^\$\s+/, "").replace(/^#\s+/, "");
  if (out.toUpperCase() === "NO") return "";
  return out;
}

/**
 * Is this text safe to *offer*? It is never safe to run, and never is.
 *
 * Not a safety check — mogeung does not execute this and could not make it safe
 * if it did. It is a **rendering** decision: a drafted command carrying `rm -rf`
 * or a pipe into a shell is one a reader should see marked, because the whole
 * hazard of this feature is a plausible-looking line arriving one keypress away
 * from a real shell. Deliberately literal, deliberately incomplete, and it says
 * so — a list of patterns is not a security boundary and pretending otherwise
 * would be worse than not marking anything.
 */
export function looksDestructive(command: string): boolean {
  const c = command.toLowerCase();
  return (
    /\brm\s+(-[a-z]*\s+)*-[a-z]*[rf]/.test(c) ||
    /\bmkfs\b|\bdd\s+if=|\b:\(\)\{/.test(c) ||
    /\bchmod\s+-r\b|\bchown\s+-r\b/.test(c) ||
    /curl[^|]*\|\s*(sudo\s+)?(ba|z|)sh/.test(c) ||
    /\bgit\s+(push\s+.*--force|reset\s+--hard)\b/.test(c) ||
    /\bsudo\b/.test(c) ||
    />\s*\/dev\/sd/.test(c)
  );
}
