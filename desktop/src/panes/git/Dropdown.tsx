/**
 * The filter bar's dropdowns — Branch, User, Date — and a resizable edge.
 *
 * A dropdown here is a *filter that shows what it holds*: the trigger reads
 * `Branch: main`, never a bare caret, so the bar itself says what the list
 * answers. Built on Radix's dropdown menu the way `Menu.tsx` is built on its
 * context menu, with the same surface and the same highlighted-item rule, so
 * the two read as one control family.
 */

import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "@/lib/cn";
import { interactive, surface } from "@/ui/styles";

export function Dropdown({
  label,
  value,
  children,
  title,
  active,
}: {
  label: string;
  value: ReactNode;
  children: ReactNode;
  title?: string;
  /** Something other than the default is in force — the value reads strong. */
  active?: boolean;
}) {
  return (
    <DropdownPrimitive.Root modal={false}>
      <DropdownPrimitive.Trigger asChild>
        <button
          type="button"
          title={title}
          className={cn(
            "inline-flex h-5 shrink-0 items-center gap-1 rounded-sm border border-transparent px-1.5 text-2xs whitespace-nowrap",
            interactive,
            "text-[var(--dim)] hover:border-[var(--border)] data-[state=open]:bg-[var(--state-focus)]",
          )}
        >
          <span>{label}:</span>
          <span className={cn(active ? "text-[var(--amber)]" : "text-[var(--text)]")}>{value}</span>
          <ChevronDown size={10} className="opacity-70" />
        </button>
      </DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content
          align="start"
          sideOffset={4}
          collisionPadding={8}
          className={cn(surface.overlay, "z-50 max-h-96 min-w-44 overflow-y-auto p-1")}
        >
          {children}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

const itemCls =
  "flex cursor-default items-center gap-2 rounded-sm px-2 py-1 text-xs outline-none select-none data-[highlighted]:bg-[var(--state-focus)] data-[highlighted]:text-[var(--text-strong)] data-[disabled]:opacity-40 data-[disabled]:pointer-events-none";

export function DropItem({
  children,
  onSelect,
  active,
  disabled,
  mono,
}: {
  children: ReactNode;
  onSelect: () => void;
  /** The value currently in force — marked, so a menu answers "which one is
   *  on" without having to be read against the trigger. */
  active?: boolean;
  disabled?: boolean;
  mono?: boolean;
}) {
  return (
    <DropdownPrimitive.Item
      onSelect={onSelect}
      disabled={disabled}
      aria-checked={active}
      className={cn(itemCls, mono && "font-mono", active && "text-[var(--text-strong)]")}
    >
      <span className={cn("w-2 shrink-0 text-2xs", active ? "text-[var(--blue)]" : "text-transparent")}>●</span>
      <span className="truncate">{children}</span>
    </DropdownPrimitive.Item>
  );
}

export function DropLabel({ children }: { children: ReactNode }) {
  return (
    <DropdownPrimitive.Label className="px-2 pt-1.5 pb-0.5 text-2xs tracking-wider text-[var(--dim)] uppercase">
      {children}
    </DropdownPrimitive.Label>
  );
}

export function DropSeparator() {
  return <DropdownPrimitive.Separator className="my-1 h-px bg-[var(--border)]" />;
}

/** A submenu — Local ▸, origin ▸ — for a list too long to sit flat. */
export function DropSub({ label, children, disabled }: { label: ReactNode; children: ReactNode; disabled?: boolean }) {
  return (
    <DropdownPrimitive.Sub>
      <DropdownPrimitive.SubTrigger disabled={disabled} className={cn(itemCls, "pl-6")}>
        <span className="truncate">{label}</span>
        <ChevronRight size={11} className="ml-auto text-[var(--dim)]" />
      </DropdownPrimitive.SubTrigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.SubContent
          sideOffset={4}
          collisionPadding={8}
          className={cn(surface.overlay, "z-50 max-h-96 min-w-44 overflow-y-auto p-1")}
        >
          {children}
        </DropdownPrimitive.SubContent>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Sub>
  );
}

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
): [number, (e: React.MouseEvent) => void] {
  const [width, setWidth] = useState(saved);
  const latest = useRef(width);
  useEffect(() => {
    setWidth(saved);
    latest.current = saved;
  }, [saved]);
  const onMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = latest.current;
    const move = (ev: MouseEvent) => {
      latest.current = Math.min(max, Math.max(min, startW + sign * (ev.clientX - startX)));
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

export function Splitter({ onMouseDown, title }: { onMouseDown: (e: React.MouseEvent) => void; title?: string }) {
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      onMouseDown={onMouseDown}
      title={title ?? "drag to resize"}
      className="w-1 shrink-0 cursor-col-resize hover:bg-[var(--blue)]"
    />
  );
}

/** An element's width, watched — the ResizeObserver the rail's tools use.
 *  Zero before the first measurement, which callers treat as "unknown". */
export function useWidth(ref: React.RefObject<HTMLElement | null>): number {
  const [w, setW] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setW(el.clientWidth));
    ro.observe(el);
    setW(el.clientWidth);
    return () => ro.disconnect();
  }, [ref]);
  return w;
}
