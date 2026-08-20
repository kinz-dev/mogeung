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

export type Unlisten = () => void;

/**
 * Tell me when the **OS window** comes to the front. `R-J38`.
 *
 * The DOM's `window.focus` is not enough, and that is the whole reason this
 * exists: in a Tauri webview, bringing the window forward does not reliably
 * dispatch a DOM focus event — the page never lost focus as far as the
 * document is concerned — so a listener that works perfectly in a browser tab
 * does nothing at all in the shipped app. `tauri://focus` is the shell's own
 * signal, and the shell is the only thing that knows.
 *
 * Answers a no-op unsubscriber in a browser, where the DOM event is the truth
 * and this has nothing to add.
 */
export async function onWindowFocus(cb: () => void): Promise<Unlisten> {
  if (!isTauri()) return () => {};
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) cb();
    });
  } catch {
    // A shell without the permission is a window that does not re-read on
    // focus, not a window that fails to open.
    return () => {};
  }
}

/**
 * Zoom the webview itself, the way a browser's `Ctrl+=` does.
 *
 * Native rather than CSS, and the difference is not cosmetic: a CSS `zoom` on
 * the document leaves mouse coordinates and element rectangles in two
 * different spaces, which anything measuring itself in device pixels — xterm
 * sizing a character cell — then maps wrongly. The webview's own zoom scales
 * the whole page including hit-testing, so the two spaces stay one.
 */
export async function setWebviewZoom(factor: number): Promise<void> {
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  await getCurrentWebview().setZoom(factor);
}

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

/**
 * Send bytes to a pty.
 *
 * `origin` names the call site and is for the trace in `lib.rs` — three places
 * can write here and, when a byte arrives that nobody admits to sending, which
 * one it came from is the whole question. It costs a string per keystroke on a
 * path that already crosses an IPC boundary.
 */
export async function ptyWrite(id: string, data: string, origin?: string): Promise<void> {
  const { core } = await api();
  await core.invoke("pty_write", { id, data, origin: origin ?? null });
}

/**
 * Ask the OS where to save, starting from `suggested`.
 *
 * `null` means the picker was declined — a cancel, and nothing should be
 * written. `undefined` means there was no picker to ask: the plugin is absent
 * or refused, and the caller should fall back to the shell's own destination
 * rather than lose the file. Two different silences, so they are two different
 * values.
 *
 * Imported lazily, for the same reason `api()` is: a browser tab has no Tauri
 * at all, and a top-level import of a plugin makes `npm run dev` fail on load
 * rather than degrade.
 */
export async function chooseSavePath(suggested: string): Promise<string | null | undefined> {
  if (!isTauri()) return undefined;
  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const picked = await save({
      defaultPath: suggested,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    return picked ?? null;
  } catch {
    return undefined;
  }
}

/**
 * Write text to a file, and answer with the path it actually went to.
 *
 * `path` is what the picker returned. Without one the shell falls back to the
 * downloads directory, sanitises the name and refuses to overwrite — see
 * `export_target` in `lib.rs`, where the difference between the two routes is
 * argued. The path comes back either way, because a save you cannot find is a
 * save that did not happen.
 */
export async function exportText(name: string, contents: string, path?: string): Promise<string> {
  const { core } = await api();
  return await core.invoke<string>("export_text", { name, contents, path: path ?? null });
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
