/**
 * A path in a column narrower than it is.
 *
 * Plain `truncate` spends the width it has on the leading directories and then
 * cuts off the file name — the one part you were reading, and the reason a
 * column of `desktop/src/store/prefs.…` says nothing about which seven files
 * changed. [`dirTail`] is the house answer and is used here rather than a
 * second one: whole segments come off the front, never mid-name, because a
 * half-cut directory reads as a directory that exists.
 *
 * The budget is in characters and the column is in pixels, so it is estimated
 * from the pane's width — deliberately roughly. It only decides *which end*
 * gives way; CSS truncation is still the backstop, and the untouched path is on
 * the title for when two directories end the same way.
 */

import { Mono } from "@/ui/primitives";
import { dirTail } from "@/lib/format";

export function FilePath({ path, chars }: { path: string; chars: number }) {
  const shown = dirTail(path, chars);
  const cut = shown.lastIndexOf("/");
  const dir = cut < 0 ? "" : shown.slice(0, cut + 1);
  const name = shown.slice(cut + 1);
  return (
    <span className="flex min-w-0 items-baseline" title={path}>
      {dir && <Mono className="truncate text-xs text-[var(--dim)]">{dir}</Mono>}
      <Mono className="shrink-0 text-xs">{name}</Mono>
    </span>
  );
}

/** How many `text-xs` monospace characters a width holds, less the state
 *  column and the padding. An estimate, and only ever an estimate. */
export function charsFor(width: number): number {
  return Math.max(12, Math.floor((width - 56) / 7));
}
