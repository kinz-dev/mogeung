/**
 * The live editor behind each file pane, so a chord can act on it. `R-J99`.
 *
 * Fold All and Unfold All are Monaco's own actions, and Monaco binds them to
 * `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` — chords that never arrive, because
 * `keymap.ts` listens in the capture phase and a chord always fires: `Ctrl+K`
 * is the palette and `Ctrl+0` is reset-zoom. Nor are they in the right-click
 * menu, which Monaco fills with *Go to Symbol*, *Copy* and *Command Palette*
 * and nothing else. Asked 2026-09-21 for IntelliJ's spellings instead.
 *
 * So the window binds them, and to run one it needs the editor — which
 * `FilePane` owns in a ref and nothing else can see. This is the same shape as
 * `marks.ts`'s cursor: the pane publishes on mount, forgets on unmount, and the
 * chord asks for *the active pane's* editor rather than *an* editor, because a
 * chord fires wherever the keyboard is and folding a file you are not looking
 * at is worse than doing nothing.
 *
 * A module and not the store on purpose, as with the cursor: an editor is not
 * state anything renders from.
 */

/** The one thing a chord needs of an editor — Monaco's editor satisfies it. */
export interface ActionEditor {
  getAction(id: string): { run(): unknown } | null;
}

const editors = new Map<string, ActionEditor>();

export function noteEditor(pane: string, editor: ActionEditor): void {
  editors.set(pane, editor);
}

/** Forgotten when its pane goes — or turns into a markdown preview, which
 *  disposes the editor without unmounting the pane. */
export function forgetEditor(pane: string): void {
  editors.delete(pane);
}

/** The editor, but only if its pane is the one the keyboard is actually in. */
export function editorIn(activePane: string | null): ActionEditor | null {
  return activePane === null ? null : (editors.get(activePane) ?? null);
}
