/**
 * The arcade's clocks: any whole second from 1 to 600, or none. Made for the
 * dice game — *"I want a more flexible setting on the time limit"* — and shared
 * since, so every game that has a clock offers the same one.
 */

/** Any whole second in here can be set. */
export const SECONDS = [1, 600] as const;

/**
 * The stops the − and + buttons walk a clock along: every second where a
 * second matters, coarser where it does not. Any value in `SECONDS` can
 * still be typed.
 */
export const CLOCK_STOPS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 15, 20, 25, 30, 40, 45, 60, 75, 90, 120, 150, 180, 240, 300, 420, 600];

/** The next stop up or down from any value, stopped at the ends. */
export function stepClock(value: number, dir: 1 | -1): number {
  if (dir > 0) return CLOCK_STOPS.find((s) => s > value) ?? CLOCK_STOPS[CLOCK_STOPS.length - 1];
  return [...CLOCK_STOPS].reverse().find((s) => s < value) ?? CLOCK_STOPS[0];
}

/** A clock setting read from anywhere untrusted: whole seconds in range, or none. */
export function clockFrom(v: unknown, fallback: number | null): number | null {
  if (v === null) return null;
  if (typeof v !== "number" || !Number.isFinite(v)) return fallback;
  return Math.max(SECONDS[0], Math.min(SECONDS[1], Math.round(v)));
}

/** `5s`, `90s`, `10m`, `no limit`. */
export function clockText(s: number | null): string {
  if (s === null) return "no limit";
  if (s >= 120 && s % 60 === 0) return `${s / 60}m`;
  return `${s}s`;
}
