/**
 * The parts of a form, told apart. `R-J130`.
 *
 * Asked 2026-10-11, of the New session and Teams windows: *"Make it visual
 * component more visible so it can easily see the diff section (by color,
 * border, alignment, etc)"*. Both had grown as a column of dim labels and
 * controls at one size, which reads fine at a glance in a pane and not at all
 * in a window of its own, where the eye has to find the part it came for.
 *
 * Three pieces, used by both and by nothing else yet:
 *
 * - **`Section`** — a card with a header strip: an icon and a title in the
 *   section's own colour, a bar of that colour down its left edge, and the
 *   actions that belong to it on the right. The colour is the whole point —
 *   *team*, *policy*, *sends* and *seats* are four colours before they are
 *   four words — and it comes from the tokens, so both themes have it.
 * - **`Field`** — a label in a fixed column and its control beside it, so the
 *   labels of one section line up and the controls start on one edge.
 * - **`Callout`** — a tinted note with an icon, for the sentence that must
 *   not be skimmed past: a flag that skips permissions, a draft that is not
 *   saved yet.
 */

import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

/** A section's colour, by what it means here. All from the tokens. */
export type Tone = "blue" | "purple" | "green" | "teal" | "amber" | "red" | "grey";

const ACCENT: Record<Tone, string> = {
  blue: "var(--blue)",
  purple: "var(--purple)",
  green: "var(--green)",
  teal: "var(--graph-4)",
  amber: "var(--amber)",
  red: "var(--red)",
  grey: "var(--dim)",
};

/** A tint behind a callout, where the tokens have one; the rest take the faint surface. */
const TINT: Partial<Record<Tone, string>> = {
  blue: "var(--tag-blue-bg)",
  purple: "var(--tag-purple-bg)",
  green: "var(--tag-green-bg)",
  amber: "var(--tag-amber-bg)",
  red: "var(--tag-red-bg)",
  grey: "var(--tag-grey-bg)",
};

export function Section({
  title,
  icon,
  tone = "grey",
  hint,
  actions,
  children,
  className,
  bodyClassName,
  testId,
}: {
  title: string;
  icon?: ReactNode;
  tone?: Tone;
  /** A few words after the title, dimmed: what this section is for. */
  hint?: ReactNode;
  /** Controls that act on the whole section, on the header's right. */
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
  testId?: string;
}) {
  const accent = ACCENT[tone];
  return (
    <section
      data-testid={testId}
      // `shrink-0` is load-bearing: a card in a scrolling column is a flex
      // item, and with its overflow hidden its minimum height is nothing, so
      // without this the column squeezed every card to fit instead of
      // scrolling — the guidance, the wakes and the second seat were clipped.
      className={cn("shrink-0 overflow-hidden rounded-md border border-[var(--border)] bg-[var(--bg-panel)]", className)}
      style={{ borderLeft: `3px solid ${accent}` }}
    >
      <header className="flex min-h-8 items-center gap-2 border-b border-[var(--border)] bg-[var(--bg-faint)] px-3 py-1.5">
        {icon && (
          <span className="flex shrink-0 items-center" style={{ color: accent }}>
            {icon}
          </span>
        )}
        <span className="shrink-0 text-xs font-semibold tracking-wider uppercase" style={{ color: accent }}>
          {title}
        </span>
        {hint && <span className="min-w-0 truncate text-xs text-[var(--dim)]">{hint}</span>}
        {actions && <span className="ml-auto flex shrink-0 items-center gap-1">{actions}</span>}
      </header>
      <div className={cn("flex flex-col gap-2.5 p-3", bodyClassName)}>{children}</div>
    </section>
  );
}

export function Field({
  label,
  hint,
  children,
  align = "center",
  className,
}: {
  label: ReactNode;
  /** Under the label, smaller: what the control takes or does. */
  hint?: ReactNode;
  children: ReactNode;
  /** `start` for a control taller than a line — a textarea, a list. */
  align?: "center" | "start";
  className?: string;
}) {
  return (
    <div
      className={cn(
        "grid grid-cols-[8.5rem_minmax(0,1fr)] gap-x-3",
        align === "start" ? "items-start" : "items-center",
        className,
      )}
    >
      <div className={cn("min-w-0 text-xs text-[var(--dim)]", align === "start" && "pt-1.5")}>
        <span className="block font-medium text-[var(--text)]">{label}</span>
        {hint && <span className="block text-2xs leading-snug">{hint}</span>}
      </div>
      <div className="flex min-w-0 flex-wrap items-center gap-2">{children}</div>
    </div>
  );
}

export function Callout({
  tone = "amber",
  icon,
  children,
  className,
  testId,
}: {
  tone?: Tone;
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
  testId?: string;
}) {
  const accent = ACCENT[tone];
  return (
    <div
      data-testid={testId}
      className={cn("flex shrink-0 items-start gap-2 rounded-md border px-3 py-2 text-xs leading-snug text-[var(--text)]", className)}
      style={{ borderColor: accent, background: TINT[tone] ?? "var(--bg-faint)" }}
    >
      {icon && (
        <span className="mt-px flex shrink-0 items-center" style={{ color: accent }}>
          {icon}
        </span>
      )}
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}
