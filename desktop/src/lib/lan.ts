/**
 * Daemons found on the LAN. `R-I17` phase 1, step 6.
 *
 * The shell listens for beacons from launch (`src-tauri/src/lan.rs`) and the
 * window asks it two things: the table as it stands, and to be told when it
 * changes. A browser tab has neither, and the panel says so rather than
 * showing an empty list.
 *
 * Everything below the two calls is pure, so the panel's decisions — what a
 * row dials, whether a saved connection is the daemon being heard, what *12 s
 * ago* reads as — are tested without a shell.
 */

import type { Connection } from "@/lib/connections";
import { isTauri, type Unlisten } from "@/lib/tauri";

export type Reach = "lan" | "ssh" | "local";

/** One row of the Found list, as the shell sends it. */
export interface FoundDaemon {
  machine: string;
  home: string;
  host: string | null;
  addr: string;
  addrs: string[];
  port: number;
  version: string;
  reach: Reach;
  ssh: string | null;
  live: number;
  waiting: number;
  last_seen_ms: number;
  beacons: number;
  gone: boolean;
  mine: boolean;
}

/** What the shell says when asked: rows, or why it is not listening. */
export type LanAnswer = { peers: FoundDaemon[] } | { error: string } | { browser: true };

async function api() {
  const core = await import("@tauri-apps/api/core");
  const event = await import("@tauri-apps/api/event");
  return { core, event };
}

export async function lanPeers(): Promise<LanAnswer> {
  if (!isTauri()) return { browser: true };
  const { core } = await api();
  try {
    return { peers: await core.invoke<FoundDaemon[]>("lan_peers") };
  } catch (e) {
    return { error: String(e) };
  }
}

export async function onLanPeers(cb: (peers: FoundDaemon[]) => void): Promise<Unlisten> {
  if (!isTauri()) return () => {};
  const { event } = await api();
  return event.listen<FoundDaemon[]>("lan:peers", (e) => cb(e.payload));
}

/** The address a saved connection would carry: the datagram's source and the
 *  port the beacon named, an IPv6 address in its brackets. */
export function wsUrlOf(d: FoundDaemon): string {
  const host = d.addr.includes(":") ? `[${d.addr}]` : d.addr;
  return `ws://${host}:${d.port}/ws`;
}

/** What to call it: the hostname it announced, else where it came from. */
export function foundName(d: FoundDaemon): string {
  return d.host ?? d.addr;
}

/** Host and port of a websocket URL, as `host:port`, or null when it is not one. */
export function hostPortOf(url: string): string | null {
  try {
    const u = new URL(url);
    if (!u.hostname) return null;
    const host = u.hostname.replace(/^\[|\]$/g, "");
    return `${host}:${u.port || (u.protocol === "wss:" ? 443 : 80)}`;
  } catch {
    return null;
  }
}

/**
 * Is this saved connection the daemon being heard? By any of the addresses
 * it has been heard from and the port it announced — the same thing a hand
 * would compare.
 */
export function matchesSaved(saved: Connection, d: FoundDaemon): boolean {
  const target = hostPortOf(saved.url);
  if (!target) return false;
  return d.addrs.some((a) => `${a}:${d.port}` === target);
}

/** *12 s ago*, *3 min ago*, *2 h ago* — on the window's own clock. */
export function agoLabel(lastSeenMs: number, nowMs: number): string {
  const s = Math.max(0, Math.round((nowMs - lastSeenMs) / 1000));
  if (s < 60) return `${s} s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  return `${Math.round(m / 60)} h ago`;
}

/** How a hand reaches it — the line under the name. */
export function reachLabel(d: FoundDaemon): string {
  switch (d.reach) {
    case "lan":
      return "reachable — add it, type its token, connect";
    case "ssh":
      return d.ssh ? `over ssh — ${d.ssh}` : "over ssh";
    case "local":
      return "on its own machine only — an ssh tunnel is the way in";
  }
}
