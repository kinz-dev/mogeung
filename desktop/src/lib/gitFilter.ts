/**
 * What narrows the log, as one value, and the small pure helpers around it.
 * `R-D26`.
 *
 * Eight things narrow a log now — message, author, path, pickaxe, the branch
 * scope, all-refs, and a date range — and every bug this pane has had was a
 * call site that carried a subset. So there is one type, one parser for the
 * search box, and one description of what is in force.
 */

import type { FileChange } from "@/wire/types";

export interface LogQuery {
  rev: string | null;
  grep: string;
  author: string;
  path: string;
  pickaxe: string;
  all: boolean;
  since: number | null;
  until: number | null;
}

export const emptyQuery: LogQuery = {
  rev: null,
  grep: "",
  author: "",
  path: "",
  pickaxe: "",
  all: true,
  since: null,
  until: null,
};

/** A whole hex word of at least seven characters: what a hash typed into
 *  the search box looks like, and what a message rarely does. */
export function looksLikeSha(text: string): boolean {
  return /^[0-9a-f]{7,40}$/i.test(text.trim());
}

/**
 * The search box's syntax — `author:`, `path:` and `find:` prefixes pull a
 * field out of one line of text and the rest is the message filter. `R-D12`'s
 * shape, kept because it is the keyboard's way to every field the dropdowns
 * reach by mouse. Quotes are not parsed; a space ends a prefixed word.
 */
export function parseSearch(text: string): Pick<LogQuery, "grep" | "author" | "path" | "pickaxe"> {
  const out = { grep: "", author: "", path: "", pickaxe: "" };
  const rest: string[] = [];
  for (const word of text.trim().split(/\s+/).filter(Boolean)) {
    const m = word.match(/^(author|path|find):(.*)$/);
    if (!m || !m[2]) {
      rest.push(word);
      continue;
    }
    if (m[1] === "author") out.author = m[2];
    else if (m[1] === "path") out.path = m[2];
    else out.pickaxe = m[2];
  }
  out.grep = rest.join(" ");
  return out;
}

/** The one line of text that reproduces the field filters in the box. */
export function searchText(q: Pick<LogQuery, "grep" | "author" | "path" | "pickaxe">): string {
  return [q.grep, q.author && `author:${q.author}`, q.path && `path:${q.path}`, q.pickaxe && `find:${q.pickaxe}`]
    .filter(Boolean)
    .join(" ");
}

/** What the list on screen answers, named so it can be doubted in time. */
export function inForce(q: LogQuery, now = new Date()): string[] {
  const out: string[] = [];
  if (q.rev) out.push(`branch ${q.rev}`);
  if (q.grep) out.push(`message ~ ${q.grep}`);
  if (q.author) out.push(`author ~ ${q.author}`);
  if (q.path) out.push(q.path);
  if (q.pickaxe) out.push(`-S ${q.pickaxe}`);
  if (q.since !== null || q.until !== null) out.push(rangeLabel(q.since, q.until, now));
  return out;
}

/** Any text filter hides the graph: lanes drawn over a filtered subset join
 *  dots that are not adjacent, and a lying graph is worse than none. `R-D13`. */
export function hidesGraph(q: LogQuery): boolean {
  return !!(q.grep || q.author || q.path || q.pickaxe);
}

export type DatePreset = "any" | "today" | "yesterday" | "week" | "month";

export const DATE_PRESETS: { value: DatePreset; label: string }[] = [
  { value: "any", label: "any time" },
  { value: "today", label: "today" },
  { value: "yesterday", label: "yesterday" },
  { value: "week", label: "last 7 days" },
  { value: "month", label: "last 30 days" },
];

/** A preset as a commit-date range in unix seconds, local midnights. */
export function presetRange(p: DatePreset, now = new Date()): { since: number | null; until: number | null } {
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = 86_400_000;
  const secs = (d: Date) => Math.floor(d.getTime() / 1000);
  switch (p) {
    case "today":
      return { since: secs(midnight), until: null };
    case "yesterday":
      return { since: secs(new Date(midnight.getTime() - day)), until: secs(midnight) - 1 };
    case "week":
      return { since: secs(new Date(now.getTime() - 7 * day)), until: null };
    case "month":
      return { since: secs(new Date(now.getTime() - 30 * day)), until: null };
    default:
      return { since: null, until: null };
  }
}

/** The preset a range came from, if one did — the dropdown shows it by name
 *  rather than as two epochs. */
export function presetOf(since: number | null, until: number | null, now = new Date()): DatePreset | null {
  for (const p of DATE_PRESETS) {
    const r = presetRange(p.value, now);
    if (r.since === since && r.until === until) return p.value;
  }
  return null;
}

function rangeLabel(since: number | null, until: number | null, now: Date): string {
  const p = presetOf(since, until, now);
  if (p) return DATE_PRESETS.find((d) => d.value === p)!.label;
  const day = (t: number) => new Date(t * 1000).toISOString().slice(0, 10);
  if (since !== null && until !== null) return `${day(since)} → ${day(until)}`;
  if (since !== null) return `since ${day(since)}`;
  return `until ${day(until!)}`;
}

/**
 * Git's `%G?` letter as a word, or `null` when there is nothing to say. An
 * unsigned commit is the ordinary case and is said quietly; the letters
 * that mean a signature exists and is wrong are the ones worth reading.
 */
export function signatureWord(letter: string | undefined | null): { word: string; bad: boolean } | null {
  switch ((letter ?? "").trim()) {
    case "G":
      return { word: "signed", bad: false };
    case "U":
      return { word: "signed, key untrusted", bad: false };
    case "X":
      return { word: "signed, signature expired", bad: true };
    case "Y":
      return { word: "signed, key expired", bad: true };
    case "R":
      return { word: "signed, key revoked", bad: true };
    case "B":
      return { word: "bad signature", bad: true };
    case "E":
      return { word: "signature not checked", bad: false };
    case "N":
      return { word: "unsigned", bad: false };
    default:
      return null;
  }
}

/**
 * Patch text rebuilt from the hunks already on screen — `R-D13`'s "copy as
 * patch", which needed no wire because the daemon keeps the raw headers and
 * the signed lines. `/dev/null` ends for an added or deleted file, so what
 * comes out is something `git apply` and another agent can both read.
 */
export function patchText(files: readonly FileChange[]): string {
  let out = "";
  for (const f of files) {
    const from = f.status === "added" ? "/dev/null" : `a/${f.old_path ?? f.path}`;
    const to = f.status === "deleted" ? "/dev/null" : `b/${f.path}`;
    out += `diff --git a/${f.old_path ?? f.path} b/${f.path}\n`;
    if (f.old_path && f.old_path !== f.path) out += `rename from ${f.old_path}\nrename to ${f.path}\n`;
    out += `--- ${from}\n+++ ${to}\n`;
    for (const h of f.hunks) {
      out += `${h.header}\n`;
      for (const l of h.lines) out += `${l}\n`;
    }
  }
  return out;
}
