/**
 * The Console tab's ledger: what the window asked git, and what came back.
 * `R-D29`.
 *
 * The window cannot see the argv the daemon ran — it sees the wire command
 * it sent and the answer or refusal it got — so that is what is recorded,
 * honestly labelled. Every `git_*` command is a row when it is sent; the
 * answer fills the row's result; a daemon `error` lands on the latest row
 * still waiting, because the wire's error carries no address. That last is
 * a heuristic and the row says so in its title.
 */

import type { ClientMsg, ServerMsg } from "@/wire/types";

export interface ConsoleEntry {
  /** Milliseconds since the epoch, when the command was sent. */
  at: number;
  /** Send order across every session — two commands in one millisecond
   *  still have a latest, and a refusal has to land on it. */
  seq: number;
  /** The wire command, `git_show`. */
  cmd: string;
  /** Its arguments, `sha fa64b61`, without the session. */
  args: string;
  /** What came back, summarised, or `null` while nothing has. */
  result: string | null;
  /** The daemon's refusal, verbatim. */
  error: string | null;
}

/** Rows kept per session. Old ones fall off the front. */
export const CONSOLE_CAP = 200;

/** The session a command or answer is about, when it is a git one. */
export function gitSessionOf(msg: object): string | null {
  const m = msg as { cmd?: unknown; ev?: unknown; session_id?: unknown };
  const name = typeof m.cmd === "string" ? m.cmd : typeof m.ev === "string" ? m.ev : "";
  if (!name.startsWith("git_") || typeof m.session_id !== "string" || !m.session_id) return null;
  return m.session_id;
}

let seq = 0;

/** A command's arguments as one line — `all · skip 0 · limit 100`. */
export function describeCmd(msg: ClientMsg): string {
  const parts: string[] = [];
  for (const [k, v] of Object.entries(msg)) {
    if (k === "cmd" || k === "session_id" || v === null || v === undefined || v === "") continue;
    if (v === true) parts.push(k);
    else if (v === false) continue;
    else if (Array.isArray(v)) parts.push(`${k} ${v.length === 1 ? v[0] : `${v.length} items`}`);
    else if (typeof v === "string") parts.push(`${k} ${v.length > 60 ? `${v.slice(0, 57)}…` : v}`);
    else parts.push(`${k} ${String(v)}`);
  }
  return parts.join(" · ");
}

/**
 * Which command an answer answers, and what to say about it. An event not
 * listed is not a git answer the console records.
 */
export function answerFor(msg: ServerMsg): { cmds: string[]; summary: string } | null {
  switch (msg.ev) {
    case "git_commits":
      return { cmds: ["git_log"], summary: `${msg.commits.length} commit${msg.commits.length === 1 ? "" : "s"}${msg.done ? " — the end of history" : ""}` };
    case "git_commit_diff": {
      const ins = msg.files.reduce((n, f) => n + f.insertions, 0);
      const del = msg.files.reduce((n, f) => n + f.deletions, 0);
      return { cmds: ["git_show"], summary: `${msg.files.length} file${msg.files.length === 1 ? "" : "s"} · +${ins} −${del}${msg.detail ? "" : " · no header"}` };
    }
    case "git_local_changes":
      // A write verb answers by re-broadcasting the status, so this closes
      // whichever of them asked most recently as well as a plain `git_status`.
      return {
        cmds: ["git_status", "git_stage", "git_unstage", "git_discard", "git_commit", "git_resolve", "git_stash_pop", "git_stash_drop", "git_stash_push", "git_switch", "git_branch_create"],
        summary: `${msg.entries.length} entr${msg.entries.length === 1 ? "y" : "ies"}`,
      };
    case "git_file_diff":
      return { cmds: ["git_diff_file"], summary: `${msg.files.length === 0 ? "nothing to show" : `${msg.files.reduce((n, f) => n + f.hunks.length, 0)} hunks`}` };
    case "git_annotation":
      return { cmds: ["git_blame"], summary: `${msg.lines.length} lines${msg.truncated ? " (capped)" : ""}` };
    case "git_refs_info":
      return {
        cmds: ["git_refs"],
        summary: `${msg.info.branches.length} local · ${msg.info.remote_branches.length} remote · ${msg.info.tags.length} tag${msg.info.tags.length === 1 ? "" : "s"} · HEAD ${msg.info.head ?? "detached"}`,
      };
    case "git_fetched":
      return {
        cmds: ["git_fetch"],
        summary: msg.updates.length === 0 ? "the remotes had nothing new" : msg.updates.join("\n"),
      };
    case "git_stash_list":
      return { cmds: ["git_stashes"], summary: `${msg.stashes.length} stash${msg.stashes.length === 1 ? "" : "es"}` };
    case "git_stash_diff":
      return { cmds: ["git_stash_show"], summary: `${msg.files.length} files` };
    case "git_submodule_list":
      return { cmds: ["git_submodules"], summary: `${msg.submodules.length} submodule${msg.submodules.length === 1 ? "" : "s"}` };
    case "git_range_diff":
      return { cmds: ["git_diff_range", "git_compare"], summary: `${msg.files.length} files · ${msg.from.slice(0, 8)}..${msg.to.slice(0, 8)}` };
    case "git_file_at_rev_content":
      return { cmds: ["git_file_at_rev"], summary: "one file" };
    case "git_reflog_list":
      return { cmds: ["git_reflog"], summary: `${msg.entries.length} entries` };
    case "git_worktree_list":
      return { cmds: ["git_worktrees"], summary: `${msg.worktrees.length} worktree${msg.worktrees.length === 1 ? "" : "s"}` };
    case "git_conflict_stages":
      return { cmds: ["git_conflict_file"], summary: `three stages${msg.truncated ? " (capped)" : ""}` };
    default:
      return null;
  }
}

/** A sent command becomes a waiting row. */
export function pushCmd(rows: readonly ConsoleEntry[], msg: ClientMsg, at = Date.now()): ConsoleEntry[] {
  const next = [...rows, { at, seq: ++seq, cmd: msg.cmd, args: describeCmd(msg), result: null, error: null }];
  return next.length > CONSOLE_CAP ? next.slice(next.length - CONSOLE_CAP) : next;
}

/** An answer closes the latest waiting row of a command it answers. Nothing
 *  changes when no row waits — a broadcast the window never asked for. */
export function attachResult(rows: readonly ConsoleEntry[], cmds: readonly string[], summary: string): ConsoleEntry[] {
  for (let i = rows.length - 1; i >= 0; i--) {
    const r = rows[i];
    if (r.result === null && r.error === null && cmds.includes(r.cmd)) {
      const out = rows.slice();
      out[i] = { ...r, result: summary };
      return out;
    }
  }
  return rows.slice();
}

/** A refusal lands on the latest waiting row, whichever command it was. */
export function attachError(rows: readonly ConsoleEntry[], message: string): ConsoleEntry[] {
  for (let i = rows.length - 1; i >= 0; i--) {
    const r = rows[i];
    if (r.result === null && r.error === null) {
      const out = rows.slice();
      out[i] = { ...r, error: message };
      return out;
    }
  }
  return rows.slice();
}

/** The console as text — for the clipboard, and for another agent. */
export function consoleText(rows: readonly ConsoleEntry[]): string {
  const t = (ms: number) => new Date(ms).toISOString().slice(11, 19);
  return rows
    .map((r) => `${t(r.at)}  ${r.cmd}${r.args ? ` ${r.args}` : ""}\n        ${r.error ? `error: ${r.error}` : (r.result ?? "…")}`)
    .join("\n");
}
