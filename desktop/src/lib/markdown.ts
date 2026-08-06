/**
 * What a Markdown source *reads* as, line by line. `R-B38`.
 *
 * `R-B29` shipped the preview and `Ctrl+F` still searched the source behind it,
 * which finds the wrong things and misses the right ones: you typed `bold` and
 * the source says `**bold**`, you typed a phrase from a table and the source has
 * a `|` in the middle of it, you typed a link's text and the source carries its
 * URL as well. Searching what is on the screen means stripping the syntax —
 * and then knowing which source line each stripped line came from, because the
 * only useful thing to do with a hit is go and edit it.
 *
 * **A stripper, not a parser.** Same posture as `outline()` in `symbols.ts`, and
 * the same warning: this walks lines and removes the marks it recognises. A
 * reference-style link definition, a footnote, an HTML block — those come out
 * looking like themselves. It is honest about the common cases and will miss
 * things, which is a better trade here than a second Markdown implementation
 * that has to agree with `react-markdown` for ever.
 */

export interface RenderedLine {
  /** The text as it reads once the syntax is gone. */
  text: string;
  /** 1-based line in the source it came from. */
  line: number;
}

/** `[label](url)` → `label`, `![alt](url)` → `alt`, `<https://x>` → `https://x`. */
function links(s: string): string {
  return s
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/!?\[([^\]]*)\]\[[^\]]*\]/g, "$1")
    .replace(/<((?:https?|mailto):[^>]+)>/g, "$1");
}

/**
 * Emphasis, code spans and strikethrough.
 *
 * Run after links, because a link's label can be emphasised and taking the
 * asterisks off first would leave `[**a**](x)` half-undone.
 */
function marks(s: string): string {
  return s
    .replace(/`{1,3}([^`]+)`{1,3}/g, "$1")
    .replace(/\*\*\*([^*]+)\*\*\*/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/___([^_]+)___/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/(^|[\s(])_([^_]+)_/g, "$1$2")
    .replace(/~~([^~]+)~~/g, "$1");
}

/** A table row's cells, as the words a reader sees rather than a pipe salad. */
function tableRow(s: string): string {
  return s
    .replace(/^\s*\|/, "")
    .replace(/\|\s*$/, "")
    .split("|")
    .map((c) => c.trim())
    .filter(Boolean)
    .join(" ");
}

const ALIGNMENT_ROW = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

/**
 * The source as it reads, with every line's origin kept.
 *
 * Blank lines and pure structure — a fence marker, a table's alignment row —
 * are dropped rather than emitted empty: they can never match anything, and a
 * hit list is easier to read when its indices mean something.
 *
 * **Inside a fenced code block nothing is stripped.** A fence is shown verbatim
 * by the preview, so `**a**` in a code sample really is on the screen with its
 * asterisks, and removing them would make the search fail on the one kind of
 * text people paste most.
 */
export function renderedLines(src: string): RenderedLine[] {
  const out: RenderedLine[] = [];
  let fenced = false;
  const lines = src.split("\n");

  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    if (/^\s*(```|~~~)/.test(raw)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) {
      if (raw.trim()) out.push({ text: raw.trim(), line: i + 1 });
      continue;
    }
    if (!raw.trim()) continue;
    if (ALIGNMENT_ROW.test(raw) && raw.includes("-")) continue;
    // A horizontal rule reads as nothing at all.
    if (/^\s*([*_-])\s*(\1\s*){2,}$/.test(raw)) continue;

    let text = raw;
    text = text.replace(/^\s{0,3}#{1,6}\s+/, "");
    text = text.replace(/^\s*>\s?/, "");
    text = text.replace(/^\s*([*+-]|\d+[.)])\s+/, "");
    // `- [ ] a task` keeps its words and loses its box.
    text = text.replace(/^\[[ xX]\]\s*/, "");
    if (text.includes("|")) text = tableRow(text);
    text = marks(links(text));
    text = text.replace(/\s+/g, " ").trim();
    if (text) out.push({ text, line: i + 1 });
  }
  return out;
}
