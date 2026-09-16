/**
 * What the Git tool window can ask the daemon, as module functions over the
 * store — the `explorer.ts` shape, so every region of the window and its
 * tests reach the same door. `R-D26`.
 *
 * The one door for the log exists because eight things narrow it and every
 * bug in this area was a second call site that carried seven.
 *
 * **The write family is sent from here since `R-D28`** — stage, unstage,
 * discard, commit, branch, switch, stash, resolve — and from nowhere else.
 * Every verb is a message already typed on the wire, guarded daemon-side
 * (loopback or a token, [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md)),
 * and answered by a `git_local_changes` re-broadcast rather than by anything
 * this client models itself. Nothing here reaches a remote but `git_fetch`.
 */

import { emptyGit, useStore } from "@/store";
import type { ResolveSide, SessionId } from "@/wire/types";
import { writeClipboard } from "@/lib/clipboard";
import { hostUrl } from "@/lib/gitTree";
import type { LogQuery } from "@/lib/gitFilter";

/** A page. One past it is fetched daemon-side so `done` costs no second call. */
export const LOG_PAGE = 100;
const RECENTS = 6;

export function queryOf(id: SessionId): LogQuery {
  const g = useStore.getState().git[id] ?? emptyGit();
  return { rev: g.rev, grep: g.grep, author: g.author, path: g.path, pickaxe: g.pickaxe, all: g.all, since: g.since, until: g.until };
}

/**
 * Ask for a page of the log. `skip === 0` starts over — the list empties so
 * the answer replaces it, and the query fields are written so the echo can
 * be matched — while a later page appends. `over` exists because a filter is
 * cleared by a button, and the state it reads has not landed yet.
 */
export function askLog(id: SessionId, skip: number, over: Partial<LogQuery> = {}): void {
  const st = useStore.getState();
  const q: LogQuery = { ...queryOf(id), ...over };
  if (skip === 0) st.patchGit(id, { commits: [], ...q, logAsked: true, logPending: true, done: false });
  else st.patchGit(id, { logPending: true });
  st.send({
    cmd: "git_log",
    session_id: id,
    skip,
    limit: LOG_PAGE,
    rev: q.rev,
    grep: q.grep || null,
    author: q.author || null,
    path: q.path || null,
    pickaxe: q.pickaxe || null,
    all: q.all,
    since: q.since,
    until: q.until,
  });
}

/** Scope the log to one ref, or to every ref again with `null`. Nothing is
 *  checked out. A ref scoped to is remembered as recent. */
export function scopeTo(id: SessionId, rev: string | null, repoRoot?: string | null): void {
  askLog(id, 0, { rev, all: rev === null });
  if (rev && repoRoot) rememberRecent(repoRoot, rev);
}

/** Select a commit: the inspector empties and the diff is asked for. */
export function selectCommit(id: SessionId, sha: string): void {
  const st = useStore.getState();
  st.patchGit(id, {
    selected: sha,
    selectedPath: null,
    selectedFile: null,
    diff: null,
    detail: null,
    conflict: null,
    diffLabel: null,
  });
  st.send({ cmd: "git_show", session_id: id, sha });
}

/** Select a working-tree file: its diff against HEAD, or its three stages
 *  when it is conflicted — a conflict has no ordinary diff worth reading. */
export function selectPath(id: SessionId, path: string, conflicted: boolean): void {
  const st = useStore.getState();
  st.patchGit(id, {
    selected: null,
    selectedPath: path,
    selectedFile: null,
    diff: null,
    detail: null,
    conflict: null,
    diffLabel: null,
  });
  if (conflicted) st.send({ cmd: "git_conflict_file", session_id: id, path });
  else st.send({ cmd: "git_diff_file", session_id: id, path });
}

/** One file of the shown diff, or all of them. No round trip — the files are
 *  already here, and the diff is filtered by this before it is drawn. */
export function selectFile(id: SessionId, path: string | null): void {
  useStore.getState().patchGit(id, { selectedFile: path });
}

/** What merging `branch` would bring — the merge base is resolved daemon-side
 *  and the answer arrives as a range diff. `R-D15`. */
export function compareWith(id: SessionId, branch: string): void {
  const st = useStore.getState();
  st.patchGit(id, { selectedPath: null, selectedFile: null, conflict: null });
  st.send({ cmd: "git_compare", session_id: id, branch });
}

/** Mark a commit; the next one diffed against it is a range. */
export function markRange(id: SessionId, sha: string | null): void {
  useStore.getState().patchGit(id, { rangeMark: sha });
}

export function diffAgainstMark(id: SessionId, to: string): void {
  const st = useStore.getState();
  const from = (st.git[id] ?? emptyGit()).rangeMark;
  if (!from || from === to) return;
  st.patchGit(id, { rangeMark: null, selectedPath: null, selectedFile: null, conflict: null });
  st.send({ cmd: "git_diff_range", session_id: id, from, to });
}

export function showStash(id: SessionId, index: number): void {
  const st = useStore.getState();
  st.patchGit(id, { selected: null, selectedPath: null, selectedFile: null, detail: null, conflict: null, diff: null, diffLabel: `stash@{${index}}` });
  st.send({ cmd: "git_stash_show", session_id: id, index });
}

// ---------------------------------------------------------------------------
// favourites and recents — facts about a repository, kept by its root
// ---------------------------------------------------------------------------

export function favouritesOf(repoRoot: string | null | undefined): string[] {
  if (!repoRoot) return [];
  return useStore.getState().prefs.gitFavourites[repoRoot] ?? [];
}

export function toggleFavourite(repoRoot: string, ref: string): void {
  const { prefs, setPrefs } = useStore.getState();
  const have = prefs.gitFavourites[repoRoot] ?? [];
  const next = have.includes(ref) ? have.filter((r) => r !== ref) : [...have, ref];
  setPrefs({ gitFavourites: { ...prefs.gitFavourites, [repoRoot]: next } });
}

export function recentsOf(repoRoot: string | null | undefined): string[] {
  if (!repoRoot) return [];
  return useStore.getState().prefs.gitRecents[repoRoot] ?? [];
}

export function rememberRecent(repoRoot: string, ref: string): void {
  const { prefs, setPrefs } = useStore.getState();
  const have = prefs.gitRecents[repoRoot] ?? [];
  const next = [ref, ...have.filter((r) => r !== ref)].slice(0, RECENTS);
  setPrefs({ gitRecents: { ...prefs.gitRecents, [repoRoot]: next } });
}

// ---------------------------------------------------------------------------
// the clipboard is the widest part of the pipe
// ---------------------------------------------------------------------------

export function copyText(text: string): void {
  void writeClipboard(text);
}

/**
 * The commit's page on the forge, as a link to copy. Copied rather than
 * opened: the shell's only URL opener refuses anything that is not this
 * machine (`open_local_url`), and a general opener is a capability this
 * window does not have and this row does not add.
 */
export function hostLink(id: SessionId, sha: string): string | null {
  const g = useStore.getState().git[id];
  const remote = g?.refs?.remotes[0]?.url;
  return remote ? hostUrl(remote, sha) : null;
}

// ---------------------------------------------------------------------------
// the write family — R-D28, A26's test
// ---------------------------------------------------------------------------

export function stage(id: SessionId, paths: string[]): void {
  if (paths.length === 0) return;
  useStore.getState().send({ cmd: "git_stage", session_id: id, paths });
}

export function unstage(id: SessionId, paths: string[]): void {
  if (paths.length === 0) return;
  useStore.getState().send({ cmd: "git_unstage", session_id: id, paths });
}

/** The one verb with no undo — git keeps no copy of an untracked file. The
 *  confirmation that names every file lives in the component; this sends. */
export function discard(id: SessionId, paths: string[]): void {
  if (paths.length === 0) return;
  useStore.getState().send({ cmd: "git_discard", session_id: id, paths });
}

/** Commits what is staged, never `-a`. A blank message is refused here
 *  because `git commit -m "   "` succeeds and makes a commit with a blank
 *  subject. */
export function commit(id: SessionId, message: string, amend: boolean, sessionTrailer: boolean): boolean {
  if (!message.trim()) return false;
  useStore.getState().send({ cmd: "git_commit", session_id: id, message, amend, session_trailer: sessionTrailer });
  return true;
}

export function branchCreate(id: SessionId, name: string, switchTo: boolean): void {
  if (!name.trim()) return;
  useStore.getState().send({ cmd: "git_branch_create", session_id: id, name: name.trim(), switch_to: switchTo });
}

/** Moves the worktree. `R-D21`'s warning — name the live sessions, proceed
 *  on confirm — belongs to the caller; see `liveSessionsIn`. */
export function switchTo(id: SessionId, name: string, detach = false): void {
  useStore.getState().send({ cmd: "git_switch", session_id: id, name, detach });
}

export function stashPush(id: SessionId, message: string, includeUntracked: boolean): void {
  useStore.getState().send({ cmd: "git_stash_push", session_id: id, message, include_untracked: includeUntracked });
}

export function stashPop(id: SessionId, index: number): void {
  useStore.getState().send({ cmd: "git_stash_pop", session_id: id, index });
}

export function stashDrop(id: SessionId, index: number): void {
  useStore.getState().send({ cmd: "git_stash_drop", session_id: id, index });
}

export function resolve(id: SessionId, path: string, side: ResolveSide): void {
  useStore.getState().send({ cmd: "git_resolve", session_id: id, path, side });
}

/**
 * The sessions whose agent is running in a worktree — what a branch switch
 * would change files underneath. Git refuses a switch that would *lose*
 * work; what it cannot see is an agent reading files that silently became
 * different content, and that is the one thing this window knows that git
 * does not. `R-D21`.
 */
export function liveSessionsIn(repoRoot: string): { id: string; title: string }[] {
  const { sessions } = useStore.getState();
  return Object.values(sessions)
    .filter((s) => s.alive && (s.repo_root ?? s.cwd) === repoRoot)
    .map((s) => ({ id: s.id, title: s.title || s.id }));
}
