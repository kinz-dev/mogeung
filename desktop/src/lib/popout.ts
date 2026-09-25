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
 * What this file is left holding is what dockview does not do: carrying the
 * **theme** across, saying something useful when a popout cannot be opened at
 * all, and — since `R-J108` — **closing the window** once its group has gone
 * home, and a button's way of sending it there.
 */

import type { DockviewApi, DockviewGroupPanel } from "dockview";
import { isTauri } from "@/lib/tauri";
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

// --- popping back in (`R-J108`) ------------------------------------------------

/**
 * The shell's label for a window it opened, which `popout.rs` puts on the page
 * as `__MOGEUNG_WINDOW_LABEL__` — or `null` in a browser tab, where there is
 * no shell and no label, or for a window that has already gone.
 */
export function labelOf(w: Window | null | undefined): string | null {
  try {
    const label = (w as unknown as { __MOGEUNG_WINDOW_LABEL__?: unknown } | null)?.__MOGEUNG_WINDOW_LABEL__;
    return typeof label === "string" ? label : null;
  } catch {
    return null;
  }
}

/** Ask the shell to close a popout window for good. See `popout_close`. */
export async function closeShellWindow(label: string): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("popout_close", { label });
  } catch {
    // A window already gone is the outcome that was wanted.
  }
}

/**
 * Put a popped-out group back into the main window. Asked 2026-09-25: *"there
 * is no easy way to pop back in"*.
 *
 * **dockview's own way home, not a move of our making.** dockview remembers
 * where a group came from and, when its popout window closes, puts it back
 * there — un-hiding the space it left, or adding it to the grid if that space
 * has gone. That code runs from the popout document's `beforeunload`, and in
 * the shell that event never comes (see `popout.rs`), so it is raised here.
 * Moving the panels ourselves would have to re-derive all of that, and a
 * group sent into the hidden space it left would arrive invisible.
 *
 * dockview then closes the window with `window.close()`, which the shell does
 * not honour — `watchPopouts` closes it once the group has gone.
 */
export function popInGroup(group: Pick<DockviewGroupPanel, "api">): boolean {
  if (group.api.location.type !== "popout") return false;
  try {
    group.api.getWindow().dispatchEvent(new Event("beforeunload"));
    return true;
  } catch {
    return false;
  }
}

/**
 * Close the shell window of every popout group dockview has finished with —
 * however it finished: popped back in, its last pane dragged out, or its
 * window closed. dockview calls `window.close()` in each case and the shell's
 * webview does not honour it, so without this the window stays, empty.
 *
 * Watches the groups rather than hooking `addPopoutGroup`'s callbacks, because
 * dockview also reopens popouts from a saved layout, and those never passed
 * through `popOutPane`.
 */
export function watchPopouts(api: Pick<DockviewApi, "groups" | "onDidLayoutChange" | "onDidRemoveGroup" | "onDidAddGroup">): () => void {
  let known = new Map<string, string>();
  const sync = () => {
    const now = new Map<string, string>();
    const windows: Window[] = [];
    for (const g of api.groups) {
      if (g.api.location.type !== "popout") continue;
      let label: string | null = null;
      try {
        const w = g.api.getWindow();
        if (w && !windows.includes(w)) windows.push(w);
        label = labelOf(w);
      } catch {
        label = null;
      }
      // A window still loading has no label yet; it is read on the next pass.
      const was = known.get(g.id);
      if (label ?? was) now.set(g.id, (label ?? was)!);
    }
    for (const [id, label] of known) if (!now.has(id)) void closeShellWindow(label);
    known = now;
    setPopoutWindows(windows);
  };
  const subs = [api.onDidLayoutChange(sync), api.onDidRemoveGroup(sync), api.onDidAddGroup(sync)];
  sync();
  return () => subs.forEach((d) => d.dispose());
}

/**
 * What to do when the shell says a popout window's close button was pressed
 * (`popout:close`): send its group home, which then closes the window — or,
 * for a window that holds no group (the arcade's, or one already emptied),
 * close it straight away.
 */
export function onPopoutCloseRequest(api: Pick<DockviewApi, "groups">, label: string): void {
  const group = api.groups.find((g) => {
    if (g.api.location.type !== "popout") return false;
    try {
      return labelOf(g.api.getWindow()) === label;
    } catch {
      return false;
    }
  });
  if (!group || !popInGroup(group)) void closeShellWindow(label);
}

/** Listen for the shell's `popout:close`. Answers an unsubscriber. */
export async function listenForPopoutCloses(api: Pick<DockviewApi, "groups">): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<string>("popout:close", (e) => onPopoutCloseRequest(api, e.payload));
}

// --- the windows the keymap listens on (`R-J109`) ------------------------------

/**
 * Every popout window open now, for whatever has to listen to them as well as
 * to this one — the keymap, which otherwise hears nothing typed in a popout.
 * Kept by `watchPopouts`, so a popout reopened from a saved layout counts too.
 */
let popoutWindows: readonly Window[] = [];
const windowWatchers = new Set<(windows: readonly Window[]) => void>();

/** Replace the list, telling the watchers if it changed. Exported for tests. */
export function setPopoutWindows(next: readonly Window[]): void {
  if (next.length === popoutWindows.length && next.every((w) => popoutWindows.includes(w))) return;
  popoutWindows = [...next];
  for (const watch of windowWatchers) watch(popoutWindows);
}

/** Hear the list now and whenever it changes. Answers an unsubscriber. */
export function onPopoutWindows(watch: (windows: readonly Window[]) => void): () => void {
  windowWatchers.add(watch);
  watch(popoutWindows);
  return () => windowWatchers.delete(watch);
}
