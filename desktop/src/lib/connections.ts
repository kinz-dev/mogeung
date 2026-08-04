/**
 * The daemons you switch between. `R-I7`.
 *
 * Client-side and local to this machine, like the keymap and the layout: which
 * daemons *you* watch is not daemon state, and a remote daemon has no business
 * holding the list of its peers. ADR-0001's "no local authority" is about the
 * session record, not about a bookmark list.
 *
 * A URL is the identity here, deliberately: two entries can name the same
 * daemon by two routes — `ws://localhost:7717/ws` through an `ssh -L` tunnel is
 * the same process as `ws://devbox:7717/ws` — and which route works is exactly
 * what you are switching between. Whether it is the *same machine* is a
 * separate question, answered by the daemon's own identity (`R-I5`) and not by
 * anything here.
 */

export interface Connection {
  name: string;
  url: string;
}

const KEY = "mogeung.connections";

export function loadConnections(): Connection[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Filtered rather than trusted: this file is hand-editable, and one bad row
    // must not cost the whole list. The same posture the prefs loader takes.
    return parsed.filter(
      (c): c is Connection =>
        !!c && typeof c === "object" && typeof (c as Connection).url === "string",
    ).map((c) => ({ name: typeof c.name === "string" ? c.name : c.url, url: c.url }));
  } catch {
    return [];
  }
}

export function saveConnections(list: Connection[]): void {
  localStorage.setItem(KEY, JSON.stringify(list));
}

/**
 * Make sure the address currently in use is in the list.
 *
 * Without this, the first thing the window shows a new user is an empty list
 * while they are plainly connected to something — and the entry they most want
 * to keep is the one they never had to type.
 */
export function withCurrent(list: Connection[], url: string): Connection[] {
  if (!url || list.some((c) => c.url === url)) return list;
  return [{ name: defaultName(url), url }, ...list];
}

/** A readable name for an address nobody has named: its host and port. */
export function defaultName(url: string): string {
  try {
    const u = new URL(url);
    return u.port ? `${u.hostname}:${u.port}` : u.hostname;
  } catch {
    return url;
  }
}
