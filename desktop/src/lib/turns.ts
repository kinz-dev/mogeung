/**
 * Which turns the Transcript shows, and in which direction.
 *
 * Pulled out of the pane so the two orderings can be tested without a layout:
 * the pane's own copy of this was three lines inside a `useMemo`, and the
 * *reversal* is not a three-line kind of change — it decides what "the first
 * hit" means to the find box, what index a bookmark jump lands on, and which
 * end of the list is the newest thing.
 */

import type { TranscriptEvent } from "@/wire/types";

/**
 * The turns to draw, in chronological order, with the reasoning blocks
 * dropped when they are not wanted.
 *
 * **Chronological on purpose, whatever the pane displays.** Anything that
 * *leaves* the window — a note, an exported file — takes this rather than the
 * display order, because a conversation written down backwards is a conversation
 * nobody can read: an answer arrives before its question, and a tool result
 * before the call that produced it.
 */
export function visibleTurns(events: TranscriptEvent[] | undefined, showThinking: boolean): TranscriptEvent[] {
  if (!events) return [];
  return showThinking ? events : events.filter((e) => e.kind.t !== "thinking");
}

/**
 * The same turns in the direction they are read on screen.
 *
 * `newestFirst` puts the latest turn at the top, which is what a queue-shaped
 * reading of a session wants: *what has it just done?* The cost is real and
 * worth knowing before turning it on — a call and its result swap places, and a
 * long answer split across turns reads bottom-up. It is a preference rather
 * than the default for exactly that reason.
 */
export function orderedTurns(visible: TranscriptEvent[], newestFirst: boolean): TranscriptEvent[] {
  // A copy, never `reverse()` in place: `visible` is the array the note and the
  // export read, and reversing it under them is the bug this file exists to
  // make impossible.
  return newestFirst ? [...visible].reverse() : visible;
}

/**
 * The index of the newest turn, in whatever direction the list runs.
 *
 * `-1` for an empty list, so the caller can tell "nothing to scroll to" from
 * "scroll to the top".
 */
export function newestIndex(count: number, newestFirst: boolean): number {
  if (count <= 0) return -1;
  return newestFirst ? 0 : count - 1;
}
