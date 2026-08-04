/**
 * The shared vocabulary every control is built from.
 *
 * This file is the design system's actual mechanism. Tokens in `index.css` say
 * what the values are; these say **which combination means "a thing you can
 * press"** — and having one answer is the difference between a system and a
 * set of variables that each component interprets slightly differently.
 *
 * The rule when adding a control: compose from here first, and only reach for a
 * one-off class when the control is genuinely unlike anything else. If you find
 * yourself writing the third one-off, it belongs in this file.
 */

/**
 * The focus ring. **The single largest interaction gap this app had** — it
 * shipped with none at all, in a tool whose keyboard-first premise (`A13`) is
 * `SUPPORTED`. A keyboard-driven interface where you cannot see what has focus
 * is not a styling shortfall, it is the product not working.
 *
 * `focus-visible`, not `focus`, so a mouse click does not leave a ring behind.
 * The outline is **inset** — a positive offset gets clipped by the scroll
 * containers these controls live in, and an inset ring is legible on the page,
 * on a panel and on a selected row without needing to know which it is on.
 */
export const focusRing =
  "outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2";

/** Colour transitions at the fast duration. Layout is never animated: a pane
 * that eases into place is a pane you wait for. */
export const motion = "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]";

/**
 * The state layer.
 *
 * Material's model: a translucent wash of the *content* colour over whatever
 * surface is underneath, rather than a hard-coded grey. One rule then works on
 * the queue, the tree, a tab strip and a menu without any of them knowing what
 * they sit on — which is precisely what a hard-coded `bg-faint` could not do.
 */
export const stateLayer =
  "hover:bg-[var(--state-hover)] active:bg-[var(--state-press)]";

/** Everything you can press or focus. */
export const interactive = `${focusRing} ${motion} ${stateLayer}`;

/** Disabled, said once. Never `pointer-events-none` — a disabled control still
 * has to be hoverable, or its tooltip explaining *why* is unreachable. */
export const disabled = "disabled:opacity-40 disabled:cursor-default disabled:hover:bg-transparent";

/** A row in a list: selected is a surface, focused is a ring, and they stack. */
export const row = `${focusRing} ${motion} cursor-default hover:bg-[var(--state-hover)]`;
export const rowSelected = "bg-[var(--selection-bg)] text-[var(--text-strong)]";

/** Floating surfaces, by how far off the page they are. */
export const surface = {
  /** Inline chrome — headers, strips. Flat. */
  flat: "bg-[var(--bg-panel)]",
  /** Cards and inputs that need to read as raised. */
  raised: "bg-[var(--bg-raised)] border border-[var(--border)]",
  /** Popovers, menus, tooltips. */
  overlay:
    "bg-[var(--bg-raised)] border border-[var(--window-stroke)] rounded-md shadow-[var(--elev-2)]",
  /** Dialogs and the palette. */
  dialog:
    "bg-[var(--bg-raised)] border border-[var(--window-stroke)] rounded-lg shadow-[var(--elev-3)]",
} as const;

/** The section labels that head every panel. One spelling, everywhere. */
export const sectionLabel =
  "text-2xs font-semibold uppercase tracking-wider text-[var(--dim)]";

/**
 * Control heights, on a 4px grid at compact density.
 *
 * Deliberately below Material's 48dp: this is a pointer-and-keyboard tool where
 * a row is ~24px and the board is read at a glance. `md` is the default; `sm`
 * is for controls that sit inside a row.
 */
export const control = {
  sm: "h-5 px-1.5 text-2xs",
  md: "h-6 px-2 text-xs",
  lg: "h-7 px-2.5 text-sm",
} as const;

export type ControlSize = keyof typeof control;
