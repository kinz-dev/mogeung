/**
 * Work the window holds that nothing has saved yet — asked before a restart.
 * `R-J105`, feature 0049.
 *
 * Most of the window has nothing to lose: scratch files save as you type, and
 * layout, keymap and prefs are written as they change. The Notes editor is the
 * exception — its draft is the component's own until you press save — so it
 * reports here, and the restart dialog names whatever is listed rather than
 * letting a relaunch throw it away in silence.
 *
 * A register rather than a store field, because the question is only ever
 * asked at one moment and nothing needs to re-render when the answer changes.
 */

const held = new Map<string, string>();

/** Say `key` holds unsaved work, described by `label` — or, with `null`, that it no longer does. */
export function markUnsaved(key: string, label: string | null): void {
  if (label) held.set(key, label);
  else held.delete(key);
}

/** Everything that would be lost now, in words. */
export function unsavedWork(): string[] {
  return [...held.values()];
}
