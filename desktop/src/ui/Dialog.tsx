/**
 * A modal window, said once.
 *
 * Escape closes, the backdrop closes, focus moves in on open and back to
 * whatever opened it on close. All four are what separates a dialog from a div
 * that happens to be on top — and all four were missing from the only overlay
 * this app had.
 *
 * And Tab stays inside, cycling. Without that, the third Tab from a two-button
 * confirmation walked out of it into whatever was behind — the Branches popup's
 * search box, for the checkout warning — while the dialog still sat on top
 * claiming to be modal. Asked 2026-10-01.
 */

/** What Tab can land on. Disabled controls are skipped. */
const TABBABLE =
  'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

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
  resizable,
}: {
  title: string;
  subtitle?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
  /**
   * Opens at this size and can be dragged bigger or smaller by its corner;
   * the size it closes at is handed back once, on close, rather than on every
   * frame of the drag. The body is then a fixed-height box its children lay
   * out inside, rather than a column that scrolls.
   */
  resizable?: { width: number; height: number; onResized: (size: { width: number; height: number }) => void };
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  // Read at first render, not in the mount effect: a child with `autoFocus`
  // takes focus during commit, before any effect runs, and an effect would
  // record that child — which is gone by close, dropping focus on the body.
  const returnTo = useRef<HTMLElement | null>(
    typeof document === "undefined" ? null : (document.activeElement as HTMLElement | null),
  );

  // Held in a ref so the effects below can depend on **nothing**.
  //
  // This is the whole bug: `onClose` is almost always an arrow function defined
  // in the caller's body, so it has a new identity on every render. An effect
  // that lists it as a dependency therefore re-runs on every render — and this
  // one calls `panel.focus()`, which yanked focus out of the input after every
  // single keystroke. The dialog was stealing focus from itself.
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const resizedRef = useRef(resizable?.onResized);
  resizedRef.current = resizable?.onResized;
  const sizeRef = useRef<{ width: number; height: number } | null>(null);

  // What the corner was dragged to, kept as it moves and saved on close.
  useEffect(() => {
    const panel = panelRef.current;
    if (!resizedRef.current || !panel || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      sizeRef.current = { width: Math.round(panel.offsetWidth), height: Math.round(panel.offsetHeight) };
    });
    ro.observe(panel);
    return () => {
      ro.disconnect();
      if (sizeRef.current) resizedRef.current?.(sizeRef.current);
    };
  }, []);

  // Mount and unmount only.
  useEffect(() => {
    const panel = panelRef.current;
    // Only if nothing inside has claimed focus already — an autofocused input
    // is the common case and must win.
    if (!panel?.contains(document.activeElement)) panel?.focus();
    return () => {
      // Back where you were. A dialog that drops focus on the body leaves the
      // keyboard nowhere, which in a keyboard-first tool is a dead end.
      //
      // **Only once the panel has really left the page.** StrictMode runs
      // this cleanup and the effect again on a mount that is not ending; a
      // restore then handed focus back to the opener, the re-run found it
      // outside and took it for the panel, and an autofocused button lost it
      // before anyone saw it. A real unmount has detached the node by now.
      if (panel?.isConnected) return;
      returnTo.current?.focus?.();
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        closeRef.current();
      } else if (e.key === "Tab") {
        trapTab(e, panelRef.current);
      }
    };
    // Capture, so the window's own Escape binding does not act first.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <div
      className={cn(
        "fixed inset-0 z-50 flex items-start justify-center bg-black/40",
        resizable ? "pt-[4vh]" : "pt-[10vh]",
      )}
      onClick={onClose}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        style={
          resizable
            ? { width: resizable.width, height: resizable.height, minWidth: 640, minHeight: 420 }
            : undefined
        }
        className={cn(
          surface.dialog,
          "flex flex-col overflow-hidden outline-none",
          resizable
            ? "max-h-[92vh] max-w-[96vw] resize"
            : cn("max-h-[76vh] w-full", wide ? "max-w-3xl" : "max-w-xl"),
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
        <div className={cn("min-h-0 flex-1", resizable ? "overflow-hidden" : "overflow-y-auto")}>{children}</div>
      </div>
    </div>
  );
}

/**
 * Keep Tab inside `panel`, wrapping at either end.
 *
 * Only the topmost modal acts: a dialog opened over another installs its own
 * listener, and two traps each pulling focus home would fight over every key.
 */
function trapTab(e: KeyboardEvent, panel: HTMLElement | null) {
  if (!panel) return;
  const modals = document.querySelectorAll('[aria-modal="true"]');
  if (modals[modals.length - 1] !== panel) return;
  // Hidden by attribute rather than by layout: layout is a question jsdom
  // cannot answer, and a filter on it kept only the focused button.
  const all = [...panel.querySelectorAll<HTMLElement>(TABBABLE)].filter((el) => !el.closest("[hidden], [inert]"));
  if (all.length === 0) {
    e.preventDefault();
    return;
  }
  const at = all.indexOf(document.activeElement as HTMLElement);
  const next = e.shiftKey
    ? at <= 0 ? all[all.length - 1] : all[at - 1]
    : at === -1 || at === all.length - 1 ? all[0] : all[at + 1];
  e.preventDefault();
  next.focus();
}
