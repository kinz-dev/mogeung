/**
 * Marks for the editors mogeung can hand a project to. `R-J92`.
 *
 * Separate from [`AgentGlyphs`], which is scoped to the agent CLIs a session
 * can be running. This is the other direction: an IDE the *window* opens, not
 * a tool the session is.
 *
 * **Vendored, not linked**, for the reasons [`AgentGlyphs`] gives at length —
 * a desktop app watching a local daemon has no business reaching a CDN for a
 * 300-byte shape, and an icon that vanishes on a train is worse than no icon.
 * This one arrived as `Intellij-Idea-Logo--Streamline-Ultimate.png`, a 48×48
 * bitmap, and is traced here as a path so it stays sharp at any size and takes
 * `currentColor` like every other icon in the header.
 *
 * These are trademarks belonging to their owners, used here to identify the
 * thing they name and for no other purpose.
 */

/**
 * The IntelliJ IDEA mark: `IJ` and a caret, knocked out of a filled square.
 *
 * **Solid where lucide's are strokes**, and full-bleed where a lucide glyph
 * keeps a two-unit margin, so at the same box it reads heavier than the icons
 * beside it. It is sized to the same `h-3.5` anyway: the `IJ` is small enough
 * at 14px that trading legibility for even ink is the wrong way round.
 *
 * `fillRule="evenodd"` is what cuts the letters out: the square is the first
 * subpath and every other subpath is a hole in it.
 */
export function IntellijGlyph({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden focusable="false">
      <path
        fill="currentColor"
        fillRule="evenodd"
        d="M0 0H24V24H0Z M3 4H8V6H6.5V11.5H8V13.5H3V11.5H4.5V6H3Z M15.5 4H17.5V10.3C17.5 12.2 16.2 13.5 14.2 13.5C12.6 13.5 11.6 12.9 11 12.2L12.3 10.9C12.8 11.4 13.4 11.7 14 11.7C15 11.7 15.5 11.2 15.5 10.2Z M3 19H11V21H3Z"
      />
    </svg>
  );
}
