/**
 * The scratch folder as a tree. `R-L9`.
 *
 * The daemon sends two flat lists — every file as a path, and every folder,
 * including ones with no files in them yet — and this is the only place they
 * become a shape. Kept out of the component because the ordering rules are the
 * interesting part and a test should be able to state them without rendering
 * anything.
 *
 * **Folders before files, each alphabetical; files newest-first inside their
 * folder.** The daemon already sorts files newest-first, which is the order
 * `R-L5` wanted for a flat list and is still right within a folder — but a
 * *tree* sorted by mtime jumps about as you type, because saving a file moves
 * its folder. So folders take a stable alphabetical order and only the files
 * inside them keep the daemon's.
 */

export interface TreeFolder {
  kind: "folder";
  /** Full path from the scratch root — `sql`, `sql/reports`. */
  path: string;
  /** The last segment, which is what the row shows. */
  name: string;
  depth: number;
}

export interface TreeFile {
  kind: "file";
  path: string;
  name: string;
  depth: number;
}

export type TreeRow = TreeFolder | TreeFile;

const parentOf = (path: string): string => {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? "" : path.slice(0, cut);
};

const leafOf = (path: string): string => path.slice(path.lastIndexOf("/") + 1);

/**
 * Flatten the tree into the rows to draw, skipping anything inside a collapsed
 * folder.
 *
 * A flat list of rows rather than a nested render: the panel is a list with
 * indentation, and keeping it flat is what lets the arrow keys move by one
 * visible row without walking a nested structure to find the next one.
 */
export function treeRows(
  names: readonly string[],
  folders: readonly string[],
  collapsed: ReadonlySet<string>,
): TreeRow[] {
  const childFolders = new Map<string, string[]>();
  for (const f of folders) {
    const parent = parentOf(f);
    const at = childFolders.get(parent) ?? [];
    at.push(f);
    childFolders.set(parent, at);
  }
  for (const list of childFolders.values()) list.sort((a, b) => leafOf(a).localeCompare(leafOf(b)));

  // Files keep the daemon's order — newest first — within their own folder.
  const childFiles = new Map<string, string[]>();
  for (const n of names) {
    const parent = parentOf(n);
    const at = childFiles.get(parent) ?? [];
    at.push(n);
    childFiles.set(parent, at);
  }

  const rows: TreeRow[] = [];
  const walk = (parent: string, depth: number) => {
    for (const folder of childFolders.get(parent) ?? []) {
      rows.push({ kind: "folder", path: folder, name: leafOf(folder), depth });
      // A collapsed folder still draws itself; what it hides is below it.
      if (!collapsed.has(folder)) walk(folder, depth + 1);
    }
    for (const file of childFiles.get(parent) ?? []) {
      rows.push({ kind: "file", path: file, name: leafOf(file), depth });
    }
  };
  walk("", 0);
  return rows;
}

/**
 * Where a file would land if it moved into `folder`.
 *
 * `""` is the root. Returns `null` when the move would change nothing, so a
 * caller can leave the wire alone rather than asking the daemon to rename a
 * file onto itself.
 */
export function movedInto(path: string, folder: string): string | null {
  const name = leafOf(path);
  const target = folder === "" ? name : `${folder}/${name}`;
  return target === path ? null : target;
}

/** Every folder a file could move to, itself excluded. */
export function moveTargets(folders: readonly string[], path: string): string[] {
  const here = parentOf(path);
  return ["", ...folders].filter((f) => f !== here);
}
