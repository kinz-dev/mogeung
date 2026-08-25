/**
 * Folders you keep, for the New session window. `R-J45`.
 *
 * The window has always offered **recent repos** — every `repo_root` the
 * daemon can see a session for — and that list answers the wrong question once
 * you start sessions from here rather than from a terminal. It is a log: it
 * grows with every repository you have touched in the last fortnight, it
 * reorders itself as sessions come and go, and the two projects you actually
 * open every morning sit in it wherever the alphabet put them. A favourite is
 * the opposite kind of list — short, hand-made, and it only changes when you
 * change it.
 *
 * **Machine-scoped**, so this lives in `ScopedPrefs` rather than beside the
 * view preferences. The reason is the one that put `shells` there: the value
 * is a filesystem path, `~/projects/mogeung` means different files on the
 * laptop than on the dev box, and a favourite carried across would open a
 * terminal in the wrong place or in nothing at all. The key is the daemon's
 * `machine_id`, which is exactly right here for a second reason — the terminal
 * opens on **the daemon's** machine (see `launch_terminal`), so the machine
 * the path belongs to and the machine the list is filed under are the same
 * one by construction.
 *
 * **`~` is never expanded here**, and that is not laziness. `shellexpand` runs
 * in the daemon, against the daemon's home directory; expanding in the client
 * would write *this* machine's `/home/me` into a path meant for another's, and
 * the failure would only show up when the window is pointed at a remote
 * daemon. Store what you typed and let the far end read it.
 */

/**
 * One spelling per folder, so `~/p/foo` and `~/p/foo/` are the same favourite.
 *
 * Trailing separators go, because a path picked out of a shell prompt or
 * completed by a file manager carries one about half the time and the list
 * would hold both. The root is the exception it always is — stripping `/`
 * leaves the empty string, which is not a folder.
 */
export function normaliseDir(dir: string): string {
  const trimmed = dir.trim();
  if (trimmed === "/") return trimmed;
  return trimmed.replace(/\/+$/, "");
}

/**
 * Add, idempotently, at the end.
 *
 * Appended rather than sorted: this is a list you built, and one that
 * re-alphabetises on every add moves the row under your cursor between the
 * moment you decide to click it and the moment you do. Blank never enters —
 * the button is disabled for it, and this is the second door.
 */
export function addFavourite(list: string[], dir: string): string[] {
  const clean = normaliseDir(dir);
  if (!clean || list.includes(clean)) return list;
  return [...list, clean];
}

/** Remove, by the same spelling `addFavourite` stored. */
export function removeFavourite(list: string[], dir: string): string[] {
  const clean = normaliseDir(dir);
  return list.filter((d) => d !== clean);
}

/** Whether this folder is already kept — what the star in the window reads. */
export function isFavourite(list: string[], dir: string): boolean {
  const clean = normaliseDir(dir);
  return clean !== "" && list.includes(clean);
}
