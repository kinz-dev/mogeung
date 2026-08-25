/**
 * The small pieces every pane is built from.
 *
 * Hand-written on Radix rather than pulled from a component kit: mogeung's look
 * is dense, bespoke and already decided (`R-J6`'s two tested palettes), and a
 * themed kit would spend the whole port fighting it.
 *
 * Every control here composes from `styles.ts`. That is the system — not the
 * tokens, which are only values, but the fact that "a thing you can press" has
 * exactly one spelling and every control uses it.
 */

import * as React from "react";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { cn } from "@/lib/cn";
import {
  control,
  disabled as disabledCls,
  interactive,
  motion,
  row as rowCls,
  rowSelected,
  sectionLabel,
  surface,
  type ControlSize,
} from "@/ui/styles";

/** Secondary text. Dim marks *secondary information*, never decoration. */
export function Dim({
  children,
  className,
  title,
}: {
  children: React.ReactNode;
  className?: string;
  title?: string;
}) {
  return (
    <span title={title} className={cn("text-[var(--dim)]", className)}>
      {children}
    </span>
  );
}

export function Mono({ children, className }: { children: React.ReactNode; className?: string }) {
  return <span className={cn("font-mono", className)}>{children}</span>;
}

/**
 * A filled badge — the queue's `WAITING`, `APPROVE`, `REVIEW`.
 *
 * Lettering colour is chosen against the fill rather than fixed: white on amber
 * is 2.36:1, which shipped once on the two badges most often on screen and is
 * exactly what `R-J6`'s contrast tests were written to stop.
 */
export function Badge({
  children,
  color,
  dark,
  className,
}: {
  children: React.ReactNode;
  color: string;
  /** Use the dark lettering — for light fills such as amber. */
  dark?: boolean;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-sm px-1.5 py-px text-2xs font-semibold tracking-wide whitespace-nowrap",
        className,
      )}
      style={{ background: color, color: dark ? "var(--on-accent-dark)" : "var(--on-accent)" }}
    >
      {children}
    </span>
  );
}

/** An outline badge, for states that should read as information not alarm. */
export function Chip({
  children,
  color = "var(--dim)",
  className,
  title,
}: {
  children: React.ReactNode;
  color?: string;
  className?: string;
  title?: string;
}) {
  return (
    <span
      title={title}
      className={cn(
        "inline-flex items-center rounded-sm border px-1 py-px text-2xs whitespace-nowrap",
        className,
      )}
      style={{ borderColor: color, color }}
    >
      {children}
    </span>
  );
}

/**
 * A keyboard shortcut, shown the same way everywhere.
 *
 * `A13` is `SUPPORTED` — the bindings are how this is driven — so a shortcut
 * printed three different ways in three places is a small tax paid constantly.
 */
export function Kbd({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        "inline-flex items-center rounded-sm border border-[var(--border)] bg-[var(--bg)]",
        "px-1 font-mono text-2xs text-[var(--dim)]",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

export function Button({
  children,
  onClick,
  title,
  disabled,
  active,
  className,
  variant = "ghost",
  size = "md",
  type = "button",
}: {
  children: React.ReactNode;
  /** Handed on for the same reason `IconButton` does — a button inside a
   *  clickable row needs to be able to stop it. */
  onClick?: (e: React.MouseEvent) => void;
  title?: string;
  disabled?: boolean;
  active?: boolean;
  className?: string;
  variant?: "ghost" | "outline" | "solid";
  size?: ControlSize;
  type?: "button" | "submit";
}) {
  return (
    <button
      type={type}
      title={title}
      disabled={disabled}
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "inline-flex shrink-0 items-center justify-center gap-1 rounded-sm whitespace-nowrap",
        control[size],
        interactive,
        disabledCls,
        variant === "outline" && "border border-[var(--border)] hover:border-[var(--border-hover)]",
        variant === "solid" &&
          "border border-transparent bg-[var(--blue)] text-[var(--on-accent)] hover:brightness-110 active:brightness-95",
        active && "bg-[var(--state-focus)] text-[var(--text-strong)]",
        className,
      )}
    >
      {children}
    </button>
  );
}

export function IconButton({
  children,
  onClick,
  title,
  active,
  disabled,
  className,
}: {
  children: React.ReactNode;
  /**
   * The event is handed on, because a button inside a clickable row has to be
   * able to `stopPropagation` — otherwise pressing it also does whatever the
   * row does, and the diff's blast-radius button folded the file it was asking
   * about.
   */
  onClick?: (e: React.MouseEvent) => void;
  title?: string;
  active?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      aria-pressed={active}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "grid h-6 w-6 shrink-0 place-items-center rounded-sm",
        interactive,
        disabledCls,
        active ? "text-[var(--blue)]" : "text-[var(--dim)] hover:text-[var(--text)]",
        className,
      )}
    >
      {children}
    </button>
  );
}

export function Input({
  value,
  onChange,
  placeholder,
  onKeyDown,
  className,
  autoFocus,
  inputRef,
  mono,
  ariaLabel,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  className?: string;
  autoFocus?: boolean;
  inputRef?: React.Ref<HTMLInputElement>;
  mono?: boolean;
  ariaLabel?: string;
}) {
  return (
    <input
      ref={inputRef}
      value={value}
      autoFocus={autoFocus}
      placeholder={placeholder}
      aria-label={ariaLabel ?? placeholder}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={onKeyDown}
      spellCheck={false}
      className={cn(
        "h-6 w-full rounded-sm border border-[var(--border)] bg-[var(--bg)] px-2 text-sm",
        motion,
        // A text box shows focus by its border rather than a ring: the border
        // is already there, so lighting it up moves nothing on the page.
        "outline-none placeholder:text-[var(--dim)]",
        "hover:border-[var(--border-hover)] focus:border-[var(--ring)]",
        mono && "font-mono",
        className,
      )}
    />
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
  title,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  title?: string;
}) {
  return (
    <label
      title={title}
      className={cn(
        "inline-flex cursor-pointer items-center gap-1.5 rounded-sm px-1 py-0.5 text-xs select-none",
        motion,
        "hover:bg-[var(--state-hover)] has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-[var(--ring)] has-[:focus-visible]:-outline-offset-2",
      )}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="h-3 w-3 accent-[var(--blue)] outline-none"
      />
      <span>{label}</span>
    </label>
  );
}

/**
 * A segmented control — pick one of a few.
 *
 * It existed three times before this, spelled three ways: the queue's scope,
 * the Git pane's sub-views and the Insight nav were each hand-rolled with
 * slightly different padding, radius and selected treatment. Three spellings of
 * one control is the most common way a UI stops feeling like one thing.
 */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  className,
}: {
  value: T;
  // `ReactNode` and not `string` since `R-J51`: the New session window's CLI
  // picker puts each agent's own mark beside its name, and a segmented control
  // is exactly the right shape for that choice — a picker that could only hold
  // text would have meant a second one built beside this.
  options: readonly { value: T; label: React.ReactNode; title?: string }[];
  onChange: (v: T) => void;
  className?: string;
}) {
  return (
    <div role="tablist" className={cn("flex items-center gap-0.5", className)}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          aria-selected={value === o.value}
          title={o.title}
          onClick={() => onChange(o.value)}
          className={cn(
            "rounded-sm px-1.5 py-0.5 text-2xs whitespace-nowrap",
            interactive,
            value === o.value
              ? "bg-[var(--state-focus)] text-[var(--text-strong)]"
              : "text-[var(--dim)] hover:text-[var(--text)]",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** A pane's header strip: a name in small caps, and actions on the right. */
export function PaneHeader({
  title,
  children,
  hint,
}: {
  title: string;
  children?: React.ReactNode;
  hint?: string;
}) {
  return (
    <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
      <span title={hint} className={sectionLabel}>
        {title}
      </span>
      <div className="ml-auto flex items-center gap-0.5">{children}</div>
    </div>
  );
}

/** A label above a group of rows, inside a scrolling pane. */
export function SectionLabel({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={cn(sectionLabel, "px-2 pt-2 pb-1", className)}>{children}</div>;
}

/**
 * "Nothing here", said in a way that can be told from "this failed to load".
 *
 * `R-J5` found seventeen places where those two were the same empty box. They
 * are different states and the difference is the whole message.
 */
export function Empty({ children, hint }: { children: React.ReactNode; hint?: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1 p-6 text-center">
      <div className="text-sm text-[var(--dim)]">{children}</div>
      {hint && <div className="max-w-xs text-xs text-[var(--dim)] opacity-70">{hint}</div>}
    </div>
  );
}

/**
 * Waiting, and saying so.
 *
 * The pulse is the whole point: a static "loading…" and a hung request look
 * identical, and this app's own `R-J7` is about exactly that confusion.
 */
export function Loading({ what }: { what: string }) {
  return (
    <div className="flex h-full items-center justify-center p-6">
      <span className="animate-pulse text-sm text-[var(--dim)]">{what}…</span>
    </div>
  );
}

export function Separator({ className }: { className?: string }) {
  return <div className={cn("h-px shrink-0 bg-[var(--border)]", className)} />;
}

export function Tooltip({ children, content }: { children: React.ReactNode; content: React.ReactNode }) {
  return (
    <TooltipPrimitive.Root delayDuration={400}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          sideOffset={6}
          className={cn(
            surface.overlay,
            "z-50 max-w-sm px-2 py-1 text-xs whitespace-pre-line text-[var(--text)]",
            "data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0",
          )}
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

export const TooltipProvider = TooltipPrimitive.Provider;

/** A scrollable region that fills its parent and never grows it. */
export function Scroll({
  children,
  className,
  scrollRef,
  horizontal,
}: {
  children: React.ReactNode;
  className?: string;
  scrollRef?: React.Ref<HTMLDivElement>;
  horizontal?: boolean;
}) {
  return (
    <div
      ref={scrollRef}
      className={cn("min-h-0 flex-1 overflow-y-auto", horizontal ? "overflow-x-auto" : "overflow-x-hidden", className)}
    >
      {children}
    </div>
  );
}

/**
 * A row that behaves like a list item.
 *
 * Focusable, because a list you can only reach with a mouse is half a list —
 * and `aria-selected` so the selection is announced rather than merely
 * coloured.
 */
export function Row({
  selected,
  children,
  className,
  ...rest
}: {
  selected?: boolean;
  children: React.ReactNode;
} & React.ComponentPropsWithoutRef<"div">) {
  return (
    // `...rest` and the forwarded `ref` are both load-bearing, not tidiness:
    // Radix's `asChild` clones this element with merged props — the
    // `onContextMenu` that opens a menu among them — and a component that
    // names its props and drops the remainder silently swallows every one of
    // them. The menu then simply never opens, with nothing to debug.
    <div
      role="option"
      aria-selected={!!selected}
      tabIndex={-1}
      {...rest}
      className={cn(rowCls, "px-2 py-0.5 text-sm", selected && rowSelected, className)}
    >
      {children}
    </div>
  );
}
