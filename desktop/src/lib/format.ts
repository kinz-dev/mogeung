/** Time and size formatting, ported from the Rust side so both agree. */

/** `attention::fmt_dur` — the queue's own duration spelling. */
export function fmtDur(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(s / 3600)}h${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}m`;
}

/** Seconds between an RFC 3339 stamp and now, never negative. */
export function secsSince(ts: string | null | undefined, now = Date.now()): number {
  if (!ts) return 0;
  return Math.max(0, Math.floor((now - Date.parse(ts)) / 1000));
}

/** `HH:MM` in local time. */
export function clock(ts: string): string {
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** `MM-DD HH:MM` in local time — the stamp search results and blame use. */
export function stamp(ts: string | number): string {
  const d = typeof ts === "number" ? new Date(ts * 1000) : new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** Thousands separators. Tokens are counted, never priced — ADR-0005. */
export function num(n: number): string {
  return n.toLocaleString();
}

/** `1.2k`, `3.4M` — for column-tight token counts. */
export function compact(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** The basename, for a path shown where the whole thing will not fit. */
export function base(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? path : path.slice(i + 1);
}

/** `…/parent/` — the one directory of context worth spending width on. */
export function shortDir(path: string): string {
  const i = path.lastIndexOf("/");
  if (i < 0) return "";
  const dir = path.slice(0, i);
  const parts = dir.split("/");
  return parts.length > 1 ? `…/${parts[parts.length - 1]}/` : `${dir}/`;
}

/**
 * A directory shortened from the *front* — `…/projects/mogeung`.
 *
 * The end of a path is the part that identifies it; `/Users/someone/work/cl…`
 * identifies nothing. So whole leading segments come off until it fits, and the
 * caller keeps the untouched path on hover.
 *
 * Segments come off whole, never mid-name, because a half-cut directory name
 * reads as a directory that exists. The one exception is a leaf longer than the
 * budget on its own: there is nothing left to drop, so it is returned in full
 * and the caller's CSS truncation is what stops it — a rare path paying for its
 * own length rather than every path paying for it.
 */
export function dirTail(path: string, max = 34): string {
  if (path.length <= max) return path;
  const parts = path.split("/").filter(Boolean);
  const kept: string[] = [];
  for (let i = parts.length - 1; i >= 0; i--) {
    const next = [parts[i], ...kept];
    if (kept.length > 0 && next.join("/").length + 2 > max) break;
    kept.unshift(parts[i]);
  }
  return `…/${kept.join("/")}`;
}

/**
 * One line of it, whitespace collapsed.
 *
 * A search result is a row, and a matched transcript turn is prose with
 * newlines in it — pasted straight in, one hit would be six rows and the list
 * would stop being scannable at the first paragraph.
 */
export function oneLine(s: string, n = 240): string {
  const flat = s.split(/\s+/).filter(Boolean).join(" ");
  return flat.length <= n ? flat : `${flat.slice(0, n)}…`;
}
