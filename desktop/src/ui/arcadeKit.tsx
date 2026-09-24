/**
 * The pieces the arcade's screens share — the picker, the mental-math game and
 * its history, the Trading Game, the dice game. Moved here when a second screen
 * arrived, rather than imported back from the first, which would have made the
 * files import each other; the market games' widgets followed when the dice
 * game became the second of those.
 */

import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { ChevronLeft, X } from "lucide-react";
import { SECONDS, clockFrom, stepClock } from "@/lib/arcadeClock";
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

// --- the pieces the market games share -------------------------------------------

export function Stepper({
  label,
  value,
  range,
  step = 1,
  unit = "",
  onChange,
}: {
  label: string;
  value: number;
  range: readonly [number, number];
  step?: number;
  unit?: string;
  onChange: (v: number) => void;
}) {
  const set = (v: number) => onChange(Math.max(range[0], Math.min(range[1], v)));
  return (
    <div className="flex items-center justify-between gap-3 border-t border-[var(--border)] py-[0.5em]">
      <span>{label}</span>
      <span className="flex items-center gap-2">
        <BigButton onClick={() => set(value - step)} disabled={value <= range[0]} title={`fewer — ${label}`}>
          −
        </BigButton>
        <span className="w-[4.5em] text-center font-mono tabular-nums" aria-label={label}>
          {value}
          {unit}
        </span>
        <BigButton onClick={() => set(value + step)} disabled={value >= range[1]} title={`more — ${label}`}>
          +
        </BigButton>
      </span>
    </div>
  );
}

export function Toggle({ label, on, onChange }: { label: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-center justify-between gap-3 border-t border-[var(--border)] py-[0.5em]">
      <span>{label}</span>
      <Choice on={on} onClick={() => onChange(!on)}>
        {on ? "on" : "off"}
      </Choice>
    </div>
  );
}

export function Tile({ n, label, tone }: { n: ReactNode; label: string; tone?: string }) {
  return (
    <div className="rounded-lg border border-[var(--border)] bg-[var(--bg-panel)] px-[0.9em] py-[0.5em]">
      <div className="font-mono font-semibold tabular-nums" style={{ ...fs(16, 3.2, 30), color: tone ?? "var(--text-strong)" }}>
        {n}
      </div>
      <div className="text-[var(--dim)]" style={fs(11, 1.6, 14)}>
        {label}
      </div>
    </div>
  );
}

export function Countdown({ deadline, total, now }: { deadline: number | null; total: number; now: number }) {
  if (deadline === null) return null;
  const left = Math.max(0, deadline - now);
  const frac = Math.min(1, left / (total * 1000));
  return (
    <div className="flex items-center gap-3" title="time to act">
      <div className="h-2 flex-1 overflow-hidden rounded-full bg-[var(--bg-faint)]">
        <div className="h-full rounded-full" style={{ width: `${frac * 100}%`, background: frac < 0.25 ? "var(--red)" : "var(--blue)" }} />
      </div>
      <span className="w-[3em] text-right font-mono tabular-nums" aria-label="seconds left">
        {Math.min(total, Math.ceil(left / 1000))}s
      </span>
    </div>
  );
}

/** A number box that takes a minus sign and digits, and nothing else. */
export function NumberBox({
  label,
  value,
  onChange,
  onEnter,
  onKey,
  autoFocus,
  width = "7em",
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  onEnter: () => void;
  onKey?: (key: string) => boolean;
  autoFocus?: boolean;
  width?: string;
}) {
  return (
    <input
      aria-label={label}
      autoFocus={autoFocus}
      inputMode="numeric"
      autoComplete="off"
      spellCheck={false}
      value={value}
      // Selected on focus, so a default (the dice game's one unit) is typed over, not onto.
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => {
        const v = e.target.value.replace(/[−–]/g, "-").replace(/[^\d-]/g, "");
        onChange(v.startsWith("-") ? `-${v.slice(1).replace(/-/g, "")}` : v.replace(/-/g, ""));
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onEnter();
        } else if (onKey && e.key.length === 1 && onKey(e.key.toLowerCase())) {
          e.preventDefault();
        }
      }}
      style={{ ...fs(20, 4.2, 40), width }}
      className="rounded-lg border-2 border-[var(--border)] bg-[var(--bg-panel)] px-3 py-[0.1em] text-center font-mono text-[var(--text-strong)] tabular-nums outline-none focus:border-[var(--selection-stroke)]"
    />
  );
}

/** Focuses the button it sits in, once. */
export function AutoFocus() {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const t = setTimeout(() => ref.current?.closest("button")?.focus(), 0);
    return () => clearTimeout(t);
  }, []);
  return <span ref={ref} hidden />;
}

/**
 * A clock: off, or any whole second from 1 to 600. − and + walk the stops in
 * `CLOCK_STOPS` (`lib/arcadeClock.ts`); the box takes any number typed into it.
 * Turning the clock off remembers where it was, so turning it back on returns
 * there.
 */
export function ClockSetting({
  label,
  value,
  fallback,
  hint,
  onChange,
}: {
  label: string;
  value: number | null;
  fallback: number;
  hint?: ReactNode;
  onChange: (v: number | null) => void;
}) {
  const last = useRef(value ?? fallback);
  useEffect(() => {
    if (value !== null) last.current = value;
  }, [value]);
  const [draft, setDraft] = useState(value === null ? "" : String(value));
  useEffect(() => setDraft(value === null ? "" : String(value)), [value]);
  const commit = () => {
    if (draft === "") return setDraft(value === null ? "" : String(value));
    onChange(clockFrom(Number(draft), value));
  };
  const [lo, hi] = SECONDS;
  return (
    <div className="flex items-center justify-between gap-3 border-t border-[var(--border)] py-[0.5em]">
      <span className="min-w-0">
        {label}
        {hint && (
          <span className="block text-[var(--dim)]" style={fs(11, 1.6, 14)}>
            {hint}
          </span>
        )}
      </span>
      <span className="flex shrink-0 items-center gap-2">
        <Choice on={value === null} onClick={() => onChange(value === null ? last.current : null)} title={value === null ? "put the clock back" : "no clock at all"}>
          no limit
        </Choice>
        <BigButton onClick={() => onChange(stepClock(value ?? last.current, -1))} disabled={value !== null && value <= lo} title={`less time — ${label}`}>
          −
        </BigButton>
        <span className="flex items-baseline">
          <input
            aria-label={`${label}, seconds`}
            inputMode="numeric"
            autoComplete="off"
            spellCheck={false}
            placeholder="∞"
            value={draft}
            onFocus={(e) => e.currentTarget.select()}
            onChange={(e) => setDraft(e.target.value.replace(/\D/g, "").slice(0, 3))}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit();
              }
            }}
            title={`any whole number of seconds, ${lo}–${hi}`}
            className="w-[3.2em] rounded-md border border-[var(--border)] bg-[var(--bg-panel)] px-1 text-center font-mono tabular-nums outline-none focus:border-[var(--selection-stroke)]"
          />
          <span className="ml-1 text-[var(--dim)]">s</span>
        </span>
        <BigButton onClick={() => onChange(stepClock(value ?? last.current, 1))} disabled={value !== null && value >= hi} title={`more time — ${label}`}>
          +
        </BigButton>
      </span>
    </div>
  );
}
