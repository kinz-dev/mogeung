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

export interface QueueView {
  queue: AttentionItem[];
  sessions: Record<string, Session>;
  scope: Scope;
  filter: string;
  scoped: ScopedPrefs;
  /**
   * Hide sessions mogeung cannot attach to, in the two working scopes.
   * `R-J93`. Absent means *do not* — the rule is the panel's setting, not
   * this function's opinion.
   */
  tmuxOnly?: boolean;
}

/**
 * Is there a tmux pane behind this session? `R-J93`.
 *
 * `tmux_target` is the daemon's answer to *can this be attached to*, and it
 * is the only one there is: it is set by walking the session's process up to
 * a pane and cleared the moment the session stops being live, so a session
 * started in iTerm2 or from a bare terminal has never had one, and one that
 * has ended no longer does. [ADR-0010](../../../docs/decisions/0010-attach-a-terminal-never-own-one.md)
 * is why that matters here rather than being trivia: a row with no pane is a
 * row whose Agent pane cannot open, so *"go and see"* is not on offer.
 */
export function underTmux(s: Session): boolean {
  return !!s.tmux_target;
}

function passes(s: QueueView, item: AttentionItem, session: Session, tmuxOnly: boolean): boolean {
  if (s.scoped.hidden.includes(session.id)) return false;
  if (s.scope === "needs_you" && !needsHuman(item.reason)) return false;
  if (s.scope === "live" && !session.alive) return false;
  // **The working scopes only**, which is the whole shape of `R-J93`: `all`
  // is what "everything mogeung has seen" has always meant, so it stays the
  // one place nothing is dropped and the answer to "where did that row go".
  if (tmuxOnly && s.scope !== "all" && !underTmux(session)) return false;
  return matchesFilter(session, s.scoped.labels[session.id], s.filter, s.scoped.tags[session.id]);
}

/** The rows the queue shows, in the order it shows them. */
export function visibleQueue(s: QueueView): VisibleRow[] {
  const rows: VisibleRow[] = [];
  for (const item of s.queue) {
    const session = s.sessions[item.session_id];
    if (!session) continue;
    if (!passes(s, item, session, !!s.tmuxOnly)) continue;
    rows.push({ item, session });
  }
  // Pin, then colour, then label — each keeping the attention rank underneath
  // as the tiebreak. See `compareByTagThenLabel` for what that costs: the
  // queue's own claim is that it is ranked by who needs you, and this puts two
  // hand-made keys above the computed one.
  rows.sort((a, b) => compareByTagThenLabel(a.session.id, b.session.id, s.scoped));
  return rows;
}

/**
 * How many rows the tmux rule is holding back right now. `R-J93`.
 *
 * Counted rather than inferred, because the panel has to *say* it: a filter
 * that removes rows silently is indistinguishable from a daemon that has
 * stopped reporting them, and this product's one claim is that it tells you
 * who needs you. Everything else is applied first, so this is the number that
 * would appear if the rule alone were lifted — not the number of sessions on
 * the machine without a pane.
 */
export function hiddenByTmux(s: QueueView): number {
  if (!s.tmuxOnly || s.scope === "all") return 0;
  let n = 0;
  for (const item of s.queue) {
    const session = s.sessions[item.session_id];
    if (!session) continue;
    if (underTmux(session)) continue;
    if (passes({ ...s, tmuxOnly: false }, item, session, false)) n++;
  }
  return n;
}
