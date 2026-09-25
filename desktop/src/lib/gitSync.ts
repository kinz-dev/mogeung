/**
 * A sync — `git fetch` — and the word on how it went. `R-J109`.
 *
 * Asked 2026-09-25: *"a little popup balloon at the bottom corner to show the
 * status of the git command … once git sync complete, popup a little nice info
 * box saying sync complete (and if there is any warn/error msg)"*.
 *
 * Before this, a fetch reported only into the Git pane: the button's spinner
 * and the strip under its toolbar. `Ctrl+T` from anywhere else ran, and said
 * nothing — which from outside reads as *the key only works in the Git pane*,
 * and is how this was reported. And a fetch that failed never cleared the
 * pane's *fetching*, so its button stayed disabled until a reload.
 *
 * This file is the arithmetic, and holds no store: a run starts, finishes or
 * fails, and says what it is. `syncRepo` in `gitActions.ts` starts one and the
 * store's `git_fetched` and `error` cases end it; `SyncBalloon` draws it.
 */

import type { SessionId } from "@/wire/types";

export type SyncPhase = "running" | "done" | "failed";

export interface SyncRun {
  /** One per press, so a second sync of the same repository is a new balloon with its own clock. */
  id: number;
  session: SessionId;
  /** What the balloon calls it: the repository's directory. */
  name: string;
  phase: SyncPhase;
  startedAt: number;
  endedAt: number | null;
  /** Ref updates, in git's own words. Empty means the remotes had nothing new. */
  updates: string[];
  /** What git said that was not a ref update — its `warning:`, `error:` and `hint:` lines. */
  warnings: string[];
  upstream: string | null;
  ahead: number;
  behind: number;
  /** Why it failed, in git's own words. */
  error: string | null;
  /** Put away by hand. Pressing the key again brings a running one back. */
  dismissed: boolean;
}

/** How many balloons can stand at once. One per repository, and few of those. */
export const MAX_RUNS = 3;

/**
 * A running sync older than this can be started again.
 *
 * A fetch has no progress to report and no deadline, and a failure is matched
 * to it by a heuristic (see the store's `error` case) — so a run whose answer
 * went astray would otherwise refuse every later press for as long as the
 * window stays open.
 */
export const STALE_MS = 2 * 60_000;

/** How long a finished balloon stays: longer the more there is to read. */
export const QUIET_MS = 6_000;
export const NEWS_MS = 10_000;
export const TROUBLE_MS = 15_000;

let seq = 0;

/** A fresh run, and the list with it in place of any earlier one for the same session. */
export function beginRun(
  runs: readonly SyncRun[],
  start: { session: SessionId; name: string; at: number },
): SyncRun[] {
  const run: SyncRun = {
    id: ++seq,
    session: start.session,
    name: start.name,
    phase: "running",
    startedAt: start.at,
    endedAt: null,
    updates: [],
    warnings: [],
    upstream: null,
    ahead: 0,
    behind: 0,
    error: null,
    dismissed: false,
  };
  return [...runs.filter((r) => r.session !== start.session), run].slice(-MAX_RUNS);
}

/**
 * Split what git printed into ref updates and everything else.
 *
 * The daemon passes git's stderr through, minus progress and the `From <url>`
 * banner (`git.rs`), and a successful fetch can still carry a `warning:` — a
 * redirected remote, a ref it could not update, `--prune` refusing something.
 * git marks those itself, so the prefix is the whole test.
 */
export function splitFetchLines(lines: readonly string[]): { updates: string[]; warnings: string[] } {
  const updates: string[] = [];
  const warnings: string[] = [];
  for (const line of lines) (/^(warning|error|hint|fatal):/i.test(line) ? warnings : updates).push(line);
  return { updates, warnings };
}

/** The running sync of this session, and whether it is still worth waiting for. */
export function runningFor(runs: readonly SyncRun[], session: SessionId, now: number): SyncRun | null {
  const run = runs.find((r) => r.session === session && r.phase === "running");
  return run && now - run.startedAt < STALE_MS ? run : null;
}

/**
 * A fetch answered. Only a sync this window started has a balloon to finish:
 * `git_fetched` is broadcast, so a fetch started in another window arrives
 * here too, and is that window's to report.
 */
export function finishRun(
  runs: readonly SyncRun[],
  answer: { session_id: SessionId; updates: string[]; upstream: string | null; ahead: number; behind: number },
  at: number,
): SyncRun[] {
  const { updates, warnings } = splitFetchLines(answer.updates);
  return runs.map((r) =>
    r.session === answer.session_id && r.phase === "running"
      ? {
          ...r,
          phase: "done",
          endedAt: at,
          updates,
          warnings,
          upstream: answer.upstream,
          ahead: answer.ahead,
          behind: answer.behind,
        }
      : r,
  );
}

/** A fetch refused. git's message is the whole explanation, so it is kept whole. */
export function failRun(runs: readonly SyncRun[], session: SessionId, message: string, at: number): SyncRun[] {
  return runs.map((r) =>
    r.session === session && r.phase === "running" ? { ...r, phase: "failed", endedAt: at, error: message } : r,
  );
}

/** When a balloon goes: never while it runs, and later the more it has to say. */
export function expiresAt(run: SyncRun): number {
  if (run.phase === "running" || run.endedAt === null) return Infinity;
  if (run.phase === "failed" || run.warnings.length > 0) return run.endedAt + TROUBLE_MS;
  return run.endedAt + (run.updates.length > 0 ? NEWS_MS : QUIET_MS);
}

/** The balloon's first line. */
export function headline(run: SyncRun): string {
  switch (run.phase) {
    case "running":
      return `Syncing ${run.name}…`;
    case "failed":
      return `Sync failed — ${run.name}`;
    case "done":
      return run.warnings.length > 0 ? `Sync complete, with warnings — ${run.name}` : `Sync complete — ${run.name}`;
  }
}

/**
 * Where the branch stands now, which is the question a fetch was asked to
 * answer. A branch with no upstream is an ordinary state, and says so rather
 * than reading as a failure.
 */
export function standing(run: SyncRun): string {
  if (!run.upstream) return "this branch has no upstream to compare with";
  const { ahead, behind, upstream } = run;
  if (ahead === 0 && behind === 0) return `up to date with ${upstream}`;
  // "3 behind origin/main", "2 ahead of origin/main", "3 behind, 2 ahead of origin/main".
  const parts = [behind > 0 ? `${behind} behind` : null, ahead > 0 ? `${ahead} ahead of` : null].filter(Boolean);
  return `${parts.join(", ")} ${upstream}`;
}

/** What moved, in a line: *the remotes had nothing new*, or how many refs did. */
export function moved(run: SyncRun): string {
  const n = run.updates.length;
  return n === 0 ? "the remotes had nothing new" : `${n} ref${n === 1 ? "" : "s"} updated`;
}

/** The last path segment of a repository root, for the balloon's name. */
export function repoName(root: string | null | undefined): string {
  if (!root) return "this repository";
  const trimmed = root.replace(/[\\/]+$/, "");
  return trimmed.slice(Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\")) + 1) || trimmed;
}
