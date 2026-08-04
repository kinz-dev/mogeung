/**
 * The native half, and how to tell whether it is there.
 *
 * The client runs in two places: a Tauri window, and an ordinary browser tab
 * pointed at the dev server. The browser is a real client — the daemon serves
 * over localhost and does not care what dialled it — but it has no pty, so the
 * terminal panes have to *say so* rather than show a black rectangle.
 *
 * Everything here is dynamically imported. A static import of `@tauri-apps/api`
 * pulls its IPC shim into the browser bundle where it can only fail; loading it
 * on demand keeps the browser path clean.
 */

/** Is this the desktop shell, rather than a browser tab? */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

type Unlisten = () => void;

async function api() {
  const core = await import("@tauri-apps/api/core");
  const event = await import("@tauri-apps/api/event");
  return { core, event };
}

/**
 * Open a pty and stream it under `id`.
 *
 * `command` is the full argv — see `lib/tmux.ts`, which decides between
 * `tmux …` and `ssh -t host tmux …`. This layer spawns what it is handed and
 * decides nothing, which is the same division the egui client drew.
 */
export async function ptyOpen(
  id: string,
  command: string[],
  cwd: string | null,
  cols: number,
  rows: number,
): Promise<void> {
  const { core } = await api();
  await core.invoke("pty_open", { id, command, cwd, cols, rows });
}

export async function ptyWrite(id: string, data: string): Promise<void> {
  const { core } = await api();
  await core.invoke("pty_write", { id, data });
}

export async function ptyResize(id: string, cols: number, rows: number): Promise<void> {
  const { core } = await api();
  await core.invoke("pty_resize", { id, cols, rows });
}

/**
 * Drop the pty.
 *
 * For a tmux-backed session this **detaches**: the session keeps running and is
 * reachable from any terminal. That is the whole of ADR-0010, and it is why
 * closing a tab here is not destructive.
 */
export async function ptyClose(id: string): Promise<void> {
  const { core } = await api();
  await core.invoke("pty_close", { id });
}

export async function onPtyData(cb: (id: string, data: string) => void): Promise<Unlisten> {
  const { event } = await api();
  return event.listen<{ id: string; data: string }>("pty:data", (e) =>
    cb(e.payload.id, e.payload.data),
  );
}

export async function onPtyClosed(cb: (id: string) => void): Promise<Unlisten> {
  const { event } = await api();
  return event.listen<string>("pty:closed", (e) => cb(e.payload));
}

/** What the shell found on the port, and what it did about it. ADR-0009. */
export type DaemonStatus =
  | { mode: "hosting" }
  | { mode: "attached"; pid: number | null; claude_home: string | null }
  | { mode: "none"; reason: string | null };

/**
 * Attach to a running daemon, or take the port and host one.
 *
 * **Making one executable enough.** Running mogeung used to mean two commands
 * in two terminals; the window now checks whether a daemon is already watching
 * and hosts one itself if not. The test is the **bind**, not a probe — two
 * windows opened together would both see an empty port and both try to start
 * one, whereas whoever wins the socket is unambiguously the daemon.
 *
 * The hosted daemon runs on a thread inside this process, so it cannot outlive
 * it: no pid file to go stale, no orphan holding the port after a crash.
 */
export async function daemonAcquire(addr: string): Promise<DaemonStatus> {
  const { core } = await api();
  return await core.invoke<DaemonStatus>("daemon_acquire", { addr });
}

/**
 * The `host:port` a WebSocket URL points at, but **only when it is loopback**.
 *
 * `null` for anything else, and that is the rule rather than an omission: a
 * window pointed at another machine has been told where to look, and starting
 * a local daemon would be answering a question nobody asked. The egui client
 * draws the same line at `--url`.
 */
export function localAddrOf(wsUrl: string): string | null {
  try {
    const u = new URL(wsUrl);
    const host = u.hostname;
    const loopback = host === "127.0.0.1" || host === "localhost" || host === "::1" || host === "[::1]";
    if (!loopback) return null;
    return `${host === "localhost" ? "127.0.0.1" : host}:${u.port || "7717"}`;
  } catch {
    return null;
  }
}

/**
 * This machine's id, read from `~/.mogeung/machine-id`.
 *
 * Read, never invented: it is what decides local-versus-remote, and an id we
 * made up would answer that question wrongly and confidently. `R-I5`.
 */
export async function machineId(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    const { core } = await api();
    return await core.invoke<string | null>("machine_id");
  } catch {
    return null;
  }
}
