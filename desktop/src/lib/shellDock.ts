/**
 * The terminal panel's own tile tree. `R-B56`.
 *
 * The shells across the bottom are arranged the way the panes in the centre
 * are — tabbed, split, dragged — but in a **second** dockview rather than the
 * first. [ADR-0017](../../../docs/decisions/0017-the-rail-is-chrome.md) still
 * holds: the panel is chrome and a shell is never a tile in the centre. What
 * changed is only what the panel does with the room it already had.
 *
 * Two stores, and which one is authoritative is the point of this file:
 *
 * - **`scoped.shells` says which shells exist.** It is in the preferences, per
 *   machine, and it is what `R-B33` has always read. A shell is added and
 *   closed there, and nowhere else.
 * - **The layout says only where they are.** It is kept beside the window's
 *   own layout in `localStorage`, per machine too, and it is allowed to be
 *   wrong: `syncShells` runs after every restore and every change, so a layout
 *   naming a shell that has gone loses it, and a shell the layout has never
 *   heard of arrives as a split beside the one in front.
 */

import type { DockviewApi } from "dockview";
import { useStore } from "@/store";
import { shellSessionName } from "@/lib/tmux";
import { moveFocusIn } from "@/lib/panes";
import type { Direction } from "@/lib/spatial";

export type Shell = [string, string] | [string, string, string];

/** Which way a split opens, in dockview's own words. */
export type SplitDirection = "right" | "below";

const PREFIX = "shell:";

/** The panel id for a tmux session name — also the pty id `TerminalView` takes. */
export function shellPaneId(name: string): string {
  return `${PREFIX}${name}`;
}

/** The tmux session a panel shows, or `null` for an id that is not a shell. */
export function shellOfPane(id: string): string | null {
  return id.startsWith(PREFIX) ? id.slice(PREFIX.length) : null;
}

/** Where a machine's arrangement is kept. Keyed like `scoped()`, by machine. */
export function shellLayoutKey(machineId: string | null | undefined): string {
  return `mogeung.shellLayout:${machineId ?? "unknown"}`;
}

/**
 * Make the tree hold exactly `shells`, one to a split, keeping the arrangement
 * of the ones it already has. Closes before it adds, so a restored layout's
 * stale panels are gone before a new one looks for a split to sit beside.
 *
 * **One shell per split, and no tab bar**, since the same day as the tree
 * itself: *"the tab header is consuming a lot of space, it is not useful in
 * the terminal tab"*. A split is a terminal and nothing else — tmux's shape,
 * and Terminator's. Which makes a second shell in one group a shell nobody
 * can reach, so there never is one: a new shell opens beside, and a layout
 * saved while splits held tabs is spread out into splits on restore.
 */
export function syncShells(api: DockviewApi, shells: readonly Shell[], title: (s: Shell) => string): void {
  const want = new Set(shells.map(([name]) => shellPaneId(name)));
  for (const panel of [...api.panels]) {
    if (!want.has(panel.id)) api.removePanel(panel);
  }
  // Spread first, so a new shell lands beside a layout that is already flat.
  for (const group of [...api.groups]) {
    for (const panel of group.panels.slice(1)) panel.api.moveTo({ group, position: "right" });
  }
  const opened: string[] = [];
  for (const shell of shells) {
    const id = shellPaneId(shell[0]);
    const existing = api.getPanel(id);
    if (existing) {
      // A title is data, not arrangement — a layout saved before a shell's
      // root was known must not keep the old name.
      if (existing.title !== title(shell)) existing.api.setTitle(title(shell));
      continue;
    }
    // Beside the shell it split from, if it was asked for that way; else to
    // the right of the one in front, which is where `+` and a shell the
    // layout has never heard of both go.
    const where = placed.get(shell[0]);
    placed.delete(shell[0]);
    const from = where?.from ? api.getPanel(shellPaneId(where.from))?.group : undefined;
    const beside = from ?? api.activeGroup ?? api.groups.at(-1);
    api.addPanel({
      id,
      component: "shell",
      title: title(shell),
      ...(beside ? { position: { referenceGroup: beside, direction: where?.direction ?? "right" } } : {}),
    });
    if (where) opened.push(id);
  }
  // Every split, however it came to be — a restore, a move, an add.
  for (const group of api.groups) group.model.header.hidden = true;
  // Only a shell somebody just opened comes forward — never one a restore
  // filled in behind your back.
  for (const id of opened) api.getPanel(id)?.api.setActive();
}

/**
 * Restore a saved arrangement. An unreadable one degrades to an empty tree —
 * which `syncShells` then fills as tabs — rather than to an error, the rule
 * the window's own layout already follows.
 */
export function restoreShellLayout(api: DockviewApi, saved: string | null): void {
  if (!saved) {
    api.clear();
    return;
  }
  try {
    api.fromJSON(JSON.parse(saved));
  } catch {
    api.clear();
  }
}

// ---------------------------------------------------------------------------
// Opening and splitting, from anywhere — the `+`, a tab's menu, the terminal's
// menu and the keymap all come through here, so a split means one thing.

/** The live tree, while the panel is up. Set by the panel, as `setDock` is. */
let dock: DockviewApi | null = null;
/**
 * A split asked for while the panel was down. Which shell is in front is the
 * restored layout's to say, so the split waits for the tree rather than
 * guessing — see `takeDeferredSplit`.
 */
let deferred: SplitDirection | null = null;
/** Shells asked for but not yet in the tree, and where they should land. */
const placed = new Map<string, { from?: string; direction?: SplitDirection }>();

export function setShellDock(api: DockviewApi | null): void {
  dock = api;
}

/** The split waiting for the tree, once. The panel asks after each restore. */
export function takeDeferredSplit(): SplitDirection | null {
  const d = deferred;
  deferred = null;
  return d;
}

/** The first `mog-N` nobody has — shared with every client, on purpose. */
export function nextShellName(shells: readonly Shell[]): string {
  const used = new Set(shells.map(([name]) => name));
  let n = 0;
  while (used.has(shellSessionName(n))) n += 1;
  return shellSessionName(n);
}

/**
 * A shell for the selected session, as `+` has always opened one.
 *
 * Rooted in the session's worktree when there is one, else home. `-c` is read
 * only on creation, so an existing shell keeps wherever you left it — the
 * behaviour every terminal has. A session on another machine (`R-I22`) has its
 * worktree *there*, so the shell goes there too — or, with no way to reach it,
 * opens here at home rather than `cd`-ing into a path this filesystem does not
 * have.
 */
export function shellForSelection(shells: readonly Shell[]): Shell {
  const { sessions, selected, machineId } = useStore.getState();
  const session = selected ? sessions[selected] : undefined;
  const root = session?.repo_root ?? session?.cwd ?? "";
  const o = session?.origin;
  const away = o && o.machine_id !== machineId;
  const name = nextShellName(shells);
  return !away ? [name, root] : o.ssh_target ? [name, root, o.ssh_target] : [name, ""];
}

/**
 * Add a shell to the list, and — when the tree next syncs — put it `where`
 * and give it the keyboard. Works with the panel hidden: it comes up, and the
 * placement waits for the tree.
 */
export function openShell(entry: Shell, where: { from?: string; direction?: SplitDirection } = {}): void {
  placed.set(entry[0], where);
  const { scoped, setScoped } = useStore.getState();
  setScoped({ shells: [...scoped().shells, entry] });
  useStore.setState({ showTerminal: true, focusTerminal: { id: shellPaneId(entry[0]), nonce: Date.now() } });
}

/**
 * What `+` opens, and `Alt+Shift+N` (`R-B63`): a split to the right of the
 * shell in front, rooted where `shellForSelection` says — the selected
 * session's worktree, not the front shell's folder, which is what a split is
 * for.
 */
export function newShell(): void {
  openShell(shellForSelection(useStore.getState().scoped().shells));
}

/**
 * A new shell beside `from` — the one in front by default — in the same
 * folder, on the same machine. With nothing to split, a plain new shell.
 *
 * With the panel down there is no "in front" to read, so a default split
 * brings the panel up and runs once the tree is restored.
 */
export function splitShell(direction: SplitDirection, from?: string | null): void {
  if (from === undefined) {
    if (!dock) {
      deferred = direction;
      useStore.setState({ showTerminal: true });
      return;
    }
    from = dock.activePanel ? shellOfPane(dock.activePanel.id) : null;
  }
  const shells = useStore.getState().scoped().shells;
  const source = from ? shells.find(([n]) => n === from) : undefined;
  if (!source) {
    openShell(shellForSelection(shells));
    return;
  }
  const name = nextShellName(shells);
  const entry: Shell = source[2] ? [name, source[1], source[2]] : [name, source[1]];
  openShell(entry, { from: source[0], direction });
}

// ---------------------------------------------------------------------------
// Getting to the panel, and around it, from the keyboard.

/**
 * Show or hide the panel — and, showing it with no shell at all, open one.
 * `R-B61`, asked 2026-09-28: *"if there is no terminal, please create a new
 * one using the current selected claude session's folder … If there are
 * already opened terminal, No need to auto create a new one."*
 *
 * The egui panel did this and the port to TypeScript lost it, so `Alt+F12`
 * arrived at *no shells open* and a `+` that needed the mouse. The shell is
 * `+`'s own — the selected session's worktree, else home — and it takes the
 * keyboard, so the chord that shows the panel is the chord that lets you type
 * in it. Home needs no path of its own: an empty root is tmux's `-c ""`, which
 * is the client's directory, and the pty starts the client in `$HOME`.
 */
export function toggleTerminal(): void {
  const { showTerminal, scoped } = useStore.getState();
  if (showTerminal) {
    useStore.setState({ showTerminal: false });
    return;
  }
  const shells = scoped().shells;
  if (shells.length === 0) openShell(shellForSelection(shells));
  else useStore.setState({ showTerminal: true });
}

/** Marks the tiles, so a chord can tell a key typed in a shell from one typed elsewhere. */
export const SHELL_TILES_ATTR = "data-shell-tiles";

/** Was this key typed in one of the panel's shells? */
export function typedInShell(e: KeyboardEvent): boolean {
  const el = e.target as Element | null;
  return typeof el?.closest === "function" && el.closest(`[${SHELL_TILES_ATTR}]`) !== null;
}

/**
 * The shell that way, and the keyboard with it. `R-B60`. The centre's own
 * move (`R-B52`) over the panel's tree; nothing that way is a wall, as it is
 * there.
 */
export function moveShellFocus(dir: Direction): void {
  moveFocusIn(dock, dir);
}
