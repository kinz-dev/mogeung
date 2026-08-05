/**
 * Copy and paste inside a terminal, which is not the gesture it is anywhere
 * else in the window.
 *
 * **xterm.js implements neither.** It draws its own selection rather than
 * making a DOM one, so the browser has nothing to copy, and it hands `Ctrl+V`
 * to the pty as `^V` rather than pasting. Both are correct for a terminal and
 * both leave the app to supply the actual gesture — which this pane never did,
 * reported 2026-08-05 as *"copy and paste in the claude session is still not
 * working"*.
 *
 * `Ctrl+C` is the reason the chords look the way they do. It is `SIGINT`, and
 * it has to stay `SIGINT` even with a selection on screen — a terminal where
 * an interrupt sometimes means *copy* is one you cannot stop a runaway build
 * in. So copying is `Ctrl+Shift+C`, which is what every Linux terminal already
 * uses for exactly this reason, and `Cmd+C` on a Mac, where nothing collides.
 *
 * Paste splits in two, and the split is not cosmetic:
 *
 * - **`Ctrl+V`, `Cmd+V` and `Shift+Insert` are the webview's own editing
 *   bindings.** Declining to handle them hands the keystroke to WebKit, which
 *   pastes into the hidden textarea xterm keeps focused; xterm's own `paste`
 *   listener then forwards it with bracketed-paste applied. No clipboard API,
 *   no permission, nothing to fail.
 * - **`Ctrl+Shift+V` is not bound to anything.** Nothing will happen unless we
 *   read the clipboard ourselves, so that one goes through `navigator`.
 *
 * Preferring the native route where one exists is what keeps this working on a
 * webview whose clipboard *read* may be gated, absent or asynchronous — three
 * different failure modes across the platforms this ships to, none of which
 * the paste chord everyone actually presses now depends on.
 *
 * The price is `^V`: quoted-insert can no longer be typed into a pane. It is
 * reachable through tmux's own prefix, and it is a far rarer thing to want
 * than paste in a pane whose job is answering a prompt.
 */

export type ClipboardIntent =
  /** Copy the terminal's selection ourselves. */
  | "copy"
  /** Let the webview's editing binding paste; xterm forwards what lands. */
  | "paste-native"
  /** Nothing is bound to this chord, so read the clipboard and paste it. */
  | "paste-read"
  | null;

/**
 * What a key event means to a terminal, or `null` for "this is the pty's".
 *
 * Deliberately platform-blind. `Cmd+C` only exists on a Mac and `Meta` is the
 * super key elsewhere, where nothing in this window binds it — so accepting
 * both spellings costs nothing and removes a platform check that would have
 * been wrong in one webview or another.
 */
export function clipboardIntent(e: KeyboardEvent): ClipboardIntent {
  if (e.type !== "keydown" || e.altKey) return null;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  const ctrl = e.ctrlKey;
  const meta = e.metaKey;
  if (!ctrl && !meta && !(e.shiftKey && key === "Insert")) return null;

  if (key === "c") {
    if (ctrl && e.shiftKey) return "copy";
    if (meta && !ctrl) return "copy";
    return null;
  }
  if (key === "v") {
    if (ctrl && e.shiftKey) return "paste-read";
    if ((ctrl && !e.shiftKey) || (meta && !ctrl)) return "paste-native";
    return null;
  }
  if (key === "Insert") {
    if (ctrl && !e.shiftKey) return "copy";
    if (e.shiftKey && !ctrl) return "paste-native";
    return null;
  }
  return null;
}

/**
 * Put text on the clipboard.
 *
 * Rejects rather than resolving quietly when there is no way to: a copy that
 * silently did nothing is discovered at the paste, in another application,
 * with the thing you meant to copy long gone from the screen.
 */
export async function writeClipboard(text: string): Promise<void> {
  if (!navigator.clipboard?.writeText) throw new Error("this webview exposes no clipboard");
  await navigator.clipboard.writeText(text);
}

/** Read the clipboard. Rejects for the same reason `writeClipboard` does. */
export async function readClipboard(): Promise<string> {
  if (!navigator.clipboard?.readText) throw new Error("this webview will not read the clipboard");
  return await navigator.clipboard.readText();
}
