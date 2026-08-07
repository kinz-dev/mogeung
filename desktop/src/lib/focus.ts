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
