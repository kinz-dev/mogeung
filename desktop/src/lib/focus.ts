/**
 * Getting the keyboard back from a pane that is holding it. `R-B51`.
 *
 * The window's rule is `focusOwns`: a **chord** always fires, and a **bare**
 * key belongs to whatever has focus. That is right and it is what makes an
 * embedded terminal usable at all — an agent has to be able to receive `j`, and
 * a shortcut that stole it would make the pane a picture of a terminal rather
 * than one.
 *
 * The cost is that once you click into a terminal there is no keyboard way out.
 * Reported 2026-08-07: *"I don't have a way from keyboard to move the focus
 * back to the application and hence my app's keymap doesn't work"* — with the
 * observation that the capture itself is correct, and what is missing is a
 * release.
 *
 * So: one binding whose whole job is to stop a pane owning the keyboard.
 */

import type { Direction } from "@/lib/spatial";

/**
 * The chord, written down **once**. `R-B51`.
 *
 * It has to exist in two places — the keymap, so it is discoverable and
 * rebindable, and inside xterm's own key handler, so it works when the terminal
 * is winning and never reaches the pty. Two literals would drift, and the way
 * they would drift is silent: the keymap entry would keep saying one thing in
 * the shortcuts window while the terminal answered to another.
 *
 * **`Shift+Escape`, at the third attempt.** The two before it were both taken
 * by the desktop rather than by us, which is the failure that is hardest to
 * diagnose from inside the app — the keystroke never arrives, so nothing is
 * wrong except that nothing happens:
 *
 * - `Alt+Escape` — GNOME's *switch windows directly*.
 * - `Alt+Shift+Escape` — also claimed on Ubuntu, and three keys for something
 *   pressed mid-flow is a bad trade even where it is free.
 *
 * `Shift+Escape` is two keys under one hand, unbound in GNOME and on macOS, and
 * not something a TUI asks for — plain `Escape` still reaches Claude Code
 * untouched, which is the one that had to keep working. Chrome binds it to its
 * task manager, and that is not a hazard here: this window is a WebKitGTK
 * webview on Linux and WKWebView on macOS, neither of which has one.
 *
 * Rebindable like every other action (`R-B12`), so a desktop that disagrees
 * costs a preference rather than a patch.
 */
export const RELEASE_CHORD = "Shift+Escape";

/** Does this event *mean* the release chord? The predicate xterm asks. */
export function isReleaseChord(e: KeyboardEvent): boolean {
  return e.type === "keydown" && e.key === "Escape" && e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey;
}

/**
 * Moving between panes: `Alt+Shift+←↑↓→`. `R-B52`.
 *
 * In the `Alt+Shift+…` family the pane actions already use, and free on GNOME —
 * which claims `Super+arrow` for tiling and `Ctrl+Alt+arrow` for workspaces,
 * neither of which this is.
 *
 * **The terminal has to swallow these too**, and that is not the same reason
 * the release chord is swallowed. An arrow key reaches a TUI as a real escape
 * sequence, so a move that also forwarded it would scroll the agent's history
 * or walk its menu on the way out — you would arrive at the next pane having
 * quietly typed into the one you left. Same lesson as `ESC`, different
 * mechanism, and the one that would have been found by using it rather than by
 * reading it.
 */
const ARROWS: Record<string, Direction> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
};

export function paneMoveDirection(e: KeyboardEvent): Direction | null {
  if (e.type !== "keydown" || !e.altKey || !e.shiftKey || e.ctrlKey || e.metaKey) return null;
  return ARROWS[e.key] ?? null;
}

/** The binding strings, so the keymap and this file cannot disagree. */
export const PANE_MOVE_CHORDS: Record<Direction, string> = {
  left: "Alt+Shift+ArrowLeft",
  right: "Alt+Shift+ArrowRight",
  up: "Alt+Shift+ArrowUp",
  down: "Alt+Shift+ArrowDown",
};

/**
 * Park the keyboard somewhere neutral inside the current pane.
 *
 * **Neutral, not somewhere useful.** Throwing focus at the Attention queue was
 * the obvious alternative and is worse: it would silently rebind `j`/`k` to
 * moving through sessions the moment you escaped a pane, so a release would
 * also be a navigation you did not ask for. A `[data-focus-host]` is an
 * ancestor of the thing that was holding focus, which is exactly enough for
 * `focusOwns` to stop deferring — it asks `closest(".xterm")`, and an ancestor
 * is not inside.
 *
 * Falls back to blurring. A blur alone already fixes bare keys, because
 * `focusOwns` reads `document.activeElement` and `<body>` owns nothing; the
 * host is preferred only so the *pane* keeps a visible focus ring rather than
 * the window looking as though nothing happened.
 */
export function releaseKeyboard(): void {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return;
  const host = el.closest("[data-focus-host]") as HTMLElement | null;
  el.blur();
  host?.focus();
}

/** Whether a pane is currently holding the keyboard — what the action tests. */
export function keyboardIsHeld(): boolean {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return false;
  if (el.closest(".xterm") || el.closest(".monaco-editor")) return true;
  const tag = el.tagName.toLowerCase();
  // `=== true` rather than a bare truthiness test: `isContentEditable` is not
  // implemented everywhere — jsdom leaves it `undefined` — and this function is
  // declared to return a `boolean`, so the last clause of an `||` chain would
  // hand back `undefined` under a type that promises otherwise. Harmless in an
  // `if`, and a lie to anyone who compares the result.
  return tag === "input" || tag === "textarea" || el.isContentEditable === true;
}
