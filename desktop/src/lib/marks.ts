/**
 * Bookmarks in a file, and the nine that carry a digit. `R-J37`.
 *
 * Asked for on 2026-08-20 in IntelliJ's own terms — *"`Ctrl+Shift+[0-9]` to
 * mark a numbered bookmark, `Ctrl+[0-9]` to navigate to that numbered
 * bookmark"* — so the gesture arrives already learnt and the only real
 * questions are about the list.
 *
 * **These are the marks that already existed**, not a second kind. Clicking the
 * glyph margin has put a `[session, path, line]` in `ScopedPrefs.bookmarks`
 * since `R-B29`; a digit is a fourth element on the same tuple. One concept in
 * one place, which is what stops *bookmark* meaning two things in one window —
 * the rail's Bookmarks tool is `R-B35`'s marks on **transcript turns**, and
 * that collision is old enough that adding a third would be the `pin` mistake
 * `R-B49` refused.
 *
 * **A digit belongs to one line.** Setting `3` where `3` already is takes it
 * off; setting it somewhere else moves it, and the line it left keeps its
 * plain mark — a chord that silently deleted a mark you made by hand would be
 * a worse surprise than one that leaves it behind unnumbered.
 *
 * **Nine, not ten.** `$mod+Digit0` is reset-zoom and `$mod+Shift+Digit0` is
 * reset-every-pane's-zoom, which is exactly the pair this feature would need
 * at `0`. `Ctrl+0` for *reset the zoom* is a convention older and more widely
 * held than anything here, and `R-B47`'s lesson is that a chord this window
 * claims is one it takes away — from the browser it runs in, and from every
 * terminal it embeds. Nine slots that cost nothing beats ten that cost that.
 */

import type { SessionId } from "@/wire/types";

/**
 * `[session, path, line]`, and since `R-J37` an optional digit.
 *
 * The fourth element is absent on every mark written before then and on every
 * mark made by clicking the margin, which is what makes this a widening rather
 * than a migration: an old `prefs.json` loads unchanged and its marks keep
 * working.
 */
export type Mark = [SessionId, string, number, string?];

/** The digits a bookmark can carry. See the note above on why `0` is not one. */
export const MNEMONICS = ["1", "2", "3", "4", "5", "6", "7", "8", "9"] as const;

export const digitOf = (m: Mark): string | undefined => m[3];

const sameLine = (m: Mark, session: SessionId, path: string, line: number) =>
  m[0] === session && m[1] === path && m[2] === line;

/** The plain mark a margin click makes: on if it was off, off if it was on. */
export function toggleMark(marks: readonly Mark[], session: SessionId, path: string, line: number): Mark[] {
  const at = marks.find((m) => sameLine(m, session, path, line));
  if (at) return marks.filter((m) => m !== at);
  return [...marks, [session, path, line]];
}

/**
 * Put digit `d` on this line — or take it off, if it is already here.
 *
 * Three things happen at once and each is a decision:
 *
 * - the digit **leaves wherever it was**, because a mnemonic that could name
 *   two lines is not a mnemonic;
 * - the line it left **keeps its mark**, unnumbered;
 * - a line that was already marked **gains** the digit rather than being
 *   marked twice.
 */
export function setMnemonic(
  marks: readonly Mark[],
  session: SessionId,
  path: string,
  line: number,
  d: string,
): Mark[] {
  const here = marks.find((m) => sameLine(m, session, path, line));
  if (here && digitOf(here) === d) {
    // Toggling the same digit off leaves the line marked. The digit is what
    // the chord is about; the mark may have been made by hand long before.
    return marks.map((m) => (m === here ? ([m[0], m[1], m[2]] as Mark) : m));
  }
  const moved = marks.map((m) => (digitOf(m) === d ? ([m[0], m[1], m[2]] as Mark) : m));
  const at = moved.find((m) => sameLine(m, session, path, line));
  if (at) return moved.map((m) => (m === at ? ([m[0], m[1], m[2], d] as Mark) : m));
  return [...moved, [session, path, line, d]];
}

export function markFor(marks: readonly Mark[], d: string): Mark | null {
  return marks.find((m) => digitOf(m) === d) ?? null;
}

/**
 * Where the cursor is, in whichever file pane has it. `R-J37`.
 *
 * Module state rather than store state, and the reason is the write rate: this
 * moves on every arrow key, and a `set` per keystroke would re-render every
 * pane subscribed to the store to tell them a number they do not read. The
 * same argument, and the same shape, as `dock` in [`panes.ts`](panes.ts).
 *
 * The **pane id** travels with it so the action can refuse to act on a stale
 * position: a chord fires wherever the keyboard is, including over a terminal,
 * and *set a bookmark on the line I was looking at ten minutes ago* is not
 * what anybody meant.
 */
let here: { pane: string; session: SessionId; path: string; line: number } | null = null;

export function noteCursor(pane: string, session: SessionId, path: string, line: number): void {
  here = { pane, session, path, line };
}

/** Forgotten when its pane goes, so a closed file cannot be bookmarked. */
export function forgetCursor(pane: string): void {
  if (here?.pane === pane) here = null;
}

/** The cursor, but only if its pane is the one the keyboard is actually in. */
export function cursorIn(activePane: string | null): typeof here {
  return here && here.pane === activePane ? here : null;
}
