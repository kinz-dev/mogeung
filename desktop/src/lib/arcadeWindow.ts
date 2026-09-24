/**
 * The arcade in a window of its own.
 *
 * Asked for 2026-09-24: *"change the hidden games to popup in a completely new
 * windows"*. π still opens it from the main window, but the games now run in
 * their own page — `arcade.html` — in their own OS window, so a game sits
 * beside your sessions rather than over them.
 *
 * **A page, not a portal.** dockview's popouts draw into a second document
 * from the main window's React tree (`lib/popout.ts`), and the arcade could
 * have done the same — except that every game listens for its keys on
 * `window`, and in a portal that is the *other* window. A page of its own gets
 * its own `window`, so the games need not know where they are.
 *
 * **Still one client.** The arcade page does not dial the daemon: that would
 * mean a second socket, a second hosted daemon check and a second set of
 * notifications, all for one line of text. The main window tells it, over a
 * `BroadcastChannel`, the two things it needs — the theme, and which session
 * is waiting — and hears back when you click that line.
 *
 * In the desktop shell `window.open` becomes a `popout-*` window through
 * `popout.rs`, which is also what gives it the capability to read and write
 * the games' history files.
 */

import { isTauri } from "@/lib/tauri";
import type { SessionId } from "@/wire/types";

export const CHANNEL = "mogeung.arcade";
export const PAGE = "/arcade.html";

/** What the arcade window shows at its foot: the first session that needs you. */
export type Waiting = { count: number; first: { id: SessionId; label: string } | null };

export type ArcadeMsg =
  /** arcade → main: I have just opened, tell me everything. */
  | { t: "hello" }
  /** main → arcade: the state of things, sent on hello and on every change. */
  | { t: "state"; theme: string | null; waiting: Waiting }
  /** arcade → main: take me to this session. */
  | { t: "go"; id: SessionId }
  /** main → arcade: π again, with the arcade already open — come forward. */
  | { t: "raise" };

/** A channel, or null where there is none (an old webview, some tests). */
export function channel(): BroadcastChannel | null {
  return typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel(CHANNEL);
}

let opened: Window | null = null;

/**
 * Open the arcade window, or bring forward the one already open.
 *
 * Returns false when no window could be opened — a popup blocker in a tab, or
 * the shell refusing `window.open` — so the caller can show the arcade where
 * it is rather than appear to ignore π.
 */
export function openArcadeWindow(): boolean {
  if (opened && !opened.closed) {
    try {
      opened.focus();
    } catch {
      // Gone between the check and the call; `raise` below still reaches it.
    }
    const ch = channel();
    ch?.postMessage({ t: "raise" } satisfies ArcadeMsg);
    ch?.close();
    return true;
  }
  const theme = document.documentElement.getAttribute("data-theme");
  const url = `${PAGE}${theme ? `?theme=${encodeURIComponent(theme)}` : ""}`;
  // A named target, so a second π in a browser tab reuses the window rather
  // than stacking another one.
  opened = window.open(url, "mogeung-arcade", "width=1280,height=860");
  return opened !== null;
}

/**
 * Bring this OS window to the front.
 *
 * `window.focus` alone does not raise a Tauri window — the document never lost
 * focus as far as it knows (see `lib/tauri.ts`) — so the shell is asked too.
 */
export async function raiseThisWindow(): Promise<void> {
  window.focus();
  if (!isTauri()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const w = getCurrentWindow();
    await w.unminimize();
    await w.setFocus();
  } catch {
    // Not raised: the click still selected the session, which is the part
    // that matters.
  }
}

/** Close this window — the arcade's, from its own Escape or close button. */
export async function closeThisWindow(): Promise<void> {
  if (isTauri()) {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().close();
      return;
    } catch {
      // Fall through to the DOM's own, which a script-opened window allows.
    }
  }
  window.close();
}
