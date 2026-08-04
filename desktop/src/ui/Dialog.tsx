/**
 * A modal window, said once.
 *
 * Escape closes, the backdrop closes, focus moves in on open and back to
 * whatever opened it on close. All four are what separates a dialog from a div
 * that happens to be on top — and all four were missing from the only overlay
 * this app had.
 */

import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/cn";
import { IconButton } from "@/ui/primitives";
import { surface } from "@/ui/styles";

export function Dialog({
  title,
  subtitle,
  onClose,
  children,
  wide,
}: {
  title: string;
  subtitle?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const returnTo = useRef<HTMLElement | null>(null);

  // Held in a ref so the effects below can depend on **nothing**.
  //
  // This is the whole bug: `onClose` is almost always an arrow function defined
  // in the caller's body, so it has a new identity on every render. An effect
  // that lists it as a dependency therefore re-runs on every render — and this
  // one calls `panel.focus()`, which yanked focus out of the input after every
  // single keystroke. The dialog was stealing focus from itself.
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  // Mount and unmount only.
  useEffect(() => {
    returnTo.current = document.activeElement as HTMLElement | null;
    // Only if nothing inside has claimed focus already — an autofocused input
    // is the common case and must win.
    if (!panelRef.current?.contains(document.activeElement)) panelRef.current?.focus();
    return () => {
      // Back where you were. A dialog that drops focus on the body leaves the
      // keyboard nowhere, which in a keyboard-first tool is a dead end.
      returnTo.current?.focus?.();
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        closeRef.current();
      }
    };
    // Capture, so the window's own Escape binding does not act first.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[10vh]"
      onClick={onClose}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        className={cn(
          surface.dialog,
          "flex max-h-[76vh] w-full flex-col overflow-hidden outline-none",
          wide ? "max-w-3xl" : "max-w-xl",
        )}
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-3 py-2">
          <span className="text-base font-semibold text-[var(--text-strong)]">{title}</span>
          {subtitle && <span className="truncate text-xs text-[var(--dim)]">{subtitle}</span>}
          <div className="ml-auto">
            <IconButton title="close  (Esc)" onClick={onClose}>
              <X size={13} />
            </IconButton>
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
      </div>
    </div>
  );
}
