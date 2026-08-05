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
  /**
   * The row surface, which is **not** `color` at lower opacity.
   *
   * The bar alone turned out not to be glanceable enough to answer the
   * question it was built for — reported 2026-08-05, *"the colour marking is
   * not obvious enough"* — so a tagged row now carries the colour across its
   * whole width, at the weight the selection already uses for a row. That
   * needs a second, hand-tested value per theme rather than a mix: the same
   * translucency over `#141518` and over `#e8eaee` lands in two different
   * places, and only one of them would ever have been looked at.
   */
  bg: string;
}

export const TAGS: readonly Tag[] = [
  { id: "red", label: "Red", color: "var(--red)", bg: "var(--tag-red-bg)" },
  { id: "orange", label: "Orange", color: "var(--urgent)", bg: "var(--tag-orange-bg)" },
  { id: "amber", label: "Amber", color: "var(--amber)", bg: "var(--tag-amber-bg)" },
  { id: "green", label: "Green", color: "var(--green)", bg: "var(--tag-green-bg)" },
  { id: "blue", label: "Blue", color: "var(--blue)", bg: "var(--tag-blue-bg)" },
  { id: "purple", label: "Purple", color: "var(--purple)", bg: "var(--tag-purple-bg)" },
  { id: "grey", label: "Grey", color: "var(--dim)", bg: "var(--tag-grey-bg)" },
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

/**
 * The row surface for a stored tag id, or `null` for none.
 *
 * Unknown ids degrade exactly as `tagColor` does, and for the same reason: the
 * value comes from a hand-editable preferences file, and one it does not
 * recognise must cost the row its tint and nothing else.
 */
export function tagBg(id: string | undefined): string | null {
  if (!id) return null;
  return TAGS.find((t) => t.id === id)?.bg ?? null;
}

export function tagLabel(id: string | undefined): string | null {
  if (!id) return null;
  return TAGS.find((t) => t.id === id)?.label ?? null;
}
