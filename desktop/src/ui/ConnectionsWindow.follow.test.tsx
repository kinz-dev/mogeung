/**
 * Follow, from the Connections panel. `R-I21`, `R-I23`, feature 0047.
 *
 * Each of these has a way of going wrong quietly:
 *
 * - **The token travels once, in its own field** — never inside the address,
 *   which the daemon keeps and shows back in its follow list.
 * - **Consent is off until a hand turns it on**, and turning it on sends
 *   exactly that, for exactly that follow.
 * - **A window on another machine is not offered the controls** — the
 *   daemon's silence (`peers === null`) is the answer, not the address.
 * - **The daemon being watched is not offered at all**: following it would be
 *   following itself.
 */

import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ConnectionsWindow } from "@/ui/ConnectionsWindow";
import { useStore } from "@/store";
import type { ClientMsg, PeerState } from "@/wire/types";

vi.mock("@/lib/tauri", async (orig) => ({
  ...(await orig<typeof import("@/lib/tauri")>()),
  isTauri: () => false,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "lan_peers") return [];
    throw new Error(`unexpected command ${cmd}`);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const HERE = "ws://127.0.0.1:7717/ws";
const THERE = "ws://devbox:7717/ws";
const DAEMON = { host: "ubuntu", version: "0.1.0", claude_home: "/h/.claude", pid: 1, federation: 1 };

const sent: ClientMsg[] = [];

async function open(peers: PeerState[] | null, daemon: object = DAEMON) {
  localStorage.setItem(
    "mogeung.connections",
    JSON.stringify([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE, token: "6f1c" },
    ]),
  );
  useStore.setState({
    showConnections: true,
    url: HERE,
    daemon,
    peers,
    send: (m: ClientMsg) => void sent.push(m),
  } as never);
  render(<ConnectionsWindow />);
  await waitFor(() => expect(screen.getAllByLabelText(/^address for /).length).toBeGreaterThan(0));
}

const pick = (name: string) => fireEvent.click(screen.getByRole("option", { name: new RegExp(`^${name}\\b`) }));

const peer = (over: Partial<PeerState> = {}): PeerState => ({
  id: "b",
  name: "there",
  url: THERE,
  state: "up",
  machine_id: "devbox-id",
  host: "devbox",
  version: "0.1.0",
  sessions: 2,
  allow_writes: false,
  since_ms: 0,
  ...over,
});

beforeEach(() => {
  localStorage.clear();
  sent.length = 0;
});
afterEach(() => {
  cleanup();
  useStore.setState({ showConnections: false, peers: null, daemon: null });
});

describe("following from the connections panel", () => {
  it("follows with the token in its own field, the address clean, and changes off", async () => {
    await open([]);
    pick("there");
    fireEvent.click(screen.getByRole("button", { name: "follow" }));
    expect(sent).toEqual([
      { cmd: "follow", id: "b", name: "there", url: THERE, token: "6f1c", allow_writes: false },
    ]);
  });

  it("shows where a follow stands, and sends consent and unfollow for that follow", async () => {
    await open([peer()]);
    pick("there");
    expect(screen.getByText(/following devbox — 2 sessions mirrored here/)).toBeInTheDocument();
    const consent = screen.getByLabelText(/allow changes on devbox/);
    expect(consent).not.toBeChecked();
    fireEvent.click(consent);
    fireEvent.click(screen.getByRole("button", { name: "unfollow" }));
    expect(sent).toEqual([
      { cmd: "set_peer_writes", id: "b", allow: true },
      { cmd: "unfollow", id: "b" },
    ]);
  });

  it("says a refused token plainly, and follow again resends it with the consent kept", async () => {
    await open([peer({ state: "refused", allow_writes: true })]);
    pick("there");
    expect(screen.getByText(/the token was turned away/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "follow again" }));
    expect(sent[0]).toMatchObject({ cmd: "follow", id: "b", token: "6f1c", allow_writes: true });
  });

  it("offers nothing on the daemon being watched — that would be following itself", async () => {
    await open([]);
    // The form lands on the watched one.
    expect(screen.queryByRole("button", { name: "follow" })).toBeNull();
  });

  it("explains, rather than offers, on a window watching another machine", async () => {
    await open(null);
    pick("there");
    expect(screen.queryByRole("button", { name: "follow" })).toBeNull();
    expect(screen.getByText(/set up from a window on the machine that follows/)).toBeInTheDocument();
  });

  it("offers nothing to a daemon from before federation", async () => {
    await open(null, { ...DAEMON, federation: undefined });
    pick("there");
    expect(screen.queryByRole("button", { name: "follow" })).toBeNull();
    expect(screen.queryByText(/machine that follows/)).toBeNull();
  });
});
