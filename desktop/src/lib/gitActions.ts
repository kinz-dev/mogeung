/**
 * What the Git tool window can ask the daemon, as module functions over the
 * store — the `explorer.ts` shape, so every region of the window and its
 * tests reach the same door. `R-D26`.
 *
 * **Read-only, still.** Nothing here sends a write verb; the family is typed
 * on the wire and `R-D28` is the decision to send it. The one door for the
 * log exists because eight things narrow it and every bug in this area was a
 * second call site that carried seven.
 */

import { emptyGit, useStore } from "@/store";
import type { SessionId } from "@/wire/types";
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
