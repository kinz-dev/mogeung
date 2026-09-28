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
 *   heard of arrives as a tab in the active group.
 */

import type { DockviewApi } from "dockview";

export type Shell = [string, string] | [string, string, string];

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
 * Make the tree hold exactly `shells`, keeping the arrangement of the ones it
 * already has. Closes before it adds, so a restored layout's stale panels are
 * gone before a new one looks for an active group to join.
 */
export function syncShells(api: DockviewApi, shells: readonly Shell[], title: (s: Shell) => string): void {
  const want = new Set(shells.map(([name]) => shellPaneId(name)));
  for (const panel of [...api.panels]) {
    if (!want.has(panel.id)) api.removePanel(panel);
  }
  for (const shell of shells) {
    const id = shellPaneId(shell[0]);
    const existing = api.getPanel(id);
    if (existing) {
      // A title is data, not arrangement — a layout saved before a shell's
      // root was known must not keep the old name on its tab.
      if (existing.title !== title(shell)) existing.api.setTitle(title(shell));
      continue;
    }
    api.addPanel({ id, component: "shell", title: title(shell) });
  }
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
