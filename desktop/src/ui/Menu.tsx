/**
 * A right-click menu.
 *
 * Actions live here rather than as icons on the row, and that is a layout
 * decision as much as a taste one: hover-revealed buttons still occupy their
 * space at `opacity: 0`, so a row full of them squeezes the thing you are
 * actually trying to read — which is exactly how the queue came to be showing
 * session titles as `M.` and `<..`.
 */

import * as ContextMenuPrimitive from "@radix-ui/react-context-menu";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { surface } from "@/ui/styles";

export function ContextMenu({ trigger, children }: { trigger: ReactNode; children: ReactNode }) {
  return (
    <ContextMenuPrimitive.Root>
      <ContextMenuPrimitive.Trigger asChild>{trigger}</ContextMenuPrimitive.Trigger>
      <ContextMenuPrimitive.Portal>
        <ContextMenuPrimitive.Content
          className={cn(surface.overlay, "z-50 min-w-44 p-1")}
          collisionPadding={8}
        >
          {children}
        </ContextMenuPrimitive.Content>
      </ContextMenuPrimitive.Portal>
    </ContextMenuPrimitive.Root>
  );
}

export function MenuItem({
  children,
  onSelect,
  danger,
}: {
  children: ReactNode;
  onSelect: () => void;
  danger?: boolean;
}) {
  return (
    <ContextMenuPrimitive.Item
      onSelect={onSelect}
      className={cn(
        "cursor-default rounded-sm px-2 py-1 text-xs outline-none select-none",
        "data-[highlighted]:bg-[var(--state-focus)] data-[highlighted]:text-[var(--text-strong)]",
        danger && "text-[var(--red)]",
      )}
    >
      {children}
    </ContextMenuPrimitive.Item>
  );
}

export function MenuSeparator() {
  return <ContextMenuPrimitive.Separator className="my-1 h-px bg-[var(--border)]" />;
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return (
    <ContextMenuPrimitive.Label className="px-2 py-1 text-2xs tracking-wider text-[var(--dim)] uppercase">
      {children}
    </ContextMenuPrimitive.Label>
  );
}
