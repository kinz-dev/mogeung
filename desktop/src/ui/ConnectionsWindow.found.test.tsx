/**
 * Found on your LAN. `R-I17` phase 1, step 6.
 *
 * What the panel adds on top of `lib/lan.ts`'s pure decisions, and how each
 * fails quietly:
 *
 * - **A row draws from what the shell holds, and follows what it emits.** A
 *   panel that only asked once would show a daemon that left an hour ago as
 *   present.
 * - **Add makes a saved entry, selects it, and dials nothing.** `R-I8`'s
 *   rule: finding is never connecting. A row that switched the window would
 *   drop the whole board on a click meant to save an address.
 * - **A gone daemon stays, greyed.** Dropping it is how a Wi-Fi client in
 *   power save — which misses 40 % of beacons — turns into a list that
 *   flickers.
 * - **This machine's own daemon is left off the list.** Asked for the day the
 *   panel was first seen; a row you cannot go to is noise above the ones you
 *   can.
 * - **A saved connection lights up when it is being heard.**
 * - **A browser tab says it cannot listen**, rather than showing an empty list.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ConnectionsWindow } from "@/ui/ConnectionsWindow";
import { useStore } from "@/store";
import type { FoundDaemon } from "@/lib/lan";

const tauri = vi.hoisted(() => ({ on: true }));
vi.mock("@/lib/tauri", async (orig) => ({
  ...(await orig<typeof import("@/lib/tauri")>()),
  isTauri: () => tauri.on,
}));

/** The shell: a connections file, a beacon table, and the event it emits. */
const shell = vi.hoisted(() => ({
  list: [] as unknown[],
  peers: [] as unknown[],
  error: null as string | null,
  emit: null as null | ((peers: unknown[]) => void),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: { list?: unknown[] }) => {
    if (cmd === "connections_load") return shell.list;
    if (cmd === "connections_save") {
      shell.list = args?.list ?? [];
      return undefined;
    }
    if (cmd === "lan_peers") {
      if (shell.error) throw new Error(shell.error);
      return shell.peers;
    }
    throw new Error(`unexpected command ${cmd}`);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown[] }) => void) => {
    if (name === "lan:peers") shell.emit = (peers) => cb({ payload: peers });
    return () => {
      shell.emit = null;
    };
  }),
}));

const HERE = "ws://127.0.0.1:7717/ws";

const found = (over: Partial<FoundDaemon> = {}): FoundDaemon => ({
  machine: "abc123",
  home: "/home/k/.claude",
  host: "devbox",
  addr: "10.0.0.30",
  addrs: ["10.0.0.30"],
  port: 7717,
  version: "0.1.0",
  reach: "lan",
  ssh: null,
  live: 2,
  waiting: 1,
  last_seen_ms: Date.now() - 12_000,
  beacons: 4,
  gone: false,
  mine: false,
  ...over,
});

async function open(list: unknown[] = []) {
  shell.list = list;
  useStore.setState({ showConnections: true, url: HERE } as never);
  render(<ConnectionsWindow />);
  await waitFor(() => expect(screen.getByText(/found on your LAN/)).toBeInTheDocument());
}

describe("found on your LAN", () => {
  beforeEach(() => {
    tauri.on = true;
    shell.peers = [];
    shell.error = null;
    shell.emit = null;
    localStorage.clear();
  });
  afterEach(() => cleanup());

  it("draws what the shell holds when the panel opens, and follows what it emits", async () => {
    shell.peers = [found()];
    await open();
    const row = await screen.findByText("devbox");
    expect(row).toBeInTheDocument();
    expect(screen.getByText(/2 live · 1 need you · 12 s ago/)).toBeInTheDocument();
    expect(screen.getByText(/type its token/)).toBeInTheDocument();
    // A second daemon appears through the event, not through another ask.
    await waitFor(() => expect(shell.emit).not.toBeNull());
    act(() => shell.emit?.([found(), found({ machine: "def456", host: "spark", addr: "10.0.0.183" })]));
    expect(await screen.findByText("spark")).toBeInTheDocument();
  });

  it("saves a found daemon on add, selects it, and dials nothing", async () => {
    const setUrl = vi.fn();
    useStore.setState({ setUrl } as never);
    shell.peers = [found()];
    await open();
    fireEvent.click(await screen.findByTitle(/save this daemon/));
    // The form on the right is now that daemon, token field and all.
    expect(screen.getByLabelText("address for devbox")).toHaveValue("ws://10.0.0.30:7717/ws");
    expect(screen.getByLabelText("token for devbox")).toHaveValue("");
    await waitFor(() => expect(shell.list.map((c) => (c as { url: string }).url)).toContain("ws://10.0.0.30:7717/ws"));
    expect(setUrl).not.toHaveBeenCalled();
  });

  it("keeps a gone daemon on the list, greyed and saying so", async () => {
    shell.peers = [found({ gone: true, last_seen_ms: Date.now() - 3 * 60_000 })];
    await open();
    const row = (await screen.findByText("devbox")).closest("[data-found]")!;
    expect(row).toHaveAttribute("data-gone");
    expect(screen.getByText(/gone — last heard 3 min ago/)).toBeInTheDocument();
  });

  it("leaves this machine's own daemon off the list", async () => {
    shell.peers = [found({ mine: true, host: "kinz-xps14" }), found({ machine: "def456", host: "spark", addr: "10.0.0.183" })];
    await open();
    expect(await screen.findByText("spark")).toBeInTheDocument();
    expect(screen.queryByText("kinz-xps14")).toBeNull();
  });

  it("says nothing else is heard when only this machine is", async () => {
    shell.peers = [found({ mine: true, host: "kinz-xps14" })];
    await open();
    expect(await screen.findByText(/nothing else heard yet/)).toBeInTheDocument();
  });

  it("lights up a saved connection that is being heard, and offers no second add", async () => {
    shell.peers = [found()];
    await open([{ id: "d", name: "the dev box", url: "ws://10.0.0.30:7717/ws" }]);
    await screen.findByText("devbox");
    expect(screen.getByTitle("being heard on your LAN right now")).toBeInTheDocument();
    expect(screen.getByText("saved as the dev box")).toBeInTheDocument();
    expect(screen.queryByTitle(/save this daemon/)).toBeNull();
  });

  it("says why when the shell is not listening", async () => {
    shell.error = "not listening for beacons on udp/7717: address in use";
    await open();
    expect(await screen.findByText(/not listening for beacons/)).toBeInTheDocument();
  });

  it("says a browser tab cannot listen, rather than showing an empty list", async () => {
    tauri.on = false;
    await open();
    expect(await screen.findByText(/only the desktop window can/)).toBeInTheDocument();
  });
});
