/**
 * Which URL to dial for an address. `R-J102`.
 *
 * The address and the token live apart, and the only moment they meet is
 * here: a saved connection keeps its token in the shell's `0600` file
 * (ADR-0036), this machine's own daemon takes its token and port from
 * `config.toml` through the shell (ADR-0009's amendment), and what the window
 * *remembers* between launches is the address alone.
 *
 * Found 2026-09-22: switching to a remote daemon and back left the window
 * refused by its own daemon — *"I think the mogeungd will be killed once I
 * connect to a remote session"*. It was alive. The token was composed once, at
 * launch, and the switch back dialled the saved local entry, which has none;
 * a token-gated daemon gates loopback too. And the switch away had written
 * the remote's token into `localStorage` on its way past.
 */

import { dialledUrl, sameDaemon, withoutToken, type Connection } from "@/lib/connections";
import { dialFor, localAddrOf, type DaemonStatus } from "@/lib/tauri";

/** Where the window looks for this machine's daemon when told nothing else. */
export const LOCAL_URL = "ws://127.0.0.1:7717/ws";

/** This machine's daemon, as the shell reported it, and the address it was
 *  asked for — `127.0.0.1:7717` unless the window was pointed elsewhere. */
export interface LocalDaemon {
  addr: string | null;
  status: DaemonStatus | null;
}

/**
 * Is this address this machine's own daemon? Loopback, and the port the shell
 * was asked for or the one the file moved it to. A loopback address on some
 * other port is somebody's `ssh -L` tunnel, which is a remote in disguise and
 * takes the remote's token, never this machine's.
 */
export function isOwnDaemon(url: string, local: LocalDaemon): boolean {
  const addr = localAddrOf(url);
  if (!addr || !local.addr || !local.status) return false;
  if (addr === local.addr) return true;
  return local.status.mode === "hosting" && addr.endsWith(`:${local.status.port}`);
}

function hasToken(url: string): boolean {
  try {
    return new URL(url).searchParams.has("token");
  } catch {
    return false;
  }
}

/**
 * The URL to dial. This machine's daemon gets the port and token the shell
 * reported; any other address keeps a token it already carries, or takes the
 * one saved against it; anything else is dialled as it is.
 */
export function composeDial(url: string, local: LocalDaemon, saved: Connection[] = []): string {
  if (isOwnDaemon(url, local) && local.status) return dialFor(local.status, url);
  if (hasToken(url)) return url;
  const c = saved.find((x) => x.token && sameDaemon(x.url, url));
  return c ? dialledUrl(c) : url;
}

/** What the window may remember between launches: the address, never the token. */
export function rememberable(url: string): string {
  return withoutToken(url);
}
