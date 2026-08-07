/**
 * Which pane is *that way*. `R-B52`.
 *
 * With one pane in the centre there was nothing to navigate. Two agents side by
 * side (`R-B49`) and a Code pane beside them makes "the one on the left" a
 * thing you can mean, and the mouse the only way to say it.
 *
 * Kept as a pure function over rectangles, with no dockview in sight, because
 * the interesting part is the *choice* and the choice is the part that goes
 * subtly wrong: a naive nearest-centre pick will happily jump diagonally past
 * the pane you were pointing at.
 */

export type Direction = "left" | "right" | "up" | "down";

export interface Box {
  id: string;
  left: number;
  top: number;
  right: number;
  bottom: number;
}

const centre = (b: Box) => ({ x: (b.left + b.right) / 2, y: (b.top + b.bottom) / 2 });

/**
 * The nearest pane in a direction, or `null` when there is nothing there.
 *
 * **No wrapping**, deliberately. A cursor that jumps from the rightmost pane
 * back to the leftmost is the standard behaviour for a *list*, and it is wrong
 * for a floorplan: the whole reason to navigate spatially is that the layout is
 * something you can picture, and a move that teleports across the screen breaks
 * the picture it is trading on. Running out of panes should feel like a wall.
 *
 * Scoring is distance along the axis you asked for, plus a penalty for drifting
 * off it, plus a large penalty for not overlapping at all. That last term is
 * what makes this a *directional* move rather than a nearest-neighbour one: a
 * pane sharing an edge with you always beats a closer one sitting diagonally,
 * which is what "left" means when you say it out loud.
 */
export function pickNeighbour(boxes: readonly Box[], activeId: string, dir: Direction): string | null {
  const from = boxes.find((b) => b.id === activeId);
  if (!from) return null;
  const fc = centre(from);

  let best: string | null = null;
  let bestScore = Infinity;

  for (const b of boxes) {
    if (b.id === activeId) continue;
    const c = centre(b);

    const along =
      dir === "left" ? fc.x - c.x : dir === "right" ? c.x - fc.x : dir === "up" ? fc.y - c.y : c.y - fc.y;
    // Strictly *that* way. A pane whose centre sits on the same line is not to
    // the left of you, whatever its edges do.
    if (along <= 0) continue;

    const across = dir === "left" || dir === "right" ? Math.abs(c.y - fc.y) : Math.abs(c.x - fc.x);
    const overlaps =
      dir === "left" || dir === "right"
        ? b.top < from.bottom && b.bottom > from.top
        : b.left < from.right && b.right > from.left;

    const score = along + across * 2 + (overlaps ? 0 : 100_000);
    if (score < bestScore) {
      bestScore = score;
      best = b.id;
    }
  }
  return best;
}
