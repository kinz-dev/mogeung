/**
 * The client's half of the change-summary protocol.
 *
 * The daemon broadcasts a `ChangeSummary` when a session's diff moves —
 * counts and paths, no hunk bodies — and answers `refresh_change` with the
 * full `Change` on the asking socket only. So the window holds two maps:
 * summaries for everyone (cheap, always current) and full changes only for
 * what someone is actually reading. These two functions keep them honest
 * against each other.
 */

import type { Change, ChangeSummary } from "@/wire/types";

/** A full change, reduced to the summary shape — so `change_updated` keeps
 *  the summary map as current as the summary broadcasts do. */
export function summarize(change: Change): ChangeSummary {
  return {
    files: change.files.map((f) => {
      // Degrade, never throw — the rule the Rust parsers follow, kept here
      // because this runs inside the message handler and a malformed change
      // must cost one summary, not the socket.
      const hunks = f.hunks ?? [];
      return {
        path: f.path,
        status: f.status,
        insertions: f.insertions,
        deletions: f.deletions,
        hunks: hunks.length,
        reviewed_hunks: hunks.filter((h) => h.reviewed).length,
        score: f.score,
      };
    }),
    insertions: change.insertions,
    deletions: change.deletions,
    error: change.error,
  };
}

/**
 * Does the summary describe a different diff than the change we hold?
 *
 * This is what tells the Changes pane to re-fetch hunks: a summary arriving
 * for the session it is showing means the worktree moved under it. Compared
 * by tallies rather than deep equality because the summary *is* the tallies
 * — agreement on all of them is agreement on everything the summary knows.
 */
export function summaryDisagrees(summary: ChangeSummary, change: Change): boolean {
  const held = summarize(change);
  if (
    held.insertions !== summary.insertions ||
    held.deletions !== summary.deletions ||
    held.error !== summary.error ||
    held.files.length !== summary.files.length
  ) {
    return true;
  }
  return held.files.some((f, i) => {
    const s = summary.files[i];
    return (
      f.path !== s.path ||
      f.status !== s.status ||
      f.insertions !== s.insertions ||
      f.deletions !== s.deletions ||
      f.hunks !== s.hunks ||
      f.reviewed_hunks !== s.reviewed_hunks
    );
  });
}
