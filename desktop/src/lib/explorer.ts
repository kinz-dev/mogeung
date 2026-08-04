/**
 * The worktree, as the client models it.
 *
 * A port of `crates/mogeung-ui/src/explorer.rs` plus `explorer_fetch`. The one
 * rule worth restating: **file bodies are never persisted**. Open tabs, pins
 * and expanded directories are view state; the content behind them is the
 * daemon's, and every restore re-asks. That is what keeps this a projection
 * rather than a second copy of the worktree.
 */

import { useStore, emptyExplorer, type ExplorerState, type FileTab } from "@/store";
import type { SessionId } from "@/wire/types";

/**
 * Ask for whatever the explorer wants and lacks: the root, every expanded
 * directory without a listing, the active tab without a body.
 *
 * One door for all fetching, which is what makes restore, reveal, refresh and
 * a plain click all re-fetch the same way. Called from the render of both the
 * Files tool and the Code pane, which is why every branch guards on `pending`:
 * with both on screen it runs twice and must send once.
 */
export function explorerFetch(id: SessionId): void {
  const { explorer, send, patchExplorer } = useStore.getState();
  const st = explorer[id] ?? emptyExplorer();

  const wants = ["", ...st.expanded].filter((d) => !(d in st.dirs) && !st.pending.includes(d));
  if (wants.length > 0) {
    patchExplorer(id, { pending: [...st.pending, ...wants] });
    for (const path of wants) send({ cmd: "list_dir", session_id: id, path });
  }

  const wantBodies: string[] = [];
  for (const group of [0, 1] as const) {
    const i = st.active[group];
    if (i === null) continue;
    const tab = st.open[i];
    if (!tab || tab.content !== null) continue;
    // Keyed by (path, rev): the worktree twin of a path must not swallow a
    // revision tab's request, nor the other way round.
    const key = tab.rev ? `${tab.rev}:${tab.path}` : tab.path;
    if (st.pendingFiles.includes(key) || wantBodies.includes(key)) continue;
    wantBodies.push(key);
    if (tab.rev === null) send({ cmd: "fetch_file", session_id: id, path: tab.path });
    else send({ cmd: "git_file_at_rev", session_id: id, sha: tab.rev, path: tab.path });
  }
  if (wantBodies.length > 0) {
    patchExplorer(id, { pendingFiles: [...st.pendingFiles, ...wantBodies] });
  }
}

export function toggleDir(id: SessionId, path: string): void {
  const { explorer, patchExplorer } = useStore.getState();
  const st = explorer[id] ?? emptyExplorer();
  patchExplorer(id, {
    expanded: st.expanded.includes(path) ? st.expanded.filter((p) => p !== path) : [...st.expanded, path],
  });
}

/**
 * Open a file in the Code pane.
 *
 * `pin` decides whether it stays: a file asked for **by name** — a search hit,
 * a diff row, go-to-file — is pinned, and a file merely browsed past reuses the
 * one unpinned preview tab per side. That is IntelliJ's rule, and it is what
 * stops a morning of clicking through a tree leaving forty tabs open.
 */
export function openFile(id: SessionId, path: string, opts: { pin?: boolean; line?: number; rev?: string | null } = {}): void {
  const { explorer, patchExplorer } = useStore.getState();
  const st = explorer[id] ?? emptyExplorer();
  const rev = opts.rev ?? null;
  const group = st.focus;

  const existing = st.open.findIndex((t) => t.path === path && t.rev === rev && t.group === group);
  if (existing >= 0) {
    const open = st.open.map((t, i) =>
      i === existing ? { ...t, pinned: t.pinned || !!opts.pin, gotoLine: opts.line ?? null } : t,
    );
    const active: ExplorerState["active"] = [...st.active];
    active[group] = existing;
    patchExplorer(id, { open, active, reveal: path });
    return;
  }

  const tab: FileTab = {
    path,
    rev,
    pinned: !!opts.pin,
    group,
    content: null,
    truncated: false,
    gotoLine: opts.line ?? null,
  };

  // Reuse this side's unpinned preview tab, if it has one.
  const previewAt = st.open.findIndex((t) => t.group === group && !t.pinned);
  let open: FileTab[];
  let index: number;
  if (!opts.pin && previewAt >= 0) {
    open = st.open.map((t, i) => (i === previewAt ? tab : t));
    index = previewAt;
  } else {
    open = [...st.open, tab];
    index = open.length - 1;
  }
  const active: ExplorerState["active"] = [...st.active];
  active[group] = index;
  patchExplorer(id, { open, active, reveal: path });
}

export function closeTab(id: SessionId, index: number): void {
  const { explorer, patchExplorer } = useStore.getState();
  const st = explorer[id] ?? emptyExplorer();
  const open = st.open.filter((_, i) => i !== index);
  const remap = (a: number | null): number | null => {
    if (a === null) return null;
    if (a === index) return null;
    return a > index ? a - 1 : a;
  };
  const active: ExplorerState["active"] = [remap(st.active[0]), remap(st.active[1])];
  // A side that lost its active tab falls back to any tab it still has,
  // rather than going blank while holding files.
  for (const g of [0, 1] as const) {
    if (active[g] === null) {
      const any = open.findIndex((t) => t.group === g);
      active[g] = any >= 0 ? any : null;
    }
  }
  patchExplorer(id, { open, active });
}

/** Every ancestor of a path, root first — what `reveal` has to expand. */
export function ancestors(path: string): string[] {
  const parts = path.split("/").slice(0, -1);
  const out: string[] = [];
  let acc = "";
  for (const p of parts) {
    acc = acc ? `${acc}/${p}` : p;
    out.push(acc);
  }
  return out;
}

/** Expand everything above a path so it can be scrolled to. */
export function reveal(id: SessionId, path: string): void {
  const { explorer, patchExplorer } = useStore.getState();
  const st = explorer[id] ?? emptyExplorer();
  const want = ancestors(path).filter((d) => !st.expanded.includes(d));
  patchExplorer(id, { expanded: [...st.expanded, ...want], reveal: path });
}

export function join(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name;
}

export interface FileFilter {
  /** Nothing typed: everything matches and the caller shows its normal view. */
  empty: boolean;
  /** False when the pattern did not compile and is being read literally. */
  regex: boolean;
  test(path: string): boolean;
}

/**
 * The Files filter: a regex, tried against **the name and the whole path**.
 *
 * Both, because anchors mean different things on each and a filter that picked
 * one would be wrong half the time: `^use` is how you ask for names beginning
 * with `use`, `^src/` is how you ask for a subtree, and `\.tsx$` wants the name.
 * Testing the path alone loses the first; testing the name alone loses the
 * second.
 *
 * **Smart case** — a lowercase pattern ignores case, one containing an
 * uppercase letter does not. The same rule the daemon's content search uses, so
 * the two boxes do not disagree about what `Foo` means.
 *
 * **An unfinished pattern is not an error.** Typing `foo(` on the way to
 * `foo(bar)` leaves a regex that does not compile, and a filter that answered
 * "nothing matches" mid-keystroke would be indistinguishable from a query with
 * no hits. So an uncompilable pattern falls back to a literal substring of
 * itself, and `regex: false` lets the UI say which of the two it did.
 */
export function fileFilter(query: string): FileFilter {
  const q = query.trim();
  if (!q) return { empty: true, regex: false, test: () => true };

  // Smart case, computed once rather than per candidate.
  const flags = /[A-Z]/.test(q) ? "" : "i";
  let re: RegExp | null = null;
  try {
    re = new RegExp(q, flags);
  } catch {
    re = null;
  }

  if (re) {
    const rx = re;
    return {
      empty: false,
      regex: true,
      test: (path) => rx.test(path) || rx.test(basename(path)),
    };
  }

  const needle = flags === "i" ? q.toLowerCase() : q;
  return {
    empty: false,
    regex: false,
    test: (path) => (flags === "i" ? path.toLowerCase() : path).includes(needle),
  };
}

export function basename(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? path : path.slice(at + 1);
}

/** The Monaco language id for a path. Extension-based, like the Rust side. */
export function languageOf(path: string): string {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  const map: Record<string, string> = {
    rs: "rust",
    ts: "typescript",
    tsx: "typescript",
    js: "javascript",
    jsx: "javascript",
    py: "python",
    go: "go",
    java: "java",
    kt: "kotlin",
    c: "c",
    h: "c",
    cc: "cpp",
    cpp: "cpp",
    hpp: "cpp",
    cs: "csharp",
    rb: "ruby",
    php: "php",
    swift: "swift",
    sh: "shell",
    bash: "shell",
    zsh: "shell",
    fish: "shell",
    sql: "sql",
    html: "html",
    css: "css",
    scss: "scss",
    json: "json",
    yaml: "yaml",
    yml: "yaml",
    toml: "toml",
    xml: "xml",
    md: "markdown",
    lock: "toml",
    dockerfile: "dockerfile",
  };
  if (path.endsWith("Dockerfile")) return "dockerfile";
  if (path.endsWith("Makefile")) return "makefile";
  return map[ext] ?? "plaintext";
}
