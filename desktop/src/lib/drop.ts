/**
 * A file dropped on the window opens in the Code pane. `R-J94`.
 *
 * **HTML5 drag and drop, deliberately, not Tauri's own.** `R-J20` set
 * `dragDropEnabled: false` in `tauri.conf.json` because wry's native handler
 * answers *copy* to every drag — including dockview's page-internal ones — and
 * never lets the webview see a `drop`, which cost pane rearranging on macOS
 * entirely. Turning it back on to receive files would trade this feature for
 * that one. With it off the drop arrives in the page as an ordinary DOM event
 * and the paths come with it in `text/uri-list`, so nothing native is needed
 * and the window keeps its layout gestures.
 *
 * The parsing and the choosing are pure functions here, because what a drop
 * *means* — which session it belongs to, whether its folder has to be admitted
 * first — is the whole decision, and it is worth stating in tests rather than
 * inside a listener.
 */

import type { Session, SessionId } from "@/wire/types";

/**
 * Is this drag carrying files from outside the window?
 *
 * `Files` is the marker the OS drag has and no page-internal drag does —
 * dockview's tab and group handlers set `text/plain` and nothing else
 * (`abstractDragHandler`), which is what keeps this guard from eating the
 * gestures `R-J20` went to such lengths to preserve. `text/uri-list` is
 * accepted beside it because that is what actually carries the paths, and a
 * WebKit that offers one without the other is not worth losing the feature to.
 */
export function isFileDrag(types: readonly string[] | undefined): boolean {
  if (!types) return false;
  return types.includes("Files") || types.includes("text/uri-list");
}

/** The `DataTransfer` fields this module reads — the whole of its dependency. */
export interface DropData {
  types: readonly string[];
  getData(type: string): string;
}

/**
 * One `file://` URL as a path on this machine, or `null`.
 *
 * The host segment is dropped rather than refused: GTK writes
 * `file:///home/…` and a few sources write `file://localhost/home/…`, and both
 * name the same file. Anything else — an `http:` URL dragged from a browser,
 * a malformed escape — answers `null`, because the only honest thing to do
 * with a path we cannot read is not to open a pane claiming we did.
 */
export function fileUrlToPath(url: string): string | null {
  const trimmed = url.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("/")) return trimmed;
  if (!trimmed.toLowerCase().startsWith("file://")) return null;
  const rest = trimmed.slice("file://".length);
  const slash = rest.indexOf("/");
  if (slash < 0) return null;
  try {
    // The host is everything before the first slash — empty for `file:///…`.
    return decodeURIComponent(rest.slice(slash));
  } catch {
    return null;
  }
}

/**
 * Every path a drop is carrying, in the order they were dragged.
 *
 * `text/uri-list` is the format the drop actually uses (RFC 2483: CRLF-
 * separated, `#` comments), and `text/plain` is the fallback every file
 * manager fills in alongside it. Duplicates are dropped because a source that
 * fills both would otherwise open each file twice.
 */
export function dropPaths(dt: DropData | null): string[] {
  if (!dt) return [];
  const out: string[] = [];
  for (const type of ["text/uri-list", "text/plain"]) {
    let raw = "";
    try {
      raw = dt.getData(type) ?? "";
    } catch {
      // A `DataTransfer` read outside a drop handler throws rather than
      // answering empty, and one unreadable flavour must not lose the other.
      raw = "";
    }
    for (const line of raw.split(/[\r\n]+/)) {
      if (!line || line.startsWith("#")) continue;
      const path = fileUrlToPath(line);
      if (path && !out.includes(path)) out.push(path);
    }
    if (out.length > 0) break;
  }
  return out;
}

/** `/a/b/c.txt` → `/a/b`. The root is its own parent. */
export function parentDir(path: string): string {
  const cut = path.lastIndexOf("/");
  if (cut <= 0) return "/";
  return path.slice(0, cut);
}

/** Is `path` inside `root`, by directory boundary rather than by prefix? */
export function inside(root: string, path: string): boolean {
  if (!root || root === "/") return path.startsWith("/");
  const base = root.endsWith("/") ? root.slice(0, -1) : root;
  return path === base || path.startsWith(`${base}/`);
}

/** A session's own root — the same `repo_root` else `cwd` the daemon uses. */
export function sessionRoot(s: Session): string {
  return s.repo_root || s.cwd || "";
}

export interface DropTarget {
  session: SessionId;
  /** What to ask the daemon for: relative inside the session's own root, absolute outside it. */
  path: string;
  /**
   * The folder that has to join the session's workspace before the file can
   * be read, or `null`. `R-J40` is the only door to a file outside a
   * session's root, and the daemon refuses anything not behind it.
   */
  addDir: string | null;
}

/**
 * Which session opens this file, and what it has to be asked for. `R-J94`.
 *
 * **The session whose root is nearest the file**, so dropping a file from a
 * repository an agent is already working in opens it in *that* session's pane
 * rather than wherever the selection happened to be — with the selection
 * winning ties, because two sessions in one checkout are the ordinary case and
 * the one you are looking at is the one you meant.
 *
 * A file under no session's root is not a refusal. It goes to the selected
 * session with its folder named in `addDir`, which is `R-J40`'s *add a folder
 * to the workspace* — the mechanism that already exists for exactly this, and
 * the only one: the daemon serves an absolute path only from a folder you
 * authorised, and a drag onto the window is that authorisation made with a
 * hand. The cost is that it **persists**, in `~/.mogeung/workspaces.json`
 * keyed by the repository, and is removed in the Files tool rather than by
 * closing the pane.
 */
export function targetForDrop(
  path: string,
  ctx: {
    sessions: Record<string, Session>;
    /** Extra roots per session — the workspace folders, where they are known. */
    extraRoots?: (id: SessionId) => readonly string[];
    selected: SessionId | null;
  },
): DropTarget | null {
  let best: { id: SessionId; root: string; own: string } | null = null;
  for (const s of Object.values(ctx.sessions)) {
    const own = sessionRoot(s);
    if (!own) continue;
    const roots = [own, ...(ctx.extraRoots?.(s.id) ?? [])];
    for (const root of roots) {
      if (!inside(root, path)) continue;
      const better =
        !best ||
        root.length > best.root.length ||
        // Same root, two sessions: the one you are looking at.
        (root.length === best.root.length && s.id === ctx.selected);
      if (better) best = { id: s.id, root, own };
    }
  }

  if (best) {
    const rel = inside(best.own, path) ? path.slice(best.own.replace(/\/$/, "").length + 1) : path;
    return { session: best.id, path: rel || path, addDir: null };
  }

  if (!ctx.selected || !ctx.sessions[ctx.selected]) return null;
  return { session: ctx.selected, path, addDir: parentDir(path) };
}
