/**
 * A pane in a window of its own. `R-B55`,
 * [ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md).
 *
 * Two halves that never meet: **asking** for a popout, which the main window
 * does, and **being** one, which the second window does. `readPopout` is how a
 * booting client finds out which it is, and it is deliberately a plain read of
 * the query string rather than store state — it has to be answerable before the
 * store exists, because it decides what gets rendered at all.
 *
 * The daemon address is **not** part of this. Both windows load from one
 * origin, so they share `localStorage`, and `defaultUrl` already falls back to
 * `mogeung.url`. Passing it would be a second source of truth, and since
 * `R-I16` an address can carry a token — which would then be in a URL bar.
 */

import { isTauri } from "@/lib/tauri";

/** The panes that may be detached. Mirrors the shell's own allowlist. */
export const POPPABLE = ["agent"] as const;
export type PoppableKind = (typeof POPPABLE)[number];

export interface Popout {
  kind: PoppableKind;
  session: string;
}

/**
 * Am I a popout, and of what?
 *
 * Reads the same two parameters the shell wrote, and validates them again on
 * the way in. The shell checked them before building the URL; this checks them
 * because a window is also reachable by hand-editing a query string in the dev
 * tools, and a `kind` that is not a component name would render nothing with no
 * explanation.
 */
export function readPopout(search: string = window.location.search): Popout | null {
  const q = new URLSearchParams(search);
  const kind = q.get("popout");
  const session = q.get("session");
  if (!kind || !session) return null;
  if (!(POPPABLE as readonly string[]).includes(kind)) return null;
  if (!/^[A-Za-z0-9_-]{1,128}$/.test(session)) return null;
  return { kind: kind as PoppableKind, session };
}

/**
 * Open this pane in its own window, and say whether it happened.
 *
 * `false` in a browser tab, where there is no shell to open a window with. The
 * caller uses that to explain rather than to fail silently: a button that does
 * nothing is worse than one that is not there, and this one *is* there in a tab
 * because the tab is otherwise a real client.
 */
export async function openPopout(
  kind: PoppableKind,
  session: string,
  title?: string,
): Promise<boolean> {
  if (!isTauri()) return false;
  const core = await import("@tauri-apps/api/core");
  await core.invoke<string>("popout_open", { kind, session, title: title ?? null });
  return true;
}

/** Close the window I am in. Only ever called from inside a popout. */
export async function closeThisWindow(): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}

/**
 * Tell me when a popped-out pane's window has gone, so its pane can come back.
 *
 * Only the shell knows a window was destroyed — the popout cannot report its
 * own funeral — so this is a shell event rather than anything the two clients
 * arrange between themselves. Listened to by the **main** window; a popout
 * receives it too, and ignores it, because it is the thing being destroyed.
 */
export async function onPopoutClosed(
  cb: (p: Popout) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return await listen<{ kind: string; session: string }>("popout:closed", ({ payload }) => {
      const kind = payload?.kind;
      const session = payload?.session;
      if (!kind || !session) return;
      if (!(POPPABLE as readonly string[]).includes(kind)) return;
      cb({ kind: kind as PoppableKind, session });
    });
  } catch {
    // A shell that cannot deliver this is a window that does not take the pane
    // back, not a window that fails to open.
    return () => {};
  }
}
