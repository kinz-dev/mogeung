/**
 * A dialog in a window of its own. `R-J128`, and `R-J130` after a first look.
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
 * `popout.rs`). So it moves and resizes on its own, and it costs no second
 * socket and no relay of the teams, the sends or the sessions across a
 * channel, which is what the arcade's page-of-its-own would have cost here.
 * It is not a dockview pop-out and cannot be docked back: there is nothing
 * to dock, the panel is the dialog.
 *
 * **Modal, and one copy.** While the window is open the main window shows a
 * backdrop that names it and brings it forward on a click; the one boolean
 * in the store that shows each dialog is what keeps it to one copy, and
 * `showDialog` raises the window that is open rather than asking for it
 * again. The shell's close button reaches the dialog through the claim
 * registered with `popout.ts`, so the OS window and the dialog's state never
 * disagree about whether it is open.
 *
 * **Every close goes through the shell, by label** (`R-J130`). The first cut
 * learnt a window's label from the page once it had loaded, and closed with
 * the page's own `window.close()` when it had not — which on this shell
 * destroys the webview and leaves the OS window standing, empty (`popout.rs`
 * says why). A dialog closed before its page loaded left exactly that, and
 * `tauri dev` does it on every open: StrictMode mounts, unmounts and mounts
 * each dialog once. So the shell now announces every window it opens, as
 * `popout:opened` with the label and the URL asked for; the URL carries which
 * dialog asked (`?dialog=teams`), and a close waits for the label rather than
 * falling back to the page. `Dialog` defers its close by a tick as well, so a
 * StrictMode re-run takes the window over instead of closing and reopening
 * it. A page that never loads is closed the same way, and the dialog stays
 * in this window.
 *
 * **Its own zoom**, because the ask began with the font. The window opens at
 * the main window's zoom and `Ctrl+=` / `Ctrl+-` / `Ctrl+0` inside it step a
 * zoom the dialogs share, remembered apart from the main window's. It is a
 * CSS `zoom` on the page's root — the one `lib/zoom.ts` refuses for the main
 * window because of the terminal's hit-testing, which no dialog has — so it
 * needs nothing of the shell and works in a browser's popup too.
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

/**
 * How long to wait for the page, and for the shell to say which window it
 * opened. Tunable so a test can make the wait short.
 */
export const tuning = { loadTimeoutMs: 5000, labelTimeoutMs: 5000 };

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
   * Let go of the window. It closes when the last holder has let go, and
   * answers the size it was left at then, in CSS pixels — null when it is
   * still held by another mount, or already gone.
   */
  close: () => Size | null;
};

type Handle = DialogHandle & { uses: number };

const open = new Map<DialogName, Handle>();

// --- which window the shell opened ------------------------------------------------

/** The page a dialog asks for, marked so the shell's announcement can be matched to it. */
function pageFor(name: DialogName): string {
  return `${PAGE}?dialog=${name}`;
}

function nameIn(url: string): DialogName | null {
  const m = /[?&]dialog=(teams|launch)(?:[&#]|$)/.exec(url);
  return m ? (m[1] as DialogName) : null;
}

/** Announced before anyone asked, by the dialog whose page it is. */
const announced = new Map<DialogName, string>();
/** Asked before the shell announced. */
const waiting = new Map<DialogName, (label: string) => void>();
let listening = false;

/**
 * Hear the shell say which window each `window.open` became. Registered at
 * startup, once: an announcement made before anyone listens is lost.
 */
export async function listenForDialogWindows(): Promise<void> {
  if (listening || !isTauri()) return;
  listening = true;
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<{ label?: unknown; url?: unknown }>("popout:opened", (e) => {
      const label = e.payload?.label;
      const url = e.payload?.url;
      if (typeof label !== "string" || typeof url !== "string") return;
      const name = nameIn(url);
      if (!name) return;
      const waiter = waiting.get(name);
      if (waiter) {
        waiting.delete(name);
        waiter(label);
      } else announced.set(name, label);
    });
  } catch (e) {
    listening = false;
    console.error("could not listen for the shell's windows:", e);
  }
}

/** The shell's announcement for `name`: already made, or the next one. Null when it never comes. */
function announcedLabel(name: DialogName): Promise<string | null> {
  const known = announced.get(name);
  if (known !== undefined) {
    announced.delete(name);
    return Promise.resolve(known);
  }
  return new Promise((resolve) => {
    const take = (label: string) => {
      clearTimeout(timer);
      resolve(label);
    };
    const timer = setTimeout(() => {
      if (waiting.get(name) === take) waiting.delete(name);
      resolve(null);
    }, tuning.labelTimeoutMs);
    waiting.set(name, take);
  });
}

// --- opening -------------------------------------------------------------------

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
    existing.uses++;
    existing.raise();
    return existing;
  }
  let win: Window | null = null;
  try {
    win = window.open(pageFor(name), `mogeung-${name}`, `width=${Math.round(size.width)},height=${Math.round(size.height)}`);
  } catch {
    win = null;
  }
  if (!win) return null;
  const target = win;

  // The label, from whichever says it first: the shell's announcement, or
  // the page once it has loaded. Null in a browser, and in a shell that
  // never said.
  let settleLabel!: (label: string | null) => void;
  const labelReady = new Promise<string | null>((resolve) => {
    let done = false;
    settleLabel = (label) => {
      if (done) return;
      done = true;
      resolve(label);
    };
  });
  if (isTauri()) void announcedLabel(name).then((label) => settleLabel(label));
  else settleLabel(null);

  let release: (() => void) | null = null;
  let poll: ReturnType<typeof setInterval> | null = null;
  let unkey: (() => void) | null = null;
  let closed = false;

  const closedFromOutside = () => {
    if (closed) return;
    onCloseRequest();
  };

  // The shell's close button, routed back as `popout:close`: the dialog
  // leaves the store, and its unmount closes the window. Claimed as soon as
  // the label is known, so a window closed before its page loaded still
  // reaches the dialog.
  void labelReady.then((label) => {
    if (closed || !label) return;
    release = claimWindow(label, closedFromOutside);
  });

  const ready = new Promise<DialogWindow | null>((resolve) => {
    let settled = false;
    const settle = (value: DialogWindow | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(value);
    };
    const timer = setTimeout(() => {
      // Never arrived: the dialog stays in this window, and the window that
      // would have held it is closed rather than left blank.
      settle(null);
      reallyClose();
    }, tuning.loadTimeoutMs);
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
        const fromPage = labelOf(target);
        if (fromPage) settleLabel(fromPage);
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
        applyZoom(target, dialogZoom());
        settle({ win: target, mount, label: fromPage });
      } catch {
        settle(null);
        reallyClose();
      }
    };
    if (isLoaded(target)) onLoad();
    else target.addEventListener("load", onLoad, { once: true });
  });

  /** The size the window is at, in CSS pixels — what `window.open` is asked for next time. */
  const measure = (): Size | null => {
    try {
      if (target.closed || !(target.innerWidth > 0) || !(target.innerHeight > 0)) return null;
      return { width: Math.round(target.innerWidth), height: Math.round(target.innerHeight) };
    } catch {
      return null;
    }
  };

  const reallyClose = () => {
    if (closed) return;
    closed = true;
    open.delete(name);
    if (poll) clearInterval(poll);
    release?.();
    unkey?.();
    forgetPopout(target);
    try {
      target.removeEventListener("pagehide", closedFromOutside);
    } catch {
      // Gone already.
    }
    void labelReady.then((label) => {
      if (label) return closeShellWindow(label);
      // No shell to ask, or one that never said which window this was: the
      // page's own close. In a browser that closes the popup; in the shell
      // it is the last resort, and the module notes say what it costs.
      try {
        target.close();
      } catch {
        // Already gone, which is what was wanted.
      }
    });
  };

  const handle: Handle = {
    uses: 1,
    ready,
    raise: () => {
      try {
        target.focus();
      } catch {
        // Gone; nothing to raise.
      }
      void labelReady.then((label) => raiseShellWindow(label));
    },
    close: () => {
      if (closed) return null;
      // Still held by another mount — a StrictMode re-run, or a dialog
      // reopened within the tick its close was deferred by.
      if (--handle.uses > 0) return null;
      const left = measure();
      reallyClose();
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
function raiseShellWindow(label: string | null): void {
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
      if (d) applyZoom(d.win, next);
    });
  }
}

/**
 * Zoom the page: a CSS `zoom` on its root, so tooltips and menus portalled
 * to its body scale with it. The window's own `innerWidth` is untouched by
 * it, which is why `measure` reads the size back without arithmetic.
 */
function applyZoom(win: Window, factor: number): void {
  try {
    (win.document.documentElement.style as unknown as { zoom: string }).zoom = String(factor);
  } catch {
    // Gone, or not ours to style.
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
