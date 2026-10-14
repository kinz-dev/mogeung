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

/**
 * The shell's IPC, loaded on demand so a browser bundle never runs it — and
 * loaded **once**, through one promise. Every function below used to import
 * it for itself; two of them in flight together (the switcher asks for the
 * list and the status at once) is also the case Vitest's mocking resolves
 * wrongly, which is how this was found.
 */
let shellModule: Promise<typeof import("@tauri-apps/api/core")> | null = null;
function shell(): Promise<typeof import("@tauri-apps/api/core")> {
  shellModule ??= import("@tauri-apps/api/core");
  return shellModule;
}

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
/**
 * Who hears the saved list whenever it is read or written — the store, which
 * keeps each entry's name for the Attention list's machine headings
 * (`R-J139`). The names only matter; a listener must not keep the tokens.
 */
type Listener = (list: Connection[]) => void;
const listeners = new Set<Listener>();

export function onConnectionsChanged(listen: Listener): () => void {
  listeners.add(listen);
  return () => listeners.delete(listen);
}

function tell(list: Connection[]): Connection[] {
  for (const listen of listeners) listen(list);
  return list;
}

export async function loadConnections(): Promise<Connection[]> {
  return tell(await readConnections());
}

async function readConnections(): Promise<Connection[]> {
  if (!isTauri()) return readLocal();

  const core = await shell();
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
  tell(list);
  if (!isTauri()) {
    localStorage.setItem(KEY, JSON.stringify(list));
    return;
  }
  const core = await shell();
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
  if (!url || list.some((c) => sameDaemon(c.url, url))) return list;
  return [{ id: newId(), name: defaultName(url), url: withoutToken(url) }, ...list];
}

/** The address with any `token` taken out of its query. */
export function withoutToken(url: string): string {
  try {
    const u = new URL(url);
    if (!u.searchParams.has("token")) return url;
    u.searchParams.delete("token");
    return u.toString();
  } catch {
    return url;
  }
}

/**
 * The same daemon, whatever token either spelling carries. The store's URL
 * has the token composed in — for a remote since `R-I16`, for the window's own
 * daemon since ADR-0009's amendment — and a saved entry keeps it in a field,
 * so comparing the strings would call the daemon you are watching *not saved*.
 */
export function sameDaemon(a: string, b: string): boolean {
  return withoutToken(a) === withoutToken(b);
}

/** The address as a tooltip may show it: the token, if any, blanked. */
export function maskToken(url: string): string {
  return url.replace(/([?&]token=)[^&]*/g, "$1…");
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

/**
 * The one refusal a socket cannot explain, said before dialling: a daemon
 * reachable beyond loopback requires a token (`R-I10`), so an entry with an
 * address on the LAN and no token will be refused every time, and the reason
 * is in the form, not on the wire. Loopback needs none, and is left alone.
 */
export function missingToken(c: Connection): string | null {
  if (c.token) return null;
  try {
    const host = new URL(c.url).hostname.replace(/^\[|\]$/g, "");
    if (host === "127.0.0.1" || host === "localhost" || host === "::1") return null;
  } catch {
    return null;
  }
  return "no token saved — a daemon reachable on the LAN requires one; type it above, then test again";
}

/** What a Test found at the other end of a connection. */
export type ProbeResult =
  | { ok: true; host: string | null; version: string | null; queued: number | null }
  | { ok: false; reason: string };

/**
 * Ask a daemon who it is, without switching to it. `R-I19`.
 *
 * A socket rather than a fetch, because the daemon sets no CORS headers and a
 * webview's `fetch` to another origin is refused before it leaves. The first
 * frame a daemon sends is its snapshot, which carries its identity (`R-I5`),
 * so one message is the whole answer. A refused token and an unreachable host
 * both arrive as a socket that errors or closes before saying anything, and
 * the browser tells a client nothing more than that — so the reason says both.
 */
export function probeConnection(
  url: string,
  timeoutMs = 4000,
  make: (u: string) => WebSocket = (u) => new WebSocket(u),
): Promise<ProbeResult> {
  return new Promise((resolve) => {
    let ws: WebSocket;
    try {
      ws = make(url);
    } catch (e) {
      resolve({ ok: false, reason: `not an address a socket can dial — ${String(e)}` });
      return;
    }
    let done = false;
    const finish = (r: ProbeResult) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      try {
        ws.close();
      } catch {
        // Already gone; nothing to close.
      }
      resolve(r);
    };
    const timer = setTimeout(
      () =>
        finish({
          ok: false,
          reason: `no answer in ${timeoutMs / 1000} s — unreachable, or the token was refused; a socket cannot tell which`,
        }),
      timeoutMs,
    );
    ws.onmessage = (e) => {
      try {
        const msg = JSON.parse(String(e.data)) as {
          daemon?: { host?: string | null; version?: string | null } | null;
          queue?: unknown;
        };
        finish({
          ok: true,
          host: msg.daemon?.host ?? null,
          version: msg.daemon?.version ?? null,
          queued: Array.isArray(msg.queue) ? msg.queue.length : null,
        });
      } catch {
        // Not JSON — keep waiting for a frame that is.
      }
    };
    ws.onerror = () =>
      finish({ ok: false, reason: "refused — unreachable, or the token was wrong; a socket cannot tell which" });
    ws.onclose = () =>
      finish({ ok: false, reason: "closed before it said who it was — the token was probably refused" });
  });
}

/** Is a saved daemon alive, and will it let us in? What the shell found. `R-I20`. */
export interface Liveness {
  id: string;
  state: "up" | "refused" | "down" | "unknown";
  host: string | null;
  version: string | null;
  detail: string | null;
}

/**
 * Ask the shell to check every saved connection's health, each with its own
 * token — which never leaves the shell to do it. `null` in a browser tab,
 * which has no shell and cannot ask a daemon on another origin anything short
 * of opening a socket and being sent its whole board.
 */
export async function connectionsStatus(): Promise<Liveness[] | null> {
  if (!isTauri()) return null;
  const core = await shell();
  try {
    return await core.invoke<Liveness[]>("connections_status");
  } catch {
    return null;
  }
}
