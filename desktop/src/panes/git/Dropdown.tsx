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

export { Splitter, useDragWidth } from "@/ui/drag";

/**
 * An element's width, watched — a ResizeObserver behind a ref. Zero before
 * the first measurement, which callers treat as "unknown".
 *
 * No dependency list, on purpose. The element behind a ref can appear after
 * the first render — the Git pane's body does, once a session is selected,
 * after a render that showed *select a session* — and a ref changing is
 * invisible to a dependency list, so an effect keyed on the ref alone
 * observed nothing for the life of the pane, and every ceiling drawn from
 * this width was `Infinity`. Found 2026-09-11 in a browser tab that had the
 * Git tool open before it had a session: 220 and 380 of a 688-px dock, and
 * 80 px of log. Re-observing is skipped while the element is the one already
 * watched, so what runs per render is one pointer comparison.
 */
export function useWidth(ref: React.RefObject<HTMLElement | null>): number {
  const [w, setW] = useState(0);
  const watched = useRef<{ el: HTMLElement; ro: ResizeObserver } | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (el === (watched.current?.el ?? null)) return;
    watched.current?.ro.disconnect();
    watched.current = null;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setW(el.clientWidth));
    ro.observe(el);
    watched.current = { el, ro };
    setW(el.clientWidth);
  });
  useEffect(() => () => watched.current?.ro.disconnect(), []);
  return w;
}
