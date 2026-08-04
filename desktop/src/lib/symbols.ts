/**
 * The symbol outline. `R-B28`.
 *
 * A port of the dispatch in `crates/mogeung-ui/src/symbols.rs`, and of its
 * posture more than its code: **these are line scanners, not parsers.** Feature
 * 0018 chose line scanning over a tree-sitter dependency deliberately — the
 * outline is syntactic and honest about it, because pillar K says this is a
 * viewer and not an IDE. They miss declarations split across lines, they can be
 * fooled by a brace in a string, and they will occasionally over-match. That is
 * the trade for no dependency and code you can read in a sitting; if it ever
 * needs to be *correct* rather than *helpful*, replace it wholesale rather than
 * patching it toward accuracy.
 *
 * Everything is bounded, so a machine-generated file clips instead of hanging:
 * the first `MAX_LINE` characters of a line are scanned, at most `MAX_SYMBOLS`
 * symbols are returned, and a name is cut at `MAX_NAME`.
 *
 * An unknown language gets an **empty** outline, never a guess.
 */

export type SymbolKind = "function" | "method" | "type" | "const" | "module" | "heading" | "other";

export interface Symbol {
  /** 1-based. */
  line: number;
  name: string;
  kind: SymbolKind;
  /** Indent level; in Markdown it is the heading level, `#` = 1. */
  depth: number;
}

const MAX_LINE = 4096;
const MAX_SYMBOLS = 2000;
const MAX_NAME = 256;

/** One recogniser: a declaration-shaped pattern and what it declares. */
interface Rule {
  re: RegExp;
  kind: SymbolKind;
  /** Which capture group holds the name. Defaults to 1. */
  group?: number;
}

const RUST: Rule[] = [
  { re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+|const\s+|unsafe\s+|extern\s+"[^"]*"\s+)*fn\s+([A-Za-z_]\w*)/, kind: "function" },
  { re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|union)\s+([A-Za-z_]\w*)/, kind: "type" },
  { re: /^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+([A-Za-z_]\w*)/, kind: "type" },
  { re: /^\s*impl(?:<[^>]*>)?\s+(.+?)\s*\{/, kind: "type" },
  { re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+([A-Za-z_]\w*)/, kind: "const" },
  { re: /^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)/, kind: "module" },
  { re: /^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+([A-Za-z_]\w*)/, kind: "type" },
];

const PYTHON: Rule[] = [
  { re: /^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)/, kind: "function" },
  { re: /^\s*class\s+([A-Za-z_]\w*)/, kind: "type" },
];

const JS: Rule[] = [
  { re: /^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)/, kind: "function" },
  { re: /^\s*(?:export\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)/, kind: "type" },
  { re: /^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>/, kind: "function" },
  { re: /^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=/, kind: "const" },
  // A method: an identifier, a parameter list, a brace. The keyword blocklist
  // is the whole of the rule's precision — `if (ready) {` has exactly this
  // shape, and a scanner that calls it a method fills the outline of every
  // file with control flow. That is what the test guards.
  {
    re: /^\s{2,}(?:public\s+|private\s+|protected\s+|static\s+|async\s+|get\s+|set\s+)*(?!(?:if|for|while|switch|catch|return|function|class|do|else)\b)([A-Za-z_$][\w$]*)\s*\([^;]*\)\s*\{/,
    kind: "method",
  },
];

const TS_EXTRA: Rule[] = [
  { re: /^\s*(?:export\s+)?interface\s+([A-Za-z_$][\w$]*)/, kind: "type" },
  { re: /^\s*(?:export\s+)?type\s+([A-Za-z_$][\w$]*)/, kind: "type" },
  { re: /^\s*(?:export\s+)?enum\s+([A-Za-z_$][\w$]*)/, kind: "type" },
];

const GO: Rule[] = [
  { re: /^\s*func\s+\([^)]*\)\s*([A-Za-z_]\w*)/, kind: "method" },
  { re: /^\s*func\s+([A-Za-z_]\w*)/, kind: "function" },
  { re: /^\s*type\s+([A-Za-z_]\w*)/, kind: "type" },
  { re: /^\s*(?:const|var)\s+([A-Za-z_]\w*)/, kind: "const" },
];

const JVM: Rule[] = [
  { re: /^\s*(?:public\s+|private\s+|protected\s+|final\s+|abstract\s+|open\s+|data\s+|sealed\s+|static\s+)*(?:class|interface|object|enum)\s+([A-Za-z_]\w*)/, kind: "type" },
  { re: /^\s*(?:public\s+|private\s+|protected\s+|internal\s+|override\s+|suspend\s+)*fun\s+([A-Za-z_]\w*)/, kind: "function" },
  { re: /^\s{2,}(?:public\s+|private\s+|protected\s+|static\s+|final\s+|synchronized\s+)+[\w<>[\]., ?]+\s+([A-Za-z_]\w*)\s*\(/, kind: "method" },
];

const TABLE: Record<string, Rule[]> = {
  rs: RUST,
  py: PYTHON,
  pyi: PYTHON,
  js: JS,
  jsx: JS,
  mjs: JS,
  cjs: JS,
  ts: [...TS_EXTRA, ...JS],
  tsx: [...TS_EXTRA, ...JS],
  mts: [...TS_EXTRA, ...JS],
  cts: [...TS_EXTRA, ...JS],
  go: GO,
  java: JVM,
  kt: JVM,
  kts: JVM,
};

/** `lang` is a file extension, as `languageOf`'s input is. */
export function outline(body: string, lang: string): Symbol[] {
  const key = lang.toLowerCase();
  if (key === "md" || key === "markdown") return outlineMarkdown(body);
  const rules = TABLE[key];
  if (!rules) return [];

  const out: Symbol[] = [];
  const lines = body.split("\n");
  const python = key === "py" || key === "pyi";

  for (let i = 0; i < lines.length && out.length < MAX_SYMBOLS; i++) {
    const line = lines[i].slice(0, MAX_LINE);
    for (const rule of rules) {
      const m = rule.re.exec(line);
      if (!m) continue;
      const name = (m[rule.group ?? 1] ?? "").trim();
      if (!name) break;
      out.push({
        line: i + 1,
        name: name.slice(0, MAX_NAME),
        kind: rule.kind,
        // Indentation is the only nesting signal a line scanner has. It is
        // right for Python by construction and a decent guess elsewhere.
        depth: python ? Math.floor(indentOf(line) / 4) : indentOf(line) > 0 ? 1 : 0,
      });
      break; // First rule that matches wins: the table is ordered by specificity.
    }
  }
  return out;
}

function indentOf(line: string): number {
  let n = 0;
  for (const c of line) {
    if (c === " ") n += 1;
    else if (c === "\t") n += 4;
    else break;
  }
  return n;
}

/**
 * Markdown headings, with the level as the depth.
 *
 * Fenced code is skipped, because ``` blocks in documentation routinely contain
 * `#` comments — a shell transcript would otherwise fill the outline with
 * headings nobody wrote.
 */
function outlineMarkdown(body: string): Symbol[] {
  const out: Symbol[] = [];
  let fenced = false;
  const lines = body.split("\n");
  for (let i = 0; i < lines.length && out.length < MAX_SYMBOLS; i++) {
    const line = lines[i].slice(0, MAX_LINE);
    if (/^\s*(```|~~~)/.test(line)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) continue;
    const m = /^(#{1,6})\s+(.*)$/.exec(line);
    if (!m) continue;
    const name = m[2].trim();
    if (!name) continue;
    out.push({ line: i + 1, name: name.slice(0, MAX_NAME), kind: "heading", depth: m[1].length });
  }
  return out;
}

/** The one-character mark an outline row wears, as in the egui client. */
export function symbolGlyph(kind: SymbolKind): string {
  switch (kind) {
    case "function":
    case "method":
      return "ƒ";
    case "type":
      return "T";
    case "const":
      return "c";
    case "module":
      return "m";
    case "heading":
      return "#";
    default:
      return "·";
  }
}
