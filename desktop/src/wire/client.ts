/**
 * The WebSocket to the daemon.
 *
 * A port of `crates/mogeung-ui/src/net.rs`, and it keeps that module's two
 * properties, which are the ones the client contract depends on:
 *
 * - **Fire-and-forget.** Commands have no replies to await — their effect
 *   arrives on the event stream. There is no correlation layer here, and there
 *   should not be one: the daemon volunteers state, and a client that waited
 *   for answers would be inventing a request/response protocol the daemon does
 *   not speak.
 * - **Reconnect is normal, not exceptional.** The daemon restarts, the tunnel
 *   drops, the laptop sleeps. Every reconnect re-subscribes and the daemon
 *   answers with a full snapshot, so the client never has to reconcile — it
 *   replaces.
 *
 * Sends made while the socket is down are queued rather than dropped, because
 * the alternative is a click that silently does nothing during the second it
 * takes to come back.
 */

import type { ClientMsg, ServerMsg } from "./types";

export type ConnState = "connecting" | "open" | "closed";

export interface ClientOptions {
  url: string;
  onMessage: (msg: ServerMsg) => void;
  onState: (state: ConnState, detail?: string) => void;
}

/** Backoff, in ms. Flat after the last step rather than growing for ever. */
const BACKOFF = [250, 500, 1000, 2000, 4000, 8000];

/** Queued sends are dropped past this, so a long outage cannot grow forever. */
const MAX_QUEUE = 500;

export class DaemonClient {
  private ws: WebSocket | null = null;
  private attempt = 0;
  private timer: number | null = null;
  private queue: string[] = [];
  private closed = false;

  constructor(private opts: ClientOptions) {}

  connect(): void {
    this.closed = false;
    this.open();
  }

  /** Stop for good — no reconnect. */
  dispose(): void {
    this.closed = true;
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.timer = null;
    this.ws?.close();
    this.ws = null;
  }

  /** Point at a different daemon. `R-I7`. */
  setUrl(url: string): void {
    if (url === this.opts.url) return;
    this.opts.url = url;
    // A queue built for the old daemon means nothing to the new one — session
    // ids do not survive the move.
    this.queue = [];
    this.attempt = 0;
    this.ws?.close();
  }

  get url(): string {
    return this.opts.url;
  }

  send(msg: ClientMsg): void {
    const text = JSON.stringify(msg);
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(text);
      return;
    }
    if (this.queue.length < MAX_QUEUE) this.queue.push(text);
  }

  private open(): void {
    if (this.closed) return;
    this.opts.onState("connecting");
    let ws: WebSocket;
    try {
      ws = new WebSocket(this.opts.url);
    } catch (e) {
      // A malformed URL throws synchronously. Treat it as a closed socket so
      // the retry path is the only path — one way to fail is one way to fix.
      this.opts.onState("closed", String(e));
      this.scheduleRetry();
      return;
    }
    this.ws = ws;

    ws.onopen = () => {
      this.attempt = 0;
      this.opts.onState("open");
      // **No `subscribe` here.** The daemon pushes the full snapshot on
      // connect, before this socket has said anything — that push is what
      // makes a reconnect a replacement rather than a merge, and it happens
      // whether or not we ask. `Subscribe` is documented as *re-send* the
      // snapshot, so asking for one on open bought a second copy of the
      // largest payload on the wire and nothing else: measured 2 × 1.19 MB per
      // connect, which with `R-J65`'s queue traffic gone was 94% of all
      // traffic. The command stays in the protocol — it is the explicit
      // recovery path, and a third-party client may want it — but the one
      // client that always reconnects on lag does not need to send it, because
      // the daemon's answer to lag is "reconnect", and a reconnect is a
      // connect. `R-J69`.
      const pending = this.queue;
      this.queue = [];
      for (const text of pending) ws.send(text);
    };

    ws.onmessage = (ev) => {
      if (typeof ev.data !== "string") return;
      let msg: ServerMsg;
      try {
        msg = JSON.parse(ev.data) as ServerMsg;
      } catch {
        // A line we cannot read is one message lost, not a session lost.
        // Same rule the Rust parsers follow: degrade, never panic.
        return;
      }
      this.opts.onMessage(msg);
    };

    ws.onerror = () => {
      // Deliberately quiet. `onclose` always follows, and reporting both
      // means every ordinary reconnect prints two lines of alarm.
    };

    ws.onclose = (ev) => {
      this.ws = null;
      if (this.closed) return;
      this.opts.onState("closed", ev.reason || undefined);
      this.scheduleRetry();
    };
  }

  private scheduleRetry(): void {
    if (this.closed) return;
    const wait = BACKOFF[Math.min(this.attempt, BACKOFF.length - 1)];
    this.attempt += 1;
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => this.open(), wait);
  }
}

/**
 * Where the daemon is.
 *
 * Order: an explicit `?url=` on the page (how a second window is pointed at a
 * second daemon), then the saved connection, then the local default. The port
 * matches `mogeungd`'s own.
 */
export function defaultUrl(): string {
  const fromQuery = new URLSearchParams(window.location.search).get("url");
  if (fromQuery) return fromQuery;
  const saved = localStorage.getItem("mogeung.url");
  if (saved) return saved;
  return "ws://127.0.0.1:7717/ws";
}

/** The REST twin of a WebSocket URL, for the few things that are plain GETs. */
export function httpBase(wsUrl: string): string {
  return wsUrl.replace(/^ws/, "http").replace(/\/ws$/, "");
}
