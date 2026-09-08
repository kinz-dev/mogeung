/**
 * The daemons you switch between. `R-I7`, `R-I16`.
 *
 * Client-side and local to this machine, like the keymap and the layout: which
 * daemons *you* watch is not daemon state, and a remote daemon has no business
 * holding the list of its peers. ADR-0001's "no local authority" is about the
 * session record, not about a bookmark list. That argument is unchanged.
 *
 * **What changed is where the bytes rest.** Since `R-I16` an entry can carry a
 * token, and a token is a shared secret — so the list moved out of
 * `localStorage`, which is unencrypted and carries no mode bits, into a file
 * the Tauri shell owns at `~/.mogeung/connections.json`, mode `0600`. See
 * [ADR-0036](../../../docs/decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md).
 * Client-side is not the same as webview-side: the client has a native half,
 * and it is the half that can hold a file properly.
 *
 * This is not a new exposure — it is the end of an old one. Before the token
 * had a field, the only way to reach a token-gated daemon was to type
 * `?token=…` into the address, which `saveConnections` then wrote verbatim
 * into `localStorage`.
 *
 * **`id` is the identity, not `url`.** Two entries can name the same daemon by
 * two routes — `ws://localhost:7717/ws` through an `ssh -L` tunnel is the same
 * process as `ws://devbox:7717/ws` — and which route works is exactly what you
 * are switching between. The URL also has to stay *editable*, which it cannot
 * be while it is also the key. Whether two entries are the *same machine* is a
 * separate question, answered by the daemon's own identity (`R-I5`).
 */

import { isTauri } from "@/lib/tauri";

export interface Connection {
  id: string;
  name: string;
  url: string;
  /** The shared token a non-loopback bind requires (`R-I10`). Never in the URL. */
  token?: string;
  /** The tunnel command you use — recorded so the panel can show it beside a
   *  dead connection, and **never run**. `R-I16` draws that line explicitly. */
  note?: string;
}

const KEY = "mogeung.connections";

/** Enough for a list you edit by hand; not a security boundary. */
export function newId(): string {
  return `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}

/**
 * Accept one row from a store that is hand-editable and may predate every
 * field added since. One bad row must not cost the whole list — the posture
 * the prefs loader takes, for the same reason.
 */
function coerce(c: unknown): Connection | null {
  if (!c || typeof c !== "object") return null;
  const r = c as Partial<Connection>;
  if (typeof r.url !== "string" || !r.url) return null;
  return {
    id: typeof r.id === "string" && r.id ? r.id : newId(),
    // The address, not `defaultName`: an unnamed row has read as its own URL
    // since `R-I7`, and quietly renaming rows was never part of this ask.
    name: typeof r.name === "string" && r.name ? r.name : r.url,
    url: r.url,
    ...(typeof r.token === "string" && r.token ? { token: r.token } : {}),
    ...(typeof r.note === "string" && r.note ? { note: r.note } : {}),
  };
}

function readLocal(): Connection[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.map(coerce).filter((c): c is Connection => c !== null);
  } catch {
    return [];
  }
}

/**
 * Where the list is actually kept, for the panel to say so.
 *
 * A browser tab is a real client — the daemon serves over localhost and does
 * not care what dialled it — but it has no shell, so it has no file. It keeps
 * working against `localStorage`, and the panel tells you, because a fallback
 * that silently held your token somewhere else would be worse than one that
 * refused.
 */
export function storageKind(): "file" | "browser" {
  return isTauri() ? "file" : "browser";
}

/**
 * Read the list, migrating a `localStorage` list into the file once.
 *
 * The migration is deliberately inside `load` rather than at startup: it has to
 * happen before anything reads, and there is exactly one reader. The key is
 * cleared **only after** the write is acknowledged — a half-run migration that
 * dropped the key would lose the list, and one that kept it would overwrite
 * later edits on the next start.
 */
export async function loadConnections(): Promise<Connection[]> {
  if (!isTauri()) return readLocal();

  const core = await import("@tauri-apps/api/core");
  const fromFile = await core.invoke<Connection[]>("connections_load");
  if (fromFile.length > 0) return fromFile.map(coerce).filter((c): c is Connection => c !== null);

  const legacy = readLocal();
  if (legacy.length === 0) return [];
  await core.invoke("connections_save", { list: legacy });
  try {
    localStorage.removeItem(KEY);
  } catch {
    // A cleared key is a tidiness, not a correctness: the file now wins on
    // every subsequent load, because it is non-empty.
  }
  return legacy;
}

export async function saveConnections(list: Connection[]): Promise<void> {
  if (!isTauri()) {
    localStorage.setItem(KEY, JSON.stringify(list));
    return;
  }
  const core = await import("@tauri-apps/api/core");
  await core.invoke("connections_save", { list });
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
  return [{ id: newId(), name: defaultName(url), url }, ...list];
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

/**
 * The URL to dial, with the token attached as the daemon expects it.
 *
 * The token lives in its own field and is composed in **here**, at the moment
 * of connecting, so that nothing which displays an entry can leak it. That is
 * the shape `R-I7` had to fix once already, when the dialled URL carrying
 * `?token=` reached a tooltip and a footer.
 *
 * A token already present in the address is left exactly as it is: someone who
 * typed one there means it, and quietly rewriting a URL is a worse surprise
 * than an ugly one.
 */
export function dialledUrl(c: Connection): string {
  if (!c.token) return c.url;
  try {
    const u = new URL(c.url);
    if (u.searchParams.has("token")) return c.url;
    u.searchParams.set("token", c.token);
    return u.toString();
  } catch {
    return c.url;
  }
}

/** Move an entry one place, or return the list untouched at either end. */
export function reorder(list: Connection[], from: number, to: number): Connection[] {
  if (to < 0 || to >= list.length || from < 0 || from >= list.length || from === to) return list;
  const next = [...list];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}
