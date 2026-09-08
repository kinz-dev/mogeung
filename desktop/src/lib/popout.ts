/**
 * A pane in a window of its own. `R-B55`,
 * [ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md)
 * and its 2026-09-08 amendment.
 *
 * **dockview owns the window, not us.** `addPopoutGroup` moves a group's DOM
 * into a second document and keeps *one* dockview across both — which is the
 * whole reason to prefer it: a pane can be dragged out of the main window and
 * back, and another pane can be dragged in and docked beside it. The first cut
 * of this row opened a second client instead, and separate React trees can
 * never do that.
 *
 * What this file is left holding is the two things dockview does not do:
 * carrying the **theme** across, and saying something useful when a popout
 * cannot be opened at all.
 */

import type { DockviewApi } from "dockview";
import { useStore } from "@/store";

/**
 * Give the popout document the theme attribute the opener has.
 *
 * dockview copies the opener's **stylesheets** into the new document, so every
 * rule arrives — but every colour in them is a `var(--…)` defined under
 * `:root[data-theme="…"]`, and the attribute lives on `<html>`, which is not a
 * stylesheet and is not copied. Without this the popout renders with whatever
 * the bare `:root` block happens to say, which is a window in the wrong palette
 * rather than an unstyled one, and therefore easy to misread as a design.
 */
export function mirrorTheme(target: Window): void {
  const theme = document.documentElement.getAttribute("data-theme");
  if (theme) target.document.documentElement.setAttribute("data-theme", theme);
}

/** Every popout document currently open, so a theme change reaches them. */
const opened = new Set<Window>();

export function trackPopout(target: Window): void {
  opened.add(target);
  mirrorTheme(target);
}

export function forgetPopout(target: Window): void {
  opened.delete(target);
}

/**
 * Re-apply the theme to every open popout.
 *
 * Called when the theme changes in the main window: the popouts are the same
 * dockview but they are not the same document, so the effect that writes
 * `data-theme` on the opener does not reach them.
 */
export function retheme(): void {
  for (const w of opened) {
    try {
      mirrorTheme(w);
    } catch {
      // A window closed between the set and the write. Dropped rather than
      // guarded with an `is it closed` check, which races the same way.
      opened.delete(w);
    }
  }
}

/**
 * Move a pane into a window of its own.
 *
 * Returns false when the window could not be opened, which in the desktop
 * build means the shell refused `window.open` — and the caller says so rather
 * than leaving a control that appears to do nothing. In a browser tab it works,
 * because a tab has a real `window.open`; it is the *shell* that has to opt in
 * (`popout.rs`), and that is the one thing about this a tab cannot check.
 */
export async function popOutPane(api: DockviewApi | null, paneId: string): Promise<boolean> {
  const panel = api?.getPanel(paneId);
  if (!api || !panel) return false;
  try {
    const ok = await api.addPopoutGroup(panel, {
      popoutUrl: "/popout.html",
      onDidOpen: ({ window }) => trackPopout(window),
      onWillClose: ({ window }) => forgetPopout(window),
    });
    if (!ok) {
      useStore
        .getState()
        .pushError("this window could not open another — the pane stays where it is");
    }
    return ok;
  } catch {
    useStore.getState().pushError("this window could not open another — the pane stays where it is");
    return false;
  }
}
