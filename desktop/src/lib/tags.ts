/**
 * Colour tags on a session, Finder-fashion.
 *
 * The queue is already full of colour that *means* something — the badge is the
 * reason a row is where it is, the live text is what the CLI says it is doing,
 * the label chip is a hash of your own name for it. A tag is different in kind:
 * it means whatever **you** decided this morning, and nothing computes it.
 *
 * So it gets its own space rather than tinting something that already speaks: a
 * bar down the leading edge of the row, which is the one place in a dense list
 * that carries no other signal. That is also how it stays glanceable, which is
 * the whole request — knowing which session is which without reading a word.
 *
 * Seven, like Finder, and named after the colour rather than a meaning. A fixed
 * palette with names attached ("review", "mine") would be a guess about your
 * workflow that you would then have to work around; the tooltip says the colour
 * and you supply the meaning.
 */

export interface Tag {
  id: string;
  label: string;
  /** A palette variable, so both themes stay hand-tested. */
  color: string;
}

export const TAGS: readonly Tag[] = [
  { id: "red", label: "Red", color: "var(--red)" },
  { id: "orange", label: "Orange", color: "var(--urgent)" },
  { id: "amber", label: "Amber", color: "var(--amber)" },
  { id: "green", label: "Green", color: "var(--green)" },
  { id: "blue", label: "Blue", color: "var(--blue)" },
  { id: "purple", label: "Purple", color: "var(--purple)" },
  { id: "grey", label: "Grey", color: "var(--dim)" },
];

/**
 * The colour for a stored tag id, or `null` for none.
 *
 * An id this build does not know reads as no tag rather than as an error: these
 * live in the preferences file, which is hand-editable and machine-scoped, and
 * one unknown value must not cost the row its rendering.
 */
export function tagColor(id: string | undefined): string | null {
  if (!id) return null;
  return TAGS.find((t) => t.id === id)?.color ?? null;
}

export function tagLabel(id: string | undefined): string | null {
  if (!id) return null;
  return TAGS.find((t) => t.id === id)?.label ?? null;
}
