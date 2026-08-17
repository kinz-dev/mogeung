/**
 * Diff presentation: highlighting, word-level diffing, side-by-side pairing.
 * `R-D4`, `R-D5`, `R-D6`.
 *
 * A port of `crates/mogeung-ui/src/diff.rs`, function for function. All of it
 * is pure — text in, spans out — which is what let the original be tested
 * without a window and lets this one be tested without a DOM.
 *
 * **The highlighter is a tokenizer, not a parser.** No tree-sitter, no
 * grammars, no per-language configuration: it recognises strings, comments,
 * numbers and one shared keyword set. It will mis-colour things. That is an
 * acceptable trade for a reading aid with no dependencies and no build step —
 * and it is why this is not Shiki, which is in the tree for Monaco's sake and
 * would mean an async highlighter and a theme pipeline for the sake of colour
 * on a five-line hunk.
 */

export type Tok = "plain" | "keyword" | "string" | "comment" | "number" | "type";

/**
 * Keywords shared across the languages this tool actually sees. Deliberately
 * one flat set: per-language tables would be more correct and would need
 * language detection, which is a bigger commitment than a reading aid earns.
 */
const KEYWORDS = new Set([
  "fn", "let", "mut", "pub", "use", "mod", "impl", "trait", "struct", "enum", "match", "if",
  "else", "for", "while", "loop", "return", "async", "await", "move", "ref", "where", "const",
  "static", "unsafe", "dyn", "as", "in", "self", "super", "crate", "type", "def", "class",
  "import", "from", "function", "var", "new", "this", "export", "default", "public", "private",
  "protected", "extends", "implements", "interface", "package", "func", "go", "defer", "chan",
  "range", "map", "nil", "None", "True", "False", "true", "false", "null", "undefined", "try",
  "catch", "except", "finally", "raise", "throw", "with", "yield", "lambda", "elif", "not",
  "and", "or", "is", "del", "pass", "break", "continue", "switch", "case", "do", "end",
]);

function isIdent(c: string): boolean {
  return /[\p{L}\p{N}_]/u.test(c);
}

export interface Piece {
  tok: Tok;
  text: string;
}

/**
 * Split one line of code into coloured spans.
 *
 * Line-local: a string or block comment spanning lines is not tracked, because
 * a diff hunk routinely starts in the middle of one and there is no reliable
 * way to know. Mis-colouring a fragment beats mis-colouring the rest of a file.
 */
export function highlight(line: string): Piece[] {
  const out: Piece[] = [];
  const chars = [...line];
  let i = 0;

  const push = (tok: Tok, text: string) => {
    if (!text) return;
    const last = out[out.length - 1];
    // Merge same-coloured neighbours, so the renderer emits far fewer nodes.
    if (last && last.tok === tok) last.text += text;
    else out.push({ tok, text });
  };

  while (i < chars.length) {
    const c = chars[i];

    // Comments run to end of line. Checked on the char array directly — the
    // rest-of-line string is only built once, on a match, not per character.
    if ((c === "/" && chars[i + 1] === "/") || c === "#" || (c === "-" && chars[i + 1] === "-")) {
      push("comment", chars.slice(i).join(""));
      break;
    }

    // Strings, with backslash escapes.
    if (c === '"' || c === "'" || c === "`") {
      const quote = c;
      let s = c;
      i++;
      while (i < chars.length) {
        const d = chars[i];
        s += d;
        i++;
        if (d === "\\" && i < chars.length) {
          s += chars[i];
          i++;
          continue;
        }
        if (d === quote) break;
      }
      push("string", s);
      continue;
    }

    // Numbers, including hex and decimals.
    if (/[0-9]/.test(c)) {
      let s = "";
      while (i < chars.length && /[0-9a-zA-Z._]/.test(chars[i])) {
        s += chars[i];
        i++;
      }
      push("number", s);
      continue;
    }

    // Identifiers: keyword, type-ish (leading capital), or plain.
    if (isIdent(c)) {
      let s = "";
      while (i < chars.length && isIdent(chars[i])) {
        s += chars[i];
        i++;
      }
      push(KEYWORDS.has(s) ? "keyword" : /^\p{Lu}/u.test(s) ? "type" : "plain", s);
      continue;
    }

    push("plain", c);
    i++;
  }
  return out;
}

/** A run of characters shared or not shared between two lines. */
export interface Span {
  text: string;
  changed: boolean;
}

/** A diff line's leading marker and its body. */
function splitMarker(line: string): [string, string] {
  const c = line[0];
  return c === "+" || c === "-" || c === " " ? [line.slice(0, 1), line.slice(1)] : ["", line];
}

/**
 * Split into words, keeping separators as their own units so reassembly is
 * lossless.
 */
function splitWords(s: string): string[] {
  const out: string[] = [];
  let start = 0;
  let prevIdent = false;
  const chars = [...s];
  let at = 0;
  for (let i = 0; i < chars.length; i++) {
    const ident = isIdent(chars[i]);
    if (at > start && ident !== prevIdent) {
      out.push(s.slice(start, at));
      start = at;
    }
    prevIdent = ident;
    at += chars[i].length;
  }
  if (start < s.length) out.push(s.slice(start));
  return out;
}

/**
 * Word-level diff between a removed line and its added counterpart. `R-D5`.
 *
 * Common prefix and suffix are found on *word* boundaries and everything
 * between is marked changed. Not a minimal edit script — a proper Myers diff
 * would highlight less — but it is O(n), never produces the confetti that
 * character-level diffing makes of reformatted code, and answers the only
 * question being asked: which part of this line actually moved?
 */
export function wordDiff(oldLine: string, newLine: string): [Span[], Span[]] {
  // The leading `-`/`+` is a marker, not content. Including it would make every
  // pair differ at position 0, so the common-prefix scan would stop immediately
  // and light the whole line — exactly the noise this removes.
  const [oldSign, oldBody] = splitMarker(oldLine);
  const [newSign, newBody] = splitMarker(newLine);

  const a = splitWords(oldBody);
  const b = splitWords(newBody);

  let pre = 0;
  while (pre < a.length && pre < b.length && a[pre] === b[pre]) pre++;
  let suf = 0;
  while (
    suf < a.length - pre &&
    suf < b.length - pre &&
    a[a.length - 1 - suf] === b[b.length - 1 - suf]
  ) {
    suf++;
  }

  const build = (sign: string, words: string[]): Span[] => {
    const out: Span[] = [];
    const add = (text: string, changed: boolean) => {
      if (!text) return;
      const last = out[out.length - 1];
      if (last && last.changed === changed) last.text += text;
      else out.push({ text, changed });
    };
    add(sign, false);
    add(words.slice(0, pre).join(""), false);
    add(words.slice(pre, words.length - suf).join(""), true);
    add(words.slice(words.length - suf).join(""), false);
    return out;
  };

  return [build(oldSign, a), build(newSign, b)];
}

/** One row of a side-by-side view. `R-D6`. */
export interface Row {
  left: string | null;
  right: string | null;
  /**
   * The line's own number in each file, `null` where that side has no line —
   * and `null` on both when the hunk header could not be read.
   */
  leftNo: number | null;
  rightNo: number | null;
}

/** Where a hunk's two sides begin, in their own files. */
export interface HunkStart {
  old: number;
  new: number;
}

/**
 * The two starting line numbers of an `@@ -a,b +c,d @@` header.
 *
 * `null` rather than a default of `1` for anything it cannot read — a combined
 * merge header (`@@@`) is the case that turns up. A gutter of confidently wrong
 * numbers is worse than no gutter: you would go to the file and edit the wrong
 * line, and nothing on screen would have warned you.
 */
export function hunkStart(header: string): HunkStart | null {
  const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(header);
  return m ? { old: Number(m[1]), new: Number(m[2]) } : null;
}

/**
 * Pair a unified hunk's lines into left/right rows, numbered.
 *
 * Runs of removals and additions are zipped so a modified line sits opposite
 * the line it replaced — which is what makes side-by-side worth having, and
 * what gives the word diff a pair to work on. Unmatched leftovers get a blank
 * opposite.
 *
 * The numbers come from `start`, walked down the hunk: each side advances only
 * on the rows that side actually has, which is what keeps a lopsided run — six
 * lines removed, two added — counting correctly on both.
 */
export function sideBySide(lines: readonly string[], start?: HunkStart | null): Row[] {
  const rows: Row[] = [];
  let dels: string[] = [];
  let adds: string[] = [];
  let old = start?.old ?? 0;
  let now = start?.new ?? 0;

  const push = (left: string | null, right: string | null) => {
    rows.push({
      left,
      right,
      leftNo: start && left !== null ? old++ : null,
      rightNo: start && right !== null ? now++ : null,
    });
  };

  const flush = () => {
    const n = Math.max(dels.length, adds.length);
    for (let i = 0; i < n; i++) push(dels[i] ?? null, adds[i] ?? null);
    dels = [];
    adds = [];
  };

  for (const l of lines) {
    const c = l[0];
    if (c === "-") dels.push(l);
    else if (c === "+") adds.push(l);
    else {
      flush();
      push(l, l);
    }
  }
  flush();
  return rows;
}

/**
 * The unified view's pairing: which added line, if any, a removed line
 * replaces — so a word diff can be drawn without switching to side-by-side.
 *
 * Same zip rule as [`sideBySide`], expressed as an index map because the
 * unified renderer walks the original array and needs to look sideways.
 */
export function pairs(lines: readonly string[]): Map<number, number> {
  const out = new Map<number, number>();
  let dels: number[] = [];
  let adds: number[] = [];
  const flush = () => {
    for (let i = 0; i < Math.min(dels.length, adds.length); i++) {
      out.set(dels[i], adds[i]);
      out.set(adds[i], dels[i]);
    }
    dels = [];
    adds = [];
  };
  lines.forEach((l, i) => {
    const c = l[0];
    if (c === "-") dels.push(i);
    else if (c === "+") adds.push(i);
    else flush();
  });
  flush();
  return out;
}
