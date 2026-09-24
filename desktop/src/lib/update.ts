/**
 * The window updating itself: what the shell answers, and the rules for when
 * to say so. `R-J105`, [feature 0049](../../../docs/features/0049-update-in-place.md).
 *
 * The shell (`src-tauri/src/update.rs`) looks at the branch, builds, installs
 * and restarts; everything it does is a command this file calls. What lives
 * here is the part worth testing without a shell: **when** an update is worth
 * interrupting you for, and how the state moves as a build and an install run.
 */

import { isTauri } from "@/lib/tauri";

export interface InstallRecord {
  commit: string;
  source: string;
  branch: string;
  at: string;
  platform: string;
  window: boolean;
}

export interface UpdateInfo {
  /** This build's commit, or empty. */
  commit: string;
  /** An installed build, rather than one run from a checkout — only those are offered updates. */
  installed: boolean;
  record: InstallRecord | null;
  check: boolean;
  branch: string;
  /** Seconds between looks. */
  interval: number;
  source: string | null;
  platform: "linux" | "macos";
}

export interface Commit {
  sha: string;
  subject: string;
}

export interface UpdateCheck {
  head: string | null;
  /** Seconds since the epoch. */
  head_time: number | null;
  commits: Commit[];
  behind: number | null;
  error: string | null;
}

export type Stage = "build" | "install";

export interface Done {
  stage: Stage;
  ok: boolean;
  code: number | null;
  cancelled: boolean;
  message: string | null;
}

/** How long a branch must have been still before it is offered. */
export const SETTLE_MS = 120_000;

export const short = (sha: string | null | undefined) => (sha && sha.length >= 7 ? sha.slice(0, 7) : "unknown");

/**
 * Is the branch somewhere this build is not, and has it stopped moving?
 *
 * Agents commit in bursts — a feature is often five commits in as many
 * minutes — and an alert for each would be five interruptions for one update.
 * So a new head is offered once it has been the head for two minutes, by this
 * window's watch or by its own commit time, whichever says so first: a commit
 * made an hour ago is settled the moment it is seen.
 */
export function settled(running: string, check: UpdateCheck, firstSeenAt: number, now: number, settleMs = SETTLE_MS): boolean {
  if (!check.head || check.head === running) return false;
  const byWatch = now - firstSeenAt >= settleMs;
  const byCommit = check.head_time !== null && now - check.head_time * 1000 >= settleMs;
  return byWatch || byCommit;
}

/** `3 commits behind main`, `behind main`, `up to date` — the status bar's words. */
export function behindText(running: string, check: UpdateCheck | null, branch: string): string | null {
  if (!check || !check.head) return null;
  if (check.head === running) return `up to date with ${branch}`;
  if (check.behind === null) return `not on ${branch} — ${branch} is ${short(check.head)}`;
  return `${check.behind} commit${check.behind === 1 ? "" : "s"} behind ${branch}`;
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const updateInfo = () => invoke<UpdateInfo>("update_info");
export const updateCheck = () => invoke<UpdateCheck>("update_check");
export const updateBuild = (commit: string) => invoke<void>("update_build", { commit });
export const updateInstall = () => invoke<void>("update_install");
export const updateCancel = () => invoke<void>("update_cancel");
export const updateRestart = () => invoke<void>("update_restart");

/** Listen for the shell's `update:log` and `update:done`; answers an unsubscriber. */
export async function onUpdateEvents(log: (stage: Stage, line: string) => void, done: (d: Done) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const a = await listen<{ stage: Stage; line: string }>("update:log", (e) => log(e.payload.stage, e.payload.line));
  const b = await listen<Done>("update:done", (e) => done(e.payload));
  return () => {
    a();
    b();
  };
}
