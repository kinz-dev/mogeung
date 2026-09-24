/**
 * The pieces the arcade's screens share — the picker, the mental-math game and
 * its history, the Trading Game. Moved here when a second screen arrived,
 * rather than imported back from the first, which would have made the files
 * import each other.
 */

import type { CSSProperties, ReactNode } from "react";
import { ChevronLeft, X } from "lucide-react";
import { cn } from "@/lib/cn";
import type { Difficulty, Kind } from "@/lib/mentalMath";
import { IconButton } from "@/ui/primitives";
import { interactive } from "@/ui/styles";

/**
 * The strip across the top of every arcade screen: back to the picker, what
 * this is, and close. `onBack` is `null` where going back is not a click away
 * — mid-run, Escape and the close button end the run first.
 */
export function ArcadeHeader({
  icon,
  title,
  detail,
  onBack,
  onClose,
  closeTitle = "close  (Esc)",
}: {
  icon: ReactNode;
  title: string;
  detail?: ReactNode;
  onBack: (() => void) | null;
  onClose: () => void;
  closeTitle?: string;
}) {
  return (
    <div className="flex shrink-0 items-center gap-3 border-b border-[var(--border)] bg-[var(--bg-panel)] px-5 py-2.5">
      {onBack && (
        <button
          type="button"
          onClick={onBack}
          title="back to the games"
          className={cn("flex items-center gap-0.5 rounded-sm px-1 text-base text-[var(--dim)] hover:text-[var(--text)]", interactive)}
        >
          <ChevronLeft size={16} /> games
        </button>
      )}
      {icon}
      <span className="text-xl font-semibold text-[var(--text-strong)]">{title}</span>
      {detail && <span className="text-base text-[var(--dim)]">{detail}</span>}
      <div className="ml-auto">
        <IconButton title={closeTitle} onClick={onClose}>
          <X size={18} />
        </IconButton>
      </div>
    </div>
  );
}

/**
 * A font size that grows with the window: `vmin`, so a wide window and a tall
 * one agree, clamped so a small window stays legible and a huge one sane.
 */
export const fs = (min: number, vmin: number, max: number): CSSProperties => ({
  fontSize: `clamp(${min}px, ${vmin}vmin, ${max}px)`,
});

export const COLOR: Record<Difficulty, string> = {
  easy: "var(--green)",
  normal: "var(--blue)",
  advanced: "var(--amber)",
  expert: "var(--purple)",
};

export const OP_NAME: Record<Kind, string> = {
  add: "addition",
  sub: "subtraction",
  mul: "multiplication",
  div: "division",
  mixed: "two-step",
};

export function clock(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export const secs = (ms: number | null) => (ms === null ? "—" : `${(ms / 1000).toFixed(1)}s`);

/** A key, at the size of the words around it. */
export function Key({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-[var(--border)] bg-[var(--bg-panel)] px-[0.4em] py-[0.05em] font-mono text-[0.85em] text-[var(--dim)]">
      {children}
    </kbd>
  );
}

export function BigButton({
  children,
  onClick,
  disabled,
  title,
  solid,
  className,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
  solid?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      title={title}
      disabled={disabled}
      onClick={onClick}
      style={fs(15, 2.4, 22)}
      className={cn(
        "inline-flex items-center justify-center gap-2 rounded-lg px-[1.4em] py-[0.55em] font-semibold whitespace-nowrap",
        interactive,
        "disabled:cursor-default disabled:opacity-40",
        // After `interactive`, so the fill survives its hover wash.
        solid
          ? "bg-[var(--blue)] text-[var(--on-accent)] hover:bg-[var(--blue)] hover:brightness-110 active:bg-[var(--blue)] active:brightness-95"
          : "border border-[var(--border)] text-[var(--text)] hover:border-[var(--border-hover)]",
        className,
      )}
    >
      {children}
    </button>
  );
}

export function Label({ children }: { children: ReactNode }) {
  return (
    <div className="mb-2 font-semibold tracking-wider text-[var(--dim)] uppercase" style={fs(11, 1.6, 14)}>
      {children}
    </div>
  );
}

export function Choice({
  on,
  onClick,
  children,
  title,
  className,
}: {
  on: boolean;
  onClick: () => void;
  children: ReactNode;
  title?: string;
  className?: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={on}
      title={title}
      onClick={onClick}
      className={cn(
        "rounded-lg border-2 px-[1em] py-[0.6em] text-left",
        interactive,
        on
          ? "border-[var(--selection-stroke)] bg-[var(--state-focus)] text-[var(--text-strong)]"
          : "border-[var(--border)] text-[var(--dim)] hover:text-[var(--text)]",
        className,
      )}
    >
      {children}
    </button>
  );
}
