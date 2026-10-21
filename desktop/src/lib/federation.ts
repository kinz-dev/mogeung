/**
 * Following another machine's daemon, from the window's side. `R-I21`,
 * `R-I23`, feature 0047.
 *
 * **The window asks; the daemon follows.** A follow is a standing
 * connection from *this machine's daemon* to another — it holds the token,
 * mirrors that daemon's sessions into this one's Attention list, and keeps
 * doing so with the window closed. So the window's part is small: turn a
 * saved connection into a `follow` message, and say in words where each
 * follow stands. The token crosses exactly once, over loopback, into the
 * daemon's own `0600` file (ADR-0036's amendment), and never comes back.
 */

import type { ClientMsg, PeerState } from "@/wire/types";
import { sameDaemon, withoutToken, type Connection } from "@/lib/connections";

/** The follow standing for a saved connection, if there is one. */
export function followOf(c: Connection, peers: PeerState[] | null): PeerState | null {
  if (!peers) return null;
  return peers.find((p) => p.id === c.id) ?? peers.find((p) => sameDaemon(p.url, c.url)) ?? null;
}

function tokenOf(c: Connection): string | null {
  if (c.token) return c.token;
  try {
    return new URL(c.url).searchParams.get("token");
  } catch {
    return null;
  }
}

/**
 * The message that follows a saved connection. Keyed by the connection's own
 * id, so a second follow of the same entry *replaces* the first — which is how
 * a corrected token reaches the daemon — rather than following it twice.
 *
 * A new follow asks for *allow changes* **on** — since 2026-10-21 (`R-J144`,
 * ADR-0013's amendment of that date); it was off until a hand turned it on,
 * and the first thing a hand did on a followed machine was hit the grey box
 * in New session and go back to Connections for it. The checkbox there turns
 * it off, per follow; the owning daemon's own gates decide either way.
 */
export function followMsg(c: Connection, allowWrites = true): ClientMsg {
  return {
    cmd: "follow",
    id: c.id,
    name: c.name || withoutToken(c.url),
    url: withoutToken(c.url),
    token: tokenOf(c),
    allow_writes: allowWrites,
  };
}

/**
 * What to call each followed machine, by its machine id: the name you gave
 * the saved connection that follows it, as it reads **now** — renaming an
 * entry does not re-send its follow — else the follow's own name, unless
 * that is only its address, in which case nothing, and the caller falls back
 * to the host the machine announces. `R-J139`, reported 2026-10-14: a
 * connection named *mac-m4p* headed its group as *Kinz-Mac-mini.local*.
 */
export function machineNames(
  peers: readonly PeerState[] | null,
  saved: readonly Pick<Connection, "id" | "name" | "url">[],
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const p of peers ?? []) {
    if (!p.machine_id) continue;
    const c = saved.find((x) => x.id === p.id) ?? saved.find((x) => sameDaemon(x.url, p.url));
    const given = c?.name?.trim() || (sameDaemon(p.name, p.url) ? "" : p.name.trim());
    if (given) out[p.machine_id] = given;
  }
  return out;
}

/** Where a follow stands, in words. */
export function peerLine(p: PeerState): string {
  const n = `${p.sessions} session${p.sessions === 1 ? "" : "s"}`;
  const who = p.host ? ` ${p.host}` : "";
  switch (p.state) {
    case "up":
      return `following${who} — ${n} mirrored here${p.version ? ` · ${p.version}` : ""}`;
    case "connecting":
      return `following — connecting…${p.detail ? ` (${p.detail})` : ""}`;
    case "down":
      return `following — not answering${p.sessions ? `; ${n} kept, greyed, last known` : ""}${
        p.detail ? ` (${p.detail})` : ""
      }`;
    case "refused":
      return "following — the token was turned away. Fix the token above, then follow again.";
    case "incompatible":
      return `not followed — ${p.detail ?? "that daemon's build cannot be followed; upgrade it"}`;
    case "myself":
      return "that is this machine's own daemon — there is nothing to follow";
  }
}

/** Is the follow somewhere a person should look at? */
export function peerTrouble(p: PeerState): boolean {
  return p.state !== "up" && p.state !== "connecting";
}
