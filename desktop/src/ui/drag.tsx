/**
 * Dragging an edge to resize a part of the window.
 *
 * Moved out of the Git pane's `Dropdown.tsx` on 2026-09-23 when the
 * Connections panel became the second place to want it, and re-exported from
 * there so the Git pane's imports stayed as they were.
 */

import { useEffect, useRef, useState } from "react";

/**
 * A pane's width, dragged by its edge and written to the preferences once
 * on release — the idiom every draggable edge in this window follows (the
 * queue, the dock, the old Git list). Local state during the drag so the
 * preferences file is not rewritten sixty times a second; a ref beside the
 * state so the `mouseup` registered at the start of the drag saves the width
 * the drag *ended* at rather than the one it started at.
 */
export function useDragWidth(
  saved: number,
  min: number,
  max: number,
  save: (px: number) => void,
  /** `-1` for an edge on the pane's left, where dragging right shrinks it. */
  sign: 1 | -1 = 1,
  /** `"y"` for a height, dragged by a horizontal edge. */
  axis: "x" | "y" = "x",
): [number, (e: React.MouseEvent) => void] {
  const [width, setWidth] = useState(saved);
  const latest = useRef(width);
  useEffect(() => {
    setWidth(saved);
    latest.current = saved;
  }, [saved]);
  const onMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    const at = (ev: { clientX: number; clientY: number }) => (axis === "x" ? ev.clientX : ev.clientY);
    const startX = at(e);
    const startW = latest.current;
    const move = (ev: MouseEvent) => {
      latest.current = Math.min(max, Math.max(min, startW + sign * (at(ev) - startX)));
      setWidth(latest.current);
    };
    const up = () => {
      save(latest.current);
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  return [width, onMouseDown];
}

export function Splitter({
  onMouseDown,
  title,
  orientation = "vertical",
}: {
  onMouseDown: (e: React.MouseEvent) => void;
  title?: string;
  /** `"horizontal"` for an edge between two stacked parts. */
  orientation?: "vertical" | "horizontal";
}) {
  return (
    <div
      role="separator"
      aria-orientation={orientation}
      onMouseDown={onMouseDown}
      title={title ?? "drag to resize"}
      className={
        orientation === "vertical"
          ? "w-1 shrink-0 cursor-col-resize hover:bg-[var(--blue)]"
          : "h-1 shrink-0 cursor-row-resize hover:bg-[var(--blue)]"
      }
    />
  );
}
