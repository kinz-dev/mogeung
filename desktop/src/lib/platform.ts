/**
 * Which desktop this window is on. `R-J19`.
 *
 * Reported 2026-08-08: the shipped bindings work on Ubuntu and *"especially
 * those `Alt+*` keys"* do nothing at all on macOS. Two separate causes, and a
 * Mac keeps half a keymap unless both are answered:
 *
 * - **Option composes, it does not modify.** macOS uses Option as a dead-key
 *   layer, so `⌥A` arrives with `event.key === "å"` and `⌥2` with `"™"`.
 *   tinykeys matches a binding's key half against `event.key` and then against
 *   `event.code`, so `Alt+a` matches neither and the chord is silently dead —
 *   nothing throws, the key simply does nothing, which is the failure hardest
 *   to report and hardest to find from a bug report.
 * - **Option is not where a Mac hand goes.** Even spelled so that it fires,
 *   `⌥` is not the modifier this platform puts application chords on. `⌘` is.
 *
 * The first is why a mechanical `Alt+a` → `Alt+KeyA` rewrite would be enough
 * to make the keymap *work*; the second is why the defaults move to `⌘`
 * instead. `MAC_KEYS` in `keymap.ts` is that table.
 */

const MAC = /Mac|iPod|iPhone|iPad/;

export type Platform = "mac" | "other";

/**
 * **Deliberately the same test tinykeys makes.**
 *
 * tinykeys decides what `$mod` means from `navigator.platform` alone. If this
 * file decided from something else, a machine the two disagreed about would
 * resolve `$mod+k` to `Ctrl+K` while being handed the `⌘` table — a palette
 * and a keymap window advertising chords that never fire, with no error
 * anywhere. So `navigator.platform` is asked first and its answer is final.
 *
 * The user agent is a **fallback only for when the property is missing**, not
 * a second opinion: it is what a future engine that has finished deprecating
 * `navigator.platform` will leave us, and on such an engine tinykeys reads
 * `""` and calls it not-a-Mac. That case is a bug in tinykeys we would then
 * have to route around; naming it here is cheaper than rediscovering it.
 */
export function currentPlatform(): Platform {
  if (typeof navigator !== "object" || navigator === null) return "other";
  const platform = navigator.platform || "";
  if (platform) return MAC.test(platform) ? "mac" : "other";
  return MAC.test(navigator.userAgent || "") ? "mac" : "other";
}

/** Is this a Mac? The question everything else in the keymap actually asks. */
export function isMac(platform: Platform = currentPlatform()): boolean {
  return platform === "mac";
}
