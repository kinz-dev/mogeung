/**
 * A dialog in a window of its own. `R-J128`.
 *
 * Asked 2026-10-11: *"make the new 'New Session' or 'Teams' a real popup
 * dialog that can move around out of the current mogeung application. (But
 * make it a modal popup, only allow one copy) because the font is too small
 * and I want to make that popup screen resizeable independently."*
 *
 * **The same door as a pop-out, not a second client.** The dialog's React
 * tree stays in the main window — same store, same socket, same `send` — and
 * only its panel is portalled into a document the main window opened with
 * `window.open`, which the shell turns into a real `popout-*` OS window with
 * the system's own title bar ([ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md),
 * `popout.rs`). So it moves, resizes and zooms on its own, and it costs no
 * second socket and no relay of the teams, the sends or the sessions across a
 * channel, which is what the arcade's page-of-its-own would have cost here.
 *
 * **Modal, and one copy.** While the window is open the main window shows a
 * backdrop that names it and brings it forward on a click; the one boolean
 * in the store that shows each dialog is what keeps it to one copy, and
 * `showDialog` raises the window that is open rather than asking for it
 * again. The shell's close button reaches the dialog through the claim
 * registered with `popout.ts`, so the OS window and the dialog's state never
 * disagree about whether it is open.
 *
 * **Its own zoom**, because the ask began with the font. The window opens at
 * the main window's zoom and `Ctrl+=` / `Ctrl+-` / `Ctrl+0` inside it step a
 * zoom the dialogs share, remembered apart from the main window's.
 *
 * A browser tab has a real `window.open` and gets a real popup; where there is
 * none — a popup blocker, or jsdom — the dialog stays in this window, as it
 * always was.
 */

import { isTauri } from "@/lib/tauri";
import { claimWindow, closeShellWindow, forgetPopout, labelOf, trackPopout } from "@/lib/popout";
import { steppedZoom } from "@/lib/zoom";
import { useStore } from "@/store";

/** The same empty, same-origin page a dockview popout opens into. */
export const PAGE = "/popout.html";

/** How long to wait for the page before giving up and staying in this window. */
const LOAD_TIMEOUT_MS = 5000;

/** How often an open window is checked for having been closed from outside. */
const CLOSED_POLL_MS = 400;

export type Size = { width: number; height: number };

/** The dialogs that open this way, and the store flag that shows each. */
export type DialogName = "teams" | "launch";

/** A window that has loaded and is ready to hold a panel. */
export type DialogWindow = {
  win: Window;
  /** Where the panel is portalled to: a full-height element in the page's body. */
  mount: HTMLElement;
  /** The shell's label for the window, or null in a browser tab. */
  label: string | null;
};

/** What `openDialogWindow` hands back while the page loads. */
export type DialogHandle = {
  /** Resolves once the page has loaded — or null if it never did. */
  ready: Promise<DialogWindow | null>;
  /** Bring the window forward. */
  raise: () => void;
  /**
   * Close the window. Answers the size it was left at, in logical pixels, or
   * null when that could not be read — a window already gone.
   */
  close: () => Size | null;
};

const open = new Map<DialogName, DialogHandle>();

/**
 * Open the window for `name`, sized `size`, or hand back the one already open
 * after bringing it forward — one copy. Null when no window could be opened,
 * which means: stay in this window.
 *
 * `onCloseRequest` is called when the window is closed from outside — the
 * shell's close button, or a browser's — so the dialog can leave the store.
 */
export function openDialogWindow(name: DialogName, title: string, size: Size, onCloseRequest: () => void): DialogHandle | null {
  const existing = open.get(name);
  if (existing) {
    existing.raise();
    return existing;
  }
  let win: Window | null = null;
  try {
    win = window.open(PAGE, `mogeung-${name}`, `width=${Math.round(size.width)},height=${Math.round(size.height)}`);
  } catch {
    win = null;
  }
  if (!win) return null;
  const target = win;

  let label: string | null = null;
  let release: (() => void) | null = null;
  let poll: ReturnType<typeof setInterval> | null = null;
  let unkey: (() => void) | null = null;
  let closed = false;

  const closedFromOutside = () => {
    if (closed) return;
    onCloseRequest();
  };

  const ready = new Promise<DialogWindow | null>((resolve) => {
    let settled = false;
    const settle = (value: DialogWindow | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(value);
    };
    const timer = setTimeout(() => settle(null), LOAD_TIMEOUT_MS);
    const onLoad = () => {
      if (closed) return settle(null);
      try {
        const doc = target.document;
        copyStyles(document, doc);
        doc.title = `${title} · mogeung`;
        trackPopout(target);
        const mount = doc.createElement("div");
        mount.style.height = "100%";
        doc.body.appendChild(mount);
        label = labelOf(target);
        if (label) release = claimWindow(label, closedFromOutside);
        // Closed by a browser's own button: the page is unloaded. The shell's
        // button never gets this far (`popout.rs` holds it and asks), and a
        // window destroyed without asking is caught by the poll.
        target.addEventListener("pagehide", closedFromOutside);
        poll = setInterval(() => {
          let gone = false;
          try {
            gone = target.closed;
          } catch {
            gone = true;
          }
          if (gone) closedFromOutside();
        }, CLOSED_POLL_MS);
        unkey = bindZoomKeys(target);
        void applyZoom(label, dialogZoom());
        settle({ win: target, mount, label });
      } catch {
        settle(null);
      }
    };
    if (isLoaded(target)) onLoad();
    else target.addEventListener("load", onLoad, { once: true });
  });

  const handle: DialogHandle = {
    ready,
    raise: () => raiseWindow(target, label),
    close: () => {
      if (closed) return null;
      closed = true;
      open.delete(name);
      if (poll) clearInterval(poll);
      release?.();
      unkey?.();
      forgetPopout(target);
      let left: Size | null = null;
      try {
        target.removeEventListener("pagehide", closedFromOutside);
        if (!target.closed && target.innerWidth > 0 && target.innerHeight > 0) {
          // The viewport is in CSS pixels, which the window's zoom has divided;
          // what `window.open` is asked for next time is the logical size.
          const zoom = dialogZoom();
          left = { width: Math.round(target.innerWidth * zoom), height: Math.round(target.innerHeight * zoom) };
        }
      } catch {
        left = null;
      }
      if (label) void closeShellWindow(label);
      else {
        try {
          target.close();
        } catch {
          // Already gone, which is what was wanted.
        }
      }
      return left;
    },
  };
  open.set(name, handle);
  return handle;
}

/**
 * Bring the window for `name` forward, if it is open. Answers whether there
 * was one — and so whether showing the dialog means raising rather than
 * opening.
 */
export function raiseDialogWindow(name: DialogName): boolean {
  const handle = open.get(name);
  if (!handle) return false;
  handle.raise();
  return true;
}

/**
 * Show a dialog: the window it is in if it is open, else the dialog itself,
 * through the store flag that mounts it. The one way in, so that a second
 * `Alt+Shift+g` finds the Teams window rather than being a no-op on a flag
 * already set.
 */
export function showDialog(name: DialogName): void {
  if (raiseDialogWindow(name)) return;
  if (name === "teams") useStore.setState({ showTeams: true });
  else useStore.setState({ showLaunch: true });
}

/** The windows open now, by name. Exported for tests. */
export function openDialogWindows(): DialogName[] {
  return [...open.keys()];
}

/** Has the page arrived, or is `win` still the `about:blank` it opened on? */
function isLoaded(win: Window): boolean {
  try {
    return win.document?.readyState === "complete" && !!win.location && win.location.href !== "about:blank";
  } catch {
    return false;
  }
}

/**
 * Give `to` every stylesheet `from` has, the way dockview does for its
 * popouts: the rules arrive, and the `data-theme` attribute they read is
 * carried by `mirrorTheme`. Relative URLs in them — the bundled terminal
 * font — resolve the same, because the page is the same origin.
 */
export function copyStyles(from: Document, to: Document): void {
  for (const node of from.querySelectorAll('link[rel="stylesheet"], style')) {
    to.head.appendChild(to.importNode(node, true));
  }
}

/**
 * Bring an OS window to the front. `focus()` on the DOM window is not enough
 * in the shell — the document never lost focus as far as it knows (see
 * `lib/tauri.ts`) — so the shell's window is asked by label as well.
 */
function raiseWindow(win: Window, label: string | null): void {
  try {
    win.focus();
  } catch {
    // Gone; nothing to raise.
  }
  if (!label || !isTauri()) return;
  void (async () => {
    try {
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const w = await WebviewWindow.getByLabel(label);
      await w?.unminimize();
      await w?.setFocus();
    } catch {
      // Not raised: the window is still open, and the taskbar still has it.
    }
  })();
}

// --- the dialogs' own zoom -----------------------------------------------------

/** The zoom the dialogs' windows share: their own once nudged, else the main window's. */
export function dialogZoom(): number {
  const { prefs } = useStore.getState();
  return prefs.dialogZoom ?? prefs.appZoom;
}

/**
 * Step the dialogs' zoom and apply it to every open dialog window. The main
 * window's own zoom is untouched: the ask was for the popup's font, and the
 * two windows are read at different distances.
 */
export function nudgeDialogZoom(direction: 1 | -1 | 0): void {
  const next = steppedZoom(dialogZoom(), direction);
  useStore.getState().setPrefs({ dialogZoom: next });
  for (const handle of open.values()) {
    void handle.ready.then((d) => {
      if (d) void applyZoom(d.label, next);
    });
  }
}

/**
 * Zoom the shell's webview for `label`. In a browser tab there is no shell and
 * the browser's own zoom keys work on the popup, so nothing is done.
 */
async function applyZoom(label: string | null, factor: number): Promise<void> {
  if (!label || !isTauri()) return;
  try {
    const { Webview } = await import("@tauri-apps/api/webview");
    const wv = await Webview.getByLabel(label);
    await wv?.setZoom(factor);
  } catch (e) {
    console.error("could not zoom the dialog's window:", e);
  }
}

/**
 * `Ctrl+=`, `Ctrl+-` and `Ctrl+0` (`Cmd` on a Mac) typed in a dialog's window
 * step its zoom. Only in the shell: a browser's popup answers those keys
 * itself. Answers the unbind.
 */
function bindZoomKeys(win: Window): () => void {
  if (!isTauri()) return () => {};
  const onKey = (e: KeyboardEvent) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const direction = e.key === "=" || e.key === "+" ? 1 : e.key === "-" ? -1 : e.key === "0" ? 0 : null;
    if (direction === null) return;
    e.preventDefault();
    nudgeDialogZoom(direction);
  };
  win.addEventListener("keydown", onKey, true);
  return () => win.removeEventListener("keydown", onKey, true);
}
