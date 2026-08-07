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
 * live in the client's machine-scoped preferences, which an older or newer
 * build may have written, and one unknown value must not cost the row its
 * rendering. (It said "hand-editable file" until 2026-08-07 — true of the
 * retired egui client, never of this one, which keeps a `localStorage` blob.
 * The conclusion outlived the premise; see
 * [ADR-0023](../../../docs/decisions/0023-judgements-stay-in-the-client.md).)
 */
export function tagColor(id: string | undefined): string | null {
  if (!id) return null;
  return TAGS.find((t) => t.id === id)?.color ?? null;
}

/**
 * The row surface for a stored tag id, or `null` for none.
 *
 * Unknown ids degrade exactly as `tagColor` does, and for the same reason: the
 * value comes from stored preferences this build did not write, and one it does not
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

/**
 * Where a tag id sits in the order. Untagged, and unknown, sort last.
 *
 * Unknown reads as untagged for the third time in this file and for the same
 * reason — the value comes from stored preferences this build did not write, and one this
 * build does not recognise must not sort a session somewhere nobody can
 * predict.
 */
export function tagRank(id: string | undefined): number {
  if (!id) return TAGS.length;
  const at = TAGS.findIndex((t) => t.id === id);
  return at < 0 ? TAGS.length : at;
}

/** What the queue knows about a session besides its rank. */
export interface Scoped {
  pinned: string[];
  tags: Record<string, string>;
  labels: Record<string, string>;
}

/**
 * The order the queue is asked for: **pin, then colour, then label**, with the
 * attention rank surviving underneath as the tiebreak.
 *
 * Asked for on 2026-08-06, and it is worth being clear about what it costs,
 * because it is not a display tweak. The queue's claim is that it is *ranked by
 * who needs you* — that is the product, not a feature of it — and this puts two
 * things you decided by hand above what the daemon computed. An untagged
 * `APPROVE` now sits below a tagged `running`.
 *
 * The mitigation is that it is a *layered* comparison rather than a
 * replacement: within a colour, and within a label, rank decides exactly as
 * before, and `Array.prototype.sort` has been stable since ES2019 so returning
 * `0` genuinely preserves it. So the ranking still orders every group; what
 * changed is which groups come first.
 *
 * Colours sort in palette order rather than alphabetically, so the strip down
 * the list reads the same way every time — red, orange, amber, green, blue,
 * purple, grey — and a red row is always above a blue one rather than depending
 * on what the colours happen to be called.
 */
export function compareByTagThenLabel(a: string, b: string, scoped: Scoped): number {
  // Pins still float above everything: they are the strongest hand-made
  // statement in the panel, and colour must not sink one.
  const pin = (scoped.pinned.includes(b) ? 1 : 0) - (scoped.pinned.includes(a) ? 1 : 0);
  if (pin !== 0) return pin;

  const ta = tagRank(scoped.tags[a]);
  const tb = tagRank(scoped.tags[b]);
  if (ta !== tb) return ta - tb;

  const la = (scoped.labels[a] ?? "").trim();
  const lb = (scoped.labels[b] ?? "").trim();
  if (!!la !== !!lb) return la ? -1 : 1;
  if (la && lb && la !== lb) return la.localeCompare(lb);

  // Equal on every hand-made key: say so, and let the rank stand.
  return 0;
}
