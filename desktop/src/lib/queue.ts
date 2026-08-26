import { needsHuman, repoName, sessionLabel, sourceLabel, type AttentionItem, type Session } from "@/wire/types";
import { compareByTagThenLabel } from "@/lib/tags";
import type { Scope, ScopedPrefs } from "@/store/prefs";

import { fmtDur, secsSince } from "@/lib/format";

/**
 * The queue row's explanation, with its clock. `R-J65`.
 *
 * The daemon used to render the duration into `detail` itself, which made
 * every waiting row differ from its own previous value on every tick — so the
 * "has anything changed" gate in front of the queue broadcast could never
 * hold, and 28.5 KB went to every window at the poll rate to move two rows by
 * two seconds. `detail` is static text now and the anchor travels beside it.
 *
 * Rendered here rather than in the panel because the keymap and the panel must
 * agree about what a row says, for the same reason `visibleQueue` exists.
 *
 * Snoozed rows are the exception the daemon cannot serve: their countdown runs
 * to a *deadline*, not from an anchor, and the window already holds it.
 */
export function queueDetail(item: AttentionItem, session: Session | undefined, now = Date.now()): string {
  const until = session?.snoozed_until ? Date.parse(session.snoozed_until) : 0;
  if (until > now) return `${item.detail} — ${fmtDur((until - now) / 1000)} left`;
  if (item.since) return `${item.detail} — ${fmtDur(secsSince(item.since, now))}`;
  return item.detail;
}

/**
 * Which sessions the queue is actually showing, and in what order. `R-J13`.
 *
 * **One definition, because there were two and they disagreed.** The panel
 * rendered a filtered, re-sorted list; the keyboard walked the raw `queue`
 * straight from the daemon. So with a scope on, `j`/`k` and the arrows stepped
 * through sessions that were not on screen, in an order that was not the
 * visible one — reported 2026-08-07 against the `live` filter.
 *
 * Kept as a **pure function over state** rather than a hook, so the keymap can
 * ask the same question from outside React. It lives in `lib/` rather than
 * beside the panel for the reason `panes.ts` already records: the keymap
 * importing a component module is how a cycle starts.
 */
/**
 * Field filters — `repo:`, `branch:`, `file:`, `label:`, `tag:`, `source:` — with bare words
 * falling through to a substring match over the label. A port of `filter.rs`.
 */
export function matchesFilter(
  s: Session,
  label: string | undefined,
  filter: string,
  tag?: string,
): boolean {
  const q = filter.trim().toLowerCase();
  if (!q) return true;
  for (const term of q.split(/\s+/)) {
    const colon = term.indexOf(":");
    const field = colon > 0 ? term.slice(0, colon) : null;
    const value = colon > 0 ? term.slice(colon + 1) : term;
    if (!value) continue;
    let hay: string;
    switch (field) {
      case "repo":
        hay = repoName(s).toLowerCase();
        break;
      case "branch":
        hay = (s.git_branch ?? "").toLowerCase();
        break;
      case "file":
        hay = s.touched_files.join(" ").toLowerCase();
        break;
      case "label":
        hay = (label ?? "").toLowerCase();
        break;
      // The colour by its own name, so "which were the red ones" is a query
      // rather than a scroll. `tag:none` asks the opposite question.
      case "tag":
        hay = (tag ?? "none").toLowerCase();
        break;
      // Which CLI the session belongs to. Worth a term of its own since
      // `R-I15`: with three agent CLIs in one queue, "just the qwen ones" is a
      // question that could not be asked at all, and the raw wire value
      // (`qwen_code`) is not what anyone would type.
      case "source":
      case "agent":
        hay = `${sourceLabel(s.source)} ${s.source}`.toLowerCase();
        break;
      default:
        hay = `${sessionLabel(s)} ${repoName(s)} ${label ?? ""}`.toLowerCase();
    }
    if (!hay.includes(value)) return false;
  }
  return true;
}

export interface VisibleRow {
  item: AttentionItem;
  session: Session;
}

/** The rows the queue shows, in the order it shows them. */
export function visibleQueue(s: {
  queue: AttentionItem[];
  sessions: Record<string, Session>;
  scope: Scope;
  filter: string;
  scoped: ScopedPrefs;
}): VisibleRow[] {
  const rows: VisibleRow[] = [];
  for (const item of s.queue) {
    const session = s.sessions[item.session_id];
    if (!session) continue;
    if (s.scoped.hidden.includes(session.id)) continue;
    if (s.scope === "needs_you" && !needsHuman(item.reason)) continue;
    if (s.scope === "live" && !session.alive) continue;
    if (!matchesFilter(session, s.scoped.labels[session.id], s.filter, s.scoped.tags[session.id])) continue;
    rows.push({ item, session });
  }
  // Pin, then colour, then label — each keeping the attention rank underneath
  // as the tiebreak. See `compareByTagThenLabel` for what that costs: the
  // queue's own claim is that it is ranked by who needs you, and this puts two
  // hand-made keys above the computed one.
  rows.sort((a, b) => compareByTagThenLabel(a.session.id, b.session.id, s.scoped));
  return rows;
}
