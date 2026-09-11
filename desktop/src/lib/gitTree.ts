/**
 * The two trees the Git tool window draws. `R-D26`.
 *
 * `fileTree` is `R-D18`'s shape rebuilt for the React client — a commit's
 * files as directories, single-child chains flattened to one row, counts
 * rolled up — and `refTree` is the branch pane: HEAD, Local, one node per
 * remote, Tags, each grouped on `/`. Both answer flat lists in draw order
 * with a `depth`, which is what a virtualised or plainly mapped list wants,
 * and both are pure so the shapes are pinned by tests without a window.
 */

import type { BranchInfo, FileChange, RefsInfo, TagInfo } from "@/wire/types";

// ---------------------------------------------------------------------------
// files
// ---------------------------------------------------------------------------

export interface FileNode {
  kind: "dir" | "file";
  /** What the row shows — a flattened chain reads `a/b/c`. */
  label: string;
  /** The directory's full path, or the file's. */
  path: string;
  depth: number;
  /** Files beneath a directory; 1 for a file. */
  count: number;
  insertions: number;
  deletions: number;
  /** The file itself, for a file row. */
  file?: FileChange;
}

interface DirNode {
  dirs: Map<string, DirNode>;
  files: FileChange[];
}

export function fileTree(files: readonly FileChange[]): FileNode[] {
  const root: DirNode = { dirs: new Map(), files: [] };
  for (const f of files) {
    const parts = f.path.split("/");
    let n = root;
    for (let i = 0; i < parts.length - 1; i++) {
      let child = n.dirs.get(parts[i]);
      if (!child) {
        child = { dirs: new Map(), files: [] };
        n.dirs.set(parts[i], child);
      }
      n = child;
    }
    n.files.push(f);
  }
  const totals = (n: DirNode): { count: number; ins: number; del: number } => {
    let count = n.files.length;
    let ins = n.files.reduce((s, f) => s + f.insertions, 0);
    let del = n.files.reduce((s, f) => s + f.deletions, 0);
    for (const d of n.dirs.values()) {
      const t = totals(d);
      count += t.count;
      ins += t.ins;
      del += t.del;
    }
    return { count, ins, del };
  };
  const out: FileNode[] = [];
  const walk = (n: DirNode, depth: number, prefix: string) => {
    const dirs = [...n.dirs.entries()].sort((a, b) => a[0].localeCompare(b[0]));
    for (const [name, child] of dirs) {
      // A directory with one subdirectory and no files of its own is a
      // corridor, not a room: it collapses into its child's row.
      let label = name;
      let path = prefix ? `${prefix}/${name}` : name;
      let node = child;
      while (node.files.length === 0 && node.dirs.size === 1) {
        const [k, v] = [...node.dirs.entries()][0];
        label = `${label}/${k}`;
        path = `${path}/${k}`;
        node = v;
      }
      const t = totals(node);
      out.push({ kind: "dir", label, path, depth, count: t.count, insertions: t.ins, deletions: t.del });
      walk(node, depth + 1, path);
    }
    const fs = [...n.files].sort((a, b) => a.path.localeCompare(b.path));
    for (const f of fs) {
      out.push({
        kind: "file",
        label: f.path.slice(f.path.lastIndexOf("/") + 1),
        path: f.path,
        depth,
        count: 1,
        insertions: f.insertions,
        deletions: f.deletions,
        file: f,
      });
    }
  };
  walk(root, 0, "");
  return out;
}

// ---------------------------------------------------------------------------
// refs
// ---------------------------------------------------------------------------

export type RefKind = "head" | "group" | "remote" | "dir" | "branch" | "tag";

export interface RefNode {
  kind: RefKind;
  /** Unique in the tree; what a collapsed set records. */
  key: string;
  /** What the row shows: the last segment for a branch, the group's name. */
  label: string;
  depth: number;
  /** The full ref name a click scopes the log to — `main`, `origin/main`, a
   *  tag's name — absent on groups and directories. */
  ref?: string;
  /** How many leaves sit beneath a group, remote or directory. */
  count: number;
  branch?: BranchInfo;
  tag?: TagInfo;
  favourite?: boolean;
}

interface Bucket {
  dirs: Map<string, Bucket>;
  leaves: { label: string; ref: string; branch?: BranchInfo; tag?: TagInfo }[];
}

const bucket = (): Bucket => ({ dirs: new Map(), leaves: [] });

function place(b: Bucket, segments: string[], leaf: Bucket["leaves"][number]) {
  let n = b;
  for (const s of segments) {
    let child = n.dirs.get(s);
    if (!child) {
      child = bucket();
      n.dirs.set(s, child);
    }
    n = child;
  }
  n.leaves.push(leaf);
}

function leaves(b: Bucket): number {
  let n = b.leaves.length;
  for (const d of b.dirs.values()) n += leaves(d);
  return n;
}

/**
 * Whether a bucket has anything matching the query. A directory stays when
 * any branch beneath it matches — the ancestors are the way to read where a
 * match sits.
 */
function keep(b: Bucket, q: string): boolean {
  if (!q) return true;
  if (b.leaves.some((l) => l.ref.toLowerCase().includes(q))) return true;
  for (const d of b.dirs.values()) if (keep(d, q)) return true;
  return false;
}

function emit(
  out: RefNode[],
  b: Bucket,
  depth: number,
  keyPrefix: string,
  q: string,
  fav: ReadonlySet<string>,
  kind: "branch" | "tag",
) {
  const dirs = [...b.dirs.entries()].sort((a, c) => a[0].localeCompare(c[0]));
  for (const [name, child] of dirs) {
    if (!keep(child, q)) continue;
    const key = `${keyPrefix}/${name}`;
    out.push({ kind: "dir", key, label: name, depth, count: leaves(child) });
    emit(out, child, depth + 1, key, q, fav, kind);
  }
  // Leaves keep the daemon's order — most recently committed first — except
  // that the current branch leads and favourites follow it.
  const ls = b.leaves.filter((l) => !q || l.ref.toLowerCase().includes(q));
  const rank = (l: Bucket["leaves"][number]) => (l.branch?.current ? 0 : fav.has(l.ref) ? 1 : 2);
  ls.sort((a, c) => rank(a) - rank(c));
  for (const l of ls) {
    out.push({
      kind,
      key: `${keyPrefix}/${l.label}`,
      label: l.label,
      depth,
      ref: l.ref,
      count: 1,
      branch: l.branch,
      tag: l.tag,
      favourite: fav.has(l.ref),
    });
  }
}

/**
 * The branch pane as a flat list. `query` narrows it (case-insensitive, on
 * the full ref name); `favourites` are the starred refs.
 */
export function refTree(refs: RefsInfo, query: string, favourites: readonly string[]): RefNode[] {
  const q = query.trim().toLowerCase();
  const fav = new Set(favourites);
  const out: RefNode[] = [];

  if (!q) {
    out.push({
      kind: "head",
      key: "head",
      label: refs.head ?? `detached at ${refs.head_sha}`,
      depth: 0,
      ref: refs.head ?? refs.head_sha,
      count: 1,
    });
  }

  const local = bucket();
  for (const b of refs.branches) {
    const parts = b.name.split("/");
    place(local, parts.slice(0, -1), { label: parts[parts.length - 1], ref: b.name, branch: b });
  }
  if (keep(local, q)) {
    out.push({ kind: "group", key: "local", label: "Local", depth: 0, count: leaves(local) });
    emit(out, local, 1, "local", q, fav, "branch");
  }

  // One node per remote. A remote-tracking name is `origin/x/y`; the first
  // segment is the remote when a remote of that name exists, and otherwise
  // still the best guess there is.
  const remotes = new Map<string, Bucket>();
  const known = new Set(refs.remotes.map((r) => r.name));
  for (const b of refs.remote_branches) {
    const parts = b.name.split("/");
    const remote = known.has(parts[0]) || parts.length > 1 ? parts[0] : "(remote)";
    const rest = known.has(parts[0]) || parts.length > 1 ? parts.slice(1) : parts;
    let r = remotes.get(remote);
    if (!r) {
      r = bucket();
      remotes.set(remote, r);
    }
    place(r, rest.slice(0, -1), { label: rest[rest.length - 1], ref: b.name, branch: b });
  }
  const remoteNames = [...remotes.keys()].sort();
  const remoteTotal = remoteNames.reduce((n, r) => n + leaves(remotes.get(r)!), 0);
  if (remoteTotal > 0 && remoteNames.some((r) => keep(remotes.get(r)!, q))) {
    out.push({ kind: "group", key: "remote", label: "Remote", depth: 0, count: remoteTotal });
    for (const name of remoteNames) {
      const r = remotes.get(name)!;
      if (!keep(r, q)) continue;
      out.push({ kind: "remote", key: `remote/${name}`, label: name, depth: 1, count: leaves(r) });
      emit(out, r, 2, `remote/${name}`, q, fav, "branch");
    }
  }

  const tags = bucket();
  for (const t of refs.tags) {
    const parts = t.name.split("/");
    place(tags, parts.slice(0, -1), { label: parts[parts.length - 1], ref: t.name, tag: t });
  }
  if (!q || keep(tags, q)) {
    out.push({ kind: "group", key: "tags", label: "Tags", depth: 0, count: leaves(tags) });
    emit(out, tags, 1, "tags", q, fav, "tag");
  }
  return out;
}

/**
 * The rows still on screen once the collapsed keys are honoured: a collapsed
 * node hides every row deeper than it until the next row at its depth or
 * shallower. Works for both trees because both are depth-flattened.
 */
export function visible<T extends { key?: string; path?: string; depth: number }>(
  rows: readonly T[],
  collapsed: ReadonlySet<string>,
  keyOf: (r: T) => string,
): T[] {
  const out: T[] = [];
  let hideBelow: number | null = null;
  for (const r of rows) {
    if (hideBelow !== null) {
      if (r.depth > hideBelow) continue;
      hideBelow = null;
    }
    out.push(r);
    if (collapsed.has(keyOf(r))) hideBelow = r.depth;
  }
  return out;
}

// ---------------------------------------------------------------------------
// the forge
// ---------------------------------------------------------------------------

/**
 * A commit's page on the host, when the remote's URL has a shape this
 * recognises — GitHub, GitLab and Bitbucket, over ssh or https. `null`
 * otherwise, and the menu item is absent rather than guessed. `R-D11`.
 */
export function hostUrl(remote: string, sha: string): string | null {
  let m = remote.trim().match(/^(?:git@|ssh:\/\/git@)([^:/]+)[:/]([^/]+)\/([^/]+?)(?:\.git)?\/?$/);
  if (!m) m = remote.trim().match(/^https?:\/\/([^/]+)\/([^/]+)\/([^/]+?)(?:\.git)?\/?$/);
  if (!m) return null;
  const [, host, org, repo] = m;
  const bare = host.toLowerCase();
  if (bare.includes("bitbucket")) return `https://${host}/${org}/${repo}/commits/${sha}`;
  if (bare.includes("github") || bare.includes("gitlab")) return `https://${host}/${org}/${repo}/commit/${sha}`;
  return null;
}
